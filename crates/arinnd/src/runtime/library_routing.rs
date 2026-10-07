// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Daemon routing for `library` providers (ADR-0054, ADR-0058, ADR-0071):
//! catalogue, resume and root-availability import, `org.arinn.Library1`, and
//! library launch through the
//! `MediaOpen1` of a media provider. The file URI of a launch goes from the
//! library provider to the media provider only; it never reaches the shell or
//! the logs.
use super::*;
use arinn_plugin_sdk::snapshots::{LibrarySnapshots, ResumeSnapshots};
use arinn_protocol::{
    library::{
        self, Collection, FEATURE_IMPORT, FEATURE_RESUME, FEATURE_ROOTS, Item, LibraryPage,
        LibraryProviderProxy, LibraryResumeProxy, LibraryRootsProxy, Resume, ResumePage, Roots,
    },
    media::{FEATURE_OPEN, MediaOpenProxy, MediaSource},
};

const KNOWN_FEATURES: &[&str] = &[
    FEATURE_RESUME,
    FEATURE_IMPORT,
    FEATURE_ROOTS,
    arinn_protocol::assets::FEATURE_ASSETS,
];
/// Library launch includes starting the player: the launch timeout of spec §9.5.
const LIBRARY_LAUNCH_TIMEOUT: Duration = Duration::from_secs(30);
const RESUME_COMMAND_TIMEOUT: Duration = Duration::from_secs(5);

type LibrarySnapshot = (u64, Vec<Collection>, Vec<Item>, String, Vec<String>);

pub(super) async fn read_library(
    proxy: &LibraryProviderProxy<'_>,
    generation: &str,
) -> Result<LibrarySnapshot, ErrorCode> {
    let mut token = String::new();
    let mut tokens = HashSet::new();
    let mut revision = 0;
    let (mut collections, mut items) = (Vec::new(), Vec::new());
    let mut total = 0;
    loop {
        let (page,): (LibraryPage,) = proxy.get_library(&token).await.map_err(normalize_error)?;
        page.validate()?;
        total += page.encoded_len()?;
        if total > 16 * 1024 * 1024
            || collections.len() + items.len() + page.collections.len() + page.items.len() > 100_000
        {
            return Err(ErrorCode::InvalidArgument);
        }
        if page.generation != generation || (revision != 0 && page.revision != revision) {
            return Err(ErrorCode::Conflict);
        }
        revision = page.revision;
        collections.extend(page.collections);
        items.extend(page.items);
        token = page.next_token;
        if token.is_empty() {
            break;
        }
        if !tokens.insert(token.clone()) {
            return Err(ErrorCode::Conflict);
        }
    }
    library::validate_entries(&collections, &items)?;
    let backend = proxy.backend_state().await.map_err(normalize_error)?;
    if !["available", "unavailable", "degraded"].contains(&backend.as_str()) {
        return Err(ErrorCode::Unsupported);
    }
    let features = proxy
        .features()
        .await
        .map_err(normalize_error)?
        .into_iter()
        .filter(|f| KNOWN_FEATURES.contains(&f.as_str()))
        .collect();
    Ok((revision, collections, items, backend, features))
}

pub(super) async fn read_resumes(
    proxy: &LibraryResumeProxy<'_>,
    generation: &str,
) -> Result<(u64, Vec<Resume>), ErrorCode> {
    let mut token = String::new();
    let mut tokens = HashSet::new();
    let mut revision = 0;
    let mut resumes = Vec::new();
    loop {
        let (page,): (ResumePage,) = proxy.get_resumes(&token).await.map_err(normalize_error)?;
        page.validate()?;
        if resumes.len() + page.resumes.len() > 100_000 {
            return Err(ErrorCode::InvalidArgument);
        }
        if page.generation != generation || (revision != 0 && page.revision != revision) {
            return Err(ErrorCode::Conflict);
        }
        revision = page.revision;
        resumes.extend(page.resumes);
        token = page.next_token;
        if token.is_empty() {
            break;
        }
        if !tokens.insert(token.clone()) {
            return Err(ErrorCode::Conflict);
        }
    }
    library::validate_resumes(&resumes)?;
    Ok((revision, resumes))
}

