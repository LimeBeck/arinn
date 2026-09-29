// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! `org.arinn.MediaProvider1` and the optional rich interfaces (ADR-0054).
//! File URIs are accepted on `MediaOpen1` and never logged.
use super::{Endpoint, RegistrationContext, backend_state, run_endpoint};
use crate::snapshots::SessionSnapshots;
use arinn_protocol::{
    media::{
        self, FEATURE_OPEN, FEATURE_TIMELINE, FEATURE_TRACKS, MediaSource, Session, SessionPage,
        Thumbnail, Timeline, Track,
    },
    production::{self, PROVIDER_PATH},
    validation::{self, ErrorCode},
};
use futures_util::Future;
use std::{
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Instant,
};
use tokio::sync::Mutex;
use zbus::{message::Header, object_server::SignalEmitter};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MediaAction {
    Play,
    Pause,
    PlayPause,
    Seek,
    Next,
    Previous,
    Stop,
}
impl MediaAction {
    fn token(self) -> &'static str {
        match self {
            Self::Play => media::ACTION_PLAY,
            Self::Pause => media::ACTION_PAUSE,
            Self::PlayPause => media::ACTION_PLAY_PAUSE,
            Self::Seek => media::ACTION_SEEK,
            Self::Next => media::ACTION_NEXT,
            Self::Previous => media::ACTION_PREVIOUS,
            Self::Stop => media::ACTION_STOP,
        }
    }
}

pub trait MediaHandler: Send + Sync + 'static {
    fn control(
        &self,
        action: MediaAction,
        id: String,
        position_us: i64,
    ) -> Pin<Box<dyn Future<Output = Result<(), ErrorCode>> + Send + '_>>;
}
pub trait MediaOpenHandler: Send + Sync + 'static {
    fn open(
        &self,
        source: MediaSource,
    ) -> Pin<Box<dyn Future<Output = Result<String, ErrorCode>> + Send + '_>>;
}
pub trait MediaTracksHandler: Send + Sync + 'static {
    fn list(
        &self,
        id: String,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<Track>, ErrorCode>> + Send + '_>>;
    fn select(
        &self,
        id: String,
        kind: String,
        track_id: String,
    ) -> Pin<Box<dyn Future<Output = Result<(), ErrorCode>> + Send + '_>>;
    fn disable_subtitles(
        &self,
        id: String,
    ) -> Pin<Box<dyn Future<Output = Result<(), ErrorCode>> + Send + '_>>;
}
pub trait MediaTimelineHandler: Send + Sync + 'static {
    fn timeline(
        &self,
        id: String,
    ) -> Pin<Box<dyn Future<Output = Result<Timeline, ErrorCode>> + Send + '_>>;
    fn thumbnail(
        &self,
        id: String,
        position_us: i64,
    ) -> Pin<Box<dyn Future<Output = Result<Thumbnail, ErrorCode>> + Send + '_>>;
}

#[derive(Clone, Default)]
pub struct MediaExtensions {
    open: Option<Arc<dyn MediaOpenHandler>>,
    tracks: Option<Arc<dyn MediaTracksHandler>>,
    timeline: Option<Arc<dyn MediaTimelineHandler>>,
}
impl MediaExtensions {
    pub fn open(mut self, handler: impl MediaOpenHandler) -> Self {
        self.open = Some(Arc::new(handler));
        self
    }
    pub fn tracks(mut self, handler: impl MediaTracksHandler) -> Self {
        self.tracks = Some(Arc::new(handler));
        self
    }
    pub fn timeline(mut self, handler: impl MediaTimelineHandler) -> Self {
        self.timeline = Some(Arc::new(handler));
        self
    }
}

struct Data {
    daemon_owner: String,
    generation: String,
    snapshots: Option<SessionSnapshots>,
    sessions: Vec<Session>,
    backend: String,
    revision: u64,
}

/// Base media provider. Rich interfaces are exported only when `extensions`
/// carries a handler, so an MPRIS provider does not offer tracks or thumbnails.
#[derive(Clone)]
pub struct MediaService {
    data: Arc<Mutex<Data>>,
    handler: Arc<dyn MediaHandler>,
    extensions: MediaExtensions,
    registrations: tokio::sync::watch::Sender<Option<RegistrationContext>>,
    running: Arc<AtomicBool>,
    updates: tokio::sync::watch::Sender<(String, u64)>,
}

impl MediaService {
    pub fn new(
        handler: impl MediaHandler,
        sessions: Vec<Session>,
        backend: &str,
    ) -> Result<Self, ErrorCode> {
        Self::with_extensions(handler, sessions, backend, MediaExtensions::default())
    }

