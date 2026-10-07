// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once
#include <QQuickWindow>

// Shell windows turn transparent over managed playback (ADR-0046). A window
// created without an alpha channel stays opaque on Wayland and hides the
// video (#236), so the shell requests one before any window exists.
inline void configureShellWindows() { QQuickWindow::setDefaultAlphaBuffer(true); }

// The EWMH adapter finds a hidden shell by the desktop identity on its native
// window (#283). Qt creates that window only on the first show, so a shell
// that has never been shown has none and the first Home cannot reach it
// (#379). Create the native window at startup; it stays unmapped until Home.
inline void createHiddenShellWindow(QWindow *window) { window->create(); }
