// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Generic music routing. Playback and account credentials stay in providers.
use super::*;
use arinn_protocol::music::{self, Command, MusicProviderProxy, Page, Query, State as MusicState};

#[derive(Clone)]
struct Cached {
    generation: String,
    owner: String,
    state: MusicState,
}
#[derive(Clone)]
struct Pending {
    id: String,
    generation: String,
    owner: String,
    client: String,
    command: Command,
}
type Receipt = (String, String, String, Command, Result<String, ErrorCode>);
#[derive(Default)]
struct Registry {
    active: Option<String>,
    pending: Option<Pending>,
    states: BTreeMap<String, Cached>,
    ledger: std::collections::VecDeque<Receipt>,
}
pub(super) struct Coordinator {
    gate: Mutex<()>,
    slots: tokio::sync::Semaphore,
    registry: Mutex<Registry>,
}
impl Default for Coordinator {
    fn default() -> Self {
        Self {
            gate: Mutex::new(()),
            slots: tokio::sync::Semaphore::new(32),
            registry: Mutex::new(Registry::default()),
        }
    }
}
impl Coordinator {
    pub(super) async fn forget(&self, id: &str) {
        let mut r = self.registry.lock().await;
        r.states.remove(id);
        if r.pending.as_ref().is_some_and(|p| p.id == id) {
            r.pending = None;
        }
        if r.active.as_deref() == Some(id) {
            r.active = None;
        }
    }
    async fn cache(&self, id: &str, generation: &str, owner: &str, state: MusicState) {
        let mut r = self.registry.lock().await;
        if let Some(previous) = r.states.get(id)
            && previous.generation == generation
            && previous.owner == owner
            && previous.state.revision > state.revision
        {
            return;
        }
        if r.states.len() >= 32
            && !r.states.contains_key(id)
            && let Some(key) = r
                .states
                .keys()
                .find(|k| Some(k.as_str()) != r.active.as_deref())
                .cloned()
        {
            r.states.remove(&key);
        }
        r.states.insert(
            id.into(),
            Cached {
                generation: generation.into(),
                owner: owner.into(),
                state,
            },
        );
    }
}
pub(super) struct Controller {
    pub core: Core,
}
#[zbus::interface(name = "org.arinn.MusicController1")]
impl Controller {
    async fn get_active(&self) -> production::Result<String> {
        Ok(music::encode(&self.core.active_music().await)?)
    }
}

