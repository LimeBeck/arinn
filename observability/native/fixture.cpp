// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "telemetry.h"
#include <sentry.h>
#include <QCoreApplication>
#include <QDebug>
#include <QDir>
#include <QTemporaryFile>
#include <csignal>
int main(int argc, char **argv) {
  QCoreApplication app(argc, argv);
  if (app.arguments().contains("recursive")) {
    ArinnTelemetry::stop();
    // Model failure of a privacy hook: the SDK must drop the fallback event.
    auto options = sentry_options_new();
    sentry_options_set_dsn(options, qgetenv("ARINN_SENTRY_DSN").constData());
    auto directory = qgetenv("XDG_CACHE_HOME") + "/arinn/sentry-shell";
    sentry_options_set_database_path(options, directory.constData());
    sentry_options_set_auto_session_tracking(options, 0);
    sentry_options_set_on_crash(options, [](const sentry_ucontext_t *, sentry_value_t event, sentry_hint_t *, void *) {
      std::raise(SIGSEGV); return event;
    }, nullptr);
    if (sentry_init(options) != 0) return 2;
    sentry_set_extra("private", sentry_value_new_string("private-token-fixture"));
    std::raise(SIGSEGV);
  }
  if (ArinnTelemetry::start()) {
    auto user = sentry_value_new_object();
    sentry_value_set_by_key(user, "email", sentry_value_new_string("private-token-fixture"));
    sentry_set_user(user);
    sentry_set_extra("payload", sentry_value_new_string("https://private.invalid/page"));
    auto context = sentry_value_new_object();
    sentry_value_set_by_key(context, "name", sentry_value_new_string("private-token-fixture"));
    sentry_set_context("device", context);
    sentry_set_tag("private", "private-token-fixture");
    sentry_add_breadcrumb(sentry_value_new_breadcrumb("http", "private-token-fixture"));
    // Raw envelopes bypass before_send; exercise the final transport boundary.
    QTemporaryFile raw;
    if (raw.open()) {
      raw.write("{}\n{\"type\":\"event\"}\n{\"event_id\":\"0123456789abcdef0123456789abcdef\",\"message\":\"private-token-fixture\",\"timestamp\":\"private-token-fixture\",\"user\":{\"username\":\"private-token-fixture\"},\"extra\":{\"url\":\"https://private.invalid/page\"}}\n");
      raw.flush();
      sentry_capture_envelope(sentry_envelope_read_from_file(raw.fileName().toLocal8Bit().constData()));
    }
    auto attributes = sentry_value_new_object();
    sentry_value_set_by_key(attributes, "private", sentry_value_new_string("private-token-fixture"));
    sentry_metrics_count("arinn.shell.messages", 1, attributes);
    sentry_capture_event(sentry_value_new_message_event(SENTRY_LEVEL_ERROR, "private-token-fixture", "private-token-fixture"));
  }
  qDebug("private-token-fixture https://private.invalid/page Private tab");
  qInfo("private-token-fixture");
  qWarning("private-token-fixture https://private.invalid/page Private tab");
  qCritical("private-token-fixture /home/private-fixture/data");
  if (app.arguments().contains("crash")) std::raise(SIGSEGV);
  return 0;
}
