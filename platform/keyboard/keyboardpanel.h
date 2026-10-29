// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once
#include <QString>

class QWindow;

// The title KWin's window rule and the KWin script recognise the panel by
// (ADR-0105); keep it equal to PANEL_TITLE in platform/kde/keyboard_config.rs.
inline const QString keyboardPanelTitle = QStringLiteral("org.arinn.KeyboardPanel");

// Prepares the on-screen keyboard's panel window before it is first shown,
// so that it never takes keyboard focus from the application under it:
//
// - X11 (EWMH and KDE/X11): override-redirect and WM_HINTS input = False.
//   The window manager never focuses, lists or stacks it; it stays above.
// - Wayland (KDE): a frameless window that does not accept focus, titled
//   for the KWin rule the daemon keeps (acceptfocus forced off, above, no
//   border, out of the taskbar and switcher). The KWin script places it.
// - Other platforms (offscreen tests): the same flags as Wayland.
void prepareKeyboardPanel(QWindow *window);
