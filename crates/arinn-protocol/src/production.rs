// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

use crate::{application::ApplicationPage, validation::ErrorCode};
use serde::{Deserialize, Serialize};
use zbus::zvariant::{OwnedObjectPath, Type};
pub const SERVICE: &str = "org.arinn.Daemon1";
pub const PATH: &str = "/org/arinn/Daemon1";
pub const PROVIDER_PATH: &str = "/org/arinn/Provider1";

#[derive(Debug, zbus::DBusError)]
#[zbus(prefix = "org.arinn.Error1")]
pub enum Error {
    NotFound(String),
    Unavailable(String),
    Unsupported(String),
    PermissionDenied(String),
    Timeout(String),
    InvalidArgument(String),
    Conflict(String),
    Internal(String),
    Cancelled(String),
    #[zbus(error)]
    ZBus(zbus::Error),
}
impl From<ErrorCode> for Error {
    fn from(code: ErrorCode) -> Self {
        let message = code.to_string();
        match code {
            ErrorCode::NotFound => Self::NotFound(message),
            ErrorCode::Unavailable => Self::Unavailable(message),
            ErrorCode::Unsupported => Self::Unsupported(message),
            ErrorCode::PermissionDenied => Self::PermissionDenied(message),
            ErrorCode::Timeout => Self::Timeout(message),
            ErrorCode::InvalidArgument => Self::InvalidArgument(message),
            ErrorCode::Conflict => Self::Conflict(message),
            ErrorCode::Internal => Self::Internal(message),
            ErrorCode::Cancelled => Self::Cancelled(message),
        }
    }
}
pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct Registration {
    pub plugin_id: String,
    pub provider_id: String,
    pub api_major: u32,
    pub api_minor: u32,
    pub version: String,
    pub capabilities: Vec<String>,
    pub object_path: OwnedObjectPath,
    pub token: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct RegistrationReply {
    pub epoch: String,
    pub generation: String,
    pub api_minor: u32,
    pub max_body_bytes: u32,
    pub max_page_entries: u32,
    pub max_operations: u32,
}
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct ProviderInfo {
    pub id: String,
    pub plugin_id: String,
    pub source_path: String,
    pub name: String,
    pub version: String,
    pub path: OwnedObjectPath,
    pub capabilities: Vec<String>,
    pub runtime_state: String,
    pub backend_state: String,
    pub generation: String,
    pub revision: u64,
    pub last_error: String,
    pub error_at: u64,
    pub recovered_at: u64,
    pub restarts_since_daemon_start: u64,
    pub cumulative_restart_count: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct ProviderPage {
    pub snapshot_id: String,
    pub providers: Vec<ProviderInfo>,
    pub next_token: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct EntityRef {
    pub provider_id: String,
    pub capability: String,
    pub local_id: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct OperationInfo {
    pub state: String,
    pub error: String,
    pub retryable: bool,
    pub started_at: u64,
    pub ended_at: u64,
    pub results: Vec<EntityRef>,
    pub cancellable: bool,
}

#[zbus::proxy(
    interface = "org.arinn.Daemon1",
    default_service = "org.arinn.Daemon1",
    default_path = "/org/arinn/Daemon1"
)]
pub trait ProductionDaemon {
    fn home_items(&self) -> zbus::Result<Vec<(String, String, String, String, String)>>;

    fn status(&self) -> zbus::Result<(crate::Status,)>;
    /// This daemon's workspace version and build id (ADR-0097); see
    /// [`crate::build`]. An older daemon answers `UnknownMethod`.
    fn build_info(&self) -> zbus::Result<(String, String)>;
    fn home(&self) -> zbus::Result<()>;
    fn back(&self) -> zbus::Result<()>;
    /// Extra keys per semantic action from `config.toml` (ADR-0079), in
    /// `input::ACTIONS` order; invalid entries are left out.
    fn get_input_bindings(&self) -> zbus::Result<Vec<crate::input::Binding>>;

    /// Trusted platform input: present Home, even when already visible.
    fn report_home_request(&self, target: &str, output: &str) -> zbus::Result<()>;
    /// An explicit presentation intent, never a state refresh or replay.
    #[zbus(signal)]
    fn home_requested(&self, epoch: &str, target: &str, output: &str) -> zbus::Result<()>;

    #[zbus(name = "Providers")]
    fn provider_page(&self, token: &str) -> zbus::Result<(ProviderPage,)>;
    fn diagnostics(&self) -> zbus::Result<String>;

    fn register(&self, request: Registration) -> zbus::Result<(RegistrationReply,)>;
    fn reload(&self, request_id: &str) -> zbus::Result<OwnedObjectPath>;
    fn restart(&self, provider_id: &str, request_id: &str) -> zbus::Result<OwnedObjectPath>;
    #[zbus(signal)]
    fn state_changed(
        &self,
        provider_id: &str,
        capability: &str,
        generation: &str,
        revision: u64,
    ) -> zbus::Result<()>;
    #[zbus(signal)]
    fn providers_changed(&self) -> zbus::Result<()>;
}
pub mod provider {
    use super::*;
    #[zbus::proxy(
        interface = "org.arinn.Provider1",
        default_path = "/org/arinn/Provider1"
    )]
    pub trait ApplicationProvider {
        fn get_applications(&self, token: &str) -> zbus::Result<(ApplicationPage,)>;
        fn launch(&self, generation: &str, id: &str) -> zbus::Result<()>;
        /// Stop the unfinished launches of `id` (ADR-0110).
        fn cancel_launch(&self, generation: &str, id: &str) -> zbus::Result<()>;
        fn activate(&self, generation: &str, id: &str) -> zbus::Result<()>;
        fn terminate(&self, generation: &str, id: &str) -> zbus::Result<()>;
        fn backend_state(&self) -> zbus::Result<String>;
        #[zbus(signal, name = "StateChanged")]
        fn provider_state_changed(&self, generation: &str, revision: u64) -> zbus::Result<()>;
    }
}
pub use provider::ApplicationProviderProxy;
#[zbus::proxy(
    interface = "org.arinn.Application1",
    default_service = "org.arinn.Daemon1"
)]
pub trait ApplicationApi {
    fn get_applications(&self, token: &str) -> zbus::Result<(ApplicationPage,)>;
    fn launch(&self, generation: &str, id: &str, request_id: &str)
    -> zbus::Result<OwnedObjectPath>;
    fn activate(
        &self,
        generation: &str,
        id: &str,
        request_id: &str,
    ) -> zbus::Result<OwnedObjectPath>;
    fn terminate(
        &self,
        generation: &str,
        id: &str,
        request_id: &str,
    ) -> zbus::Result<OwnedObjectPath>;
}
#[zbus::proxy(
    interface = "org.arinn.Operation1",
    default_service = "org.arinn.Daemon1"
)]
pub trait ProductionOperation {
    fn get(&self) -> zbus::Result<(OperationInfo,)>;
    fn cancel(&self) -> zbus::Result<()>;
    #[zbus(signal)]
    fn changed(&self) -> zbus::Result<()>;
}

impl ProductionDaemonProxy<'_> {
    pub async fn providers(&self) -> zbus::Result<Vec<ProviderInfo>> {
        let mut token = String::new();
        let mut snapshot = String::new();
        let mut result = Vec::new();
        let mut tokens = std::collections::HashSet::new();
        loop {
            let (page,) = self.provider_page(&token).await?;
            if page.providers.len() > 200
                || (!snapshot.is_empty() && snapshot != page.snapshot_id)
                || result.len() + page.providers.len() > 2048
            {
                return Err(zbus::Error::Failure("Invalid provider page".into()));
            }
            snapshot = page.snapshot_id;
            token = page.next_token;
            result.extend(page.providers);
            if token.is_empty() {
                return Ok(result);
            }
            if !tokens.insert(token.clone()) {
                return Err(zbus::Error::Failure("Invalid provider page".into()));
            }
        }
    }
    pub async fn capabilities(&self) -> zbus::Result<Vec<(String, String)>> {
        Ok(self
            .providers()
            .await?
            .into_iter()
            .filter(|p| p.runtime_state == "ready")
            .flat_map(|p| {
                p.capabilities
                    .into_iter()
                    .map(move |cap| (p.id.clone(), cap))
            })
            .collect())
    }
}
