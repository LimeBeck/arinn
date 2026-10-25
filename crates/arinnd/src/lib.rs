// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Production core components, independently testable before wire API migration.
pub mod browse;
pub mod favorites;
#[path = "../../../platform/kde/global_home.rs"]
pub mod global_home;
#[path = "../../../platform/key_sequence.rs"]
pub mod key_sequence;
#[path = "../../../platform/kde/keyboard_config.rs"]
pub mod keyboard_config;
mod listings;
pub mod operations;
#[path = "../../../platform/kde/bridge.rs"]
pub mod platform;
#[path = "../../../platform/power_inhibitor.rs"]
pub mod power_inhibitor;
#[path = "../../../platform/kde/power_key.rs"]
pub mod power_key;
pub mod registry;
pub mod runtime;
#[path = "../../../platform/session_lock.rs"]
pub mod session_lock;
#[path = "../../../platform/shell_ready.rs"]
pub mod shell_ready;
pub mod shortcuts;
#[path = "../../../platform/status_sources.rs"]
pub mod status_sources;
pub mod storage;
pub mod supervisor;
#[path = "../../../platform/system_control.rs"]
pub mod system_control;

pub mod telemetry;
#[path = "../../../platform/kde/windows.rs"]
pub mod windows;
