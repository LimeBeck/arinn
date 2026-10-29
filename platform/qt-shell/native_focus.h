// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once
#include <QString>

// KWin reports the Wayland app ID as resourceClass. X11's WM_CLASS has a
// different identity scheme and cannot corroborate this shell this way.
struct NativeShellFocusIdentity {
  QString platform, applicationId;
  bool supported() const {
    return platform.startsWith(QLatin1String("wayland")) && !applicationId.isEmpty();
  }
  QString classify(bool connected, const QString &window, const QString &klass) const {
    if (!supported() || !connected || window.isEmpty() || klass.isEmpty())
      return QStringLiteral("unknown");
    return klass == applicationId ? QStringLiteral("shell") : QStringLiteral("other");
  }
};
