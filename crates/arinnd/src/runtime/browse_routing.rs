// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `org.arinn.Browse1` (ADR-0066): typed addresses and searches, recent
//! entries and the packaged search engines. Typed text, URLs and queries are
//! never logged (spec §11, §13).
use super::*;
use crate::browse::{Engine, selected};
use arinn_protocol::browse::{KIND_INVALID, Target, resolve_input};

impl Core {
    /// The packaged search engines (`crate::browse::load_engines`).
    pub fn with_search_engines(mut self, engines: Vec<Engine>) -> Self {
        self.search_engines = Arc::new(engines);
        self
    }
    pub async fn export_browse(&self) -> Result<(), ErrorCode> {
        self.connection
            .object_server()
            .at(production::PATH, BrowseEndpoint { core: self.clone() })
            .await
            .map(|_| ())
            .map_err(|_| ErrorCode::Internal)
    }

    fn resolve_text(&self, text: &str) -> Target {
        let configured = Config::load(&self.paths.config)
            .map(|c| c.search_engine)
            .unwrap_or_default();
        match selected(&self.search_engines, &configured) {
            Some(engine) => resolve_input(text, &engine.template, &engine.title),
            None => resolve_input(text, "", ""),
        }
    }

    async fn recent_store<T: Send + 'static>(
        &self,
        work: impl FnOnce(&mut Store) -> Result<T, ErrorCode> + Send + 'static,
    ) -> Result<T, ErrorCode> {
        let state = self.paths.state.clone();
        tokio::task::spawn_blocking(move || work(&mut Store::open(&state)?))
            .await
            .map_err(|_| ErrorCode::Internal)?
    }

    async fn recent_changed(&self) {
        if let Ok(emitter) = SignalEmitter::new(&self.connection, production::PATH) {
            let _ = BrowseEndpoint::recent_changed(&emitter).await;
        }
    }

    async fn open_text(
        &self,
        text: &str,
        request_id: &str,
        caller: &str,
    ) -> Result<OwnedObjectPath, ErrorCode> {
        let target = self.resolve_text(text);
        if target.kind == KIND_INVALID {
            return Err(ErrorCode::InvalidArgument);
        }
        let fingerprint = format!("browse-open\0{}", target.url);
        let (path, operation, fresh) = self
            .operations
            .admit(
                &self.connection,
                caller,
                request_id,
                &fingerprint,
                "",
                Label::new("browse.open"),
            )
            .await?;
        if !fresh {
            return Ok(path);
        }
        let entry = text.trim().to_owned();
        if self
            .recent_store(move |store| store.push_browse_recent(&entry))
            .await
            .is_ok()
        {
            self.recent_changed().await;
        }
        let core = self.clone();
        let object = path.clone();
        tokio::spawn(async move {
            {
                let mut value = operation.value.lock().await;
                if value.state != "queued" {
                    return;
                }
                value.state = "running".into();
                value.started_at = now();
            }
            let result = core.open_url(&target.url).await;
            operation
                .finish(&core.connection, &object, result.map(|_| Vec::new()))
                .await;
        });
        Ok(path)
    }
}

pub(super) struct BrowseEndpoint {
    core: Core,
}
#[zbus::interface(name = "org.arinn.Browse1")]
impl BrowseEndpoint {
    async fn resolve(&self, text: &str) -> (Target,) {
        (self.core.resolve_text(text),)
    }
    async fn open(
        &self,
        text: &str,
        request_id: &str,
        #[zbus(header)] h: Header<'_>,
    ) -> production::Result<OwnedObjectPath> {
        Ok(self.core.open_text(text, request_id, &sender(&h)?).await?)
    }
    async fn get_recent(&self) -> production::Result<Vec<String>> {
        Ok(self
            .core
            .recent_store(|store| store.browse_recent())
            .await?)
    }
    async fn clear_recent(&self) -> production::Result<()> {
        self.core
            .recent_store(|store| store.clear_browse_recent())
            .await?;
        self.core.recent_changed().await;
        Ok(())
    }
    #[zbus(signal)]
    async fn recent_changed(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;
}

#[cfg(test)]
#[path = "browse_tests.rs"]
mod tests;
