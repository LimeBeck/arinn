// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "telemetry.h"
#include <sentry.h>
#include <QCoreApplication>
#include <QDebug>
#include <csignal>
int main(int argc, char **argv) {
  QCoreApplication app(argc, argv);
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
    sentry_capture_event(sentry_value_new_message_event(SENTRY_LEVEL_ERROR, "private-token-fixture", "private-token-fixture"));
  }
  qDebug("private-token-fixture https://private.invalid/page Private tab");
  qInfo("private-token-fixture");
  qWarning("private-token-fixture https://private.invalid/page Private tab");
  qCritical("private-token-fixture /home/private-fixture/data");
  if (app.arguments().contains("crash")) std::raise(SIGSEGV);
  return 0;
}
