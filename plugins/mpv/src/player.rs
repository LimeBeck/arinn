// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! One playback session. Reconnecting observes the player and does not send
//! `loadfile` or repeat a command that already failed.

use crate::{
    ipc::{self, Client, Event, Incoming},
    parse,
    scope::{self},
    thumbnail::Extractor,
};
use arinn_plugin_sdk::runtime::MediaAction;
use arinn_protocol::{
    media::{self, MediaSource, Session, Thumbnail, Timeline, Track},
    production::EntityRef,
    validation::ErrorCode,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::HashSet,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{
    sync::{Mutex, mpsc, oneshot, watch},
    task::JoinHandle,
};
use zbus::Connection;

const SEEK_SLACK_US: i64 = 1_500_000;
const RECONNECT: Duration = Duration::from_millis(200);
/// `Open` waits this long for `file-loaded` once the player is reachable.
/// The daemon allows 30 s for a library launch. Unit and socket waits are
/// already finished, so this budget stays inside that limit.
pub const OPEN_LOAD_WAIT: Duration = Duration::from_secs(20);
const CONFIRM_WAIT: Duration = Duration::from_secs(1);
const PROPS: &[(i64, &str)] = &[
    (1, "pause"),
    (2, "time-pos"),
    (3, "duration"),
    (4, "eof-reached"),
    (5, "idle-active"),
    (6, "track-list"),
    (7, "chapter-list"),
    (8, "demuxer-cache-state"),
    (9, "media-title"),
];

#[derive(Clone, Debug)]
pub enum Launch {
    /// The socket is already served. Nothing is spawned.
    External,
    /// Start `app-arinn-mpv.service` on Open when the socket is down.
    Systemd { binary: PathBuf },
    /// mpv is not installed. Open returns `Unavailable`.
    Missing,
}

/// `Systemd` when `binary` exists, otherwise `Missing`.
pub(crate) fn launch_for(binary: &Path) -> Launch {
    if binary.is_file() {
        Launch::Systemd {
            binary: binary.to_path_buf(),
        }
    } else {
        Launch::Missing
    }
}

pub struct Settings {
    pub socket: PathBuf,
    pub state_path: PathBuf,
    pub launch: Launch,
    pub publish_interval: Duration,
    pub ffmpeg: Option<PathBuf>,
    pub thumb_dir: PathBuf,
    pub backend: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct View {
    pub sessions: Vec<Session>,
    pub backend: String,
    pub snapshot: u64,
    pub touch: u64,
}

enum Ask {
    Open(MediaSource, oneshot::Sender<Result<String, ErrorCode>>),
    Control(
        MediaAction,
        String,
        i64,
        oneshot::Sender<Result<(), ErrorCode>>,
    ),
    Tracks(String, oneshot::Sender<Result<Vec<Track>, ErrorCode>>),
    Select(
        String,
        String,
        String,
        oneshot::Sender<Result<(), ErrorCode>>,
    ),
    Disable(String, oneshot::Sender<Result<(), ErrorCode>>),
    Timeline(String, oneshot::Sender<Result<Timeline, ErrorCode>>),
    View(oneshot::Sender<View>),
    LaunchId(String, oneshot::Sender<Result<String, ErrorCode>>),
    Thumb(String, oneshot::Sender<Result<ThumbTarget, ErrorCode>>),
}

struct ThumbTarget {
    path: PathBuf,
}

struct Anchor {
    position_us: i64,
    anchor_us: u64,
    at: Instant,
    state: String,
}

struct Live {
    id: String,
    content: EntityRef,
    has_content: bool,
    path: Option<PathBuf>,
    title: String,
    pause: bool,
    eof: bool,
    stopped: bool,
    time_us: i64,
    duration_us: i64,
    tracks: Vec<Track>,
    timeline: Timeline,
    has_video: bool,
}

#[derive(Serialize, Deserialize)]
struct Stored {
    id: String,
    provider_id: String,
    capability: String,
    local_id: String,
    has_content: bool,
}

struct Core {
    socket: PathBuf,
    state_path: PathBuf,
    launch: Launch,
    interval: Duration,
    incoming_tx: mpsc::UnboundedSender<Incoming>,
    notes: watch::Sender<View>,
    client: Option<Client>,
    generation: u64,
    observed: bool,
    launch_id: String,
    live: Option<Live>,
    idle: bool,
    backend: String,
    snapshot: u64,
    touch: u64,
    file_loaded: bool,
    load_fault: Option<ErrorCode>,
    seen: HashSet<String>,
    force_anchor: bool,
    anchor: Option<Anchor>,
    published: Option<Session>,
    published_tracks: Vec<Track>,
    published_timeline: Timeline,
}

struct Task {
    join: Mutex<Option<JoinHandle<()>>>,
}

impl Drop for Task {
    fn drop(&mut self) {
        if let Some(join) = self.join.get_mut().take() {
            join.abort();
        }
    }
}

#[derive(Clone)]
pub struct Player {
    tx: mpsc::Sender<Ask>,
    notes: watch::Sender<View>,
    ffmpeg: Option<PathBuf>,
    thumb_dir: PathBuf,
    extractor: Arc<Extractor>,
    _task: Arc<Task>,
}

impl Player {
    pub fn start(settings: Settings) -> Result<Self, ErrorCode> {
        let (tx, rx) = mpsc::channel(16);
        let (incoming_tx, incoming_rx) = mpsc::unbounded_channel();
        let view = View {
            sessions: Vec::new(),
            backend: settings.backend.clone(),
            snapshot: 0,
            touch: 0,
        };
        let (notes, _) = watch::channel(view);
        let core = Core {
            socket: settings.socket,
            state_path: settings.state_path,
            launch: settings.launch,
            interval: settings.publish_interval,
            incoming_tx,
            notes: notes.clone(),
            client: None,
            generation: 0,
            observed: false,
            launch_id: String::new(),
            live: None,
            idle: true,
            backend: settings.backend,
            snapshot: 0,
            touch: 0,
            file_loaded: false,
            load_fault: None,
            seen: HashSet::new(),
            force_anchor: false,
            anchor: None,
            published: None,
            published_tracks: Vec::new(),
            published_timeline: Timeline {
                chapters: Vec::new(),
                ranges: Vec::new(),
            },
        };
        let join = tokio::spawn(core.run(rx, incoming_rx));
        Ok(Self {
            tx,
            notes,
            ffmpeg: settings.ffmpeg,
            thumb_dir: settings.thumb_dir,
            extractor: Arc::new(Extractor::default()),
            _task: Arc::new(Task {
                join: Mutex::new(Some(join)),
            }),
        })
    }

    pub fn subscribe(&self) -> watch::Receiver<View> {
        self.notes.subscribe()
    }

    pub async fn open(&self, source: MediaSource) -> Result<String, ErrorCode> {
        self.call(|tx| Ask::Open(source, tx)).await?
    }

    pub async fn control(
        &self,
        action: MediaAction,
        id: String,
        position_us: i64,
    ) -> Result<(), ErrorCode> {
        self.call(|tx| Ask::Control(action, id, position_us, tx))
            .await?
    }

    pub async fn tracks(&self, id: String) -> Result<Vec<Track>, ErrorCode> {
        self.call(|tx| Ask::Tracks(id, tx)).await?
    }

    pub async fn select(
        &self,
        id: String,
        kind: String,
        track_id: String,
    ) -> Result<(), ErrorCode> {
        self.call(|tx| Ask::Select(id, kind, track_id, tx)).await?
    }

    pub async fn disable_subtitles(&self, id: String) -> Result<(), ErrorCode> {
        self.call(|tx| Ask::Disable(id, tx)).await?
    }

    pub async fn launch_id(&self, id: String) -> Result<String, ErrorCode> {
        self.call(|tx| Ask::LaunchId(id, tx)).await?
    }

    pub async fn timeline(&self, id: String) -> Result<Timeline, ErrorCode> {
        self.call(|tx| Ask::Timeline(id, tx)).await?
    }

    pub async fn view(&self) -> Result<View, ErrorCode> {
        self.call(Ask::View).await
    }

    pub async fn thumbnail(&self, id: String, position_us: i64) -> Result<Thumbnail, ErrorCode> {
        let target = self.call(|tx| Ask::Thumb(id, tx)).await??;
        let Some(ffmpeg) = self.ffmpeg.clone() else {
            return Err(ErrorCode::Unsupported);
        };
        if self.thumb_dir.as_os_str().is_empty() {
            return Err(ErrorCode::Unsupported);
        }
        self.extractor
            .extract(&ffmpeg, &self.thumb_dir, &target.path, position_us)
            .await
    }

    async fn call<T>(&self, make: impl FnOnce(oneshot::Sender<T>) -> Ask) -> Result<T, ErrorCode> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(make(tx))
            .await
            .map_err(|_| ErrorCode::Unavailable)?;
        rx.await.map_err(|_| ErrorCode::Unavailable)
    }
}

impl Core {
    async fn run(
        mut self,
        mut inbox: mpsc::Receiver<Ask>,
        mut incoming: mpsc::UnboundedReceiver<Incoming>,
    ) {
        loop {
            let disconnected = self.client.is_none();
            tokio::select! {
                biased;
                ask = inbox.recv() => {
                    let Some(ask) = ask else { break };
                    self.on_ask(ask, &mut incoming).await;
                }
                message = incoming.recv() => {
                    let Some(message) = message else { break };
                    self.on_incoming(message);
                }
                _ = tokio::time::sleep(RECONNECT), if disconnected => {
                    self.try_reconnect(&mut incoming).await;
                }
            }
        }
    }

    async fn on_ask(&mut self, ask: Ask, incoming: &mut mpsc::UnboundedReceiver<Incoming>) {
        match ask {
            Ask::Open(source, reply) => {
                let _ = reply.send(self.open(source, incoming).await);
            }
            Ask::Control(action, id, position_us, reply) => {
                let _ = reply.send(self.control(action, &id, position_us, incoming).await);
            }
            Ask::Tracks(id, reply) => {
                let _ = reply.send(self.tracks(&id));
            }
            Ask::Select(id, kind, track_id, reply) => {
                let _ = reply.send(self.select(&id, &kind, &track_id, incoming).await);
            }
            Ask::Disable(id, reply) => {
                let _ = reply.send(self.disable_subtitles(&id, incoming).await);
            }
            Ask::Timeline(id, reply) => {
                let _ = reply.send(self.timeline(&id));
            }
            Ask::LaunchId(id, reply) => {
                let result = if self.live.as_ref().is_none_or(|live| live.id != id) {
                    Err(ErrorCode::NotFound)
                } else if self.phase() == "stopped" || self.client.is_none() {
                    Ok(String::new())
                } else {
                    Ok(self.launch_id.clone())
                };
                let _ = reply.send(result);
            }
            Ask::View(reply) => {
                let _ = reply.send(self.current_view());
            }
            Ask::Thumb(id, reply) => {
                let _ = reply.send(self.thumb_target(&id));
            }
        }
    }

    fn on_incoming(&mut self, message: Incoming) {
        match message {
            Incoming::Event { generation, event } if generation == self.generation => {
                self.apply(event);
            }
            Incoming::Dead { generation } if generation == self.generation => {
                self.fail_link();
            }
            Incoming::Event { .. } | Incoming::Dead { .. } => {}
        }
    }

    async fn open(
        &mut self,
        source: MediaSource,
        incoming: &mut mpsc::UnboundedReceiver<Incoming>,
    ) -> Result<String, ErrorCode> {
        source.validate()?;
        let path = crate::paths::file_path(&source.uri)?;
        self.ensure_for_open(incoming).await?;
        let id = uuid::Uuid::new_v4().to_string();
        self.install(&id, &source, path.clone());
        if let Err(error) = self
            .load(
                path.to_str().ok_or(ErrorCode::InvalidArgument)?,
                &source,
                incoming,
            )
            .await
        {
            self.live = None;
            self.sync();
            return Err(error);
        }
        self.persist();
        Ok(id)
    }

    fn install(&mut self, id: &str, source: &MediaSource, path: PathBuf) {
        self.idle = false;
        self.file_loaded = false;
        self.force_anchor = true;
        self.anchor = None;
        self.live = Some(Live {
            id: id.to_string(),
            content: source.content.clone(),
            has_content: true,
            path: Some(path.clone()),
            title: parse::session_title(&path, None),
            pause: false,
            eof: false,
            stopped: false,
            time_us: source.start_position_us,
            duration_us: -1,
            tracks: Vec::new(),
            timeline: Timeline {
                chapters: Vec::new(),
                ranges: Vec::new(),
            },
            has_video: false,
        });
        self.backend = "available".into();
        self.sync();
    }

    async fn load(
        &mut self,
        path: &str,
        source: &MediaSource,
        incoming: &mut mpsc::UnboundedReceiver<Incoming>,
    ) -> Result<(), ErrorCode> {
        let options = parse::options_for(
            source.start_position_us,
            &source.audio_language,
            &source.subtitle_language,
        );
        self.load_fault = None;
        // A stopped player closes its idle window; open it again for the new
        // file so it is ready to present (#236).
        let _ = self
            .request(json!(["set_property", "force-window", true]), incoming)
            .await;
        let modern = self
            .request(parse::loadfile(path, &options, true), incoming)
            .await?;
        if modern.error != "success" {
            if modern.error != "invalid parameter" {
                return ipc::map_error(&modern.error);
            }
            let legacy = self
                .request(parse::loadfile(path, &options, false), incoming)
                .await?;
            ipc::map_error(&legacy.error)?;
        }
        self.pump(incoming, OPEN_LOAD_WAIT, |core| {
            core.file_loaded || core.load_fault.is_some()
        })
        .await;
        if let Some(error) = self.load_fault.take() {
            return Err(error);
        }
        if !self.file_loaded {
            let _ = self.request(json!(["stop"]), incoming).await;
            self.close_idle_window(incoming).await;
            return Err(ErrorCode::Unavailable);
        }
        Ok(())
    }

    async fn ensure_for_open(
        &mut self,
        incoming: &mut mpsc::UnboundedReceiver<Incoming>,
    ) -> Result<(), ErrorCode> {
        if self.client.as_ref().is_some_and(|client| client.alive()) {
            if !self.observed {
                self.observe(incoming).await?;
            }
            return Ok(());
        }
        self.drop_client();
        if self.connect_socket().await.is_err() {
            match &self.launch {
                Launch::Systemd { binary } => {
                    let binary = binary.clone();
                    let socket = self.socket.clone();
                    let connection = Connection::session()
                        .await
                        .map_err(|_| ErrorCode::Unavailable)?;
                    scope::ensure_unit(&connection, &binary, &socket).await?;
                    self.wait_for_socket().await?;
                }
                Launch::External | Launch::Missing => return Err(ErrorCode::Unavailable),
            }
        }
        self.observe(incoming).await
    }

    async fn wait_for_socket(&mut self) -> Result<(), ErrorCode> {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if self.connect_socket().await.is_ok() {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(ErrorCode::Unavailable);
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    async fn try_reconnect(&mut self, incoming: &mut mpsc::UnboundedReceiver<Incoming>) {
        if self.client.is_some() || self.connect_socket().await.is_err() {
            return;
        }
        if self.observe(incoming).await.is_err() {
            self.drop_client();
            return;
        }
        // Launch availability only controls creation. A reachable, observed
        // player can be controlled even when its executable is now absent.
        self.backend = "available".into();
        self.sync();
    }

    async fn connect_socket(&mut self) -> Result<(), ErrorCode> {
        self.generation = self.generation.wrapping_add(1);
        match Client::connect(&self.socket, self.incoming_tx.clone(), self.generation).await {
            Ok(client) => {
                self.client = Some(client);
                self.observed = false;
                Ok(())
            }
            Err(error) => {
                self.generation = self.generation.wrapping_add(1);
                Err(error)
            }
        }
    }

    async fn observe(
        &mut self,
        incoming: &mut mpsc::UnboundedReceiver<Incoming>,
    ) -> Result<(), ErrorCode> {
        // Recover the player process identity, including after a provider restart.
        // A legacy/unmanaged player remains controllable without window presentation.
        self.launch_id.clear();
        if let Ok(reply) = self
            .request(json!(["get_property", "options/wayland-app-id"]), incoming)
            .await
            && reply.error == "success"
            && let Some(value) = reply.data.as_str()
            && let Some(launch) = value.strip_prefix(arinn_protocol::media_window::APP_ID_PREFIX)
            && !launch.is_empty()
            && arinn_protocol::media_window::validate_launch(launch).is_ok()
        {
            self.launch_id = launch.into();
        }
        self.seen.clear();
        for (id, name) in PROPS {
            let reply = self
                .request(json!(["observe_property", id, name]), incoming)
                .await?;
            if reply.error != "success" {
                return Err(ErrorCode::Unavailable);
            }
        }
        self.pump(incoming, Duration::from_millis(200), |core| {
            core.seen.len() == PROPS.len()
        })
        .await;
        self.observed = true;
        Ok(())
    }

    async fn control(
        &mut self,
        action: MediaAction,
        id: &str,
        position_us: i64,
        incoming: &mut mpsc::UnboundedReceiver<Incoming>,
    ) -> Result<(), ErrorCode> {
        if self.live.as_ref().is_none_or(|live| live.id != id) {
            return Err(ErrorCode::NotFound);
        }
        let state = self.phase();
        let allowed = actions(state);
        let token = action_token(action);
        if !allowed.iter().any(|name| name == token) {
            return Err(ErrorCode::Unsupported);
        }
        let command = match action {
            MediaAction::Play => json!(["set_property", "pause", false]),
            MediaAction::Pause => json!(["set_property", "pause", true]),
            MediaAction::PlayPause => {
                let paused = self.live.as_ref().is_some_and(|live| live.pause);
                json!(["set_property", "pause", !paused])
            }
            MediaAction::Seek => {
                let duration = self
                    .live
                    .as_ref()
                    .map(|live| live.duration_us)
                    .unwrap_or(-1);
                media::position_us(position_us, duration)?;
                json!(["seek", position_us as f64 / 1_000_000.0, "absolute"])
            }
            MediaAction::Stop => json!(["stop"]),
            MediaAction::Next | MediaAction::Previous => return Err(ErrorCode::Unsupported),
        };
        let reply = self.request(command, incoming).await?;
        ipc::map_error(&reply.error)?;
        if action == MediaAction::Stop {
            self.close_idle_window(incoming).await;
        }
        if let Some(live) = self.live.as_mut() {
            match action {
                MediaAction::Seek => {
                    live.time_us = position_us;
                    live.eof = false;
                    self.force_anchor = true;
                }
                MediaAction::Stop => {
                    live.stopped = true;
                    self.idle = true;
                    self.force_anchor = true;
                }
                MediaAction::Play => live.pause = false,
                MediaAction::Pause => live.pause = true,
                MediaAction::PlayPause => live.pause = !live.pause,
                MediaAction::Next | MediaAction::Previous => {}
            }
        }
        self.sync();
        Ok(())
    }

    async fn select(
        &mut self,
        id: &str,
        kind: &str,
        track_id: &str,
        incoming: &mut mpsc::UnboundedReceiver<Incoming>,
    ) -> Result<(), ErrorCode> {
        let live = self
            .live
            .as_ref()
            .filter(|live| live.id == id)
            .ok_or(ErrorCode::NotFound)?;
        let track = live
            .tracks
            .iter()
            .find(|track| track.id == track_id)
            .ok_or(ErrorCode::NotFound)?;
        if track.kind != kind {
            return Err(ErrorCode::InvalidArgument);
        }
        let (property, number) =
            parse::track_property(track_id).ok_or(ErrorCode::InvalidArgument)?;
        let reply = self
            .request(json!(["set_property", property, number]), incoming)
            .await?;
        ipc::map_error(&reply.error)?;
        self.confirm_tracks(incoming, kind, Some(track_id)).await
    }

    async fn disable_subtitles(
        &mut self,
        id: &str,
        incoming: &mut mpsc::UnboundedReceiver<Incoming>,
    ) -> Result<(), ErrorCode> {
        if self.live.as_ref().is_none_or(|live| live.id != id) {
            return Err(ErrorCode::NotFound);
        }
        let reply = self
            .request(json!(["set_property", "sid", "no"]), incoming)
            .await?;
        ipc::map_error(&reply.error)?;
        self.confirm_tracks(incoming, media::TRACK_SUBTITLE, None)
            .await
    }

    async fn confirm_tracks(
        &mut self,
        incoming: &mut mpsc::UnboundedReceiver<Incoming>,
        kind: &str,
        track_id: Option<&str>,
    ) -> Result<(), ErrorCode> {
        self.pump(incoming, CONFIRM_WAIT, |core| {
            tracks_match(core, kind, track_id)
        })
        .await;
        if tracks_match(self, kind, track_id) {
            Ok(())
        } else {
            Err(ErrorCode::Unavailable)
        }
    }

    fn tracks(&self, id: &str) -> Result<Vec<Track>, ErrorCode> {
        self.live
            .as_ref()
            .filter(|live| live.id == id)
            .map(|live| live.tracks.clone())
            .ok_or(ErrorCode::NotFound)
    }

    fn timeline(&self, id: &str) -> Result<Timeline, ErrorCode> {
        self.live
            .as_ref()
            .filter(|live| live.id == id)
            .map(|live| live.timeline.clone())
            .ok_or(ErrorCode::NotFound)
    }

    fn thumb_target(&self, id: &str) -> Result<ThumbTarget, ErrorCode> {
        let live = self
            .live
            .as_ref()
            .filter(|live| live.id == id)
            .ok_or(ErrorCode::NotFound)?;
        if !live.has_video {
            return Err(ErrorCode::Unsupported);
        }
        let path = live.path.clone().ok_or(ErrorCode::Unsupported)?;
        Ok(ThumbTarget { path })
    }

    async fn request(
        &mut self,
        command: Value,
        incoming: &mut mpsc::UnboundedReceiver<Incoming>,
    ) -> Result<ipc::Reply, ErrorCode> {
        let client = self.client.clone().ok_or(ErrorCode::Unavailable)?;
        let reply = client.request(command).await;
        self.drain(incoming);
        reply
    }

    fn drain(&mut self, incoming: &mut mpsc::UnboundedReceiver<Incoming>) {
        while let Ok(message) = incoming.try_recv() {
            self.on_incoming(message);
        }
    }

    async fn pump(
        &mut self,
        incoming: &mut mpsc::UnboundedReceiver<Incoming>,
        limit: Duration,
        done: impl Fn(&Core) -> bool,
    ) {
        let deadline = Instant::now() + limit;
        self.drain(incoming);
        while !done(self) && Instant::now() < deadline {
            let remaining = deadline.saturating_duration_since(Instant::now());
            match tokio::time::timeout(remaining, incoming.recv()).await {
                Ok(Some(message)) => self.on_incoming(message),
                _ => break,
            }
        }
    }

    fn apply(&mut self, event: Event) {
        match event.name.as_str() {
            "property-change" => {
                self.apply_property(event.property.as_deref().unwrap_or(""), &event.data)
            }
            "seek" => self.force_anchor = true,
            "file-loaded" => {
                self.file_loaded = true;
                self.idle = false;
                if let Some(live) = self.live.as_mut() {
                    live.stopped = false;
                    live.eof = false;
                }
            }
            "idle" => self.mark_idle(),
            "end-file" => match event.reason.as_deref() {
                Some("eof") => {
                    if let Some(live) = self.live.as_mut() {
                        live.eof = true;
                    }
                }
                Some("error") => {
                    if !self.file_loaded && self.load_fault.is_none() {
                        self.load_fault = Some(open_failure(event.file_error.as_deref()));
                    }
                    self.mark_idle();
                }
                _ => self.mark_idle(),
            },
            _ => {}
        }
        self.sync();
    }

    fn apply_property(&mut self, name: &str, data: &Value) {
        if PROPS.iter().any(|(_, property)| *property == name) {
            self.seen.insert(name.to_string());
        }
        match name {
            "pause" => {
                if let Some(paused) = data.as_bool()
                    && let Some(live) = self.live.as_mut()
                {
                    live.pause = paused;
                }
            }
            "time-pos" => {
                if let Some(position) = parse::us_from_seconds(data)
                    && let Some(live) = self.live.as_mut()
                {
                    live.time_us = position;
                }
            }
            "duration" => {
                if let Some(live) = self.live.as_mut() {
                    live.duration_us = parse::us_from_seconds(data).unwrap_or(-1);
                }
            }
            "eof-reached" => {
                if let Some(eof) = data.as_bool()
                    && let Some(live) = self.live.as_mut()
                {
                    live.eof = eof;
                }
            }
            "idle-active" => {
                if let Some(idle) = data.as_bool() {
                    if idle {
                        self.mark_idle();
                    } else {
                        self.idle = false;
                        self.restore();
                        if let Some(live) = self.live.as_mut() {
                            live.stopped = false;
                        }
                    }
                }
            }
            "track-list" => {
                if let Ok((tracks, has_video)) = parse::tracks(data)
                    && let Some(live) = self.live.as_mut()
                {
                    live.tracks = tracks;
                    live.has_video = has_video;
                }
            }
            "chapter-list" => {
                if let Some(live) = self.live.as_mut() {
                    live.timeline.chapters = parse::chapters(data);
                }
            }
            "demuxer-cache-state" => {
                if let Some(live) = self.live.as_mut() {
                    live.timeline.ranges = parse::ranges(data);
                }
            }
            "media-title" => {
                if let Some(title) = data.as_str()
                    && let Some(live) = self.live.as_mut()
                {
                    let path = live.path.clone().unwrap_or_else(|| PathBuf::from("/"));
                    live.title = parse::session_title(&path, Some(title));
                }
            }
            _ => {}
        }
    }

    fn mark_idle(&mut self) {
        self.idle = true;
        if let Some(live) = self.live.as_mut() {
            live.stopped = true;
        }
    }

    fn restore(&mut self) {
        if self.live.is_some() || self.idle {
            return;
        }
        self.live = Some(match read_state(&self.state_path) {
            Some(stored) => Live::from_stored(stored),
            None => Live::anonymous(),
        });
        self.force_anchor = true;
    }

    fn fail_link(&mut self) {
        self.drop_client();
        if let Some(live) = self.live.as_mut() {
            live.stopped = true;
        }
        if self.backend != "unavailable" {
            self.backend = "degraded".into();
        }
        self.force_anchor = true;
        self.sync();
    }

    /// The player stays running between files (`--idle`), but an empty
    /// window must not stay on the desktop after playback stops (#236).
    /// Best effort: the stop itself already succeeded.
    async fn close_idle_window(&mut self, incoming: &mut mpsc::UnboundedReceiver<Incoming>) {
        let _ = self
            .request(json!(["set_property", "force-window", false]), incoming)
            .await;
    }

    fn drop_client(&mut self) {
        self.launch_id.clear();
        self.client = None;
        self.observed = false;
        self.generation = self.generation.wrapping_add(1);
    }

    fn phase(&self) -> &'static str {
        let Some(live) = self.live.as_ref() else {
            return media::STATE_STOPPED;
        };
        if live.stopped || self.idle {
            media::STATE_STOPPED
        } else if live.eof {
            media::STATE_ENDED
        } else if live.pause {
            media::STATE_PAUSED
        } else {
            media::STATE_PLAYING
        }
    }

    fn sync(&mut self) {
        let now = Instant::now();
        let state = self.phase().to_string();
        if let Some(live) = self.live.as_mut() {
            if live.duration_us >= 0 && live.time_us > live.duration_us {
                live.time_us = live.duration_us;
            }
            if live.time_us < 0 {
                live.time_us = 0;
            }
        }
        let sample = self.live.as_ref().map(|live| live.time_us).unwrap_or(0);
        let refresh = self.anchor.as_ref().is_none_or(|anchor| {
            refresh_anchor(
                anchor,
                &state,
                sample,
                self.force_anchor,
                self.interval,
                now,
            )
        });
        if refresh && self.live.is_some() {
            self.anchor = Some(Anchor {
                position_us: sample,
                anchor_us: monotonic_us(),
                at: now,
                state: state.clone(),
            });
            self.force_anchor = false;
        }
        let session = self.session_snapshot();
        let tracks = self
            .live
            .as_ref()
            .map(|live| live.tracks.clone())
            .unwrap_or_default();
        let timeline = self
            .live
            .as_ref()
            .map(|live| live.timeline.clone())
            .unwrap_or(Timeline {
                chapters: Vec::new(),
                ranges: Vec::new(),
            });
        let session_changed = self.published.as_ref() != session.as_ref()
            || self.notes.borrow().backend != self.backend;
        let details_changed =
            tracks != self.published_tracks || timeline != self.published_timeline;
        if session_changed {
            self.published = session.clone();
            self.published_tracks = tracks;
            self.published_timeline = timeline;
            self.snapshot = self.snapshot.wrapping_add(1);
            self.emit();
        } else if details_changed {
            self.published_tracks = tracks;
            self.published_timeline = timeline;
            self.touch = self.touch.wrapping_add(1);
            self.emit();
        }
    }

    fn session_snapshot(&self) -> Option<Session> {
        let live = self.live.as_ref()?;
        let anchor = self.anchor.as_ref()?;
        let state = self.phase();
        Some(Session {
            id: live.id.clone(),
            title: live.title.clone(),
            subtitle: String::new(),
            artwork: String::new(),
            state: state.to_string(),
            duration_us: live.duration_us,
            position_us: anchor.position_us,
            anchor_us: anchor.anchor_us,
            actions: actions(state),
            has_content: live.has_content,
            content: if live.has_content {
                live.content.clone()
            } else {
                empty_entity()
            },
        })
    }

    fn current_view(&self) -> View {
        View {
            sessions: self.published.clone().into_iter().collect(),
            backend: self.backend.clone(),
            snapshot: self.snapshot,
            touch: self.touch,
        }
    }

    fn emit(&self) {
        self.notes.send_replace(self.current_view());
    }

    fn persist(&self) {
        let Some(live) = self.live.as_ref() else {
            return;
        };
        let stored = Stored {
            id: live.id.clone(),
            provider_id: live.content.provider_id.clone(),
            capability: live.content.capability.clone(),
            local_id: live.content.local_id.clone(),
            has_content: live.has_content,
        };
        let Ok(text) = serde_json::to_string(&stored) else {
            return;
        };
        if let Some(parent) = self.state_path.parent() {
            let _ = crate::paths::ensure_private_dir(parent);
        }
        let temporary = self.state_path.with_extension("tmp");
        if std::fs::write(&temporary, &text).is_err() {
            return;
        }
        let _ = std::fs::set_permissions(&temporary, std::fs::Permissions::from_mode(0o600));
        let _ = std::fs::rename(&temporary, &self.state_path);
    }
}

impl Live {
    fn from_stored(stored: Stored) -> Self {
        Self {
            id: stored.id,
            content: EntityRef {
                provider_id: stored.provider_id,
                capability: stored.capability,
                local_id: stored.local_id,
            },
            has_content: stored.has_content,
            path: None,
            title: "Playback".into(),
            pause: false,
            eof: false,
            stopped: false,
            time_us: 0,
            duration_us: -1,
            tracks: Vec::new(),
            timeline: Timeline {
                chapters: Vec::new(),
                ranges: Vec::new(),
            },
            has_video: false,
        }
    }

    fn anonymous() -> Self {
        Self::from_stored(Stored {
            id: uuid::Uuid::new_v4().to_string(),
            provider_id: String::new(),
            capability: String::new(),
            local_id: String::new(),
            has_content: false,
        })
    }
}

fn read_state(path: &Path) -> Option<Stored> {
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

fn empty_entity() -> EntityRef {
    EntityRef {
        provider_id: String::new(),
        capability: String::new(),
        local_id: String::new(),
    }
}

fn actions(state: &str) -> Vec<String> {
    match state {
        media::STATE_PLAYING | media::STATE_PAUSED => {
            ["play", "pause", "play-pause", "seek", "stop"]
                .into_iter()
                .map(str::to_string)
                .collect()
        }
        // Unpause at end of file succeeds in mpv and changes nothing.
        media::STATE_ENDED => ["seek", "stop"].into_iter().map(str::to_string).collect(),
        _ => Vec::new(),
    }
}

fn tracks_match(core: &Core, kind: &str, track_id: Option<&str>) -> bool {
    let Some(live) = core.live.as_ref() else {
        return false;
    };
    match track_id {
        Some(id) => live
            .tracks
            .iter()
            .any(|track| track.kind == kind && track.id == id && track.selected),
        None => live
            .tracks
            .iter()
            .filter(|track| track.kind == kind)
            .all(|track| !track.selected),
    }
}

fn open_failure(file_error: Option<&str>) -> ErrorCode {
    let text = file_error.unwrap_or("").to_ascii_lowercase();
    if text.contains("not found")
        || text.contains("no such file")
        || text.contains("nothing to play")
    {
        ErrorCode::NotFound
    } else {
        ErrorCode::Unavailable
    }
}

fn action_token(action: MediaAction) -> &'static str {
    match action {
        MediaAction::Play => media::ACTION_PLAY,
        MediaAction::Pause => media::ACTION_PAUSE,
        MediaAction::PlayPause => media::ACTION_PLAY_PAUSE,
        MediaAction::Seek => media::ACTION_SEEK,
        MediaAction::Next => media::ACTION_NEXT,
        MediaAction::Previous => media::ACTION_PREVIOUS,
        MediaAction::Stop => media::ACTION_STOP,
    }
}

fn refresh_anchor(
    anchor: &Anchor,
    state: &str,
    position_us: i64,
    force: bool,
    interval: Duration,
    now: Instant,
) -> bool {
    if force || anchor.state != state {
        return true;
    }
    if state != media::STATE_PLAYING {
        return anchor.position_us != position_us;
    }
    let elapsed = now.saturating_duration_since(anchor.at);
    let elapsed_us = i64::try_from(elapsed.as_micros()).unwrap_or(i64::MAX);
    let expected = anchor.position_us.saturating_add(elapsed_us);
    if (position_us - expected).saturating_abs() > SEEK_SLACK_US {
        return true;
    }
    elapsed >= interval
}

fn monotonic_us() -> u64 {
    let mut time = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    let rc = unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut time) };
    if rc != 0 {
        return 0;
    }
    (time.tv_sec as u64).saturating_mul(1_000_000) + (time.tv_nsec as u64) / 1_000
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_binary_does_not_launch() {
        let dir = std::env::temp_dir().join(format!("arinn-mpv-bin-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let binary = dir.join("mpv");
        std::fs::write(&binary, b"x").unwrap();
        assert!(matches!(launch_for(&binary), Launch::Systemd { .. }));
        assert!(matches!(launch_for(&dir.join("absent")), Launch::Missing));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