    pub fn with_extensions(
        handler: impl MediaHandler,
        sessions: Vec<Session>,
        backend: &str,
        extensions: MediaExtensions,
    ) -> Result<Self, ErrorCode> {
        media::validate_sessions(&sessions)?;
        backend_state(backend)?;
        Ok(Self {
            data: Arc::new(Mutex::new(Data {
                daemon_owner: String::new(),
                generation: String::new(),
                snapshots: None,
                sessions,
                backend: backend.into(),
                revision: 0,
            })),
            handler: Arc::new(handler),
            extensions,
            registrations: tokio::sync::watch::channel(None).0,
            running: Arc::new(AtomicBool::new(false)),
            updates: tokio::sync::watch::channel((String::new(), 0)).0,
        })
    }

    pub fn registrations(&self) -> tokio::sync::watch::Receiver<Option<RegistrationContext>> {
        self.registrations.subscribe()
    }

    pub async fn replace(&self, sessions: Vec<Session>, backend: &str) -> Result<(), ErrorCode> {
        media::validate_sessions(&sessions)?;
        backend_state(backend)?;
        let (generation, revision) = {
            let mut data = self.data.lock().await;
            let backend_changed = data.backend != backend;
            let before = data.revision;
            let revision = if let Some(snapshots) = data.snapshots.as_mut() {
                let mut revision = snapshots.publish(sessions.clone())?;
                if backend_changed && revision == before {
                    revision = snapshots.touch()?;
                }
                revision
            } else {
                0
            };
            if revision > 0 {
                data.revision = revision;
            }
            data.sessions = sessions;
            data.backend = backend.into();
            (data.generation.clone(), revision)
        };
        if revision > 0 {
            self.updates.send_replace((generation, revision));
        }
        Ok(())
    }

    /// Publish a new revision without changing sessions, for track or timeline
    /// changes the base snapshot does not carry.
    pub async fn touch(&self) -> Result<(), ErrorCode> {
        let (generation, revision) = {
            let mut data = self.data.lock().await;
            let revision = data
                .snapshots
                .as_mut()
                .ok_or(ErrorCode::Unavailable)?
                .touch()?;
            data.revision = revision;
            (data.generation.clone(), revision)
        };
        self.updates.send_replace((generation, revision));
        Ok(())
    }

    fn feature_names(&self) -> Vec<String> {
        let mut names = Vec::new();
        if self.extensions.open.is_some() {
            names.push(FEATURE_OPEN.into());
        }
        if self.extensions.tracks.is_some() {
            names.push(FEATURE_TRACKS.into());
        }
        if self.extensions.timeline.is_some() {
            names.push(FEATURE_TIMELINE.into());
        }
        names
    }

    async fn authorized(&self, header: &Header<'_>) -> production::Result<()> {
        let data = self.data.lock().await;
        if !self.running.load(Ordering::Acquire)
            || data.daemon_owner.is_empty()
            || header.sender().map(|s| s.as_str()) != Some(&data.daemon_owner)
        {
            return Err(ErrorCode::PermissionDenied.into());
        }
        Ok(())
    }

    async fn ready(&self, generation: &str, header: &Header<'_>) -> production::Result<()> {
        self.authorized(header).await?;
        let data = self.data.lock().await;
        if data.generation != generation {
            return Err(ErrorCode::Conflict.into());
        }
        if data.backend == "unavailable" {
            return Err(ErrorCode::Unavailable.into());
        }
        Ok(())
    }

    async fn session(&self, generation: &str, id: &str) -> production::Result<Session> {
        let data = self.data.lock().await;
        if data.generation != generation {
            return Err(ErrorCode::Conflict.into());
        }
        data.sessions
            .iter()
            .find(|session| session.id == id)
            .cloned()
            .ok_or(ErrorCode::NotFound.into())
    }

    async fn control(
        &self,
        action: MediaAction,
        generation: &str,
        id: &str,
        position_us: i64,
        header: Header<'_>,
    ) -> production::Result<()> {
        validation::id(id)?;
        if action == MediaAction::Seek && position_us < 0 {
            return Err(ErrorCode::InvalidArgument.into());
        }
        self.ready(generation, &header).await?;
        let session = self.session(generation, id).await?;
        if !session.actions.iter().any(|name| name == action.token()) {
            return Err(ErrorCode::Unsupported.into());
        }
        if action == MediaAction::Seek {
            media::position_us(position_us, session.duration_us)?;
        }
        self.handler
            .control(action, id.into(), position_us)
            .await
            .map_err(Into::into)
    }
}

#[zbus::interface(name = "org.arinn.MediaProvider1")]
impl MediaService {
    async fn get_sessions(
        &self,
        token: &str,
        #[zbus(header)] header: Header<'_>,
    ) -> production::Result<(SessionPage,)> {
        self.authorized(&header).await?;
        let owner = header.sender().ok_or(ErrorCode::PermissionDenied)?.as_str();
        Ok((self
            .data
            .lock()
            .await
            .snapshots
            .as_mut()
            .ok_or(ErrorCode::Unavailable)?
            .page(owner, token, Instant::now())?,))
    }

