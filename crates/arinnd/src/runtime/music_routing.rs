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
type Receipt = (String, String, String, Command, Result<String, ErrorCode>);
#[derive(Default)]
struct Registry {
    active: Option<String>,
    states: BTreeMap<String, Cached>,
    ledger: std::collections::VecDeque<Receipt>,
    /// Providers whose start timed out: they may be playing. Their state is
    /// read again before another start is admitted.
    uncertain: std::collections::BTreeSet<String>,
    /// The (caller, request ID) whose start timed out, per provider. Its
    /// retry goes to the provider, whose receipt holds the real outcome.
    retries: BTreeMap<String, (String, String)>,
}
/// Receipts retained per provider endpoint (music.md).
const RECEIPTS: usize = 32;
/// A command's whole budget, and the longest it may wait behind others;
/// forwarding always keeps the rest (at least 3 s) of the budget.
const COMMAND: Duration = Duration::from_secs(5);
const QUEUED: Duration = Duration::from_secs(2);
/// Actions that start audio. Play and radio replace any audible session;
/// the others only resume or move the session they address, so they are
/// fenced only while another provider is audible.
const STARTS: [&str; 2] = ["play", "radio"];
const RESUMES: [&str; 4] = ["next", "previous", "retry", "resume"];
fn audible(state: &MusicState) -> bool {
    ["playing", "paused", "loading"].contains(&state.playback.as_str())
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
        r.uncertain.remove(id);
        r.retries.remove(id);
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
    /// The session the shell shows and transport addresses: a confirmed
    /// loading/playing/paused session first, preferring the last started
    /// provider; otherwise the last started provider's session with a
    /// current entry (for example an ended one).
    async fn active_music(&self) -> Option<music::Active> {
        let candidates = {
            let r = self.music.registry.lock().await;
            let mut values: Vec<_> = r
                .states
                .iter()
                .map(|(id, value)| (id.clone(), value.clone()))
                .collect();
            values.sort_by_key(|(id, cached)| {
                (
                    !audible(&cached.state),
                    Some(id.as_str()) != r.active.as_deref(),
                )
            });
            values
        };
        for (id, cached) in candidates {
            if cached.state.current.is_some()
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
        let json = tokio::time::timeout_at(
            deadline,
            proxy.command(generation, client, &music::encode(command)?),
        )
        .await
        .map_err(|_| ErrorCode::Timeout)?
        .map_err(normalize_error)?;
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
    /// Reads a provider's state again and caches it (after an uncertain start).
    async fn refresh_music(&self, id: &str) -> Result<(), ErrorCode> {
        let generation = {
            let r = self.music.registry.lock().await;
            r.states.get(id).map(|c| c.generation.clone())
        };
        let generation = match generation {
            Some(generation) => generation,
            None => {
                let state = self.state.lock().await;
                state
                    .providers
                    .get(id)
                    .ok_or(ErrorCode::NotFound)?
                    .info
                    .generation
                    .clone()
            }
        };
        let owner = self.music_identity(id, &generation).await?;
        let proxy = MusicProviderProxy::builder(&self.connection)
            .destination(owner.as_str())
            .and_then(|b| b.path(production::PROVIDER_PATH))
            .map_err(|_| ErrorCode::Internal)?
            .build()
            .await
            .map_err(normalize_error)?;
        let json = tokio::time::timeout(Duration::from_secs(2), proxy.get_state(&generation))
            .await
            .map_err(|_| ErrorCode::Timeout)?
            .map_err(normalize_error)?;
        let snapshot: MusicState = music::decode(&json)?;
        snapshot.validate()?;
        if self.music_identity(id, &generation).await? != owner {
            return Err(ErrorCode::Conflict);
        }
        self.music.cache(id, &generation, &owner, snapshot).await;
        Ok(())
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
        let begun = tokio::time::Instant::now();
        let deadline = begun + COMMAND;
        let _slot = self
            .core
            .music
            .slots
            .try_acquire()
            .map_err(|_| ErrorCode::Unavailable)?;
        // Not forwarded yet: a long queue is a definite refusal, not an
        // uncertain timeout, and forwarding keeps at least 3 s.
        let _serial = tokio::time::timeout_at(begun + QUEUED, self.core.music.gate.lock())
            .await
            .map_err(|_| ErrorCode::Unavailable)?;
        self.core.music_identity(&self.id, generation).await?;
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
        let action = command.action.as_str();
        let starts = STARTS.contains(&action);
        let resumes = RESUMES.contains(&action);
        let result = async {
            if starts || resumes {
                // A start that timed out may still be playing: confirm it.
                let uncertain: Vec<String> = self
                    .core
                    .music
                    .registry
                    .lock()
                    .await
                    .uncertain
                    .iter()
                    .cloned()
                    .collect();
                for id in uncertain {
                    self.core.refresh_music(&id).await?;
                    self.core.music.registry.lock().await.uncertain.remove(&id);
                }
            }
            let retry = self.core.music.registry.lock().await.retries.get(&self.id)
                == Some(&(client.clone(), command.request_id.clone()));
            if (starts || resumes)
                && !retry
                && let Some(active) = self.core.active_music().await
                && audible(&active.state)
                && (starts || active.provider_id != self.id)
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
            let forwarded = self
                .core
                .forward_music(&self.id, generation, &client, &command, deadline)
                .await;
            if starts || resumes {
                let mut registry = self.core.music.registry.lock().await;
                match &forwarded {
                    Ok(_) => {
                        registry.active = Some(self.id.clone());
                        registry.retries.remove(&self.id);
                    }
                    // The provider may have started: it becomes the active
                    // provider and is confirmed before the next start.
                    Err(ErrorCode::Timeout) => {
                        registry.active = Some(self.id.clone());
                        registry.uncertain.insert(self.id.clone());
                        registry.retries.insert(
                            self.id.clone(),
                            (client.clone(), command.request_id.clone()),
                        );
                    }
                    Err(_) => {}
                }
            }
            music::encode(&forwarded?)
        }
        .await;
        // A timeout is an uncertain outcome, not a receipt: a retry runs again
        // and reaches the provider, whose own receipt holds the real result.
        if result == Err(ErrorCode::Timeout) {
            return Err(ErrorCode::Timeout.into());
        }
        let mut registry = self.core.music.registry.lock().await;
        registry.ledger.push_back((
            client,
            self.id.clone(),
            generation.into(),
            command,
            result.clone(),
        ));
        // At most RECEIPTS per provider endpoint: another endpoint's traffic
        // never evicts this one's receipts.
        if registry
            .ledger
            .iter()
            .filter(|(_, id, _, _, _)| id == &self.id)
            .count()
            > RECEIPTS
            && let Some(oldest) = registry
                .ledger
                .iter()
                .position(|(_, id, _, _, _)| id == &self.id)
        {
            registry.ledger.remove(oldest);
        }
        result.map_err(Into::into)
    }
}

#[cfg(test)]
#[path = "music_tests.rs"]
mod tests;
