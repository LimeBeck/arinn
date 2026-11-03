// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `org.arinn.Music1` and `MusicController1` routing (ADR-0134) on a private
//! bus, with two scripted music providers. No audio, account or real
//! provider process is involved.
use super::*;
use crate::test_support::served;
use arinn_protocol::music::{Entity, QueueEntry};
use std::{
    io::{BufRead, BufReader},
    process::{Child, Command as Process, Stdio},
    sync::Mutex as StdMutex,
};

#[zbus::proxy(interface = "org.arinn.Music1", default_service = "org.arinn.Daemon1")]
trait MusicApi {
    fn command(&self, generation: &str, command: &str) -> zbus::Result<String>;
}
#[zbus::proxy(
    interface = "org.arinn.MusicController1",
    default_service = "org.arinn.Daemon1",
    default_path = "/org/arinn/Daemon1"
)]
trait MusicControllerApi {
    fn get_active(&self) -> zbus::Result<String>;
}

/// A scripted provider: it records actions, answers stop by clearing its
/// session, and can delay or fail a chosen action.
#[derive(Default)]
struct Script {
    state: MusicState,
    actions: Vec<String>,
    delay: Option<(String, Duration)>,
    fail: Option<(String, ErrorCode)>,
}
#[derive(Clone, Default)]
struct Fake(Arc<StdMutex<Script>>);
impl Fake {
    fn playing(session: &str) -> Self {
        let fake = Self::default();
        fake.set(session, "playing");
        fake
    }
    fn set(&self, session: &str, playback: &str) {
        let mut s = self.0.lock().unwrap();
        s.state.revision += 1;
        s.state.session = session.into();
        s.state.playback = playback.into();
        s.state.current = (!session.is_empty()).then(|| QueueEntry {
            occurrence: format!("{session}-1"),
            track: Entity {
                id: format!("{session}-track"),
                kind: "track".into(),
                title: "Synthetic".into(),
                ..Entity::default()
            },
        });
    }
    fn actions(&self) -> Vec<String> {
        self.0.lock().unwrap().actions.clone()
    }
}
#[zbus::interface(name = "org.arinn.MusicProvider1")]
impl Fake {
    async fn get_state(&self, _generation: &str) -> zbus::fdo::Result<String> {
        Ok(music::encode(&self.0.lock().unwrap().state).unwrap())
    }
    async fn get_page(
        &self,
        _generation: &str,
        _client: &str,
        _query: &str,
    ) -> zbus::fdo::Result<String> {
        Err(zbus::fdo::Error::NotSupported(String::new()))
    }
    async fn command(
        &self,
        _generation: &str,
        _client: &str,
        command: &str,
    ) -> production::Result<String> {
        let command: Command = music::decode(command)?;
        let (delay, fail) = {
            let mut s = self.0.lock().unwrap();
            s.actions.push(command.action.clone());
            (
                s.delay
                    .clone()
                    .filter(|(a, _)| *a == command.action)
                    .map(|(_, d)| d),
                s.fail
                    .clone()
                    .filter(|(a, _)| *a == command.action)
                    .map(|(_, e)| e),
            )
        };
        if let Some(delay) = delay {
            tokio::time::sleep(delay).await;
        }
        if let Some(error) = fail {
            return Err(error.into());
        }
        match command.action.as_str() {
            "stop" => self.set("", "stopped"),
            "play" | "radio" => self.set(&format!("session-{}", command.entity), "playing"),
            "next" | "previous" | "resume" | "retry" => {
                let session = self.0.lock().unwrap().state.session.clone();
                self.set(&session, "playing");
            }
            "pause" => {
                let session = self.0.lock().unwrap().state.session.clone();
                self.set(&session, "paused");
            }
            _ => {}
        }
        Ok(music::encode(&self.0.lock().unwrap().state).unwrap())
    }
}