    async fn get_session(
        &self,
        generation: &str,
        id: &str,
        #[zbus(header)] header: Header<'_>,
    ) -> production::Result<(Session,)> {
        validation::id(id)?;
        self.authorized(&header).await?;
        Ok((self.session(generation, id).await?,))
    }

    async fn backend_state(
        &self,
        #[zbus(header)] header: Header<'_>,
    ) -> production::Result<String> {
        self.authorized(&header).await?;
        Ok(self.data.lock().await.backend.clone())
    }

    async fn features(
        &self,
        #[zbus(header)] header: Header<'_>,
    ) -> production::Result<Vec<String>> {
        self.authorized(&header).await?;
        Ok(self.feature_names())
    }

    async fn play(
        &self,
        generation: &str,
        id: &str,
        #[zbus(header)] header: Header<'_>,
    ) -> production::Result<()> {
        self.control(MediaAction::Play, generation, id, 0, header)
            .await
    }
    async fn pause(
        &self,
        generation: &str,
        id: &str,
        #[zbus(header)] header: Header<'_>,
    ) -> production::Result<()> {
        self.control(MediaAction::Pause, generation, id, 0, header)
            .await
    }
    async fn play_pause(
        &self,
        generation: &str,
        id: &str,
        #[zbus(header)] header: Header<'_>,
    ) -> production::Result<()> {
        self.control(MediaAction::PlayPause, generation, id, 0, header)
            .await
    }
    async fn seek(
        &self,
        generation: &str,
        id: &str,
        position_us: i64,
        #[zbus(header)] header: Header<'_>,
    ) -> production::Result<()> {
        self.control(MediaAction::Seek, generation, id, position_us, header)
            .await
    }
    async fn next(
        &self,
        generation: &str,
        id: &str,
        #[zbus(header)] header: Header<'_>,
    ) -> production::Result<()> {
        self.control(MediaAction::Next, generation, id, 0, header)
            .await
    }
    async fn previous(
        &self,
        generation: &str,
        id: &str,
        #[zbus(header)] header: Header<'_>,
    ) -> production::Result<()> {
        self.control(MediaAction::Previous, generation, id, 0, header)
            .await
    }
    async fn stop(
        &self,
        generation: &str,
        id: &str,
        #[zbus(header)] header: Header<'_>,
    ) -> production::Result<()> {
        self.control(MediaAction::Stop, generation, id, 0, header)
            .await
    }

    #[zbus(signal)]
    async fn state_changed(
        emitter: &SignalEmitter<'_>,
        generation: &str,
        revision: u64,
    ) -> zbus::Result<()>;
}

#[derive(Clone)]
struct OpenIface(MediaService);
#[zbus::interface(name = "org.arinn.MediaOpen1")]
impl OpenIface {
    async fn open(
        &self,
        generation: &str,
        source: MediaSource,
        #[zbus(header)] header: Header<'_>,
    ) -> production::Result<String> {
        source.validate()?;
        self.0.ready(generation, &header).await?;
        let id = self
            .0
            .extensions
            .open
            .as_ref()
            .ok_or(ErrorCode::Unsupported)?
            .open(source)
            .await?;
        validation::id(&id).map_err(|_| ErrorCode::Internal)?;
        Ok(id)
    }
}

#[derive(Clone)]
struct TracksIface(MediaService);
#[zbus::interface(name = "org.arinn.MediaTracks1")]
impl TracksIface {
    async fn list_tracks(
        &self,
        generation: &str,
        id: &str,
        #[zbus(header)] header: Header<'_>,
    ) -> production::Result<Vec<Track>> {
        validation::id(id)?;
        self.0.ready(generation, &header).await?;
        self.0.session(generation, id).await?;
        let tracks = self
            .0
            .extensions
            .tracks
            .as_ref()
            .ok_or(ErrorCode::Unsupported)?
            .list(id.into())
            .await?;
        media::validate_tracks(&tracks).map_err(|_| ErrorCode::Internal)?;
        Ok(tracks)
    }

    async fn select_track(
        &self,
        generation: &str,
        id: &str,
        kind: &str,
        track_id: &str,
        #[zbus(header)] header: Header<'_>,
    ) -> production::Result<()> {
        validation::id(id)?;
        validation::id(track_id)?;
        if kind != media::TRACK_AUDIO && kind != media::TRACK_SUBTITLE {
            return Err(ErrorCode::InvalidArgument.into());
        }
        self.0.ready(generation, &header).await?;
        self.0.session(generation, id).await?;
        self.0
            .extensions
            .tracks
            .as_ref()
            .ok_or(ErrorCode::Unsupported)?
            .select(id.into(), kind.into(), track_id.into())
            .await
            .map_err(Into::into)
    }

