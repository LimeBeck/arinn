// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `org.arinn.BrowserProcess1` (ADR-0096): the browser process of a window,
//! for the registered daemon only. Never logged.
use crate::bridge::Bridge;
use arinn_plugin_sdk::runtime::RegistrationContext;
use arinn_protocol::{media_association, production, validation::ErrorCode};
use tokio::sync::watch;

pub struct Processes {
    bridge: Bridge,
    registrations: watch::Receiver<Option<RegistrationContext>>,
}
impl Processes {
    pub fn new(
        bridge: Bridge,
        registrations: watch::Receiver<Option<RegistrationContext>>,
    ) -> Self {
        Self {
            bridge,
            registrations,
        }
    }
    /// The caller first, then the arguments: anyone but the daemon gets
    /// `PermissionDenied` whatever it sends.
    fn check(&self, sender: &str, generation: &str, id: &str) -> Result<(), ErrorCode> {
        let current = self.registrations.borrow();
        // Unregistered, nobody is the daemon: refuse every caller alike, so
        // the error does not reveal the registration state.
        let current = current.as_ref().ok_or(ErrorCode::PermissionDenied)?;
        if current.daemon_owner != sender {
            return Err(ErrorCode::PermissionDenied);
        }
        media_association::validate_process_query(generation, id)?;
        if current.generation != generation {
            return Err(ErrorCode::Conflict);
        }
        Ok(())
    }
}
#[zbus::interface(name = "org.arinn.BrowserProcess1")]
impl Processes {
    async fn get_process_id(
        &self,
        generation: &str,
        window_id: &str,
        #[zbus(header)] header: zbus::message::Header<'_>,
    ) -> production::Result<u32> {
        let sender = header.sender().ok_or(ErrorCode::PermissionDenied)?.as_str();
        self.check(sender, generation, window_id)?;
        Ok(self.bridge.browser_process(window_id)?)
    }
}
