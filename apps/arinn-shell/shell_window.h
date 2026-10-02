// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once
#include <QQuickWindow>

// Shell windows turn transparent over managed playback (ADR-0046). A window
// created without an alpha channel stays opaque on Wayland and hides the
// video (#236), so the shell requests one before any window exists.
inline void configureShellWindows() { QQuickWindow::setDefaultAlphaBuffer(true); }
