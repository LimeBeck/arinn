// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Raise the exact window selected by an OpenUrl command (ADR-0119).
use crate::bridge::Bridge;
use arinn_plugin_sdk::{platform::WindowClient, runtime::RegistrationContext};
use arinn_protocol::{browser::BrowserWindow, production, validation::ErrorCode};
use std::{
    sync::{Mutex, OnceLock},
    time::Duration,
};
use tokio::sync::watch;

struct Links {
    registration: RegistrationContext,
    platform_generation: String,
    windows: Vec<BrowserWindow>,
}

#[derive(Default)]
pub struct OpenWindows {
    registrations: OnceLock<watch::Receiver<Option<RegistrationContext>>>,
    links: Mutex<Option<Links>>,
}

fn same_registration(a: &RegistrationContext, b: &RegistrationContext) -> bool {
    a.daemon_owner == b.daemon_owner
        && a.daemon_epoch == b.daemon_epoch
        && a.generation == b.generation
}

impl OpenWindows {
    /// Bound once after constructing the SDK service, before registration.
    pub fn bind(&self, registrations: watch::Receiver<Option<RegistrationContext>>) {
        assert!(self.registrations.set(registrations).is_ok());
    }

    fn registration(&self) -> Result<RegistrationContext, ErrorCode> {
        self.registrations
            .get()
            .filter(|r| r.has_changed().is_ok())
            .and_then(|r| r.borrow().clone())
            .ok_or(ErrorCode::Unavailable)
    }

    fn check(&self, registration: &RegistrationContext) -> Result<(), ErrorCode> {
        if same_registration(&self.registration()?, registration) {
            Ok(())
        } else {
            Err(ErrorCode::Unavailable)
        }
    }

    /// Called with the same validated links published in BrowserProvider1.
    /// No link survives a lost registration or platform snapshot.
    pub fn publish(
        &self,
        registration: Option<&RegistrationContext>,
        generation: &str,
        windows: &[BrowserWindow],
    ) {
        *self.links.lock().unwrap() =
            registration
                .filter(|_| !generation.is_empty())
                .map(|r| Links {
                    registration: r.clone(),
                    platform_generation: generation.into(),
                    windows: windows.to_vec(),
                });
    }

    fn linked(
        &self,
        registration: &RegistrationContext,
        id: &str,
    ) -> Result<(String, String), ErrorCode> {
        let links = self.links.lock().unwrap();
        let links = links
            .as_ref()
            .filter(|l| same_registration(&l.registration, registration))
            .ok_or(ErrorCode::Unavailable)?;
        let token = links
            .windows
            .iter()
            .find(|w| w.id == id)
            .map(|w| w.window_token.clone())
            .filter(|t| !t.is_empty())
            .ok_or(ErrorCode::Unavailable)?;
        Ok((links.platform_generation.clone(), token))
    }

    pub async fn open(
        &self,
        connection: &zbus::Connection,
        bridge: &Bridge,
        url: &str,
    ) -> Result<(), ErrorCode> {
        // Leave reply time inside the daemon's ten-second OpenUrl budget.
        // Cancellation never replays a command already sent to the platform.
        tokio::time::timeout(
            Duration::from_secs(8),
            self.open_selected(connection, bridge, url),
        )
        .await
        .map_err(|_| ErrorCode::Timeout)?
    }

    async fn open_selected(
        &self,
        connection: &zbus::Connection,
        bridge: &Bridge,
        url: &str,
    ) -> Result<(), ErrorCode> {
        let registration = self.registration()?;
        let (id, created) = bridge.open(connection, url).await?;
        self.check(&registration)?;
        // The browser raises a newly mapped window; it has no settled link yet.
        if created {
            return Ok(());
        }
        let (generation, token) = self.linked(&registration, &id)?;
        let client = WindowClient::new(connection).await.map_err(code)?;
        if client.proxy().inner().destination().as_str() != registration.daemon_owner {
            return Err(ErrorCode::Unavailable);
        }
        // Re-read the inventory so a remap cannot reuse an old association.
        if client.snapshot().await.map_err(code)?.generation != generation {
            return Err(ErrorCode::Conflict);
        }
        self.check(&registration)?;
        if self.linked(&registration, &id)? != (generation.clone(), token.clone()) {
            return Err(ErrorCode::Conflict);
        }
        let path = client
            .activate(&generation, &token, &uuid::Uuid::new_v4().to_string())
            .await
            .map_err(code)?;
        client.wait_operation(&path).await.map_err(code)?;
        self.check(&registration)
    }
}

fn code(error: production::Error) -> ErrorCode {
    match error {
        production::Error::NotFound(_) => ErrorCode::NotFound,
        production::Error::Conflict(_) => ErrorCode::Conflict,
        production::Error::PermissionDenied(_) => ErrorCode::PermissionDenied,
        production::Error::Timeout(_) => ErrorCode::Timeout,
        production::Error::InvalidArgument(_) => ErrorCode::InvalidArgument,
        production::Error::Unsupported(_) => ErrorCode::Unsupported,
        _ => ErrorCode::Unavailable,
    }
}
