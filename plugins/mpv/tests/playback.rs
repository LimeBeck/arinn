// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Fake mpv JSON IPC peer: open, anchors, tracks, chapters and reconnect.

use arinn_plugin_mpv::{Launch, OPEN_LOAD_WAIT, Player, Settings, log_line};
use arinn_plugin_sdk::runtime::MediaAction;
use arinn_protocol::{
    media::{MediaSource, STATE_ENDED},
    production::EntityRef,
    validation::ErrorCode,
};
use serde_json::{Value, json};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::UnixListener,
    sync::{Mutex, mpsc},
};

struct Peer {
    dir: PathBuf,
    commands: Arc<Mutex<Vec<Value>>>,
    hold_seek: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
    extra: mpsc::UnboundedSender<Value>,
    task: tokio::task::JoinHandle<()>,
}

impl Peer {
    async fn spawn(dir: PathBuf, has_video: bool, legacy_only: bool) -> Self {
        Self::spawn_configured(dir, has_video, legacy_only, None, false, None, false).await
    }

    async fn spawn_configured(
        dir: PathBuf,
        has_video: bool,
        legacy_only: bool,
        open_error: Option<String>,
        ignore_select: bool,
        load_delay: Option<Duration>,
        ignore_unpause: bool,
    ) -> Self {
        let _ = std::fs::remove_file(dir.join("ipc"));
        std::fs::create_dir_all(&dir).unwrap();
        let listener = UnixListener::bind(dir.join("ipc")).unwrap();
        let commands = Arc::new(Mutex::new(Vec::new()));
        let hold_seek = Arc::new(AtomicBool::new(false));
        let stop = Arc::new(AtomicBool::new(false));
        let (extra, extra_rx) = mpsc::unbounded_channel();
        let task = tokio::spawn(serve(
            listener,
            Arc::clone(&commands),
            Arc::clone(&hold_seek),
            Arc::clone(&stop),
            extra_rx,
            PeerScript {
                has_video,
                legacy_only,
                open_error,
                ignore_select,
                load_delay,
                ignore_unpause,
            },
        ));
        Self {
            dir,
            commands,
            hold_seek,
            stop,
            extra,
            task,
        }
    }

    fn socket(&self) -> PathBuf {
        self.dir.join("ipc")
    }

    async fn commands(&self) -> Vec<Value> {
        self.commands.lock().await.clone()
    }

    fn emit(&self, value: Value) {
        let _ = self.extra.send(value);
    }
}

impl Drop for Peer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        self.task.abort();
        let _ = std::fs::remove_file(self.dir.join("ipc"));
    }
}

struct PeerScript {
    has_video: bool,
    legacy_only: bool,
    open_error: Option<String>,
    ignore_select: bool,
    load_delay: Option<Duration>,
    ignore_unpause: bool,
}

