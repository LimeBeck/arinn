// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Chrome integration (spec §7.1, ADR-0011, ADR-0021, ADR-0048).
//!
//! `extension (MV3) ⇄ Native Messaging ⇄ arinn-chrome-host ⇄ D-Bus ⇄
//! arinn-plugin-chrome ⇄ arinnd`. The host is short-lived and started by the
//! browser; the provider is the systemd-supervised plugin. The host ↔ provider
//! link below is plugin-internal and never reaches the core. Tab titles and
//! URLs are never logged.
pub mod association;
pub mod bridge;
pub mod framing;
pub mod host;
pub mod icons;
pub mod messages;
pub mod processes;

/// Well-known bus name and path the provider exports for hosts.
pub const BRIDGE_NAME: &str = "org.arinn.ChromeBridge1";
pub const BRIDGE_PATH: &str = "/org/arinn/ChromeBridge1";
/// Path each host exports for commands from the provider.
pub const HOST_PATH: &str = "/org/arinn/ChromeHost1";
/// Native Messaging host name, as in the browser's host manifest.
pub const NATIVE_HOST: &str = "org.arinn.chrome_host";
/// The packaged extension's fixed ID (derived from the `key` in its manifest).
pub const EXTENSION_ID: &str = "eajlncomibfmpmkhkbcdmajmkpfaaibf";

/// Window and tab records on the host ↔ provider link: browser IDs are the
/// extension API's integers.
pub type WireWindow = (i64, bool);
pub type WireTab = (i64, i64, String, String, bool, bool);