pub(super) async fn read_roots(
    proxy: &LibraryRootsProxy<'_>,
    generation: &str,
) -> Result<Roots, ErrorCode> {
    let (roots,): (Roots,) = proxy.get_roots(generation).await.map_err(normalize_error)?;
    roots.validate()?;
    if roots.generation != generation {
        return Err(ErrorCode::Conflict);
    }
    Ok(roots)
}

impl Core {
    pub(super) async fn watch_library(&self, id: String, owner: String, generation: String) {
        let build = |path| {
            LibraryProviderProxy::builder(&self.connection)
                .destination(owner.clone())
                .and_then(|b| b.path(path))
        };
        let Ok(proxy) = build(production::PROVIDER_PATH).unwrap().build().await else {
            return;
        };
        let Ok(resume) = LibraryResumeProxy::builder(&self.connection)
            .destination(owner.as_str())
            .and_then(|b| b.path(production::PROVIDER_PATH))
            .unwrap()
            .build()
            .await
        else {
            return;
        };
        let Ok(mut changes) = proxy.receive_provider_state_changed().await else {
            return;
        };
        // Resume changes have their own signal; a provider without
        // LibraryResume1 simply never sends it.
        let Ok(mut resume_changes) = resume.receive_resume_state_changed().await else {
            return;
        };
        let Ok(roots) = LibraryRootsProxy::builder(&self.connection)
            .destination(owner.as_str())
            .and_then(|b| b.path(production::PROVIDER_PATH))
            .unwrap()
            .build()
            .await
        else {
            return;
        };
        // Likewise for LibraryRoots1 (ADR-0071).
        let Ok(mut root_changes) = roots.receive_roots_state_changed().await else {
            return;
        };
        let mut tick = tokio::time::interval(Duration::from_secs(1));
        loop {
            tokio::select! {
                _=tick.tick()=>{},
                event=changes.next()=>{if event.is_none(){return;}},
                event=resume_changes.next()=>{if event.is_none(){return;}},
                event=root_changes.next()=>{if event.is_none(){return;}},
            }
            if !self
                .state
                .lock()
                .await
                .providers
                .get(&id)
                .is_some_and(|p| p.owner == owner && p.info.generation == generation)
            {
                return;
            }
            let result = tokio::time::timeout(Duration::from_secs(3), async {
                let snapshot = read_library(&proxy, &generation).await?;
                let resumes = if snapshot.4.iter().any(|f| f == FEATURE_RESUME) {
                    Some(read_resumes(&resume, &generation).await?)
                } else {
                    None
                };
                let roots = if snapshot.4.iter().any(|f| f == FEATURE_ROOTS) {
                    Some(read_roots(&roots, &generation).await?)
                } else {
                    None
                };
                Ok::<_, ErrorCode>((snapshot, resumes, roots))
            })
            .await
            .unwrap_or(Err(ErrorCode::Timeout));
            let mut state = self.state.lock().await;
            let State {
                providers,
                library_roots,
                ..
            } = &mut *state;
            let Some(p) = providers.get_mut(&id) else {
                return;
            };
            if p.owner != owner || p.info.generation != generation {
                return;
            }
            let mut signal = None;
            match result {
                Ok(((revision, collections, items, backend, features), resumes, roots)) => {
                    let cache = p
                        .library_cache
                        .get_or_insert_with(|| LibrarySnapshots::new(&generation).unwrap());
                    let imported = cache
                        .import(revision, collections.clone(), items.clone())
                        .and_then(|()| match &resumes {
                            Some((resume_revision, list)) => p
                                .resume_cache
                                .get_or_insert_with(|| ResumeSnapshots::new(&generation).unwrap())
                                .import(*resume_revision, list.clone()),
                            None => {
                                p.resume_cache = None;
                                Ok(())
                            }
                        });
                    match imported {
                        Ok(()) => {
                            let resumes_changed = resumes.as_ref().map(|(_, r)| r)
                                != Some(&p.resumes)
                                && !(resumes.is_none() && p.resumes.is_empty());
                            // The shell reads roots with Library1.GetRoots; a
                            // change is announced with the library's StateChanged.
                            let roots_changed = roots.as_ref().map(|r| &r.roots)
                                != library_roots.get(&id).map(|r| &r.roots);
                            if revision != p.info.revision || resumes_changed || roots_changed {
                                signal = Some(revision);
                            }
                            match roots {
                                Some(roots) => {
                                    library_roots.insert(id.clone(), roots);
                                }
                                None => {
                                    library_roots.remove(&id);
                                }
                            }
                            p.collections = collections;
                            p.items = items;
                            p.resumes = resumes.map(|(_, r)| r).unwrap_or_default();
                            p.features = features;
                            p.info.revision = revision;
                            p.info.backend_state = backend;
                            p.info.runtime_state = "ready".into();
                            if !p.info.last_error.is_empty() && p.info.recovered_at == 0 {
                                p.info.recovered_at = now();
                            }
                        }
                        Err(error) => failed(&mut p.info, error),
                    }
                }
                Err(error) => failed(&mut p.info, error),
            }
            drop(state);
            if let Some(revision) = signal {
                if let Ok(emitter) = SignalEmitter::new(&self.connection, production::PATH) {
                    let _ = Daemon::state_changed(&emitter, &id, "library", &generation, revision)
                        .await;
                }
                self.changed().await;
            }
        }
    }

