// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once
#include <QtCore/QtGlobal>
namespace ArinnTelemetry {
bool start();
void stop();
void finishStartup();
void breadcrumb(const char *operation);
void qtMessage(QtMsgType type, const QMessageLogContext &context);
}