struct Fixture {
    bus: Child,
    folder: PathBuf,
    core: Core,
    client: Connection,
    _providers: Vec<Connection>,
    a: Fake,
    b: Fake,
    generation: String,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = self.bus.kill();
        let _ = self.bus.wait();
        let _ = std::fs::remove_dir_all(&self.folder);
    }
}
async fn connect(address: &str) -> Connection {
    zbus::connection::Builder::address(address.trim())
        .unwrap()
        .build()
        .await
        .unwrap()
}
fn provider(id: &str, owner: &Connection, generation: &str, folder: &std::path::Path) -> Provider {
    Provider {
        info: ProviderInfo {
            id: id.into(),
            plugin_id: id.into(),
            source_path: String::new(),
            name: format!("{id} name"),
            version: "0.0.1".into(),
            path: format!("/org/arinn/Daemon1/providers/p_{id}")
                .try_into()
                .unwrap(),
            capabilities: vec!["music".into()],
            runtime_state: "ready".into(),
            backend_state: "available".into(),
            generation: generation.into(),
            revision: 1,
            last_error: String::new(),
            error_at: 0,
            recovered_at: 0,
            restarts_since_daemon_start: 0,
            cumulative_restart_count: 0,
        },
        manifest: None,
        unit: format!("{id}.service"),
        token_file: folder.join(format!("{id}.token")),
        token: String::new(),
        owner: owner.unique_name().unwrap().to_string(),
        cache: None,
        applications: Vec::new(),
        running: Vec::new(),
        tab_cache: None,
        session_cache: None,
        sessions: Vec::new(),
        features: Vec::new(),
        library_cache: None,
        collections: Vec::new(),
        items: Vec::new(),
        resume_cache: None,
        resumes: Vec::new(),
        settings_schema: None,
        windows: Vec::new(),
        tabs: Vec::new(),
        started: Instant::now(),
        enabled: true,
    }
}
impl Fixture {
    async fn new(a: Fake, b: Fake) -> Self {
        let mut bus = Process::new("dbus-daemon")
            .args(["--session", "--nofork", "--print-address=1"])
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let mut address = String::new();
        BufReader::new(bus.stdout.take().unwrap())
            .read_line(&mut address)
            .unwrap();
        let server = connect(&address).await;
        let client = connect(&address).await;
        server.request_name(production::SERVICE).await.unwrap();
        let folder = std::env::temp_dir().join(format!("arinn-music-{}", Uuid::new_v4()));
        std::fs::create_dir(&folder).unwrap();
        let core = Core::new(
            &server,
            Paths {
                config: folder.join("config"),
                data: folder.join("data"),
                state: folder.join("state"),
            },
            folder.clone(),
        );
        let generation = Uuid::new_v4().to_string();
        let mut providers = Vec::new();
        for (id, fake) in [("a", &a), ("b", &b)] {
            let connection = connect(&address).await;
            connection
                .object_server()
                .at(production::PROVIDER_PATH, fake.clone())
                .await
                .unwrap();
            let entry = provider(id, &connection, &generation, &folder);
            server
                .object_server()
                .at(
                    entry.info.path.clone(),
                    music_routing::MusicEndpoint {
                        core: core.clone(),
                        id: id.into(),
                    },
                )
                .await
                .unwrap();
            core.state.lock().await.providers.insert(id.into(), entry);
            served(&client, &connection).await;
            providers.push(connection);
        }
        server
            .object_server()
            .at(
                production::PATH,
                music_routing::Controller { core: core.clone() },
            )
            .await
            .unwrap();
        served(&client, &server).await;
        let fixture = Self {
            bus,
            folder,
            core,
            client,
            _providers: providers,
            a,
            b,
            generation,
        };
        // The daemon polls providers; tests read their state once instead.
        for id in ["a", "b"] {
            fixture.core.refresh_music(id).await.unwrap();
        }
        fixture
    }
    async fn send(&self, id: &str, command: &Command) -> Result<MusicState, String> {
        let api = MusicApiProxy::builder(&self.client)
            .path(format!("/org/arinn/Daemon1/providers/p_{id}"))
            .unwrap()
            .build()
            .await
            .unwrap();
        match api
            .command(&self.generation, &music::encode(command).unwrap())
            .await
        {
            Ok(json) => Ok(music::decode(&json).unwrap()),
            Err(zbus::Error::MethodError(name, _, _)) => {
                Err(name.as_str().rsplit('.').next().unwrap().to_owned())
            }
            Err(other) => panic!("{other:?}"),
        }
    }
    async fn active(&self) -> Option<music::Active> {
        let controller = MusicControllerApiProxy::new(&self.client).await.unwrap();
        music::decode(&controller.get_active().await.unwrap()).unwrap()
    }
}
fn error(result: Result<MusicState, String>) -> String {
    match result {
        Err(name) => name,
        Ok(_) => panic!("expected an error"),
    }
}
fn ok(result: Result<MusicState, String>) -> MusicState {
    match result {
        Ok(state) => state,
        Err(name) => panic!("unexpected {name}"),
    }
}
fn command(action: &str, entity: &str) -> Command {
    Command {
        request_id: Uuid::new_v4().to_string(),
        action: action.into(),
        session: String::new(),
        entity: entity.into(),
        occurrence: String::new(),
        position_us: 0,
        replaces_provider: String::new(),
        replaces_session: String::new(),
        text: String::new(),
    }
}
fn replacing(action: &str, entity: &str, provider: &str, session: &str) -> Command {
    Command {
        replaces_provider: provider.into(),
        replaces_session: session.into(),
        ..command(action, entity)
    }
}

