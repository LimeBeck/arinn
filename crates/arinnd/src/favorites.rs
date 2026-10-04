// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Durable Favorites1 export. Only the daemon owns the saved ordered selection.
use crate::{runtime::Core, storage::Store};
use arinn_protocol::{
    favorites::FavoriteList,
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