impl Core {
    async fn active_music(&self) -> Option<music::Active> {
        let candidates = {
            let r = self.music.registry.lock().await;
            let mut values: Vec<_> = r
                .states
                .iter()
                .map(|(id, value)| (id.clone(), value.clone()))
                .collect();
            values.sort_by_key(|(id, _)| Some(id.as_str()) != r.active.as_deref());
            values
        };
        for (id, cached) in candidates {
            if (cached.state.current.is_some()
                || cached.state.continuous
                || (!cached.state.session.is_empty() && cached.state.playback == "loading"))
                && self
                    .music_identity(&id, &cached.generation)
                    .await
                    .as_deref()
                    == Ok(cached.owner.as_str())
            {
                return Some(music::Active {
                    provider_id: id,
                    generation: cached.generation,
                    state: cached.state,
                });
            }
        }
        None
    }
    async fn forward_music(
        &self,
        id: &str,
        generation: &str,
        client: &str,
        command: &Command,
        deadline: tokio::time::Instant,
    ) -> Result<MusicState, ErrorCode> {
        let owner = self.music_identity(id, generation).await?;
        let proxy = MusicProviderProxy::builder(&self.connection)
            .destination(owner.as_str())
            .and_then(|b| b.path(production::PROVIDER_PATH))
            .map_err(|_| ErrorCode::Internal)?
            .build()
            .await
            .map_err(normalize_error)?;
        let payload = music::encode(command)?;
        self.music.registry.lock().await.pending = Some(Pending {
            id: id.into(),
            generation: generation.into(),
            owner: owner.clone(),
            client: client.into(),
            command: command.clone(),
        });
        // A transport deadline is uncertain; an explicit provider error is a
        // terminal receipt, including the provider's own Timeout error.
        let response =
            tokio::time::timeout_at(deadline, proxy.command(generation, client, &payload))
                .await
                .map_err(|_| ErrorCode::Timeout)?;
        self.music.registry.lock().await.pending = None;
        let json = response.map_err(normalize_error)?;
        let snapshot: MusicState = music::decode(&json)?;
        snapshot.validate()?;
        if self.music_identity(id, generation).await? != owner {
            return Err(ErrorCode::Conflict);
        }
        self.music
            .cache(id, generation, &owner, snapshot.clone())
            .await;
        Ok(snapshot)
    }
    async fn music_identity(&self, id: &str, generation: &str) -> Result<String, ErrorCode> {
        let state = self.state.lock().await;
        let p = state.providers.get(id).ok_or(ErrorCode::NotFound)?;
        if p.capability() != "music" {
            return Err(ErrorCode::Unsupported);
        }
        if p.info.generation != generation {
            return Err(ErrorCode::Conflict);
        }
        if p.owner.is_empty() {
            return Err(ErrorCode::Unavailable);
        }
        Ok(p.owner.clone())
    }
    pub(super) async fn watch_music(&self, id: String, owner: String, generation: String) {
        let Ok(proxy) = MusicProviderProxy::builder(&self.connection)
            .destination(owner.as_str())
            .and_then(|b| b.path(production::PROVIDER_PATH))
            .unwrap()
            .build()
            .await
        else {
            return;
        };
        let features = tokio::time::timeout(Duration::from_secs(3), proxy.features())
            .await
            .ok()
            .and_then(Result::ok)
            .unwrap_or_default();
        let supports_assets = features.len() <= 8
            && features
                .iter()
                .any(|f| f == arinn_protocol::assets::FEATURE_ASSETS);
        let mut tick = tokio::time::interval(Duration::from_secs(1));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tick.tick().await;
            if self.music_identity(&id, &generation).await.as_deref() != Ok(owner.as_str()) {
                return;
            }
            let result = tokio::time::timeout(Duration::from_secs(3), proxy.get_state(&generation))
                .await
                .map_err(|_| ErrorCode::Timeout)
                .and_then(|r| r.map_err(normalize_error))
                .and_then(|s| {
                    let state: MusicState = music::decode(&s)?;
                    state.validate()?;
                    Ok(state)
                });
            let mut state = self.state.lock().await;
            let Some(p) = state.providers.get_mut(&id) else {
                return;
            };
            if p.owner != owner || p.info.generation != generation {
                return;
            }
            let changed = match result {
                Ok(snapshot) if snapshot.revision < p.info.revision => false,
                Ok(snapshot) => {
                    let changed = p.info.revision != snapshot.revision
                        || p.info.backend_state != snapshot.backend
                        || p.info.runtime_state != "ready";
                    p.info.revision = snapshot.revision;
                    p.info.backend_state = snapshot.backend.clone();
                    self.music.cache(&id, &generation, &owner, snapshot).await;
                    p.features = if supports_assets {
                        vec![arinn_protocol::assets::FEATURE_ASSETS.into()]
                    } else {
                        vec![]
                    };
                    p.info.runtime_state = "ready".into();
                    changed
                }
                Err(error) => {
                    failed(&mut p.info, error);
                    false
                }
            };
            drop(state);
            if changed {
                self.changed().await;
            }
        }
    }
}
pub(super) struct MusicEndpoint {
    pub core: Core,
    pub id: String,
}
#[zbus::interface(name = "org.arinn.Music1")]
impl MusicEndpoint {
    async fn get_state(&self, generation: &str) -> production::Result<String> {
        let owner = self.core.music_identity(&self.id, generation).await?;
        let proxy = MusicProviderProxy::builder(&self.core.connection)
            .destination(owner.as_str())
            .and_then(|b| b.path(production::PROVIDER_PATH))
            .map_err(|_| ErrorCode::Internal)?
            .build()
            .await
            .map_err(normalize_error)?;
        let json = tokio::time::timeout(Duration::from_secs(3), proxy.get_state(generation))
            .await
            .map_err(|_| ErrorCode::Timeout)?
            .map_err(normalize_error)?;
        let snapshot: MusicState = music::decode(&json)?;
        snapshot.validate()?;
        if self.core.music_identity(&self.id, generation).await? != owner {
            return Err(ErrorCode::Conflict.into());
        }
        Ok(music::encode(&snapshot)?)
    }
    async fn get_page(
        &self,
        generation: &str,
        query: &str,
        #[zbus(header)] h: Header<'_>,
    ) -> production::Result<String> {
        let query: Query = music::decode(query)?;
        query.validate()?;
        let client = sender(&h)?;
        let owner = self.core.music_identity(&self.id, generation).await?;
        let proxy = MusicProviderProxy::builder(&self.core.connection)
            .destination(owner.as_str())
            .and_then(|b| b.path(production::PROVIDER_PATH))
            .map_err(|_| ErrorCode::Internal)?
            .build()
            .await
            .map_err(normalize_error)?;
        let json = tokio::time::timeout(
            Duration::from_secs(5),
            proxy.get_page(generation, &client, &music::encode(&query)?),
        )
        .await
        .map_err(|_| ErrorCode::Timeout)?
        .map_err(normalize_error)?;
        let page: Page = music::decode(&json)?;
        page.validate()?;
        if page.generation != generation
            || self.core.music_identity(&self.id, generation).await? != owner
        {
            return Err(ErrorCode::Conflict.into());
        }
        Ok(music::encode(&page)?)
    }
    async fn command(
        &self,
        generation: &str,
        command: &str,
        #[zbus(header)] h: Header<'_>,
    ) -> production::Result<String> {
        let command: Command = music::decode(command)?;
        command.validate()?;
        let client = sender(&h)?;
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        let _slot = self
            .core
            .music
            .slots
            .try_acquire()
            .map_err(|_| ErrorCode::Unavailable)?;
        let _serial = tokio::time::timeout_at(deadline, self.core.music.gate.lock())
            .await
            .map_err(|_| ErrorCode::Timeout)?;
        self.core.music_identity(&self.id, generation).await?;
        let pending = self.core.music.registry.lock().await.pending.clone();
        if let Some(pending) = pending {
            if self
                .core
                .music_identity(&pending.id, &pending.generation)
                .await
                .as_deref()
                == Ok(pending.owner.as_str())
            {
                let settled = self
                    .core
                    .forward_music(
                        &pending.id,
                        &pending.generation,
                        &pending.client,
                        &pending.command,
                        deadline,
                    )
                    .await;
                if self.core.music.registry.lock().await.pending.is_some() {
                    return Err(ErrorCode::Timeout.into());
                }
                let encoded = settled.and_then(|state| music::encode(&state));
                let mut registry = self.core.music.registry.lock().await;
                if let Some(receipt) =
                    registry
                        .ledger
                        .iter_mut()
                        .find(|(client, id, generation, command, _)| {
                            client == &pending.client
                                && id == &pending.id
                                && generation == &pending.generation
                                && command == &pending.command
                        })
                {
                    receipt.4 = encoded;
                }
            } else {
                self.core.music.registry.lock().await.pending = None;
            }
        }
        {
            let registry = self.core.music.registry.lock().await;
            if let Some((_, _, _, previous, result)) =
                registry.ledger.iter().find(|(c, id, g, request, _)| {
                    c == &client
                        && id == &self.id
                        && g == generation
                        && request.request_id == command.request_id
                })
            {
                if previous != &command {
                    return Err(ErrorCode::Conflict.into());
                }
                return result.clone().map_err(Into::into);
            }
        }
        let result = async {
            if ["play", "radio"].contains(&command.action.as_str())
                && let Some(active) = self.core.active_music().await
                && (active.state.continuous
                    || ["playing", "paused", "loading"].contains(&active.state.playback.as_str()))
            {
                if command.replaces_provider != active.provider_id
                    || command.replaces_session != active.state.session
                {
                    return Err(ErrorCode::Conflict);
                }
                let stop = Command {
                    request_id: Uuid::new_v4().to_string(),
                    action: "stop".into(),
                    session: active.state.session,
                    entity: String::new(),
                    occurrence: String::new(),
                    position_us: 0,
                    replaces_provider: String::new(),
                    replaces_session: String::new(),
                    text: String::new(),
                };
                let stopped = self
                    .core
                    .forward_music(
                        &active.provider_id,
                        &active.generation,
                        &client,
                        &stop,
                        deadline,
                    )
                    .await?;
                if stopped.current.is_some() || stopped.playback != "stopped" {
                    return Err(ErrorCode::Conflict);
                }
            }
            let snapshot = self
                .core
                .forward_music(&self.id, generation, &client, &command, deadline)
                .await?;
            if ["play", "radio"].contains(&command.action.as_str()) {
                self.core.music.registry.lock().await.active = Some(self.id.clone());
            }
            music::encode(&snapshot)
        }
        .await;
        let mut registry = self.core.music.registry.lock().await;
        registry.ledger.push_back((
            client,
            self.id.clone(),
            generation.into(),
            command,
            result.clone(),
        ));
        while registry.ledger.len() > 32 {
            registry.ledger.pop_front();
        }
        result.map_err(Into::into)
    }
}

#[cfg(test)]
#[path = "music_tests.rs"]
mod routing_tests;

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn delayed_poll_cannot_replace_a_newer_command_snapshot() {
        let coordinator = Coordinator::default();
        coordinator
            .cache(
                "provider",
                "generation",
                "owner",
                MusicState {
                    revision: 5,
                    playback: "paused".into(),
                    ..MusicState::default()
                },
            )
            .await;
        coordinator
            .cache(
                "provider",
                "generation",
                "owner",
                MusicState {
                    revision: 4,
                    playback: "playing".into(),
                    ..MusicState::default()
                },
            )
            .await;
        assert_eq!(
            coordinator.registry.lock().await.states["provider"]
                .state
                .playback,
            "paused"
        );
        coordinator.forget("provider").await;
        assert!(coordinator.registry.lock().await.states.is_empty());
    }
}
