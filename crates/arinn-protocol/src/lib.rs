// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Typed protocol shared by the daemon, SDK and clients.
use serde::{Deserialize, Serialize};
use zbus::zvariant::Type;
pub mod application;
pub mod assets;
pub mod background;
pub mod browse;
pub mod browser;
pub mod download;
pub mod favorites;
pub mod folders;
pub mod input;
pub mod kwin;
pub mod library;
pub mod media;
pub mod media_association;
pub mod media_window;
pub mod platform;
pub mod production;
pub mod return_association;
pub mod settings;
pub mod validation;
pub mod web;

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct Status {
    pub epoch: String,
    pub provider_available: bool,
    pub generation: String,
    pub revision: u64,
    pub home_visible: bool,
    pub return_window: String,
    pub active_window: String,
    pub active_class: String,
    pub active_fullscreen: bool,
    pub platform_connected: bool,
    pub home_output: String,
    pub active_output: String,
}
pub mod web_icons;
