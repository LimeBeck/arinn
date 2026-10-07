// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Daemon routing for `media` providers (ADR-0054, ADR-0058): session
//! snapshot import, `org.arinn.Media1` and commands as `Operation1` objects.
//! Optional interfaces are called only when the provider's `Features` lists
//! them; otherwise the daemon answers `Unsupported`.
use super::*;
use arinn_plugin_sdk::snapshots::SessionSnapshots;
use arinn_protocol::media::{
    self, FEATURE_OPEN, FEATURE_TIMELINE, FEATURE_TRACKS, MediaProviderProxy, MediaTimelineProxy,
    MediaTracksProxy, Session, SessionPage, Timeline, Track,
};

/// Feature flags the daemon knows; others are ignored.
const KNOWN_FEATURES: &[&str] = &[
    FEATURE_OPEN,
    arinn_protocol::media_window::FEATURE,
    FEATURE_TRACKS,
    FEATURE_TIMELINE,
    arinn_protocol::assets::FEATURE_ASSETS,
];
/// Command timeout for playback controls (spec §9.5).
const MEDIA_COMMAND_TIMEOUT: Duration = Duration::from_secs(5);

/// A `Media1` command. Base commands need their token in the session's
/// `actions`; extension commands need their feature flag.
#[derive(Clone, Debug, serde::Serialize)]
pub(super) enum MediaAction {
    Play,
    Pause,
    PlayPause,
    Seek(i64),
    Next,
    Previous,
    Stop,
    SelectTrack { kind: String, track: String },
    DisableSubtitles,
    Thumbnail(i64),
}
impl MediaAction {
    fn token(&self) -> Option<&'static str> {
        Some(match self {
            Self::Play => "play",
            Self::Pause => "pause",
            Self::PlayPause => "play-pause",
            Self::Seek(_) => "seek",
            Self::Next => "next",
            Self::Previous => "previous",
            Self::Stop => "stop",
            _ => return None,
        })
    }
    /// The fixed `telemetry::KINDS` entry of this command.
    pub(super) fn kind(&self) -> &'static str {
        match self {
            Self::Play => "media.command:play",
            Self::Pause => "media.command:pause",
            Self::PlayPause => "media.command:play-pause",
            Self::Seek(_) => "media.command:seek",
            Self::Next => "media.command:next",
            Self::Previous => "media.command:previous",
            Self::Stop => "media.command:stop",
            Self::SelectTrack { .. } => "media.command:select-track",
            Self::DisableSubtitles => "media.command:disable-subtitles",
            Self::Thumbnail(_) => "media.command:thumbnail",
        }
    }
    fn feature(&self) -> Option<&'static str> {
        match self {
            Self::SelectTrack { .. } | Self::DisableSubtitles => Some(FEATURE_TRACKS),
            Self::Thumbnail(_) => Some(FEATURE_TIMELINE),
            _ => None,
        }
    }
    /// Checks the daemon can make before admitting an operation.
    fn check(&self, session: &Session, features: &[String]) -> Result<(), ErrorCode> {
        if let Some(token) = self.token()
            && !session.actions.iter().any(|a| a == token)
        {
            return Err(ErrorCode::Unsupported);
        }
        if let Some(feature) = self.feature()
            && !features.iter().any(|f| f == feature)
        {
            return Err(ErrorCode::Unsupported);
        }
        match self {
            Self::Seek(position) | Self::Thumbnail(position) if *position < 0 => {
                Err(ErrorCode::InvalidArgument)
            }
            Self::SelectTrack { kind, track } => {
                if !["audio", "subtitle"].contains(&kind.as_str()) {
                    return Err(ErrorCode::InvalidArgument);
                }
                validation::id(track)
            }
            _ => Ok(()),
        }
    }
}