/// Starting play elsewhere needs the confirmation fields; the daemon stops
/// the audible provider, confirms it stopped and only then starts the new
/// one, which becomes the active session.
#[tokio::test]
async fn a_start_elsewhere_needs_confirmation_and_stops_the_audible_session_first() {
    let f = Fixture::new(Fake::playing("old"), Fake::default()).await;
    assert_eq!(f.active().await.unwrap().provider_id, "a");
    assert_eq!(error(f.send("b", &command("play", "x")).await), "Conflict");
    assert!(f.a.actions().is_empty() && f.b.actions().is_empty());
    let state = ok(f.send("b", &replacing("play", "x", "a", "old")).await);
    assert_eq!(state.playback, "playing");
    assert_eq!(f.a.actions(), ["stop"]);
    assert_eq!(f.b.actions(), ["play"]);
    let active = f.active().await.unwrap();
    assert_eq!(active.provider_id, "b");
    assert_eq!(active.state.session, "session-x");
}

/// A failed stop leaves the new source unstarted.
#[tokio::test]
async fn a_failed_stop_leaves_the_new_source_unstarted() {
    let a = Fake::playing("old");
    a.0.lock().unwrap().fail = Some(("stop".into(), ErrorCode::Unavailable));
    let f = Fixture::new(a, Fake::default()).await;
    assert_eq!(
        error(f.send("b", &replacing("play", "x", "a", "old")).await),
        "Unavailable"
    );
    assert!(f.b.actions().is_empty());
    assert_eq!(f.active().await.unwrap().provider_id, "a");
}

/// A retry with the same request ID and payload replays the receipt without
/// forwarding again; the same ID with another payload is a conflict.
#[tokio::test]
async fn receipts_replay_retries_and_reject_a_reused_request_id() {
    let f = Fixture::new(Fake::default(), Fake::default()).await;
    let play = command("play", "x");
    let first = ok(f.send("b", &play).await);
    let again = ok(f.send("b", &play).await);
    assert_eq!(
        music::encode(&again).unwrap(),
        music::encode(&first).unwrap()
    );
    assert_eq!(f.b.actions(), ["play"]);
    let reused = Command {
        entity: "y".into(),
        ..play.clone()
    };
    assert_eq!(error(f.send("b", &reused).await), "Conflict");
    // Another endpoint's traffic does not evict this endpoint's receipts.
    for _ in 0..40 {
        ok(f.send("a", &command("scan", "")).await);
    }
    assert_eq!(
        music::encode(&ok(f.send("b", &play).await)).unwrap(),
        music::encode(&first).unwrap()
    );
    assert_eq!(f.b.actions(), ["play"]);
}