    /// The media provider that opens library content: the configured default
    /// for `media` when it declares `MediaOpen1`, otherwise the first ready
    /// media provider that does. Returns (id, owner, generation).
    async fn media_opener(&self) -> Result<(String, String, String), ErrorCode> {
        let preferred = Config::load(&self.paths.config)
            .ok()
            .and_then(|c| c.defaults.get("media").cloned());
        let state = self.state.lock().await;
        let usable = |p: &Provider| {
            p.capability() == "media"
                && p.info.runtime_state == "ready"
                && p.info.backend_state != "unavailable"
                && !p.owner.is_empty()
                && p.features.iter().any(|f| f == FEATURE_OPEN)
        };
        let chosen = preferred
            .as_ref()
            .and_then(|id| state.providers.get_key_value(id))
            .filter(|(_, p)| usable(p))
            .or_else(|| state.providers.iter().find(|(_, p)| usable(p)))
            .ok_or(ErrorCode::Unavailable)?;
        Ok((
            chosen.0.clone(),
            chosen.1.owner.clone(),
            chosen.1.info.generation.clone(),
        ))
    }

    /// Library1 commands as operations. `launch` asks the library for the
    /// playback source and opens it in the chosen media provider; its result
    /// is the new media session. `restart` needs LibraryResume1.
    pub(super) async fn library_command(
        &self,
        id: &str,
        action: &'static str,
        generation: &str,
        item: &str,
        request_id: &str,
        caller: &str,
    ) -> Result<OwnedObjectPath, ErrorCode> {
        validation::id(item)?;
        let fingerprint = serde_json::to_string(&(id, action, generation, item))
            .map_err(|_| ErrorCode::Internal)?;
        if let Some(path) = self
            .operations
            .existing(caller, request_id, &fingerprint)
            .await?
        {
            return Ok(path);
        }
        let state = self.state.lock().await;
        let p = state.providers.get(id).ok_or(ErrorCode::NotFound)?;
        if p.capability() != "library" {
            return Err(ErrorCode::Unsupported);
        }
        if p.info.generation != generation {
            return Err(ErrorCode::Conflict);
        }
        if p.info.runtime_state != "ready" || p.info.backend_state == "unavailable" {
            return Err(ErrorCode::Unavailable);
        }
        if action == "restart" && !p.features.iter().any(|f| f == FEATURE_RESUME) {
            return Err(ErrorCode::Unsupported);
        }
        let entry = p
            .items
            .iter()
            .find(|i| i.id == item)
            .ok_or(ErrorCode::NotFound)?;
        if action == "launch" && !entry.available {
            return Err(ErrorCode::Unavailable);
        }
        let kind = if action == "restart" {
            "library.restart"
        } else {
            "library.launch"
        };
        let label = Label::new(kind).provider("library", &p.info.plugin_id);
        let owner = p.owner.clone();
        let (path, op, fresh) = self
            .operations
            .admit(
                &self.connection,
                caller,
                request_id,
                &fingerprint,
                id,
                label,
            )
            .await?;
        drop(state);
        if !fresh {
            return Ok(path);
        }
        let core = self.clone();
        let (id, generation, item) = (id.to_owned(), generation.to_owned(), item.to_owned());
        let path_copy = path.clone();
        tokio::spawn(async move {
            {
                let mut value = op.value.lock().await;
                if value.state != "queued" {
                    return;
                }
                value.state = "running".into();
                value.started_at = now();
            }
            let work = async {
                if action == "restart" {
                    op.phase("provider.call", "library.restart", async {
                        LibraryResumeProxy::builder(&core.connection)
                            .destination(owner.as_str())
                            .map_err(|_| ErrorCode::Internal)?
                            .build()
                            .await
                            .map_err(|_| ErrorCode::Unavailable)?
                            .restart_from_beginning(&generation, &item)
                            .await
                            .map_err(normalize_error)
                    })
                    .await?;
                    return Ok(EntityRef {
                        provider_id: id.clone(),
                        capability: "library".into(),
                        local_id: item.clone(),
                    });
                }
                let (source,) = op
                    .phase("provider.call", "library.launch", async {
                        LibraryProviderProxy::builder(&core.connection)
                            .destination(owner.as_str())
                            .map_err(|_| ErrorCode::Internal)?
                            .build()
                            .await
                            .map_err(|_| ErrorCode::Unavailable)?
                            .launch(&generation, &item)
                            .await
                            .map_err(normalize_error)
                    })
                    .await?;
                source.validate()?;
                if source.local_id != item {
                    return Err(ErrorCode::InvalidArgument);
                }
                let (media_id, media_owner, media_generation) = core.media_opener().await?;
                let session = op
                    .phase("provider.call", "media.open", async {
                        MediaOpenProxy::builder(&core.connection)
                            .destination(media_owner.as_str())
                            .map_err(|_| ErrorCode::Internal)?
                            .build()
                            .await
                            .map_err(|_| ErrorCode::Unavailable)?
                            .open(
                                &media_generation,
                                MediaSource {
                                    content: EntityRef {
                                        provider_id: id.clone(),
                                        capability: "library".into(),
                                        local_id: item.clone(),
                                    },
                                    uri: source.uri,
                                    start_position_us: source.start_position_us,
                                    audio_language: source.audio_language,
                                    subtitle_language: source.subtitle_language,
                                },
                            )
                            .await
                            .map_err(normalize_error)
                    })
                    .await?;
                validation::id(&session)?;
                Ok(EntityRef {
                    provider_id: media_id,
                    capability: "media".into(),
                    local_id: session,
                })
            };
            let timeout = if action == "launch" {
                LIBRARY_LAUNCH_TIMEOUT
            } else {
                RESUME_COMMAND_TIMEOUT
            };
            let result = tokio::time::timeout(timeout, work)
                .await
                .unwrap_or(Err(ErrorCode::Timeout));
            let valid = core
                .state
                .lock()
                .await
                .providers
                .get(&id)
                .is_some_and(|p| p.owner == owner && p.info.generation == generation);
            let result = if valid {
                result
            } else {
                op.reason("provider_changed");
                Err(ErrorCode::Unavailable)
            };
            op.finish(&core.connection, &path_copy, result.map(|r| vec![r]))
                .await;
        });
        Ok(path)
    }
}

