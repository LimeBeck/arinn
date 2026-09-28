// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Production core components, independently testable before wire API migration.
mod listings;
pub mod operations;
#[path = "../../../platform/kde/bridge.rs"]
pub mod platform;
pub mod registry;
pub mod runtime;
pub mod storage;
pub mod supervisor;

#[path = "../../../platform/kde/windows.rs"]
pub mod windows;
