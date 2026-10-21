// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `org.arinn.SystemControl1` (ADR-0108): power and volume actions for the
//! shell. The work is done by `crate::system_control` in the platform layer;
//! this module checks the caller, serializes the actions and logs outcomes.
use super::{Core, sender};
use crate::system_control::Control;
use arinn_protocol::{
    production,
    system_control::{self, PowerAction, PowerState, Volume},
    validation::ErrorCode,
};
use std::{sync::Arc, time::Instant};
use tokio::sync::Mutex;
use zbus::message::Header;

#[derive(Clone)]
pub(super) struct ControlExport {
    core: Core,
    control: Arc<Control>,
    /// One power request at a time.
    power: Arc<Mutex<()>>,
    /// Volume changes apply in the order they arrive.
    volume: Arc<Mutex<()>>,
}
impl ControlExport {
    /// Plugins never execute system actions (§13): a connection that owns a
    /// registered provider is refused.
    async fn caller(&self, h: &Header<'_>) -> Result<(), ErrorCode> {
        let name = sender(h)?;
        if self.core.is_provider_owner(&name).await {
            Err(ErrorCode::PermissionDenied)
        } else {
            Ok(())
        }
    }

    async fn act(&self, h: &Header<'_>, action: PowerAction) -> production::Result<()> {
        self.caller(h).await?;
        let _one = self.power.lock().await;
        let started = Instant::now();
        let result = self.control.act(action).await;
        log(action.as_str(), result.err(), started);
        Ok(result?)
    }

    async fn change(
        &self,
        h: &Header<'_>,
        operation: &str,
        change: impl std::future::Future<Output = Result<Volume, ErrorCode>>,
    ) -> production::Result<(Volume,)> {
        self.caller(h).await?;
        let _ordered = self.volume.lock().await;
        let started = Instant::now();
        let result = change.await;
        log(operation, result.as_ref().err().copied(), started);
        let volume = result?;
        volume.validate().map_err(|_| ErrorCode::Internal)?;
        Ok((volume,))
    }
}

/// Outcome only: never an inhibitor's text, a device or a level (§13).
fn log(operation: &str, error: Option<ErrorCode>, started: Instant) {
    eprintln!(
        "{}",
        serde_json::json!({
            "level": if error.is_some() { "warn" } else { "info" },
            "component": "arinnd",
            "provider_id": null,
            "request_id": null,
            "operation": format!("system-{operation}"),
            "error": error.map(|e| e.to_string()),
            "duration_ms": started.elapsed().as_millis() as u64,
        })
    );
}

#[zbus::interface(name = "org.arinn.SystemControl1")]
impl ControlExport {
    async fn get_power(&self, #[zbus(header)] h: Header<'_>) -> production::Result<(PowerState,)> {
        self.caller(&h).await?;
        let state = self.control.power().await;
        state.validate().map_err(|_| ErrorCode::Internal)?;
        Ok((state,))
    }
    async fn suspend(&self, #[zbus(header)] h: Header<'_>) -> production::Result<()> {
        self.act(&h, PowerAction::Suspend).await
    }
    async fn reboot(&self, #[zbus(header)] h: Header<'_>) -> production::Result<()> {
        self.act(&h, PowerAction::Reboot).await
    }
    async fn power_off(&self, #[zbus(header)] h: Header<'_>) -> production::Result<()> {
        self.act(&h, PowerAction::PowerOff).await
    }
    async fn get_volume(&self, #[zbus(header)] h: Header<'_>) -> production::Result<(Volume,)> {
        self.caller(&h).await?;
        let volume = self.control.volume().await;
        volume.validate().map_err(|_| ErrorCode::Internal)?;
        Ok((volume,))
    }
    async fn set_muted(
        &self,
        #[zbus(header)] h: Header<'_>,
        muted: bool,
    ) -> production::Result<(Volume,)> {
        let control = self.control.clone();
        self.change(
            &h,
            "set-muted",
            async move { control.set_muted(muted).await },
        )
        .await
    }
    async fn toggle_muted(&self, #[zbus(header)] h: Header<'_>) -> production::Result<(Volume,)> {
        let control = self.control.clone();
        self.change(
            &h,
            "toggle-muted",
            async move { control.toggle_muted().await },
        )
        .await
    }
    async fn step_volume(
        &self,
        #[zbus(header)] h: Header<'_>,
        steps: i32,
    ) -> production::Result<(Volume,)> {
        let steps = system_control::steps(steps)?;
        let control = self.control.clone();
        self.change(&h, "step-volume", async move {
            control.step_volume(steps).await
        })
        .await
    }
}

impl Core {
    /// Export `SystemControl1` at the daemon root.
    pub async fn export_system_control(&self, control: Control) -> Result<(), ErrorCode> {
        let export = ControlExport {
            core: self.clone(),
            control: Arc::new(control),
            power: Arc::default(),
            volume: Arc::default(),
        };
        self.connection
            .object_server()
            .at(production::PATH, export)
            .await
            .map_err(|_| ErrorCode::Internal)?;
        Ok(())
    }
}

#[cfg(test)]
#[path = "system_control_tests.rs"]
mod tests;
