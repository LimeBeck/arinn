// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once
#include <QtCore/QtGlobal>
#include <QByteArray>
#include <optional>
namespace ArinnTelemetry {
QByteArray resolveDsn(const std::optional<QByteArray> &environment, const std::optional<QByteArray> &configuration);
bool start();
void stop();
void finishStartup();
void breadcrumb(const char *operation);
void qtMessage(QtMsgType type, const QMessageLogContext &context);
}