async fn serve(
    listener: UnixListener,
    commands: Arc<Mutex<Vec<Value>>>,
    hold_seek: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
    mut extra: mpsc::UnboundedReceiver<Value>,
    script: PeerScript,
) {
    let Ok((stream, _)) = listener.accept().await else {
        return;
    };
    let (read, mut write) = stream.into_split();
    let mut lines = BufReader::new(read).lines();
    let mut state = Fake::new(
        script.has_video,
        script.open_error,
        script.ignore_select,
        script.ignore_unpause,
    );
    let mut observed: Vec<(i64, String)> = Vec::new();
    let mut delayed_load: Option<tokio::time::Instant> = None;
    loop {
        if stop.load(Ordering::SeqCst) {
            break;
        }
        let loaded_at = delayed_load;
        tokio::select! {
            line = lines.next_line() => {
                let Ok(Some(line)) = line else { break };
                let Ok(message) = serde_json::from_str::<Value>(&line) else { continue };
                commands.lock().await.push(message.clone());
                let id = message.get("request_id").cloned().unwrap_or(json!(0));
                let command = message.get("command").and_then(|v| v.as_array()).cloned().unwrap_or_default();
                let name = command.first().and_then(|v| v.as_str()).unwrap_or("");
                if name == "seek" && hold_seek.load(Ordering::SeqCst) {
                    std::future::pending::<()>().await;
                }
                if name == "loadfile" && script.legacy_only && command.get(3).and_then(|v| v.as_i64()).is_some() {
                    send(&mut write, &json!({"request_id": id, "error": "invalid parameter"})).await;
                    continue;
                }
                let mut events = state.handle(&command, &mut observed);
                if name == "loadfile"
                    && let Some(delay) = script.load_delay
                {
                    events.retain(|event| {
                        event.get("event").and_then(|value| value.as_str()) != Some("file-loaded")
                    });
                    delayed_load = Some(tokio::time::Instant::now() + delay);
                }
                send(&mut write, &json!({"request_id": id, "error": "success", "data": null})).await;
                for event in events {
                    send(&mut write, &event).await;
                }
            }
            _ = async {
                if let Some(at) = loaded_at {
                    tokio::time::sleep_until(at).await;
                } else {
                    std::future::pending::<()>().await;
                }
            } => {
                delayed_load = None;
                send(&mut write, &json!({"event": "file-loaded"})).await;
            }
            extra = extra.recv() => {
                let Some(event) = extra else { break };
                send(&mut write, &event).await;
            }
        }
    }
}

async fn send(write: &mut tokio::net::unix::OwnedWriteHalf, value: &Value) {
    let mut line = serde_json::to_string(value).unwrap();
    line.push('\n');
    let _ = write.write_all(line.as_bytes()).await;
    let _ = write.flush().await;
}

struct Fake {
    pause: bool,
    time: Option<f64>,
    duration: Option<f64>,
    eof: bool,
    idle: bool,
    title: Option<String>,
    tracks: Value,
    chapters: Value,
    cache: Value,
    has_video: bool,
    open_error: Option<String>,
    ignore_select: bool,
    ignore_unpause: bool,
}

impl Fake {
    fn new(
        has_video: bool,
        open_error: Option<String>,
        ignore_select: bool,
        ignore_unpause: bool,
    ) -> Self {
        Self {
            pause: true,
            time: None,
            duration: None,
            eof: false,
            idle: true,
            title: None,
            tracks: json!([]),
            chapters: json!([]),
            cache: json!({"seekable-ranges": []}),
            has_video,
            open_error,
            ignore_select,
            ignore_unpause,
        }
    }

