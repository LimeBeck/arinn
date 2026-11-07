// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Optional fixed text metadata, qualified by installation and publication.
//! No client language, Qt parser, payload logging or provider state mutation.
use super::*;
use arinn_protocol::localization::{self, Catalog, ProviderTextsProxy, Scope, TextRequest};
use sha2::{Digest, Sha256};
use std::collections::VecDeque;
use tokio::sync::OnceCell;

const DEADLINE: Duration = Duration::from_secs(3);
const MAX_PROVIDERS: usize = 64;
const MAX_PAGE_FIELDS: usize = 512;
const MAX_PAGE_TEXT_BYTES: usize = 256 * 1024;

#[derive(Clone, PartialEq, Eq)]
struct Target {
    id: String,
    plugin: String,
    owner: String,
    generation: String,
}
#[derive(Clone)]
struct Source {
    catalog: Catalog,
    json: String,
}
#[derive(Default)]
struct PageFields {
    revision: u64,
    fields: BTreeMap<String, Option<String>>,
    order: VecDeque<String>,
    bytes: usize,
}
impl PageFields {
    fn observe(&mut self, page: &arinn_protocol::music::Page) {
        if page.revision < self.revision {
            return;
        }
        if page.revision > self.revision {
            *self = Self {
                revision: page.revision,
                ..Self::default()
            };
        }
        for entity in &page.entries {
            if let Some(previous) = self.fields.get_mut(&entity.id) {
                if previous.as_deref().is_some_and(|text| text != entity.title) {
                    self.bytes -= previous.take().map_or(0, |text| text.len());
                }
                continue;
            }
            let bytes = entity.id.len() + entity.title.len();
            while self.fields.len() >= MAX_PAGE_FIELDS || self.bytes + bytes > MAX_PAGE_TEXT_BYTES {
                let Some(old) = self.order.pop_front() else {
                    break;
                };
                if let Some(text) = self.fields.remove(&old) {
                    self.bytes -= old.len() + text.map_or(0, |text| text.len());
                }
            }
            if bytes <= MAX_PAGE_TEXT_BYTES {
                self.bytes += bytes;
                self.order.push_back(entity.id.clone());
                self.fields
                    .insert(entity.id.clone(), Some(entity.title.clone()));
            }
        }
    }
    fn matches(&self, request: &TextRequest) -> bool {
        self.revision == request.revision
            && self
                .fields
                .get(&request.object_id)
                .and_then(Option::as_deref)
                == Some(request.fallback_text.as_str())
    }
}
struct Entry {
    target: Target,
    source: Arc<OnceCell<Option<Source>>>,
    pages: PageFields,
    translations: BTreeMap<String, (usize, [u8; 32])>,
}
#[derive(Default)]
struct Registry {
    entries: BTreeMap<String, Entry>,
    order: VecDeque<String>,
}
impl Registry {
    fn forget(&mut self, id: &str) {
        self.entries.remove(id);
        self.order.retain(|key| key != id);
    }
    fn entry(&mut self, target: &Target) -> &mut Entry {
        if self
            .entries
            .get(&target.id)
            .is_some_and(|entry| entry.target != *target)
        {
            self.forget(&target.id);
        }
        if !self.entries.contains_key(&target.id)
            && self.entries.len() >= MAX_PROVIDERS
            && let Some(old) = self.order.pop_front()
        {
            self.entries.remove(&old);
        }
        self.order.retain(|key| key != &target.id);
        self.order.push_back(target.id.clone());
        self.entries
            .entry(target.id.clone())
            .or_insert_with(|| Entry {
                target: target.clone(),
                source: Arc::default(),
                pages: PageFields::default(),
                translations: BTreeMap::new(),
            })
    }
}
pub(super) struct Router {
    registry: Mutex<Registry>,
    slots: tokio::sync::Semaphore,
}
impl Default for Router {
    fn default() -> Self {
        Self {
            registry: Mutex::default(),
            slots: tokio::sync::Semaphore::new(16),
        }
    }
}
impl Router {
    pub(super) async fn forget(&self, id: &str) {
        self.registry.lock().await.forget(id);
    }
}
fn absent(error: &zbus::Error) -> bool {
    match error {
        zbus::Error::MethodError(name, _, _) => [
            "org.freedesktop.DBus.Error.UnknownInterface",
            "org.freedesktop.DBus.Error.UnknownMethod",
            "org.freedesktop.DBus.Error.UnknownObject",
        ]
        .contains(&name.as_str()),
        zbus::Error::FDO(error) => matches!(
            **error,
            zbus::fdo::Error::UnknownInterface(_)
                | zbus::fdo::Error::UnknownMethod(_)
                | zbus::fdo::Error::UnknownObject(_)
        ),
        _ => false,
    }
}
impl Core {
    pub async fn export_texts(&self) -> Result<(), ErrorCode> {
        self.connection
            .object_server()
            .at(production::PATH, Endpoint { core: self.clone() })
            .await
            .map(|_| ())
            .map_err(|_| ErrorCode::Internal)
    }
    async fn texts_target(&self, id: &str, generation: Option<&str>) -> Result<Target, ErrorCode> {
        validation::id(id)?;
        if let Some(generation) = generation {
            validation::id(generation)?;
        }
        let state = self.state.lock().await;
        let provider = state.providers.get(id).ok_or(ErrorCode::NotFound)?;
        if generation.is_some_and(|generation| generation != provider.info.generation) {
            return Err(ErrorCode::Conflict);
        }
        if provider.owner.is_empty() || provider.info.runtime_state != "ready" {
            return Err(ErrorCode::Unavailable);
        }
        Ok(Target {
            id: id.into(),
            plugin: provider.info.plugin_id.clone(),
            owner: provider.owner.clone(),
            generation: provider.info.generation.clone(),
        })
    }
    async fn texts_current(&self, target: &Target) -> Result<(), ErrorCode> {
        if self
            .texts_target(&target.id, Some(&target.generation))
            .await
            .as_ref()
            == Ok(target)
        {
            Ok(())
        } else {
            Err(ErrorCode::Conflict)
        }
    }
    async fn text_source(
        &self,
        target: &Target,
        deadline: tokio::time::Instant,
    ) -> Result<Source, ErrorCode> {
        let cell = self
            .texts
            .registry
            .lock()
            .await
            .entry(target)
            .source
            .clone();
        let source = tokio::time::timeout_at(
            deadline,
            cell.get_or_try_init(|| async {
                let proxy = ProviderTextsProxy::builder(&self.connection)
                    .destination(target.owner.as_str())
                    .map_err(|_| ErrorCode::Internal)?
                    .build()
                    .await
                    .map_err(normalize_error)?;
                let reply = proxy.get_catalog(&target.generation).await;
                self.texts_current(target).await?;
                let json = match reply {
                    Ok(json) => json,
                    Err(error) if absent(&error) => return Ok(None),
                    Err(error) => return Err(normalize_error(error)),
                };
                let catalog = Catalog::from_json(&json).map_err(|_| ErrorCode::Unavailable)?;
                if catalog.plugin_id != target.plugin {
                    return Err(ErrorCode::Unavailable);
                }
                Ok(Some(Source { catalog, json }))
            }),
        )
        .await
        .map_err(|_| ErrorCode::Timeout)??;
        self.texts_current(target).await?;
        source.clone().ok_or(ErrorCode::Unsupported)
    }
    pub(super) async fn remember_text_page(
        &self,
        id: &str,
        generation: &str,
        owner: &str,
        page: &arinn_protocol::music::Page,
    ) -> Result<(), ErrorCode> {
        // Observation must not add a readiness gate to ordinary GetPage.
        // A registered provider can answer valid content before its first poll.
        let state = self.state.lock().await;
        let provider = state
            .providers
            .get(id)
            .filter(|provider| provider.owner == owner && provider.info.generation == generation)
            .ok_or(ErrorCode::Conflict)?;
        let target = Target {
            id: id.into(),
            plugin: provider.info.plugin_id.clone(),
            owner: owner.into(),
            generation: generation.into(),
        };
        self.texts
            .registry
            .lock()
            .await
            .entry(&target)
            .pages
            .observe(page);
        Ok(())
    }
    /// Downloads publish a projected revision; providers resolve their own
    /// original job revision. Other snapshot addresses pass through unchanged.
    async fn text_originals(
        &self,
        target: &Target,
        requests: &[TextRequest],
    ) -> Result<Vec<Option<TextRequest>>, ErrorCode> {
        let mut originals = vec![None; requests.len()];
        {
            let state = self.state.lock().await;
            let provider = state
                .providers
                .get(&target.id)
                .filter(|provider| {
                    provider.owner == target.owner && provider.info.generation == target.generation
                })
                .ok_or(ErrorCode::Conflict)?;
            for (index, request) in requests.iter().enumerate() {
                let valid = match request.scope {
                    Scope::Provider => {
                        request.field == "description"
                            || provider.info.name == request.fallback_text
                    }
                    Scope::MediaSession => {
                        provider.capability() == "media"
                            && provider.info.revision == request.revision
                            && provider.sessions.iter().any(|session| {
                                session.id == request.object_id
                                    && session.title == request.fallback_text
                            })
                    }
                    _ => false,
                };
                if valid {
                    originals[index] = Some(request.clone());
                }
            }
        }
        let music = self
            .music
            .text_state(&target.id, &target.generation, &target.owner)
            .await;
        {
            let mut registry = self.texts.registry.lock().await;
            let entry = registry.entry(target);
            for (index, request) in requests.iter().enumerate() {
                let valid = match request.scope {
                    Scope::MusicEntity => {
                        entry.pages.matches(request)
                            && music
                                .as_ref()
                                .is_none_or(|state| state.catalogue_revision <= request.revision)
                    }
                    Scope::MusicStation => music.as_ref().is_some_and(|state| {
                        state.revision == request.revision
                            && state.station.as_ref().is_some_and(|station| {
                                station.id == request.object_id
                                    && station.title == request.fallback_text
                            })
                    }),
                    _ => false,
                };
                if valid {
                    originals[index] = Some(request.clone());
                }
            }
        }
        if requests
            .iter()
            .any(|request| request.scope == Scope::DownloadJob)
        {
            let downloads = self.downloads.lock().await;
            if let Some(snapshot) = downloads.get(&target.id) {
                for (index, request) in requests.iter().enumerate() {
                    if request.scope == Scope::DownloadJob {
                        originals[index] = snapshot.text_request(request);
                    }
                }
            }
        }
        self.texts_current(target).await?;
        Ok(originals)
    }
    async fn text_catalog(&self, id: &str) -> Result<(String, String), ErrorCode> {
        let _slot = self
            .texts
            .slots
            .try_acquire()
            .map_err(|_| ErrorCode::Unavailable)?;
        let target = self.texts_target(id, None).await?;
        let source = self
            .text_source(&target, tokio::time::Instant::now() + DEADLINE)
            .await?;
        Ok((target.generation, source.json))
    }
    async fn text_translation(
        &self,
        id: &str,
        generation: &str,
        locale: &str,
    ) -> Result<Vec<u8>, ErrorCode> {
        localization::validate_locale(locale)?;
        let _slot = self
            .texts
            .slots
            .try_acquire()
            .map_err(|_| ErrorCode::Unavailable)?;
        let target = self.texts_target(id, Some(generation)).await?;
        let deadline = tokio::time::Instant::now() + DEADLINE;
        let source = self.text_source(&target, deadline).await?;
        if !source
            .catalog
            .locales
            .iter()
            .any(|declared| declared == locale)
        {
            return Err(ErrorCode::NotFound);
        }
        let proxy = ProviderTextsProxy::builder(&self.connection)
            .destination(target.owner.as_str())
            .map_err(|_| ErrorCode::Internal)?
            .build()
            .await
            .map_err(normalize_error)?;
        let bytes = tokio::time::timeout_at(deadline, proxy.get_translation(generation, locale))
            .await
            .map_err(|_| ErrorCode::Timeout)?
            .map_err(normalize_error)?;
        self.texts_current(&target).await?;
        if bytes.is_empty() || bytes.len() > localization::MAX_QM_BYTES {
            return Err(ErrorCode::Unavailable);
        }
        let digest: [u8; 32] = Sha256::digest(&bytes).into();
        let mut registry = self.texts.registry.lock().await;
        let entry = registry.entry(&target);
        if let Some(previous) = entry.translations.get(locale) {
            if *previous != (bytes.len(), digest) {
                return Err(ErrorCode::Unavailable);
            }
        } else {
            if entry
                .translations
                .values()
                .map(|(size, _)| size)
                .sum::<usize>()
                + bytes.len()
                > localization::MAX_TOTAL_QM_BYTES
            {
                return Err(ErrorCode::Unavailable);
            }
            entry
                .translations
                .insert(locale.into(), (bytes.len(), digest));
        }
        drop(registry);
        self.texts_current(&target).await?;
        Ok(bytes)
    }
    async fn resolve_fixed_text(
        &self,
        id: &str,
        generation: &str,
        json: &str,
    ) -> Result<String, ErrorCode> {
        let requests = localization::requests_from_json(json)?;
        let _slot = self
            .texts
            .slots
            .try_acquire()
            .map_err(|_| ErrorCode::Unavailable)?;
        let target = self.texts_target(id, Some(generation)).await?;
        let deadline = tokio::time::Instant::now() + DEADLINE;
        let source = self.text_source(&target, deadline).await?;
        let originals = self.text_originals(&target, &requests).await?;
        let mapping: Vec<_> = originals
            .iter()
            .enumerate()
            .filter_map(|(index, request)| request.as_ref().map(|request| (index, request.clone())))
            .collect();
        if mapping.is_empty() {
            return Ok("[]".into());
        }
        let forwarded: Vec<_> = mapping.iter().map(|(_, request)| request.clone()).collect();
        let proxy = ProviderTextsProxy::builder(&self.connection)
            .destination(target.owner.as_str())
            .map_err(|_| ErrorCode::Internal)?
            .build()
            .await
            .map_err(normalize_error)?;
        let payload = localization::requests_to_json(&forwarded)?;
        let reply = tokio::time::timeout_at(deadline, proxy.resolve(generation, &payload))
            .await
            .map_err(|_| ErrorCode::Timeout)?
            .map_err(normalize_error)?;
        self.texts_current(&target).await?;
        let resolved = localization::resolved_from_json(&source.catalog, &forwarded, &reply)
            .map_err(|_| ErrorCode::Unavailable)?;
        let current = self.text_originals(&target, &requests).await?;
        let mut accepted = Vec::new();
        for mut result in resolved {
            let (index, original) = &mapping[result.request_index as usize];
            if current[*index].as_ref() == Some(original) {
                result.request_index = *index as u32;
                accepted.push(result);
            }
        }
        localization::resolved_to_json(&source.catalog, &requests, &accepted)
    }
}

async fn bounded<T>(
    future: impl std::future::Future<Output = Result<T, ErrorCode>>,
) -> Result<T, ErrorCode> {
    tokio::time::timeout(DEADLINE, future)
        .await
        .map_err(|_| ErrorCode::Timeout)?
}
struct Endpoint {
    core: Core,
}
#[zbus::interface(name = "org.arinn.Texts1")]
impl Endpoint {
    async fn get_catalog(&self, provider_id: &str) -> production::Result<(String, String)> {
        Ok(bounded(self.core.text_catalog(provider_id)).await?)
    }
    async fn get_translation(
        &self,
        provider_id: &str,
        generation: &str,
        locale: &str,
    ) -> production::Result<Vec<u8>> {
        Ok(bounded(self.core.text_translation(provider_id, generation, locale)).await?)
    }
    async fn resolve(
        &self,
        provider_id: &str,
        generation: &str,
        requests: &str,
    ) -> production::Result<String> {
        Ok(bounded(
            self.core
                .resolve_fixed_text(provider_id, generation, requests),
        )
        .await?)
    }
}

#[cfg(test)]
#[path = "texts_tests.rs"]
mod tests;
