// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! ScreenSaver bus names and object paths probed by the shell, daemon, X11
//! adapter, and XFCE session helper (see `docs/architecture/x11-ewmh.md`).

/// Probe order, identical to the shell's guard.
pub const LOCK_ENDPOINTS: [(&str, &str, &str); 3] = [
    (
        "org.freedesktop.ScreenSaver",
        "/org/freedesktop/ScreenSaver",
        "org.freedesktop.ScreenSaver",
    ),
    (
        "org.freedesktop.ScreenSaver",
        "/ScreenSaver",
        "org.freedesktop.ScreenSaver",
    ),
    (
        "org.xfce.ScreenSaver",
        "/org/xfce/ScreenSaver",
        "org.xfce.ScreenSaver",
    ),
];
