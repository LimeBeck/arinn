// SPDX-FileCopyrightText: 2026 Arinn contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Experimental Phase 0 contract, deliberately separate from stable Plugin API v1.
use serde::{Deserialize, Serialize};
use zbus::zvariant::Type;
pub const SERVICE: &str = "org.arinn.Spike1";
pub const PATH: &str = "/org/arinn/Spike1";
pub const PROVIDER_PATH: &str = "/org/arinn/Provider1";

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct Snapshot {
    pub generation: String,
    pub revision: u64,
    pub available: bool,
    pub items: Vec<(String, String)>,
}
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
#[zbus::proxy(
    interface = "org.arinn.Spike1",
    default_service = "org.arinn.Spike1",
    default_path = "/org/arinn/Spike1"
)]
pub trait Daemon {
    fn status(&self) -> zbus::Result<(Status,)>;
    fn snapshot(&self) -> zbus::Result<(Snapshot,)>;
    fn register(&self, token: &str, api_version: u32) -> zbus::Result<String>;
    fn publish(
        &self,
        generation: &str,
        revision: u64,
        items: Vec<(String, String)>,
    ) -> zbus::Result<()>;
    fn activate(
        &self,
        generation: &str,
        id: &str,
        request_id: &str,
    ) -> zbus::Result<zbus::zvariant::OwnedObjectPath>;
    fn home(&self) -> zbus::Result<()>;
    fn back(&self) -> zbus::Result<()>;
    #[zbus(signal)]
    fn changed(&self) -> zbus::Result<()>;
}
#[zbus::proxy(
    interface = "org.arinn.SpikeProvider1",
    default_path = "/org/arinn/Provider1"
)]
pub trait Provider {
    fn activate(&self, id: &str) -> zbus::Result<()>;
}
#[zbus::proxy(interface = "org.arinn.SpikeOperation1")]
pub trait Operation {
    #[zbus(property)]
    fn state(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn error(&self) -> zbus::Result<String>;
}