pub(super) struct LibraryEndpoint {
    pub(super) core: Core,
    pub(super) id: String,
}
#[zbus::interface(name = "org.arinn.Library1")]
impl LibraryEndpoint {
    async fn get_library(
        &self,
        token: &str,
        #[zbus(header)] h: Header<'_>,
    ) -> production::Result<(LibraryPage,)> {
        let owner = sender(&h)?;
        let mut state = self.core.state.lock().await;
        let p = state
            .providers
            .get_mut(&self.id)
            .ok_or(ErrorCode::NotFound)?;
        Ok((p
            .library_cache
            .as_mut()
            .ok_or(ErrorCode::Unavailable)?
            .page(&owner, token, Instant::now())?,))
    }
    async fn get_item(&self, generation: &str, id: &str) -> production::Result<(Item,)> {
        let state = self.core.state.lock().await;
        let p = state.providers.get(&self.id).ok_or(ErrorCode::NotFound)?;
        if p.info.generation != generation {
            return Err(ErrorCode::Conflict.into());
        }
        Ok((p
            .items
            .iter()
            .find(|i| i.id == id)
            .cloned()
            .ok_or(ErrorCode::NotFound)?,))
    }
    async fn features(&self) -> production::Result<Vec<String>> {
        let state = self.core.state.lock().await;
        let p = state.providers.get(&self.id).ok_or(ErrorCode::NotFound)?;
        Ok(p.features.clone())
    }
    async fn launch(
        &self,
        generation: &str,
        id: &str,
        request_id: &str,
        #[zbus(header)] h: Header<'_>,
    ) -> production::Result<OwnedObjectPath> {
        Ok(self
            .core
            .library_command(&self.id, "launch", generation, id, request_id, &sender(&h)?)
            .await?)
    }
    async fn get_resumes(
        &self,
        token: &str,
        #[zbus(header)] h: Header<'_>,
    ) -> production::Result<(ResumePage,)> {
        let owner = sender(&h)?;
        let mut state = self.core.state.lock().await;
        let p = state
            .providers
            .get_mut(&self.id)
            .ok_or(ErrorCode::NotFound)?;
        if !p.features.iter().any(|f| f == FEATURE_RESUME) {
            return Err(ErrorCode::Unsupported.into());
        }
        Ok((p
            .resume_cache
            .as_mut()
            .ok_or(ErrorCode::Unavailable)?
            .page(&owner, token, Instant::now())?,))
    }
    /// Configured roots and their availability (ADR-0071). `Unsupported`
    /// unless `Features` lists `LibraryRoots1`; `Unavailable` until the
    /// first read.
    async fn get_roots(&self) -> production::Result<(Roots,)> {
        let state = self.core.state.lock().await;
        let p = state.providers.get(&self.id).ok_or(ErrorCode::NotFound)?;
        if !p.features.iter().any(|f| f == FEATURE_ROOTS) {
            return Err(ErrorCode::Unsupported.into());
        }
        let roots = state
            .library_roots
            .get(&self.id)
            .ok_or(ErrorCode::Unavailable)?;
        if roots.generation != p.info.generation {
            return Err(ErrorCode::Unavailable.into());
        }
        Ok((roots.clone(),))
    }
    async fn restart_from_beginning(
        &self,
        generation: &str,
        id: &str,
        request_id: &str,
        #[zbus(header)] h: Header<'_>,
    ) -> production::Result<OwnedObjectPath> {
        Ok(self
            .core
            .library_command(
                &self.id,
                "restart",
                generation,
                id,
                request_id,
                &sender(&h)?,
            )
            .await?)
    }
}

#[cfg(test)]
#[path = "library_tests.rs"]
mod tests;
