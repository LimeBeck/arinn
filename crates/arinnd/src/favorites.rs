// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Durable Favorites1 export. Only the daemon owns the saved ordered selection.
use crate::{runtime::Core, storage::Store};
use arinn_protocol::{
    favorites::FavoriteList,
    music_pins::{MusicPin, MusicPinList},
    production::{self, EntityRef, PATH},
    validation::ErrorCode,
};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::sync::Mutex;
use zbus::{Connection, object_server::SignalEmitter};

#[derive(Clone)]
pub struct Favorites {
    state: PathBuf,
    lock: Arc<Mutex<()>>,
    core: Option<Core>,
}
impl Favorites {
    pub fn new(state: &Path) -> Self {
        Self {
            state: state.into(),
            lock: Arc::default(),
            core: None,
        }
    }
    pub fn with_core(mut self, core: Core) -> Self {
        self.core = Some(core);
        self
    }
    async fn store<T: Send + 'static>(
        &self,
        work: impl FnOnce(&mut Store) -> Result<T, ErrorCode> + Send + 'static,
    ) -> Result<T, ErrorCode> {
        let state = self.state.clone();
        tokio::task::spawn_blocking(move || work(&mut Store::open(&state)?))
            .await
            .map_err(|_| ErrorCode::Internal)?
    }
}
#[zbus::interface(name = "org.arinn.Favorites1")]
impl Favorites {
    async fn list(&self) -> production::Result<(FavoriteList,)> {
        let _guard = self.lock.lock().await;
        Ok((self.store(|store| store.favorite_list()).await?,))
    }
    async fn set(
        &self,
        revision: u64,
        entries: Vec<EntityRef>,
        #[zbus(connection)] connection: &Connection,
    ) -> production::Result<u64> {
        let _guard = self.lock.lock().await;
        FavoriteList {
            revision,
            entries: entries.clone(),
        }
        .validate()?;
        let current = self.store(|store| store.favorite_list()).await?;
        if current.revision != revision {
            return Err(ErrorCode::Conflict.into());
        }
        let added: Vec<_> = entries
            .iter()
            .filter(|e| !current.entries.contains(e) && e.capability == "application")
            .cloned()
            .collect();
        if !added.is_empty() {
            self.core
                .as_ref()
                .ok_or(ErrorCode::Unavailable)?
                .admit_favorite_applications(&added)
                .await?;
        }
        let (revision, changed) = self
            .store(move |store| store.replace_favorites(revision, &entries))
            .await?;
        // Keep the serializer lock until post-commit publication; observers cannot
        // see a notification for an uncommitted or older write out of order.
        if changed && let Ok(emitter) = SignalEmitter::new(connection, PATH) {
            let _ = Self::changed(&emitter, revision).await;
        }
        Ok(revision)
    }
    #[zbus(signal)]
    async fn changed(emitter: &SignalEmitter<'_>, revision: u64) -> zbus::Result<()>;
}

/// `org.arinn.MusicPins1` (ADR-0143): pinned music stations, written with the
/// favorites they belong to. It shares the favorites' serializer and
/// revision, and announces a change through `Favorites1.Changed`.
#[derive(Clone)]
pub struct MusicPins(Favorites);
impl Favorites {
    pub fn music_pins(&self) -> MusicPins {
        MusicPins(self.clone())
    }
}
impl MusicPins {
    async fn announce(connection: &Connection, revision: u64) {
        if let Ok(emitter) = SignalEmitter::new(connection, PATH) {
            let _ = Favorites::changed(&emitter, revision).await;
        }
    }
}
#[zbus::interface(name = "org.arinn.MusicPins1")]
impl MusicPins {
    async fn list(&self) -> production::Result<(MusicPinList,)> {
        let _guard = self.0.lock.lock().await;
        Ok((self.0.store(|store| store.music_pins()).await?,))
    }
    async fn pin(
        &self,
        revision: u64,
        pin: MusicPin,
        #[zbus(connection)] connection: &Connection,
    ) -> production::Result<u64> {
        let _guard = self.0.lock.lock().await;
        pin.validate()?;
        let (revision, changed) = self
            .0
            .store(move |store| store.pin_music_station(revision, &pin))
            .await?;
        // Post-commit, under the serializer, as Favorites1.Set does.
        if changed {
            Self::announce(connection, revision).await;
        }
        Ok(revision)
    }
    async fn unpin(
        &self,
        revision: u64,
        provider_id: String,
        station_id: String,
        #[zbus(connection)] connection: &Connection,
    ) -> production::Result<u64> {
        let _guard = self.0.lock.lock().await;
        let revision = self
            .0
            .store(move |store| store.unpin_music_station(revision, &provider_id, &station_id))
            .await?;
        Self::announce(connection, revision).await;
        Ok(revision)
    }
}
