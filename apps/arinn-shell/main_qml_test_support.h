// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

#pragma once

#include "interface_sounds.h"
#include <QQmlContext>
#include <QQmlEngine>

// Context properties Main.qml expects alongside installAssetImageProvider().
inline void installMainQmlContextProperties(QQmlEngine *engine, QObject *backend,
                                            QObject *sessionGuard,
                                            InterfaceSounds *interfaceSounds) {
  auto *ctx = engine->rootContext();
  ctx->setContextProperty(QStringLiteral("backend"), backend);
  ctx->setContextProperty(QStringLiteral("sessionGuard"), sessionGuard);
  ctx->setContextProperty(QStringLiteral("interfaceSounds"), interfaceSounds);
}