/// The complete session snapshot, backend state and features of a provider.
pub(super) async fn read_sessions(
    proxy: &MediaProviderProxy<'_>,
    generation: &str,
) -> Result<(u64, Vec<Session>, String, Vec<String>), ErrorCode> {
    let mut token = String::new();
    let mut tokens = HashSet::new();
    let mut revision = 0;
    let mut sessions = Vec::new();
    let mut total = 0;
    loop {
        let (page,): (SessionPage,) = proxy.get_sessions(&token).await.map_err(normalize_error)?;
        page.validate()?;
        total += page.encoded_len()?;
        if total > 16 * 1024 * 1024 || sessions.len() + page.sessions.len() > 100_000 {
            return Err(ErrorCode::InvalidArgument);
        }
        if page.generation != generation || (revision != 0 && page.revision != revision) {
            return Err(ErrorCode::Conflict);
        }
        revision = page.revision;
        sessions.extend(page.sessions);
        token = page.next_token;
        if token.is_empty() {
            break;
        }
        if !tokens.insert(token.clone()) {
            return Err(ErrorCode::Conflict);
        }
    }
    media::validate_sessions(&sessions)?;
    let backend = proxy.backend_state().await.map_err(normalize_error)?;
    if !["available", "unavailable", "degraded"].contains(&backend.as_str()) {
        return Err(ErrorCode::Unsupported);
    }
    let features = proxy.features().await.map_err(normalize_error)?;
    // Unknown flags are ignored; the daemon only acts on the ones it knows.
    let features = features
        .into_iter()
        .filter(|f| KNOWN_FEATURES.contains(&f.as_str()))
        .collect();
    Ok((revision, sessions, backend, features))
}

// Shared by asynchronous discovery and the final locked admission fence.
fn checked_media_target(
    p: &Provider,
    generation: &str,
    session: &str,
    feature: &str,
) -> Result<String, ErrorCode> {
    if p.info.generation != generation {
        return Err(ErrorCode::Conflict);
    }
    if p.info.runtime_state != "ready" || p.info.backend_state == "unavailable" {
        return Err(ErrorCode::Unavailable);
    }
    if !p.features.iter().any(|f| f == feature) {
        return Err(ErrorCode::Unsupported);
    }
    if !p.sessions.iter().any(|s| s.id == session) {
        return Err(ErrorCode::NotFound);
    }
    Ok(p.owner.clone())
}

