// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "telemetry.h"
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QUrl>
#include <toml++/toml.hpp>
#include <sentry.h>
#include <cstring>
#include <atomic>

namespace {
std::atomic_bool active{false};
void copy(sentry_value_t out, sentry_value_t in, const char *key) {
  auto value = sentry_value_get_by_key(in, key);
  if (!sentry_value_is_null(value)) {
    sentry_value_incref(value);
    sentry_value_set_by_key(out, key, value);
  }
}
sentry_value_t safeFrames(sentry_value_t stack) {
  auto out = sentry_value_new_object();
  auto frames = sentry_value_new_list();
  auto original = sentry_value_get_by_key(stack, "frames");
  for (size_t i = 0; i < sentry_value_get_length(original) && i < 128; ++i) {
    auto frame = sentry_value_new_object();
    auto source = sentry_value_get_by_index(original, i);
    for (auto key : {"instruction_addr", "symbol_addr", "image_addr", "in_app"})
      copy(frame, source, key);
    sentry_value_append(frames, frame);
  }
  sentry_value_set_by_key(out, "frames", frames);
  return out;
}
// Only SDK values are used here: this callback also runs inside a crash handler.
sentry_value_t sanitize(sentry_value_t event, sentry_hint_t *, void *) {
  auto clean = sentry_value_new_object();
  for (auto key : {"event_id", "timestamp", "level", "platform", "release"})
    copy(clean, event, key);
  auto tags = sentry_value_new_object();
  sentry_value_set_by_key(tags, "component", sentry_value_new_string("arinn-shell"));
  sentry_value_set_by_key(clean, "tags", tags);
  sentry_value_set_by_key(clean, "logger", sentry_value_new_string("arinn-shell"));
  sentry_value_set_by_key(clean, "message", sentry_value_new_string("Native technical diagnostic"));
  auto original = sentry_value_get_by_key(sentry_value_get_by_key(event, "exception"), "values");
  auto exceptions = sentry_value_new_list();
  for (size_t i = 0; i < sentry_value_get_length(original) && i < 4; ++i) {
    auto source = sentry_value_get_by_index(original, i);
    auto exception = sentry_value_new_object();
    auto type = sentry_value_get_by_key(source, "type");
    const char *name = sentry_value_as_string(type);
    const char *safe = "NativeError";
    for (auto allowed : {"SIGSEGV", "SIGABRT", "SIGBUS", "SIGILL", "SIGFPE", "QtWarning", "QtCritical", "QtFatal"})
      if (name && std::strcmp(name, allowed) == 0) safe = allowed;
    sentry_value_set_by_key(exception, "type", sentry_value_new_string(safe));
    sentry_value_set_by_key(exception, "value", sentry_value_new_string("Technical failure; private message omitted"));
    auto stack = sentry_value_get_by_key(source, "stacktrace");
    if (!sentry_value_is_null(stack))
      sentry_value_set_by_key(exception, "stacktrace", safeFrames(stack));
    sentry_value_append(exceptions, exception);
  }
  if (sentry_value_get_length(exceptions)) {
    auto wrapper = sentry_value_new_object();
    sentry_value_set_by_key(wrapper, "values", exceptions);
    sentry_value_set_by_key(clean, "exception", wrapper);
  } else sentry_value_decref(exceptions);
  auto stack = sentry_value_get_by_key(event, "stacktrace");
  if (!sentry_value_is_null(stack)) sentry_value_set_by_key(clean, "stacktrace", safeFrames(stack));
  // Keep only module identifiers and addresses required for server symbolication.
  auto images = sentry_value_get_by_key(sentry_value_get_by_key(event, "debug_meta"), "images");
  auto cleanImages = sentry_value_new_list();
  for (size_t i = 0; i < sentry_value_get_length(images) && i < 256; ++i) {
    auto image = sentry_value_new_object();
    auto source = sentry_value_get_by_index(images, i);
    for (auto key : {"type", "debug_id", "code_id", "image_addr", "image_size"}) copy(image, source, key);
    sentry_value_append(cleanImages, image);
  }
  auto debug = sentry_value_new_object();
  sentry_value_set_by_key(debug, "images", cleanImages);
  sentry_value_set_by_key(clean, "debug_meta", debug);
  auto crumbs = sentry_value_get_by_key(event, "breadcrumbs");
  auto cleanCrumbs = sentry_value_new_list();
  for (size_t i = 0; i < sentry_value_get_length(crumbs) && i < 100; ++i) {
    auto source = sentry_value_get_by_index(crumbs, i);
    const char *message = sentry_value_as_string(sentry_value_get_by_key(source, "message"));
    bool allowed = false;
    for (auto known : {"startup", "shutdown", "about-to-quit", "qt.debug", "qt.info", "qt.warning", "qt.critical", "qt.fatal", "input.key", "input.pointer", "focus.in", "focus.out", "window.show", "window.hide", "window.activate", "window.deactivate"})
      if (message && std::strcmp(message, known) == 0) allowed = true;
    if (!allowed) continue;
    auto crumb = sentry_value_new_breadcrumb("default", message);
    copy(crumb, source, "timestamp");
    sentry_value_set_by_key(crumb, "category", sentry_value_new_string("shell.lifecycle"));
    sentry_value_append(cleanCrumbs, crumb);
  }
  sentry_value_set_by_key(clean, "breadcrumbs", cleanCrumbs);
  sentry_value_decref(event);
  return clean;
}
}
namespace ArinnTelemetry {
bool start() {
  if (active) return true;
  auto configRoot = qEnvironmentVariable("XDG_CONFIG_HOME");
  if (configRoot.isEmpty()) {
    const auto home = qEnvironmentVariable("HOME");
    if (!QDir::isAbsolutePath(home)) return false;
    configRoot = home + "/.config";
  }
  if (!QDir::isAbsolutePath(configRoot)) return false;
  QFile file(configRoot + "/arinn/telemetry.toml");
  if (!file.open(QIODevice::ReadOnly) || file.size() > 1024 * 1024) return false;
  QByteArray dsn;
  try {
    const auto bytes = file.readAll();
    const auto config = toml::parse(std::string_view(bytes.constData(), bytes.size()));
    if (!config["telemetry"]["enabled"].value<bool>().value_or(false)) return false;
    if (qEnvironmentVariableIsSet("ARINN_SENTRY_DSN")) dsn = qgetenv("ARINN_SENTRY_DSN");
    else {
      const auto setting = config["telemetry"]["dsn"].value<std::string>();
      if (setting) dsn = QByteArray::fromStdString(*setting);
    }
  } catch (const toml::parse_error &) { return false; }
  const QUrl url(QString::fromUtf8(dsn));
  const bool local = url.host() == "127.0.0.1" || url.host() == "::1";
  if (dsn.isEmpty() || dsn.size() > 2048 || !url.isValid() || url.host().isEmpty() ||
      url.userName().isEmpty() || !url.password().isEmpty() || url.hasQuery() || url.hasFragment() ||
      (url.scheme() != "https" && !(local && url.scheme() == "http"))) return false;
  auto base = qEnvironmentVariable("XDG_CACHE_HOME");
  if (base.isEmpty()) {
    const auto home = qEnvironmentVariable("HOME");
    if (!QDir::isAbsolutePath(home)) return false;
    base = home + "/.cache";
  }
  if (!QDir::isAbsolutePath(base)) return false;
  const auto directory = base + "/arinn/sentry-shell";
  const QFileInfo existing(directory);
  if (existing.isSymLink() || (existing.exists() && !existing.isDir()) ||
      !QDir().mkpath(directory) || !QFile::setPermissions(directory, QFile::ReadOwner | QFile::WriteOwner | QFile::ExeOwner)) return false;
  auto options = sentry_options_new();
  sentry_options_set_dsn(options, dsn.constData());
  sentry_options_set_release(options, ARINN_RELEASE);
  sentry_options_set_database_path(options, QFile::encodeName(directory).constData());
  sentry_options_set_auto_session_tracking(options, 0);
  sentry_options_set_cache_keep(options, SENTRY_CACHE_KEEP_NONE);
  sentry_options_set_max_breadcrumbs(options, 100);
  sentry_options_set_shutdown_timeout(options, 2000);
  sentry_options_set_symbolize_stacktraces(options, 0);
  sentry_options_set_before_send(options, sanitize, nullptr);
  // Mirror the development flag in the SDK transport gate; public consent UI is pending.
  sentry_options_set_require_user_consent(options, 1);
  if (sentry_init(options) != 0) return false;
  sentry_user_consent_give();
  active = true;
  breadcrumb("startup");
  return true;
}
void stop() {
  if (!active) return;
  breadcrumb("shutdown");
  active = false;
  sentry_close();
}
void breadcrumb(const char *operation) {
  if (!active) return;
  auto crumb = sentry_value_new_breadcrumb("default", operation);
  sentry_value_set_by_key(crumb, "category", sentry_value_new_string("shell.lifecycle"));
  sentry_add_breadcrumb(crumb);
}
void qtMessage(QtMsgType type, const QMessageLogContext &) {
  if (!active) return;
  const char *kind = "qt.debug";
  if (type == QtInfoMsg) kind = "qt.info";
  if (type == QtWarningMsg) kind = "qt.warning";
  if (type == QtCriticalMsg) kind = "qt.critical";
  if (type == QtFatalMsg) kind = "qt.fatal";
  breadcrumb(kind);
  if (type == QtDebugMsg || type == QtInfoMsg) return;
  auto event = sentry_value_new_event();
  const char *name = type == QtWarningMsg ? "QtWarning" : type == QtCriticalMsg ? "QtCritical" : "QtFatal";
  sentry_value_set_by_key(event, "level", sentry_value_new_string(type == QtWarningMsg ? "warning" : type == QtFatalMsg ? "fatal" : "error"));
  auto exception = sentry_value_new_exception(name, "Technical failure; private message omitted");
  sentry_value_set_by_key(exception, "stacktrace", sentry_value_new_stacktrace(nullptr, 0));
  sentry_event_add_exception(event, exception);
  sentry_capture_event(event);
  if (type == QtFatalMsg) sentry_flush(2000);
}
}