    async fn disable_subtitles(
        &self,
        generation: &str,
        id: &str,
        #[zbus(header)] header: Header<'_>,
    ) -> production::Result<()> {
        validation::id(id)?;
        self.0.ready(generation, &header).await?;
        self.0.session(generation, id).await?;
        self.0
            .extensions
            .tracks
            .as_ref()
            .ok_or(ErrorCode::Unsupported)?
            .disable_subtitles(id.into())
            .await
            .map_err(Into::into)
    }
}

#[derive(Clone)]
struct TimelineIface(MediaService);
#[zbus::interface(name = "org.arinn.MediaTimeline1")]
impl TimelineIface {
    async fn get_timeline(
        &self,
        generation: &str,
        id: &str,
        #[zbus(header)] header: Header<'_>,
    ) -> production::Result<(Timeline,)> {
        validation::id(id)?;
        self.0.ready(generation, &header).await?;
        self.0.session(generation, id).await?;
        let timeline = self
            .0
            .extensions
            .timeline
            .as_ref()
            .ok_or(ErrorCode::Unsupported)?
            .timeline(id.into())
            .await?;
        timeline.validate().map_err(|_| ErrorCode::Internal)?;
        Ok((timeline,))
    }

    async fn request_thumbnail(
        &self,
        generation: &str,
        id: &str,
        position_us: i64,
        #[zbus(header)] header: Header<'_>,
    ) -> production::Result<(Thumbnail,)> {
        validation::id(id)?;
        self.0.ready(generation, &header).await?;
        let session = self.0.session(generation, id).await?;
        media::position_us(position_us, session.duration_us)?;
        let thumbnail = self
            .0
            .extensions
            .timeline
            .as_ref()
            .ok_or(ErrorCode::Unsupported)?
            .thumbnail(id.into(), position_us)
            .await?;
        thumbnail.validate().map_err(|_| ErrorCode::Internal)?;
        Ok((thumbnail,))
    }
}

impl Endpoint for MediaService {
    const CAPABILITY: &'static str = "media";
    fn registrations(&self) -> &tokio::sync::watch::Sender<Option<RegistrationContext>> {
        &self.registrations
    }
    fn running(&self) -> &Arc<AtomicBool> {
        &self.running
    }
    fn updates(&self) -> &tokio::sync::watch::Sender<(String, u64)> {
        &self.updates
    }
    async fn clear(&self) {
        let mut data = self.data.lock().await;
        data.snapshots = None;
        data.generation.clear();
        data.daemon_owner.clear();
    }
    async fn daemon_owner(&self) -> String {
        self.data.lock().await.daemon_owner.clone()
    }
    async fn set_daemon_owner(&self, owner: String) {
        self.data.lock().await.daemon_owner = owner;
    }
    async fn registered(&self) -> bool {
        self.data.lock().await.snapshots.is_some()
    }
    async fn install(&self, generation: &str) -> Result<(), ErrorCode> {
        let mut data = self.data.lock().await;
        let mut snapshots = SessionSnapshots::new(generation)?;
        data.revision = snapshots.publish(data.sessions.clone())?;
        data.generation = generation.into();
        data.snapshots = Some(snapshots);
        Ok(())
    }
    async fn emit(
        &self,
        connection: &zbus::Connection,
        generation: &str,
        revision: u64,
    ) -> zbus::Result<()> {
        let emitter = SignalEmitter::new(connection, PROVIDER_PATH)?;
        MediaService::state_changed(&emitter, generation, revision).await
    }
    async fn export(
        &self,
        connection: &zbus::Connection,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if !connection
            .object_server()
            .at(PROVIDER_PATH, self.clone())
            .await?
        {
            return Err("provider endpoint already exported".into());
        }
        if self.extensions.open.is_some() {
            connection
                .object_server()
                .at(PROVIDER_PATH, OpenIface(self.clone()))
                .await?;
        }
        if self.extensions.tracks.is_some() {
            connection
                .object_server()
                .at(PROVIDER_PATH, TracksIface(self.clone()))
                .await?;
        }
        if self.extensions.timeline.is_some() {
            connection
                .object_server()
                .at(PROVIDER_PATH, TimelineIface(self.clone()))
                .await?;
        }
        Ok(())
    }
}

pub async fn run_media(
    service: MediaService,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let connection = zbus::connection::Builder::session()?.build().await?;
    run_media_on(connection, service).await
}

pub async fn run_media_on(
    connection: zbus::Connection,
    service: MediaService,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    run_endpoint(connection, service).await
}