/// Review of #710: a start that timed out left GetActive and the replacement
/// fence on the previous provider while the new one played, so the next
/// play started a second stream. The timed-out provider is now active, its
/// state is confirmed before the next start, and the timeout is not stored
/// as a receipt.
#[tokio::test]
async fn a_timed_out_start_becomes_active_and_fences_the_next_start() {
    let a = Fake::default();
    a.set("ended", "ended");
    let b = Fake::default();
    b.0.lock().unwrap().delay = Some(("play".into(), Duration::from_millis(5500)));
    let f = Fixture::new(a, b).await;
    let play = command("play", "x");
    assert_eq!(error(f.send("b", &play).await), "Timeout");
    // The provider finished starting after the deadline.
    tokio::time::sleep(Duration::from_millis(800)).await;
    assert_eq!(f.b.0.lock().unwrap().state.playback, "playing");
    // Another start must confirm replacing b, not a.
    assert_eq!(error(f.send("a", &command("play", "y")).await), "Conflict");
    assert_eq!(f.active().await.unwrap().provider_id, "b");
    assert!(!f.a.actions().contains(&"play".to_owned()));
    // The timeout was not a receipt: the retry reaches the provider again.
    f.b.0.lock().unwrap().delay = None;
    ok(f.send("b", &play).await);
    assert_eq!(f.b.actions(), ["play", "play"]);
}

/// Review of #710: next/previous/resume/retry on a provider with an ended
/// session could start audio while another provider played.
#[tokio::test]
async fn resuming_an_inactive_provider_needs_confirmation_while_another_plays() {
    let a = Fake::default();
    a.set("ended", "ended");
    let f = Fixture::new(a, Fake::playing("live")).await;
    assert_eq!(f.active().await.unwrap().provider_id, "b");
    for action in ["next", "previous", "resume", "retry"] {
        let mut resume = command(action, "");
        resume.session = "ended".into();
        assert_eq!(error(f.send("a", &resume).await), "Conflict", "{action}");
    }
    assert!(f.a.actions().is_empty());
    // Transport on the audible provider itself needs no confirmation.
    let mut next = command("next", "");
    next.session = "live".into();
    ok(f.send("b", &next).await);
    assert_eq!(f.b.actions(), ["next"]);
    // With confirmation the inactive provider resumes after b stops.
    let mut resume = replacing("next", "", "b", "live");
    resume.session = "ended".into();
    ok(f.send("a", &resume).await);
    assert_eq!(f.b.actions(), ["next", "stop"]);
    assert_eq!(f.a.actions(), ["next"]);
    assert_eq!(f.active().await.unwrap().provider_id, "a");
}

/// GetActive prefers a confirmed audible session over a stale pointer.
#[tokio::test]
async fn get_active_prefers_an_audible_session() {
    let a = Fake::default();
    a.set("ended", "ended");
    let f = Fixture::new(a, Fake::playing("live")).await;
    f.core.music.registry.lock().await.active = Some("a".into());
    assert_eq!(f.active().await.unwrap().provider_id, "b");
}

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

/// The daemon's music exports match the published interface files.
#[tokio::test]
async fn the_daemon_music_exports_match_the_published_xml() {
    let f = Fixture::new(Fake::default(), Fake::default()).await;
    for (path, interface, file) in [
        (
            "/org/arinn/Daemon1/providers/p_a",
            "org.arinn.Music1",
            "music1.xml",
        ),
        (
            production::PATH,
            "org.arinn.MusicController1",
            "music-controller1.xml",
        ),
    ] {
        let live = zbus::fdo::IntrospectableProxy::builder(&f.client)
            .destination(production::SERVICE)
            .unwrap()
            .path(path)
            .unwrap()
            .build()
            .await
            .unwrap()
            .introspect()
            .await
            .unwrap();
        let published = std::fs::read_to_string(format!(
            "{}/../../docs/plugin-api/{file}",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap();
        let mut live = super::super::media_routing::tests::methods(&live, interface);
        let mut published = super::super::media_routing::tests::methods(&published, interface);
        live.sort();
        published.sort();
        assert_eq!(live, published, "{interface}");
        assert!(!published.is_empty());
    }
}
