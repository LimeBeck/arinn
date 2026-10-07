// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "telemetry.h"
#include "transport.h"
#include <QDateTime>
#include <QElapsedTimer>
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
QElapsedTimer startupTimer;
sentry_transaction_t *startupTransaction = nullptr;
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
  auto originalTags = sentry_value_get_by_key(event, "tags");
  const char *location = sentry_value_as_string(sentry_value_get_by_key(originalTags, "qt.location"));
  if (ArinnTelemetry::safeLocation(location)) sentry_value_set_by_key(tags, "qt.location", sentry_value_new_string(location));
  const char *category = sentry_value_as_string(sentry_value_get_by_key(originalTags, "qt.category"));
  if (ArinnTelemetry::safeCategory(category)) sentry_value_set_by_key(tags, "qt.category", sentry_value_new_string(category));
  sentry_value_set_by_key(clean, "tags", tags);
  // Grouping: component, type, location and template, each checked again.
  auto print = sentry_value_get_by_key(event, "fingerprint");
  if (sentry_value_get_length(print) == 4) {
    auto cleanPrint = sentry_value_new_list();
    bool safe = true;
    for (size_t i = 0; i < 4; ++i) {
      const char *part = sentry_value_as_string(sentry_value_get_by_index(print, i));
      safe = safe && (ArinnTelemetry::safeTemplateText(part) || ArinnTelemetry::safeLocation(part));
      if (safe) sentry_value_append(cleanPrint, sentry_value_new_string(part));
    }
    if (safe) sentry_value_set_by_key(clean, "fingerprint", cleanPrint);
    else sentry_value_decref(cleanPrint);
  }
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
    // Qt messages carry a template that our own handler already reduced.
    const char *value = sentry_value_as_string(sentry_value_get_by_key(source, "value"));
    const bool qt = std::strncmp(safe, "Qt", 2) == 0;
    sentry_value_set_by_key(exception, "value", sentry_value_new_string(
      qt && ArinnTelemetry::safeTemplateText(value) ? value : "Technical failure; private message omitted"));
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
    for (auto known : {"startup", "shutdown", "about-to-quit", "qt.debug", "qt.info", "qt.warning", "qt.critical", "qt.fatal", "qt.noise", "input.key", "input.pointer", "focus.in", "focus.out", "window.show", "window.hide", "window.activate", "window.deactivate"})
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
sentry_value_t sanitizeTransaction(sentry_value_t transaction, void *) {
  const char *name = sentry_value_as_string(sentry_value_get_by_key(transaction, "transaction"));
  if (!name || std::strcmp(name, "shell.startup") != 0) {
    sentry_value_decref(transaction); return sentry_value_new_null();
  }
  auto clean = sentry_value_new_object();
  for (auto key : {"event_id", "start_timestamp", "timestamp"}) copy(clean, transaction, key);
  sentry_value_set_by_key(clean, "type", sentry_value_new_string("transaction"));
  sentry_value_set_by_key(clean, "transaction", sentry_value_new_string("shell.startup"));
  sentry_value_set_by_key(clean, "platform", sentry_value_new_string("native"));
  sentry_value_set_by_key(clean, "release", sentry_value_new_string(ARINN_RELEASE));
  auto tags = sentry_value_new_object();
  sentry_value_set_by_key(tags, "component", sentry_value_new_string("arinn-shell"));
  sentry_value_set_by_key(clean, "tags", tags);
  sentry_value_set_by_key(clean, "spans", sentry_value_new_list());
  auto original = sentry_value_get_by_key(sentry_value_get_by_key(transaction, "contexts"), "trace");
  auto trace = sentry_value_new_object();
  for (auto key : {"trace_id", "span_id", "parent_span_id", "status"}) copy(trace, original, key);
  sentry_value_set_by_key(trace, "op", sentry_value_new_string("shell.startup"));
  auto contexts = sentry_value_new_object();
  sentry_value_set_by_key(contexts, "trace", trace);
  sentry_value_set_by_key(clean, "contexts", contexts);
  sentry_value_decref(transaction);
  return clean;
}
sentry_value_t sanitizeMetric(sentry_value_t metric, void *) {
  const char *name = sentry_value_as_string(sentry_value_get_by_key(metric, "name"));
  bool known = false;
  for (auto allowed : {"arinn.shell.messages", "arinn.shell.errors", "arinn.shell.messages.benign", "arinn.shell.messages.suppressed", "arinn.shell.startup.duration"})
    known = known || (name && std::strcmp(name, allowed) == 0);
  if (!known) {
    sentry_value_decref(metric); return sentry_value_new_null();
  }
  auto clean = sentry_value_new_object();
  for (auto key : {"name", "type", "value", "timestamp", "trace_id", "span_id"}) copy(clean, metric, key);
  if (std::strcmp(name, "arinn.shell.startup.duration") == 0) sentry_value_set_by_key(clean, "unit", sentry_value_new_string("millisecond"));
  auto component = sentry_value_new_object();
  sentry_value_set_by_key(component, "type", sentry_value_new_string("string"));
  sentry_value_set_by_key(component, "value", sentry_value_new_string("arinn-shell"));
  auto attributes = sentry_value_new_object();
  sentry_value_set_by_key(attributes, "component", component);
  sentry_value_set_by_key(clean, "attributes", attributes);
  sentry_value_decref(metric);
  return clean;
}

}
namespace ArinnTelemetry {
QByteArray resolveDsn(const std::optional<QByteArray> &environment, const std::optional<QByteArray> &configuration) {
  return environment.value_or(configuration.value_or(ARINN_DEFAULT_SENTRY_DSN));
}
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
    const auto node = config["telemetry"]["dsn"];
    if (node && !node.is_string()) return false;
    const auto setting = node.value<std::string>();
    const std::optional<QByteArray> configured = setting ? std::optional(QByteArray::fromStdString(*setting)) : std::nullopt;
    const std::optional<QByteArray> environment = qEnvironmentVariableIsSet("ARINN_SENTRY_DSN") ? std::optional(qgetenv("ARINN_SENTRY_DSN")) : std::nullopt;
    dsn = resolveDsn(environment, configured);
  } catch (const toml::parse_error &) { return false; }
  const auto text = QString::fromUtf8(dsn);
  if (text.toUtf8() != dsn) return false;
  const QUrl url(text);
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
  startupTimer.start();
  auto *sender = transport();
  if (!sender) return false;
  auto options = sentry_options_new();
  sentry_options_set_transport(options, sender);
  sentry_options_set_traces_sample_rate(options, 1.0);
  sentry_options_set_dsn(options, dsn.constData());
  sentry_options_set_release(options, ARINN_RELEASE);
  sentry_options_set_database_path(options, QFile::encodeName(directory).constData());
  sentry_options_set_auto_session_tracking(options, 0);
  sentry_options_set_cache_keep(options, SENTRY_CACHE_KEEP_NONE);
  sentry_options_set_max_breadcrumbs(options, 100);
  sentry_options_set_shutdown_timeout(options, 2000);
  sentry_options_set_symbolize_stacktraces(options, 0);
  sentry_options_set_before_send(options, sanitize, nullptr);
  sentry_options_set_before_transaction(options, sanitizeTransaction, nullptr);
  sentry_options_set_before_send_metric(options, sanitizeMetric, nullptr);
  // Mirror the development flag in the SDK transport gate; public consent UI is pending.
  sentry_options_set_require_user_consent(options, 1);
  if (sentry_init(options) != 0) return false;
  sentry_user_consent_give();
  active = true;
  startupTransaction = sentry_transaction_start(sentry_transaction_context_new("shell.startup", "shell.startup"), sentry_value_new_null());
  breadcrumb("startup");
  return true;
}
void stop() {
  if (!active) return;
  finishStartup();
  breadcrumb("shutdown");
  active = false;
  sentry_close();
}
void finishStartup() {
  if (!startupTransaction) return;
  sentry_transaction_finish(startupTransaction);
  startupTransaction = nullptr;
  sentry_metrics_distribution("arinn.shell.startup.duration", startupTimer.elapsed(), "millisecond", sentry_value_new_object());
}
void breadcrumb(const char *operation) {
  if (!active) return;
  auto crumb = sentry_value_new_breadcrumb("default", operation);
  sentry_value_set_by_key(crumb, "category", sentry_value_new_string("shell.lifecycle"));
  sentry_add_breadcrumb(crumb);
}
void qtMessage(QtMsgType type, const QMessageLogContext &context, const QString &message) {
  // A message emitted while reporting another one is not reported again.
  thread_local bool reporting = false;
  if (!active || reporting) return;
  reporting = true;
  const struct Reset { bool &flag; ~Reset() { flag = false; } } reset{reporting};
  sentry_metrics_count("arinn.shell.messages", 1, sentry_value_new_object());
  if (type == QtDebugMsg || type == QtInfoMsg) {
    breadcrumb(type == QtDebugMsg ? "qt.debug" : "qt.info");
    return;
  }
  const auto shape = qtTemplate(context, message);
  const char *name = type == QtWarningMsg ? "QtWarning" : type == QtCriticalMsg ? "QtCritical" : "QtFatal";
  const auto key = QByteArray(name) + '\n' + shape.location + '\n' + shape.text;
  const bool fatal = type == QtFatalMsg;
  if (shape.benign && !fatal) {
    // Expected noise, such as a missing icon: a counter, and one breadcrumb per window.
    sentry_metrics_count("arinn.shell.messages.benign", 1, sentry_value_new_object());
    if (admitTemplate(key, QDateTime::currentMSecsSinceEpoch())) breadcrumb("qt.noise");
    return;
  }
  sentry_metrics_count("arinn.shell.errors", 1, sentry_value_new_object());
  if (!fatal && !admitTemplate(key, QDateTime::currentMSecsSinceEpoch())) {
    sentry_metrics_count("arinn.shell.messages.suppressed", 1, sentry_value_new_object());
    return;
  }
  breadcrumb(type == QtWarningMsg ? "qt.warning" : type == QtCriticalMsg ? "qt.critical" : "qt.fatal");
  auto event = sentry_value_new_event();
  sentry_value_set_by_key(event, "level", sentry_value_new_string(type == QtWarningMsg ? "warning" : fatal ? "fatal" : "error"));
  auto exception = sentry_value_new_exception(name, shape.text.constData());
  sentry_value_set_by_key(exception, "stacktrace", sentry_value_new_stacktrace(nullptr, 0));
  sentry_event_add_exception(event, exception);
  auto tags = sentry_value_new_object();
  if (!shape.location.isEmpty()) sentry_value_set_by_key(tags, "qt.location", sentry_value_new_string(shape.location.constData()));
  if (!shape.category.isEmpty()) sentry_value_set_by_key(tags, "qt.category", sentry_value_new_string(shape.category.constData()));
  sentry_value_set_by_key(event, "tags", tags);
  auto print = sentry_value_new_list();
  for (const QByteArray &part : {QByteArray("arinn-shell"), QByteArray(name),
                                 shape.location.isEmpty() ? QByteArray("unknown.location") : shape.location, shape.text})
    sentry_value_append(print, sentry_value_new_string(part.constData()));
  sentry_value_set_by_key(event, "fingerprint", print);
  sentry_capture_event(event);
  if (fatal) sentry_flush(2000);
}
}
