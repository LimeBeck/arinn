// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `org.arinn.SystemStatus1` (ADR-0100): mute and network state for Home's
//! status area. The values come from `crate::status_sources` in the platform
//! layer; this module only publishes them.
use super::Core;
use crate::status_sources::{NetworkState, Sources, Update};
use arinn_protocol::{
    production,
    system_status::{Audio, SystemStatus},
    validation::ErrorCode,
};
use std::sync::Arc;
use tokio::sync::{Mutex, mpsc};
use zbus::object_server::SignalEmitter;

struct Published {
    revision: u64,
    audio: Audio,
    network: NetworkState,
    /// The revision cannot grow any more: everything stays unknown.
    exhausted: bool,
}

#[derive(Clone)]
pub(super) struct StatusExport {
    epoch: String,
    published: Arc<Mutex<Published>>,
}
impl StatusExport {
    fn new(epoch: &str) -> Self {
        Self {
            epoch: epoch.into(),
            published: Arc::new(Mutex::new(Published {
                revision: 1,
                audio: Audio::Unknown,
                network: NetworkState::UNKNOWN,
                exhausted: false,
            })),
        }
    }
    /// Applies one update; returns the new revision when the content changed.
    async fn apply(&self, update: Update) -> Option<u64> {
        let mut published = self.published.lock().await;
        let (audio, network) = match update {
            Update::Audio(audio) => (audio, published.network),
            Update::Network(network) => (published.audio, network),
        };
        if published.exhausted || (audio, network) == (published.audio, published.network) {
            return None;
        }
        match published.revision.checked_add(1) {
            Some(revision) => {
                published.revision = revision;
                published.audio = audio;
                published.network = network;
                Some(revision)
            }
            None => {
                published.exhausted = true;
                published.audio = Audio::Unknown;
                published.network = NetworkState::UNKNOWN;
                None
            }
        }
    }
}
#[zbus::interface(name = "org.arinn.SystemStatus1")]
impl StatusExport {
    async fn get(&self) -> production::Result<(SystemStatus,)> {
        let published = self.published.lock().await;
        let network = published.network;
        let snapshot = SystemStatus {
            epoch: self.epoch.clone(),
            revision: published.revision,
            audio: published.audio.as_str().into(),
            network: network.network.as_str().into(),
            link: network.link.as_str().into(),
            signal: network.signal,
        };
        snapshot.validate().map_err(|_| ErrorCode::Internal)?;
        Ok((snapshot,))
    }
    #[zbus(signal)]
    async fn changed(emitter: &SignalEmitter<'_>, epoch: &str, revision: u64) -> zbus::Result<()>;
}

impl Core {
    /// Export `SystemStatus1` at the daemon root and keep it current from
    /// `sources`.
    pub async fn export_system_status(&self, sources: Sources) -> Result<(), ErrorCode> {
        let export = StatusExport::new(&self.epoch);
        self.connection
            .object_server()
            .at(production::PATH, export.clone())
            .await
            .map_err(|_| ErrorCode::Internal)?;
        let (sender, mut updates) = mpsc::channel(16);
        tokio::spawn(sources.run(sender));
        let core = self.clone();
        tokio::spawn(async move {
            while let Some(update) = updates.recv().await {
                if let Some(revision) = export.apply(update).await {
                    core.status_changed(revision).await;
                }
            }
        });
        Ok(())
    }

    async fn status_changed(&self, revision: u64) {
        let Ok(iface) = self
            .connection
            .object_server()
            .interface::<_, StatusExport>(production::PATH)
            .await
        else {
            return;
        };
        let _ = StatusExport::changed(iface.signal_emitter(), &self.epoch, revision).await;
    }
}

#[cfg(test)]
#[path = "system_status_tests.rs"]
mod tests;
