// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once
#include <QtCore/QtGlobal>
#include <QByteArray>
#include <QString>
#include <optional>
namespace ArinnTelemetry {
QByteArray resolveDsn(const std::optional<QByteArray> &environment, const std::optional<QByteArray> &configuration);
bool start();
void stop();
void finishStartup();
void breadcrumb(const char *operation);
void qtMessage(QtMsgType type, const QMessageLogContext &context, const QString &message);

// A Qt or QML message reduced to a fixed, non-personal template (ADR-0092).
struct QtTemplate {
  QByteArray location; // Arinn source file and line, for example "HomeView.qml:42".
  QByteArray category; // Qt logging category, for example "qt.qpa.wayland".
  QByteArray text;     // Message without URLs, paths, file names, numbers or quoted text.
  bool benign = false; // Known noise: counted, never sent as an event.
};
QtTemplate qtTemplate(const QMessageLogContext &context, const QString &message);
// Crash-safe checks shared by the SDK callback and the transport filter.
bool safeTemplateText(const char *text);
bool safeLocation(const char *text);
bool safeCategory(const char *text);
// Bounded per-template limiter: true when this template may be reported now.
bool admitTemplate(const QByteArray &key, qint64 nowMs);
}