impl Core {
    pub(super) async fn watch_media(&self, id: String, owner: String, generation: String) {
        let Ok(proxy) = MediaProviderProxy::builder(&self.connection)
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
        let mut tick = tokio::time::interval(Duration::from_secs(1));
        loop {
            tokio::select! {_=tick.tick()=>{},event=changes.next()=>{if event.is_none(){return;}}}
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
            let result =
                tokio::time::timeout(Duration::from_secs(3), read_sessions(&proxy, &generation))
                    .await
                    .unwrap_or(Err(ErrorCode::Timeout));
            let mut state = self.state.lock().await;
            let Some(p) = state.providers.get_mut(&id) else {
                return;
            };
            if p.owner != owner || p.info.generation != generation {
                return;
            }
            let mut signal = None;
            let mut window_changed = false;
            match result {
                Ok((revision, sessions, backend, features)) => {
                    let cache = p
                        .session_cache
                        .get_or_insert_with(|| SessionSnapshots::new(&generation).unwrap());
                    match cache.import(revision, sessions.clone()) {
                        Ok(()) => {
                            if revision != p.info.revision {
                                signal = Some(revision);
                            }
                            window_changed = revision != p.info.revision
                                || backend != p.info.backend_state
                                || features != p.features
                                || p.info.runtime_state != "ready";
                            let valid_sessions = if backend == "unavailable"
                                || !features
                                    .iter()
                                    .any(|f| f == arinn_protocol::media_window::FEATURE)
                            {
                                &[][..]
                            } else {
                                sessions.as_slice()
                            };
                            if window_changed {
                                self.windows
                                    .sync_media_sessions(
                                        &crate::windows::Scope {
                                            provider: id.clone(),
                                            owner: owner.clone(),
                                            registration: generation.clone(),
                                        },
                                        valid_sessions,
                                    )
                                    .await;
                            }
                            p.sessions = sessions;
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
            let sessions = (!p.sessions.is_empty() || p.info.runtime_state == "ready")
                .then(|| p.sessions.clone());
            drop(state);
            if let Some(revision) = signal {
                if let Ok(emitter) = SignalEmitter::new(&self.connection, production::PATH) {
                    let _ =
                        Daemon::state_changed(&emitter, &id, "media", &generation, revision).await;
                }
                self.changed().await;
            }
            if window_changed
                && let Some(p) = self.state.lock().await.providers.get(&id)
                && let Ok(emitter) = SignalEmitter::new(&self.connection, p.info.path.clone())
            {
                let _ = MediaWindowEndpoint::changed(&emitter, &generation).await;
            }
            // Library-linked sessions report progress on every import and
            // tick (ADR-0061).
            if let Some(sessions) = sessions {
                self.forward_progress(&id, &sessions).await;
            }
        }
    }

    /// The provider's owner after checking generation, readiness, the session
    /// and the feature flag an extension read needs.
    async fn media_target(
        &self,
        id: &str,
        generation: &str,
        session: &str,
        feature: &str,
    ) -> Result<String, ErrorCode> {
        validation::id(session)?;
        let state = self.state.lock().await;
        let p = state.providers.get(id).ok_or(ErrorCode::NotFound)?;
        checked_media_target(p, generation, session, feature)
    }

    pub(super) async fn media_tracks(
        &self,
        id: &str,
        generation: &str,
        session: &str,
    ) -> Result<Vec<Track>, ErrorCode> {
        let owner = self
            .media_target(id, generation, session, FEATURE_TRACKS)
            .await?;
        let proxy = MediaTracksProxy::builder(&self.connection)
            .destination(owner.as_str())
            .map_err(|_| ErrorCode::Internal)?
            .build()
            .await
            .map_err(|_| ErrorCode::Unavailable)?;
        let tracks = tokio::time::timeout(
            MEDIA_COMMAND_TIMEOUT,
            proxy.list_tracks(generation, session),
        )
        .await
        .map_err(|_| ErrorCode::Timeout)?
        .map_err(normalize_error)?;
        media::validate_tracks(&tracks)?;
        Ok(tracks)
    }

    pub(super) async fn media_timeline(
        &self,
        id: &str,
        generation: &str,
        session: &str,
    ) -> Result<Timeline, ErrorCode> {
        let owner = self
            .media_target(id, generation, session, FEATURE_TIMELINE)
            .await?;
        let proxy = MediaTimelineProxy::builder(&self.connection)
            .destination(owner.as_str())
            .map_err(|_| ErrorCode::Internal)?
            .build()
            .await
            .map_err(|_| ErrorCode::Unavailable)?;
        let (timeline,) = tokio::time::timeout(
            MEDIA_COMMAND_TIMEOUT,
            proxy.get_timeline(generation, session),
        )
        .await
        .map_err(|_| ErrorCode::Timeout)?
        .map_err(normalize_error)?;
        timeline.validate()?;
        Ok(timeline)
    }

    /// Admit a media command as an operation and run it against the
    /// provider. Mutating commands are never retried. A thumbnail's asset id
    /// is returned as the operation's result (ADR-0058).
    pub(super) async fn media_command(
        &self,
        id: &str,
        action: MediaAction,
        generation: &str,
        session: &str,
        request_id: &str,
        caller: &str,
    ) -> Result<OwnedObjectPath, ErrorCode> {
        validation::id(session)?;
        let fingerprint = serde_json::to_string(&(id, &action, generation, session))
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
        if p.capability() != "media" {
            return Err(ErrorCode::Unsupported);
        }
        if p.info.generation != generation {
            return Err(ErrorCode::Conflict);
        }
        if p.info.runtime_state != "ready" || p.info.backend_state == "unavailable" {
            return Err(ErrorCode::Unavailable);
        }
        let entry = p
            .sessions
            .iter()
            .find(|s| s.id == session)
            .ok_or(ErrorCode::NotFound)?;
        action.check(entry, &p.features)?;
        let label = Label::new(action.kind()).provider("media", &p.info.plugin_id);
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
        let (id, generation, session) = (id.to_owned(), generation.to_owned(), session.to_owned());
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
                let base = || async {
                    MediaProviderProxy::builder(&core.connection)
                        .destination(owner.as_str())
                        .map_err(|_| ErrorCode::Internal)?
                        .build()
                        .await
                        .map_err(|_| ErrorCode::Unavailable)
                };
                let (g, s) = (generation.as_str(), session.as_str());
                let mut asset = None;
                match &action {
                    MediaAction::Play => base().await?.play(g, s).await,
                    MediaAction::Pause => base().await?.pause(g, s).await,
                    MediaAction::PlayPause => base().await?.play_pause(g, s).await,
                    MediaAction::Seek(position) => base().await?.seek(g, s, *position).await,
                    MediaAction::Next => base().await?.next(g, s).await,
                    MediaAction::Previous => base().await?.previous(g, s).await,
                    MediaAction::Stop => base().await?.stop(g, s).await,
                    MediaAction::SelectTrack { kind, track } => {
                        MediaTracksProxy::builder(&core.connection)
                            .destination(owner.as_str())
                            .map_err(|_| ErrorCode::Internal)?
                            .build()
                            .await
                            .map_err(|_| ErrorCode::Unavailable)?
                            .select_track(g, s, kind, track)
                            .await
                    }
                    MediaAction::DisableSubtitles => {
                        MediaTracksProxy::builder(&core.connection)
                            .destination(owner.as_str())
                            .map_err(|_| ErrorCode::Internal)?
                            .build()
                            .await
                            .map_err(|_| ErrorCode::Unavailable)?
                            .disable_subtitles(g, s)
                            .await
                    }
                    MediaAction::Thumbnail(position) => {
                        MediaTimelineProxy::builder(&core.connection)
                            .destination(owner.as_str())
                            .map_err(|_| ErrorCode::Internal)?
                            .build()
                            .await
                            .map_err(|_| ErrorCode::Unavailable)?
                            .request_thumbnail(g, s, *position)
                            .await
                            .map(|(thumbnail,)| {
                                asset = Some(thumbnail.asset);
                            })
                    }
                }
                .map_err(normalize_error)?;
                Ok::<_, ErrorCode>(asset)
            };
            let result = tokio::time::timeout(
                MEDIA_COMMAND_TIMEOUT,
                op.phase("provider.call", op.kind(), work),
            )
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
            let result = result.and_then(|asset| match asset {
                Some(asset) if validation::id(&asset).is_err() => Err(ErrorCode::InvalidArgument),
                Some(asset) => Ok(vec![EntityRef {
                    provider_id: id.clone(),
                    capability: "media".into(),
                    local_id: asset,
                }]),
                None => Ok(vec![EntityRef {
                    provider_id: id.clone(),
                    capability: "media".into(),
                    local_id: session.clone(),
                }]),
            });
            op.finish(&core.connection, &path_copy, result).await;
        });
        Ok(path)
    }
}

pub(super) struct MediaEndpoint {
    pub(super) core: Core,
    pub(super) id: String,
}
#[zbus::interface(name = "org.arinn.Media1")]
impl MediaEndpoint {
    async fn get_sessions(
        &self,
        token: &str,
        #[zbus(header)] h: Header<'_>,
    ) -> production::Result<(SessionPage,)> {
        let owner = sender(&h)?;
        let mut state = self.core.state.lock().await;
        let p = state
            .providers
            .get_mut(&self.id)
            .ok_or(ErrorCode::NotFound)?;
        Ok((p
            .session_cache
            .as_mut()
            .ok_or(ErrorCode::Unavailable)?
            .page(&owner, token, Instant::now())?,))
    }
    async fn get_session(&self, generation: &str, id: &str) -> production::Result<(Session,)> {
        let state = self.core.state.lock().await;
        let p = state.providers.get(&self.id).ok_or(ErrorCode::NotFound)?;
        if p.info.generation != generation {
            return Err(ErrorCode::Conflict.into());
        }
        Ok((p
            .sessions
            .iter()
            .find(|s| s.id == id)
            .cloned()
            .ok_or(ErrorCode::NotFound)?,))
    }
    async fn features(&self) -> production::Result<Vec<String>> {
        let state = self.core.state.lock().await;
        let p = state.providers.get(&self.id).ok_or(ErrorCode::NotFound)?;
        Ok(p.features.clone())
    }
    async fn play(
        &self,
        generation: &str,
        id: &str,
        request_id: &str,
        #[zbus(header)] h: Header<'_>,
    ) -> production::Result<OwnedObjectPath> {
        self.command(MediaAction::Play, generation, id, request_id, &h)
            .await
    }
    async fn pause(
        &self,
        generation: &str,
        id: &str,
        request_id: &str,
        #[zbus(header)] h: Header<'_>,
    ) -> production::Result<OwnedObjectPath> {
        self.command(MediaAction::Pause, generation, id, request_id, &h)
            .await
    }
    async fn play_pause(
        &self,
        generation: &str,
        id: &str,
        request_id: &str,
        #[zbus(header)] h: Header<'_>,
    ) -> production::Result<OwnedObjectPath> {
        self.command(MediaAction::PlayPause, generation, id, request_id, &h)
            .await
    }
    async fn seek(
        &self,
        generation: &str,
        id: &str,
        position_us: i64,
        request_id: &str,
        #[zbus(header)] h: Header<'_>,
    ) -> production::Result<OwnedObjectPath> {
        self.command(
            MediaAction::Seek(position_us),
            generation,
            id,
            request_id,
            &h,
        )
        .await
    }
    async fn next(
        &self,
        generation: &str,
        id: &str,
        request_id: &str,
        #[zbus(header)] h: Header<'_>,
    ) -> production::Result<OwnedObjectPath> {
        self.command(MediaAction::Next, generation, id, request_id, &h)
            .await
    }
    async fn previous(
        &self,
        generation: &str,
        id: &str,
        request_id: &str,
        #[zbus(header)] h: Header<'_>,
    ) -> production::Result<OwnedObjectPath> {
        self.command(MediaAction::Previous, generation, id, request_id, &h)
            .await
    }
    async fn stop(
        &self,
        generation: &str,
        id: &str,
        request_id: &str,
        #[zbus(header)] h: Header<'_>,
    ) -> production::Result<OwnedObjectPath> {
        self.command(MediaAction::Stop, generation, id, request_id, &h)
            .await
    }
    async fn list_tracks(&self, generation: &str, id: &str) -> production::Result<Vec<Track>> {
        Ok(self.core.media_tracks(&self.id, generation, id).await?)
    }
    async fn select_track(
        &self,
        generation: &str,
        id: &str,
        kind: &str,
        track_id: &str,
        request_id: &str,
        #[zbus(header)] h: Header<'_>,
    ) -> production::Result<OwnedObjectPath> {
        let action = MediaAction::SelectTrack {
            kind: kind.into(),
            track: track_id.into(),
        };
        self.command(action, generation, id, request_id, &h).await
    }
    async fn disable_subtitles(
        &self,
        generation: &str,
        id: &str,
        request_id: &str,
        #[zbus(header)] h: Header<'_>,
    ) -> production::Result<OwnedObjectPath> {
        self.command(
            MediaAction::DisableSubtitles,
            generation,
            id,
            request_id,
            &h,
        )
        .await
    }
    async fn get_timeline(&self, generation: &str, id: &str) -> production::Result<(Timeline,)> {
        Ok((self.core.media_timeline(&self.id, generation, id).await?,))
    }
    async fn request_thumbnail(
        &self,
        generation: &str,
        id: &str,
        position_us: i64,
        request_id: &str,
        #[zbus(header)] h: Header<'_>,
    ) -> production::Result<OwnedObjectPath> {
        self.command(
            MediaAction::Thumbnail(position_us),
            generation,
            id,
            request_id,
            &h,
        )
        .await
    }
}
impl MediaEndpoint {
    async fn command(
        &self,
        action: MediaAction,
        generation: &str,
        session: &str,
        request_id: &str,
        h: &Header<'_>,
    ) -> production::Result<OwnedObjectPath> {
        Ok(self
            .core
            .media_command(
                &self.id,
                action,
                generation,
                session,
                request_id,
                &sender(h)?,
            )
            .await?)
    }
}

#[cfg(test)]
#[path = "media_tests.rs"]
pub(super) mod tests;

pub(super) type ExitFences = BTreeMap<(String, String, String), u64>;

pub(super) struct MediaWindowEndpoint {
    pub(super) core: Core,
    pub(super) id: String,
}
impl Core {
    async fn playback_window(
        &self,
        id: &str,
        generation: &str,
        session: &str,
    ) -> Result<
        (
            String,
            String,
            Vec<arinn_protocol::return_association::WindowRef>,
        ),
        ErrorCode,
    > {
        use arinn_protocol::media_window::{self, MediaWindowProviderProxy};
        media_window::validate_session(generation, session)?;
        let owner = self
            .media_target(id, generation, session, media_window::FEATURE)
            .await?;
        let platform = self.windows.current_generation().await;
        let proxy = MediaWindowProviderProxy::builder(&self.connection)
            .destination(owner.as_str())
            .map_err(|_| ErrorCode::Internal)?
            .build()
            .await
            .map_err(|_| ErrorCode::Unavailable)?;
        let launch = proxy
            .get_launch_id(generation, session)
            .await
            .map_err(normalize_error)?;
        media_window::validate_launch(&launch)?;
        if self
            .media_target(id, generation, session, media_window::FEATURE)
            .await?
            != owner
        {
            return Err(ErrorCode::Conflict);
        }
        // Hold the provider lifetime fence through the final authoritative
        // inventory read. No later await may return a cached window reference.
        let state = self.state.lock().await;
        let p = state.providers.get(id).ok_or(ErrorCode::NotFound)?;
        if checked_media_target(p, generation, session, media_window::FEATURE)? != owner {
            return Err(ErrorCode::Conflict);
        }
        let windows = self
            .windows
            .media_window(&self.connection, &launch, &platform)
            .await?;
        drop(state);
        Ok((owner, launch, windows))
    }
    async fn playback_command(
        &self,
        id: &str,
        generation: &str,
        session: &str,
        request_id: &str,
        caller: &str,
        present: bool,
    ) -> Result<OwnedObjectPath, ErrorCode> {
        use arinn_protocol::media_window;
        media_window::validate_session(generation, session)?;
        let fence_key = (id.to_owned(), generation.to_owned(), session.to_owned());
        // Capture ordering before any admission/provider/platform awaits.
        let present_fence = if present {
            self.playback_exits
                .lock()
                .await
                .get(&fence_key)
                .copied()
                .unwrap_or(0)
        } else {
            0
        };
        let fingerprint =
            serde_json::to_string(&("media-window", id, generation, session, present))
                .map_err(|_| ErrorCode::Internal)?;
        if let Some(path) = self
            .operations
            .existing(caller, request_id, &fingerprint)
            .await?
        {
            return Ok(path);
        }
        let kind = if present {
            "media.present"
        } else {
            "media.release"
        };
        let rejected = |error: ErrorCode, reason: &'static str| {
            log_rejection(id, request_id, kind, error, reason);
            error
        };
        let owner = self
            .media_target(id, generation, session, media_window::FEATURE)
            .await
            .map_err(|e| rejected(e, "session_unavailable"))?;
        self.windows
            .media_supported(&self.connection)
            .await
            .map_err(|e| {
                rejected(
                    e,
                    if e == ErrorCode::Unsupported {
                        "media_channel_absent"
                    } else {
                        "platform_unavailable"
                    },
                )
            })?;
        let (launch, window) = if present {
            {
                let state = self.state.lock().await;
                let p = state.providers.get(id).ok_or(ErrorCode::NotFound)?;
                let s = p
                    .sessions
                    .iter()
                    .find(|s| s.id == session)
                    .ok_or(ErrorCode::NotFound)?;
                if !["playing", "paused", "ended"].contains(&s.state.as_str()) {
                    return Err(ErrorCode::Unsupported);
                }
            }
            let (resolved_owner, launch, windows) = tokio::time::timeout(
                Duration::from_secs(3),
                self.playback_window(id, generation, session),
            )
            .await
            .map_err(|_| rejected(ErrorCode::Timeout, "window_lookup_timeout"))?
            .map_err(|e| rejected(e, "window_lookup_failed"))?;
            if resolved_owner != owner {
                return Err(rejected(ErrorCode::Conflict, "owner_changed"));
            }
            let window = windows
                .into_iter()
                .next()
                .ok_or_else(|| rejected(ErrorCode::NotFound, "no_matching_window"))?;
            (launch, Some(window))
        } else {
            (String::new(), None)
        };
        let state = self.state.lock().await;
        let p = state.providers.get(id).ok_or(ErrorCode::NotFound)?;
        if p.owner != owner || p.info.generation != generation {
            return Err(rejected(ErrorCode::Conflict, "generation_changed"));
        }
        if present {
            checked_media_target(p, generation, session, media_window::FEATURE)?;
        }
        let entry = p
            .sessions
            .iter()
            .find(|s| s.id == session)
            .ok_or(ErrorCode::NotFound)?;
        if present && !["playing", "paused", "ended"].contains(&entry.state.as_str()) {
            return Err(ErrorCode::Unsupported);
        }
        let mut exits = self.playback_exits.lock().await;
        if present && exits.get(&fence_key).copied().unwrap_or(0) != present_fence {
            return Err(ErrorCode::Conflict);
        }
        let release_fence = if present {
            None
        } else {
            if !exits.contains_key(&fence_key) && exits.len() >= 100_000 {
                return Err(ErrorCode::Unavailable);
            }
            Some(
                exits
                    .get(&fence_key)
                    .copied()
                    .unwrap_or(0)
                    .checked_add(1)
                    .ok_or(ErrorCode::Unavailable)?,
            )
        };
        let kind = if present {
            "media.present"
        } else {
            "media.release"
        };
        let label = Label::new(kind).provider("media", &p.info.plugin_id);
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
        if !fresh {
            return Ok(path);
        }
        // Only an admitted Release cancels an older Present still waiting
        // for identity. Hold the same fence lock through admission and enqueue.
        if let Some(next) = release_fence {
            exits.insert(fence_key, next);
        }
        let scope = crate::windows::Scope {
            provider: id.into(),
            owner: owner.clone(),
            registration: generation.into(),
        };
        let queued = self
            .windows
            .enqueue_media(
                &self.connection,
                scope,
                session,
                &launch,
                window.as_ref(),
                present,
            )
            .await;
        drop(exits);
        drop(state);
        let core = self.clone();
        let (id, generation, session, path_copy) = (
            id.to_owned(),
            generation.to_owned(),
            session.to_owned(),
            path.clone(),
        );
        tokio::spawn(async move {
            {
                let mut value = op.value.lock().await;
                if value.state != "queued" {
                    return;
                }
                value.state = "running".into();
                value.started_at = now();
            }
            let result = match queued {
                Ok(Some(receive)) => {
                    op.phase("platform.wait", op.kind(), platform_wait(&op, receive))
                        .await
                }
                Ok(None) => Ok(()),
                Err(error) => Err(error),
            };
            if let Err(error) = result {
                let poller_live = core.windows.media_poller_live().await;
                if let Some(reason) = playback_failure_reason(error, poller_live) {
                    op.reason(reason);
                }
            }
            let state = core.state.lock().await;
            let valid = state.providers.get(&id).is_some_and(|p| {
                p.owner == owner
                    && p.info.generation == generation
                    && p.sessions.iter().any(|s| s.id == session)
            });
            drop(state);
            let result = if valid {
                result
            } else {
                op.reason("provider_changed");
                Err(ErrorCode::Unavailable)
            };
            op.finish(
                &core.connection,
                &path_copy,
                result.map(|_| {
                    vec![EntityRef {
                        provider_id: id,
                        capability: "media".into(),
                        local_id: session,
                    }]
                }),
            )
            .await;
        });
        Ok(path)
    }
}
/// Logs why a playback window command was refused before it became an
/// operation (#369). Fixed fields only: no title, URL, path or window identity.
fn log_rejection(
    provider_id: &str,
    request_id: &str,
    kind: &'static str,
    error: ErrorCode,
    reason: &'static str,
) {
    eprintln!(
        "{}",
        serde_json::json!({"level":"info","component":"arinnd","provider_id":provider_id,"request_id":request_id,"operation":"rejected","kind":kind,"error":error.to_string(),"reason":reason})
    );
}
/// The normalized reason of a failed Present or Release, from the platform
/// adapter's answer and whether it is still polling the media channel.
pub(super) fn playback_failure_reason(error: ErrorCode, poller_live: bool) -> Option<&'static str> {
    match error {
        ErrorCode::Timeout if !poller_live => Some("platform_not_polling"),
        ErrorCode::NotFound => Some("platform_window_missing"),
        ErrorCode::Conflict => Some("launch_mismatch"),
        ErrorCode::PermissionDenied => Some("session_locked"),
        ErrorCode::Cancelled => Some("platform_cancelled"),
        _ => None,
    }
}
#[zbus::interface(name = "org.arinn.MediaWindow1")]
impl MediaWindowEndpoint {
    async fn get_window(
        &self,
        generation: &str,
        id: &str,
    ) -> production::Result<Vec<arinn_protocol::return_association::WindowRef>> {
        Ok(tokio::time::timeout(
            Duration::from_secs(3),
            self.core.playback_window(&self.id, generation, id),
        )
        .await
        .map_err(|_| ErrorCode::Timeout)??
        .2)
    }
    async fn present(
        &self,
        generation: &str,
        id: &str,
        request_id: &str,
        #[zbus(header)] h: Header<'_>,
    ) -> production::Result<OwnedObjectPath> {
        Ok(self
            .core
            .playback_command(&self.id, generation, id, request_id, &sender(&h)?, true)
            .await?)
    }
    async fn release(
        &self,
        generation: &str,
        id: &str,
        request_id: &str,
        #[zbus(header)] h: Header<'_>,
    ) -> production::Result<OwnedObjectPath> {
        Ok(self
            .core
            .playback_command(&self.id, generation, id, request_id, &sender(&h)?, false)
            .await?)
    }
    #[zbus(signal)]
    pub(super) async fn changed(emitter: &SignalEmitter<'_>, generation: &str) -> zbus::Result<()>;
}
