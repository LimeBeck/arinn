// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Session-bus MPRIS players as an Arinn `media` provider (ADR-0013, ADR-0054).
//! Player names are opaque ids. Titles and artwork URLs are not logged.
mod map;
mod watch;

pub use watch::serve;
