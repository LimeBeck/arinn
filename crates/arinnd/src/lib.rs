// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Production core components, independently testable before wire API migration.
pub mod browse;
pub mod favorites;
#[path = "../../../platform/kde/global_home.rs"]
pub mod global_home;
#[path = "../../../platform/key_sequence.rs"]
pub mod key_sequence;
mod listings;
pub mod operations;
#[path = "../../../platform/kde/bridge.rs"]
pub mod platform;
pub mod registry;
pub mod runtime;
#[path = "../../../platform/session_lock.rs"]
pub mod session_lock;
pub mod shortcuts;
#[path = "../../../platform/status_sources.rs"]
pub mod status_sources;
pub mod storage;
pub mod supervisor;

pub mod telemetry;
#[path = "../../../platform/kde/windows.rs"]
pub mod windows;