    fn handle(&mut self, command: &[Value], observed: &mut Vec<(i64, String)>) -> Vec<Value> {
        let name = command.first().and_then(|v| v.as_str()).unwrap_or("");
        match name {
            "observe_property" => {
                let id = command.get(1).and_then(|v| v.as_i64()).unwrap_or(0);
                let property = command
                    .get(2)
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                observed.push((id, property.clone()));
                vec![self.change(id, &property)]
            }
            "loadfile" => {
                if let Some(file_error) = self.open_error.clone() {
                    return vec![json!({
                        "event": "end-file",
                        "reason": "error",
                        "file_error": file_error
                    })];
                }
                let options = command
                    .get(4)
                    .cloned()
                    .filter(|v| v.is_object())
                    .or_else(|| command.get(3).cloned())
                    .unwrap_or(json!({}));
                let start = options
                    .get("start")
                    .and_then(|v| v.as_str())
                    .and_then(|text| text.parse::<f64>().ok())
                    .unwrap_or(0.0);
                self.idle = false;
                self.pause = false;
                self.eof = false;
                self.time = Some(start);
                self.duration = Some(120.0);
                self.title = Some("Secret Title".into());
                self.tracks = sample_tracks(self.has_video);
                self.chapters = json!([
                    {"title": "Intro", "time": 0},
                    {"title": "Next", "time": 30.5}
                ]);
                self.cache = json!({"seekable-ranges": [{"start": 0, "end": 15.25}]});
                let mut events = vec![json!({"event": "file-loaded"})];
                events.extend(self.snapshot(observed));
                events
            }
            "set_property" => {
                let property = command.get(1).and_then(|v| v.as_str()).unwrap_or("");
                let value = command.get(2).cloned().unwrap_or(Value::Null);
                match property {
                    "pause" if self.ignore_unpause && value.as_bool() == Some(false) => Vec::new(),
                    "pause" => {
                        self.pause = value.as_bool().unwrap_or(self.pause);
                        self.snapshot_one(observed, "pause")
                    }
                    "aid" | "sid" if self.ignore_select => Vec::new(),
                    "aid" | "sid" => {
                        self.select(property, &value);
                        self.snapshot_one(observed, "track-list")
                    }
                    _ => Vec::new(),
                }
            }
            "seek" => {
                self.time = command.get(1).and_then(|v| v.as_f64());
                self.eof = false;
                let mut events = vec![json!({"event": "seek"})];
                events.extend(self.snapshot_one(observed, "time-pos"));
                events
            }
            "stop" => {
                self.idle = true;
                self.pause = true;
                let mut events = vec![json!({"event": "end-file", "reason": "stop"})];
                events.extend(self.snapshot_one(observed, "idle-active"));
                events
            }
            _ => Vec::new(),
        }
    }

    fn select(&mut self, property: &str, value: &Value) {
        let kind = if property == "aid" { "audio" } else { "sub" };
        let selected = value.as_i64();
        let Some(items) = self.tracks.as_array_mut() else {
            return;
        };
        for item in items {
            if item.get("type").and_then(|v| v.as_str()) != Some(kind) {
                continue;
            }
            let on = selected.is_some() && item.get("id").and_then(|v| v.as_i64()) == selected;
            item["selected"] = json!(on);
        }
    }

    fn snapshot(&self, observed: &[(i64, String)]) -> Vec<Value> {
        observed
            .iter()
            .map(|(id, property)| self.change(*id, property))
            .collect()
    }

    fn snapshot_one(&self, observed: &[(i64, String)], property: &str) -> Vec<Value> {
        observed
            .iter()
            .filter(|(_, name)| name == property)
            .map(|(id, name)| self.change(*id, name))
            .collect()
    }

    fn change(&self, id: i64, property: &str) -> Value {
        let data = match property {
            "pause" => json!(self.pause),
            "time-pos" => self.time.map(|time| json!(time)).unwrap_or(Value::Null),
            "duration" => self.duration.map(|time| json!(time)).unwrap_or(Value::Null),
            "eof-reached" => json!(self.eof),
            "idle-active" => json!(self.idle),
            "track-list" => self.tracks.clone(),
            "chapter-list" => self.chapters.clone(),
            "demuxer-cache-state" => self.cache.clone(),
            "media-title" => self
                .title
                .clone()
                .map(|title| json!(title))
                .unwrap_or(Value::Null),
            _ => Value::Null,
        };
        json!({"event": "property-change", "id": id, "name": property, "data": data})
    }
}

fn sample_tracks(has_video: bool) -> Value {
    let mut items = Vec::new();
    if has_video {
        items.push(json!({"id": 1, "type": "video", "selected": true, "codec": "h264"}));
    }
    items.push(
        json!({"id": 1, "type": "audio", "lang": "eng", "title": "Stereo", "codec": "aac",
        "demux-channel-count": 2, "selected": true, "external": false}),
    );
    items.push(
        json!({"id": 2, "type": "audio", "lang": "jpn", "title": "Commentary", "codec": "aac",
        "demux-channel-count": 2, "selected": false, "external": false}),
    );
    items.push(
        json!({"id": 1, "type": "sub", "lang": "eng", "title": "English", "codec": "subrip",
        "selected": true, "external": false}),
    );
    items.push(
        json!({"id": 2, "type": "sub", "lang": "rus", "title": "External", "codec": "subrip",
        "selected": false, "external": true}),
    );
    json!(items)
}

