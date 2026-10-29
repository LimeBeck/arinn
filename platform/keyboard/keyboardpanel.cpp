// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "keyboardpanel.h"
#include <QGuiApplication>
#include <QWindow>

void prepareKeyboardPanel(QWindow *window) {
  if (!window)
    return;
  Qt::WindowFlags flags = Qt::Window | Qt::FramelessWindowHint |
                          Qt::WindowDoesNotAcceptFocus |
                          Qt::WindowStaysOnTopHint;
  if (QGuiApplication::platformName() == QLatin1String("xcb"))
    flags |= Qt::X11BypassWindowManagerHint;
  window->setFlags(flags);
  window->setTitle(keyboardPanelTitle);
}
