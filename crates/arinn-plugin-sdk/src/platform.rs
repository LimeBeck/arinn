// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Client for the pre-stable daemon-owned platform-window contract.
use arinn_protocol::{
    platform::{self, MAX_SNAPSHOT_BYTES, MAX_SNAPSHOT_ENTRIES, PlatformWindowsProxy, Window},
    production::{self, ProductionOperationProxy},
    validation::ErrorCode,
};
use futures_util::StreamExt;
use std::{collections::HashSet, time::Duration};
use zbus::{Connection, names::OwnedUniqueName, zvariant::OwnedObjectPath};

/// Only a complete, validated snapshot is returned to the caller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowSnapshot {
    pub generation: String,
    pub revision: u64,
    pub windows: Vec<Window>,
}

/// Pinned to one unique daemon owner. Recreate after NameOwnerChanged; never
/// reuse snapshots, operations or tokens across daemon/platform generations.
pub struct WindowClient {
    connection: Connection,
    owner: OwnedUniqueName,
    proxy: PlatformWindowsProxy<'static>,
}

impl WindowClient {
    pub async fn new(connection: &Connection) -> production::Result<Self> {
        tokio::time::timeout(Duration::from_secs(3), async {
            let bus = zbus::fdo::DBusProxy::new(connection).await?;
            let owner = bus
                .get_name_owner(
                    production::SERVICE
                        .try_into()
                        .expect("constant service name"),
                )
                .await
                .map_err(|_| production::Error::from(ErrorCode::Unavailable))?;
            let proxy = PlatformWindowsProxy::builder(connection)
                .destination(owner.clone())?
                .build()
                .await?;
            Ok(Self {
                connection: connection.clone(),
                owner,
                proxy,
            })
        })
        .await
        .map_err(|_| production::Error::from(ErrorCode::Timeout))?
    }

    /// Install WindowsChanged before reading, and reconcile buffered revisions
    /// after this returns. The caller owns subscription/reload scheduling.
    pub fn proxy(&self) -> &PlatformWindowsProxy<'static> {
        &self.proxy
    }

    async fn ensure_owner(&self) -> production::Result<()> {
        let bus = zbus::fdo::DBusProxy::new(&self.connection).await?;
        let current = bus
            .get_name_owner(
                production::SERVICE
                    .try_into()
                    .expect("constant service name"),
            )
            .await
            .map_err(|_| production::Error::from(ErrorCode::Unavailable))?;
        if current != self.owner {
            return Err(ErrorCode::Unavailable.into());
        }
        Ok(())
    }

    /// No partial snapshot escapes on an invalid page, conflict or timeout.
    pub async fn snapshot(&self) -> production::Result<WindowSnapshot> {
        tokio::time::timeout(Duration::from_secs(3), self.read_snapshot())
            .await
            .map_err(|_| production::Error::from(ErrorCode::Timeout))?
    }

    async fn read_snapshot(&self) -> production::Result<WindowSnapshot> {
        self.ensure_owner().await?;
        let mut token = String::new();
        let mut tokens = HashSet::new();
        let mut result = WindowSnapshot {
            generation: String::new(),
            revision: 0,
            windows: Vec::new(),
        };
        let mut bytes = 0usize;
        loop {
            let (page,) = self
                .proxy
                .get_windows(&token)
                .await
                .map_err(production::Error::from)?;
            page.validate()?;
            if result.revision != 0
                && (result.generation != page.generation || result.revision != page.revision)
            {
                return Err(ErrorCode::Conflict.into());
            }
            bytes += page.encoded_len()?;
            if bytes > MAX_SNAPSHOT_BYTES
                || result.windows.len() + page.windows.len() > MAX_SNAPSHOT_ENTRIES
            {
                return Err(ErrorCode::InvalidArgument.into());
            }
            result.generation = page.generation;
            result.revision = page.revision;
            result.windows.extend(page.windows);
            token = page.next_token;
            if token.is_empty() {
                platform::validate_windows(&result.windows)?;
                self.ensure_owner().await?;
                return Ok(result);
            }
            if !tokens.insert(token.clone()) {
                return Err(ErrorCode::Conflict.into());
            }
        }
    }

    pub async fn activate(
        &self,
        generation: &str,
        token: &str,
        request_id: &str,
    ) -> production::Result<OwnedObjectPath> {
        platform::validate_command(generation, token, request_id)?;
        tokio::time::timeout(Duration::from_secs(3), async {
            self.ensure_owner().await?;
            let path = self
                .proxy
                .activate(generation, token, request_id)
                .await
                .map_err(production::Error::from)?;
            self.ensure_owner().await?;
            Ok(path)
        })
        .await
        .map_err(|_| production::Error::from(ErrorCode::Timeout))?
    }

    /// Requests graceful close only. Success means dispatch, not disappearance.
    pub async fn close(
        &self,
        generation: &str,
        token: &str,
        request_id: &str,
    ) -> production::Result<OwnedObjectPath> {
        platform::validate_command(generation, token, request_id)?;
        tokio::time::timeout(Duration::from_secs(3), async {
            self.ensure_owner().await?;
            let path = self
                .proxy
                .close(generation, token, request_id)
                .await
                .map_err(production::Error::from)?;
            self.ensure_owner().await?;
            Ok(path)
        })
        .await
        .map_err(|_| production::Error::from(ErrorCode::Timeout))?
    }

    /// Subscribe before Get: completion can precede the admission reply. This
    /// does not retry a command or infer completion from a timer.
    pub async fn wait_operation(&self, path: &OwnedObjectPath) -> production::Result<()> {
        tokio::time::timeout(Duration::from_secs(5), async {
            let bus = zbus::fdo::DBusProxy::new(&self.connection).await?;
            let mut owners = bus.receive_name_owner_changed().await?;
            self.ensure_owner().await?;
            let operation = ProductionOperationProxy::builder(&self.connection)
                .destination(self.owner.clone())?
                .path(path.clone())?
                .build()
                .await?;
            let mut changes = operation.receive_changed().await?;
            loop {
                let (info,) = operation.get().await.map_err(production::Error::from)?;
                self.ensure_owner().await?;
                match info.state.as_str() {
                    "succeeded" if info.error.is_empty() => return Ok(()),
                    "failed" | "cancelled" => return Err(operation_error(&info.error).into()),
                    "queued" | "running" if info.error.is_empty() => {}
                    _ => return Err(ErrorCode::InvalidArgument.into()),
                }
                loop {
                    tokio::select! {
                        event = changes.next() => {
                            if event.is_none() { return Err(ErrorCode::Unavailable.into()); }
                            break;
                        },
                        event = owners.next() => {
                            let Some(event) = event else { return Err(ErrorCode::Unavailable.into()); };
                            if event.args()?.name.as_str() == production::SERVICE {
                                return Err(ErrorCode::Unavailable.into());
                            }
                        },
                    }
                }
            }
        })
        .await
        .map_err(|_| production::Error::from(ErrorCode::Timeout))?
    }
}

fn operation_error(error: &str) -> ErrorCode {
    match error {
        "NotFound" => ErrorCode::NotFound,
        "Unavailable" => ErrorCode::Unavailable,
        "Unsupported" => ErrorCode::Unsupported,
        "PermissionDenied" => ErrorCode::PermissionDenied,
        "Timeout" => ErrorCode::Timeout,
        "InvalidArgument" => ErrorCode::InvalidArgument,
        "Conflict" => ErrorCode::Conflict,
        "Internal" => ErrorCode::Internal,
        "Cancelled" => ErrorCode::Cancelled,
        _ => ErrorCode::InvalidArgument,
    }
}