fn source(dir: &std::path::Path) -> MediaSource {
    MediaSource {
        content: EntityRef {
            provider_id: "library".into(),
            capability: "library".into(),
            local_id: "film-1".into(),
        },
        uri: format!("file://{}/clip.mkv", dir.display()),
        start_position_us: 1_000_000,
        audio_language: "en".into(),
        subtitle_language: "ru".into(),
    }
}

fn player(dir: &std::path::Path, socket: PathBuf) -> Player {
    Player::start(Settings {
        socket,
        state_path: dir.join("session"),
        launch: Launch::External,
        publish_interval: Duration::from_secs(3600),
        ffmpeg: None,
        thumb_dir: PathBuf::new(),
        backend: "available".into(),
    })
    .unwrap()
}

fn names(commands: &[Value]) -> Vec<String> {
    commands
        .iter()
        .filter_map(|message| {
            message
                .get("command")
                .and_then(|v| v.as_array())
                .and_then(|command| command.first())
                .and_then(|name| name.as_str())
                .map(str::to_string)
        })
        .collect()
}

#[tokio::test]
async fn open_reports_anchors_tracks_chapters_and_hides_paths() {
    let dir = std::env::temp_dir().join(format!("arinn-mpv-{}", uuid::Uuid::new_v4()));
    let peer = Peer::spawn(dir.clone(), true, false).await;
    let player = player(&dir, peer.socket());
    let id = player.open(source(&dir)).await.unwrap();
    let view = player.view().await.unwrap();
    let session = view
        .sessions
        .iter()
        .find(|session| session.id == id)
        .unwrap();
    assert_eq!(
        player.launch_id(id.clone()).await.unwrap(),
        "",
        "legacy player cannot assert a managed identity"
    );
    assert_eq!(
        player.launch_id("missing".into()).await,
        Err(ErrorCode::NotFound)
    );
    assert_eq!(session.title, "Secret Title");
    assert_eq!(session.state, "playing");
    assert_eq!(session.position_us, 1_000_000);
    assert!(session.anchor_us > 0);
    assert!(session.has_content);
    assert_eq!(session.content.local_id, "film-1");
    assert!(session.actions.iter().any(|action| action == "seek"));

    let stored = std::fs::read_to_string(dir.join("session")).unwrap();
    assert!(!stored.contains("Secret Title"), "{stored}");
    assert!(!stored.contains("clip.mkv"), "{stored}");
    assert!(!log_line("ipc", ErrorCode::Unavailable).contains('/'));

    let commands = peer.commands().await;
    let load = commands
        .iter()
        .find(|message| message.pointer("/command/0").and_then(|v| v.as_str()) == Some("loadfile"))
        .unwrap();
    assert_eq!(
        load.pointer("/command/2").and_then(|v| v.as_str()),
        Some("replace")
    );
    assert_eq!(
        load.pointer("/command/3").and_then(|v| v.as_i64()),
        Some(-1)
    );
    assert_eq!(
        load.pointer("/command/4/start").and_then(|v| v.as_str()),
        Some("1.000000")
    );
    assert_eq!(
        load.pointer("/command/4/alang").and_then(|v| v.as_str()),
        Some("en")
    );

    let tracks = player.tracks(id.clone()).await.unwrap();
    assert_eq!(
        tracks.iter().filter(|track| track.kind == "audio").count(),
        2
    );
    assert!(
        tracks
            .iter()
            .any(|track| track.id == "a1" && track.selected)
    );
    assert!(
        tracks
            .iter()
            .any(|track| track.external && track.kind == "subtitle")
    );
    player
        .select(id.clone(), "audio".into(), "a2".into())
        .await
        .unwrap();
    let tracks = player.tracks(id.clone()).await.unwrap();
    assert!(
        tracks
            .iter()
            .any(|track| track.id == "a2" && track.selected)
    );
    assert!(
        tracks
            .iter()
            .any(|track| track.id == "a1" && !track.selected)
    );

    player.disable_subtitles(id.clone()).await.unwrap();
    let tracks = player.tracks(id.clone()).await.unwrap();
    assert!(
        tracks
            .iter()
            .filter(|track| track.kind == "subtitle")
            .all(|track| !track.selected)
    );
    let commands = peer.commands().await;
    assert!(
        commands
            .iter()
            .any(|message| message.pointer("/command")
                == Some(&json!(["set_property", "sid", "no"])))
    );

    let timeline = player.timeline(id.clone()).await.unwrap();
    assert_eq!(timeline.chapters.len(), 2);
    assert_eq!(timeline.chapters[1].title, "Next");
    assert_eq!(timeline.ranges.len(), 1);
    assert_eq!(timeline.ranges[0].end_us, 15_250_000);

    let anchor = session.anchor_us;
    peer.emit(json!({"event":"property-change","id":2,"name":"time-pos","data":1.2}));
    peer.emit(json!({"event":"property-change","id":2,"name":"time-pos","data":1.4}));
    tokio::time::sleep(Duration::from_millis(50)).await;
    let held = player.view().await.unwrap().sessions[0].position_us;
    assert_eq!(held, 1_000_000);

    let before = player.view().await.unwrap().sessions[0].anchor_us;
    player
        .control(MediaAction::Pause, id.clone(), 0)
        .await
        .unwrap();
    let paused = &player.view().await.unwrap().sessions[0];
    assert_eq!(paused.state, "paused");
    assert!(paused.anchor_us >= before);

    player
        .control(MediaAction::Seek, id.clone(), 5_000_000)
        .await
        .unwrap();
    let sought = &player.view().await.unwrap().sessions[0];
    assert_eq!(sought.position_us, 5_000_000);
    assert!(sought.anchor_us >= anchor);

    player
        .control(MediaAction::Stop, id.clone(), 0)
        .await
        .unwrap();
    let stopped = &player.view().await.unwrap().sessions[0];
    assert_eq!(stopped.state, "stopped");
    assert!(stopped.actions.is_empty());
    assert_eq!(
        player.control(MediaAction::Play, id, 0).await,
        Err(ErrorCode::Unsupported)
    );
    drop(player);
    drop(peer);
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn stop_closes_the_idle_window_and_open_does_not_force_one() {
    let dir = std::env::temp_dir().join(format!("arinn-mpv-{}", uuid::Uuid::new_v4()));
    let peer = Peer::spawn(dir.clone(), true, false).await;
    let player = player(&dir, peer.socket());
    let id = player.open(source(&dir)).await.unwrap();
    player.control(MediaAction::Stop, id, 0).await.unwrap();
    player.open(source(&dir)).await.unwrap();
    let window = |value: bool| json!(["set_property", "force-window", value]);
    let commands: Vec<Value> = peer
        .commands()
        .await
        .into_iter()
        .filter_map(|message| message.get("command").cloned())
        .collect();
    let stop = commands.iter().position(|c| c == &json!(["stop"])).unwrap();
    assert!(
        commands[stop..].contains(&window(false)),
        "a stopped player closes its idle window"
    );
    // An early empty window would be replaced when the video starts, which
    // ends a presentation of it (#236).
    assert!(
        !commands.contains(&window(true)),
        "open never forces a window before the video"
    );
    drop(peer);
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn thumbnail_without_video_or_ffmpeg_is_unsupported_and_does_not_seek() {
    let dir = std::env::temp_dir().join(format!("arinn-mpv-{}", uuid::Uuid::new_v4()));
    let peer = Peer::spawn(dir.clone(), false, false).await;
    let player = player(&dir, peer.socket());
    let id = player.open(source(&dir)).await.unwrap();
    let before = names(&peer.commands().await);
    assert_eq!(player.thumbnail(id, 0).await, Err(ErrorCode::Unsupported));
    let after = names(&peer.commands().await);
    assert_eq!(
        before.iter().filter(|name| *name == "seek").count(),
        after.iter().filter(|name| *name == "seek").count()
    );
    drop(player);
    drop(peer);
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn rejected_modern_loadfile_falls_back_once() {
    let dir = std::env::temp_dir().join(format!("arinn-mpv-{}", uuid::Uuid::new_v4()));
    let peer = Peer::spawn(dir.clone(), true, true).await;
    let player = player(&dir, peer.socket());
    let id = player.open(source(&dir)).await.unwrap();
    assert!(!id.is_empty());
    let loads: Vec<_> = peer
        .commands()
        .await
        .into_iter()
        .filter(|message| {
            message.pointer("/command/0").and_then(|v| v.as_str()) == Some("loadfile")
        })
        .collect();
    assert_eq!(loads.len(), 2);
    assert_eq!(
        loads[0].pointer("/command/3").and_then(|v| v.as_i64()),
        Some(-1)
    );
    assert!(loads[1].pointer("/command/3").unwrap().is_object());
    drop(player);
    drop(peer);
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn peer_crash_does_not_replay_loadfile_or_the_failed_seek() {
    let dir = std::env::temp_dir().join(format!("arinn-mpv-{}", uuid::Uuid::new_v4()));
    let peer = Peer::spawn(dir.clone(), true, false).await;
    let socket = peer.socket();
    let player = player(&dir, socket.clone());
    let id = player.open(source(&dir)).await.unwrap();
    peer.hold_seek.store(true, Ordering::SeqCst);
    let seeking = {
        let player = player.clone();
        let id = id.clone();
        tokio::spawn(async move { player.control(MediaAction::Seek, id, 8_000_000).await })
    };
    for _ in 0..50 {
        if names(&peer.commands().await)
            .iter()
            .any(|name| name == "seek")
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    drop(peer);
    assert!(seeking.await.unwrap().is_err());
    let peer = Peer::spawn(dir.clone(), true, false).await;
    let connected = tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let seen = names(&peer.commands().await);
            if seen.iter().any(|name| name == "observe_property") {
                return seen;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("reconnect");
    assert!(!connected.iter().any(|name| name == "loadfile"));
    assert!(!connected.iter().any(|name| name == "seek"));
    assert_eq!(
        player.control(MediaAction::Play, "missing".into(), 0).await,
        Err(ErrorCode::NotFound)
    );
    drop(player);
    drop(peer);
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn missing_mpv_stays_unavailable_and_does_not_open() {
    let dir = std::env::temp_dir().join(format!("arinn-mpv-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let player = Player::start(Settings {
        socket: dir.join("ipc"),
        state_path: dir.join("session"),
        launch: Launch::Missing,
        publish_interval: Duration::from_secs(3600),
        ffmpeg: None,
        thumb_dir: dir.join("cache"),
        backend: "unavailable".into(),
    })
    .unwrap();
    assert_eq!(player.view().await.unwrap().backend, "unavailable");
    assert!(player.view().await.unwrap().sessions.is_empty());
    assert_eq!(player.open(source(&dir)).await, Err(ErrorCode::Unavailable));
    tokio::time::sleep(Duration::from_millis(300)).await;
    let view = player.view().await.unwrap();
    assert_eq!(view.backend, "unavailable");
    assert!(view.sessions.is_empty());
    assert!(!dir.join("session").exists());
    drop(player);
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn missing_launcher_recovers_readiness_from_a_live_ipc_player() {
    let dir = std::env::temp_dir().join(format!("arinn-mpv-{}", uuid::Uuid::new_v4()));
    let peer = Peer::spawn(dir.clone(), true, false).await;
    let player = Player::start(Settings {
        socket: peer.socket(),
        state_path: dir.join("session"),
        launch: Launch::Missing,
        publish_interval: Duration::from_secs(3600),
        ffmpeg: None,
        thumb_dir: PathBuf::new(),
        backend: "unavailable".into(),
    })
    .unwrap();
    let mut notes = player.subscribe();
    tokio::time::timeout(Duration::from_secs(5), async {
        while notes.borrow_and_update().backend != "available" {
            notes.changed().await.unwrap();
        }
    })
    .await
    .expect("observed live player must recover backend readiness");
    let id = player.open(source(&dir)).await.unwrap();
    assert_eq!(player.launch_id(id.clone()).await.unwrap(), "");
    player.control(MediaAction::Pause, id, 0).await.unwrap();
    assert!(
        names(&peer.commands().await)
            .iter()
            .any(|name| name == "set_property")
    );
    drop(player);
    drop(peer);
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn bad_uri_is_rejected() {
    let dir = std::env::temp_dir().join(format!("arinn-mpv-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let player = player(&dir, dir.join("ipc"));
    let mut item = source(&dir);
    item.uri = "https://example.org/a.mkv".into();
    assert_eq!(player.open(item).await, Err(ErrorCode::InvalidArgument));
    drop(player);
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn open_of_a_missing_or_unreadable_file_is_not_success() {
    for (file_error, expected) in [
        ("file not found", ErrorCode::NotFound),
        ("permission denied", ErrorCode::Unavailable),
    ] {
        let dir = std::env::temp_dir().join(format!("arinn-mpv-{}", uuid::Uuid::new_v4()));
        let peer = Peer::spawn_configured(
            dir.clone(),
            true,
            false,
            Some(file_error.into()),
            false,
            None,
            false,
        )
        .await;
        let player = player(&dir, peer.socket());
        assert_eq!(player.open(source(&dir)).await, Err(expected));
        let view = player.view().await.unwrap();
        assert!(view.sessions.is_empty());
        assert!(!dir.join("session").exists());
        drop(player);
        drop(peer);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[tokio::test]
async fn ignored_track_selection_is_not_applied_locally() {
    let dir = std::env::temp_dir().join(format!("arinn-mpv-{}", uuid::Uuid::new_v4()));
    let peer = Peer::spawn_configured(dir.clone(), true, false, None, true, None, false).await;
    let player = player(&dir, peer.socket());
    let id = player.open(source(&dir)).await.unwrap();
    assert_eq!(
        player.select(id.clone(), "audio".into(), "a2".into()).await,
        Err(ErrorCode::Unavailable)
    );
    let tracks = player.tracks(id.clone()).await.unwrap();
    assert!(
        tracks
            .iter()
            .any(|track| track.id == "a1" && track.selected)
    );
    assert!(
        tracks
            .iter()
            .any(|track| track.id == "a2" && !track.selected)
    );
    assert_eq!(
        player.disable_subtitles(id.clone()).await,
        Err(ErrorCode::Unavailable)
    );
    let tracks = player.tracks(id).await.unwrap();
    assert!(
        tracks
            .iter()
            .any(|track| track.id == "s1" && track.selected)
    );
    drop(player);
    drop(peer);
    let _ = std::fs::remove_dir_all(&dir);
}

async fn wait_for_state(player: &Player, expected: &str) {
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let state = player.view().await.unwrap().sessions[0].state.clone();
            if state == expected {
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("state did not become {expected}"));
}

#[tokio::test]
async fn confirmed_eof_is_ended_and_a_seek_is_not() {
    let dir = std::env::temp_dir().join(format!("arinn-mpv-{}", uuid::Uuid::new_v4()));
    let peer = Peer::spawn(dir.clone(), true, false).await;
    let player = player(&dir, peer.socket());
    let id = player.open(source(&dir)).await.unwrap();
    player
        .control(MediaAction::Seek, id.clone(), 120_000_000)
        .await
        .unwrap();
    let sought = &player.view().await.unwrap().sessions[0];
    assert_ne!(sought.state, STATE_ENDED);
    assert_eq!(sought.position_us, 120_000_000);

    peer.emit(json!({"event":"property-change","name":"eof-reached","data":true}));
    wait_for_state(&player, STATE_ENDED).await;
    peer.emit(json!({"event":"property-change","name":"eof-reached","data":false}));
    wait_for_state(&player, "playing").await;

    peer.emit(json!({"event":"end-file","reason":"eof"}));
    wait_for_state(&player, STATE_ENDED).await;
    player
        .control(MediaAction::Seek, id, 5_000_000)
        .await
        .unwrap();
    let after = &player.view().await.unwrap().sessions[0];
    assert_ne!(after.state, STATE_ENDED);
    assert_eq!(after.position_us, 5_000_000);
    drop(player);
    drop(peer);
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn slow_open_stops_the_player_and_drops_the_session() {
    let dir = std::env::temp_dir().join(format!("arinn-mpv-{}", uuid::Uuid::new_v4()));
    let peer = Peer::spawn_configured(
        dir.clone(),
        true,
        false,
        None,
        false,
        Some(OPEN_LOAD_WAIT + Duration::from_secs(2)),
        false,
    )
    .await;
    let player = player(&dir, peer.socket());
    let started = std::time::Instant::now();
    assert_eq!(player.open(source(&dir)).await, Err(ErrorCode::Unavailable));
    let elapsed = started.elapsed();
    assert!(elapsed >= OPEN_LOAD_WAIT, "{elapsed:?}");
    assert!(
        elapsed < OPEN_LOAD_WAIT + Duration::from_secs(5),
        "{elapsed:?}"
    );
    let commands = peer.commands().await;
    assert!(
        commands
            .iter()
            .any(|message| message.pointer("/command") == Some(&json!(["stop"]))),
        "{commands:?}"
    );
    assert!(player.view().await.unwrap().sessions.is_empty());
    assert!(!dir.join("session").exists());
    tokio::time::sleep(Duration::from_secs(3)).await;
    assert!(player.view().await.unwrap().sessions.is_empty());
    drop(player);
    drop(peer);
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn ended_advertises_seek_and_stop_only() {
    let dir = std::env::temp_dir().join(format!("arinn-mpv-{}", uuid::Uuid::new_v4()));
    let peer = Peer::spawn_configured(dir.clone(), true, false, None, false, None, true).await;
    let player = player(&dir, peer.socket());
    let id = player.open(source(&dir)).await.unwrap();
    peer.emit(json!({"event":"end-file","reason":"eof"}));
    wait_for_state(&player, STATE_ENDED).await;
    let session = player.view().await.unwrap().sessions[0].clone();
    assert_eq!(
        session.actions,
        vec!["seek".to_string(), "stop".to_string()]
    );
    let position = session.position_us;
    let pauses_before = peer
        .commands()
        .await
        .into_iter()
        .filter(|message| message.pointer("/command/1").and_then(|v| v.as_str()) == Some("pause"))
        .count();
    assert_eq!(
        player.control(MediaAction::Play, id.clone(), 0).await,
        Err(ErrorCode::Unsupported)
    );
    assert_eq!(
        player.control(MediaAction::PlayPause, id.clone(), 0).await,
        Err(ErrorCode::Unsupported)
    );
    let session = &player.view().await.unwrap().sessions[0];
    assert_eq!(session.state, STATE_ENDED);
    assert_eq!(session.position_us, position);
    let pauses_after = peer
        .commands()
        .await
        .into_iter()
        .filter(|message| message.pointer("/command/1").and_then(|v| v.as_str()) == Some("pause"))
        .count();
    assert_eq!(pauses_before, pauses_after);
    drop(player);
    drop(peer);
    let _ = std::fs::remove_dir_all(&dir);
}
