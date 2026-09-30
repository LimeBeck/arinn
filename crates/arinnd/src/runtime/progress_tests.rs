// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Progress forwarding from library-linked media sessions to
//! `LibraryResume1.SetProgress` (ADR-0061), on a private bus.
use super::*;
use arinn_protocol::{
    library::{Collection, Item, LibraryPage, Resume, ResumePage},
    media::{PlaybackSource, SessionPage},
    production::EntityRef,
};
use std::{
    io::{BufRead, BufReader},
    process::{Child, Command, Stdio},
    sync::Mutex as StdMutex,
};

type Calls = Arc<StdMutex<Vec<(String, i64, i64, bool)>>>;

fn session(id: &str, state: &str, position_us: i64, linked: bool) -> Session {
    Session {
        id: id.into(),
        title: "Film".into(),
        subtitle: String::new(),
        artwork: String::new(),
        state: state.into(),
        duration_us: 5_400_000_000,
        position_us,
        anchor_us: monotonic_us(),
        actions: vec!["play".into(), "pause".into()],
        has_content: linked,
        content: if linked {
            EntityRef {
                provider_id: "library".into(),
                capability: "library".into(),
                local_id: "film-1".into(),
            }
        } else {
            EntityRef {
                provider_id: String::new(),
                capability: String::new(),
                local_id: String::new(),
            }
        },
    }
}

#[derive(Clone)]
struct FakePlayer {
    generation: String,
    sessions: Arc<StdMutex<(u64, Vec<Session>)>>,
}
#[zbus::interface(name = "org.arinn.MediaProvider1")]
impl FakePlayer {
    fn get_sessions(&self, _token: &str) -> production::Result<(SessionPage,)> {
        let (revision, sessions) = self.sessions.lock().unwrap().clone();
        Ok((SessionPage {
            generation: self.generation.clone(),
            revision,
            sessions,
            next_token: String::new(),
        },))
    }
    fn backend_state(&self) -> String {
        "available".into()
    }
    fn features(&self) -> Vec<String> {
        Vec::new()
    }
}

#[derive(Clone)]
struct FakeLibrary {
    generation: String,
}
#[zbus::interface(name = "org.arinn.LibraryProvider1")]
impl FakeLibrary {
    fn get_library(&self, _token: &str) -> production::Result<(LibraryPage,)> {
        Ok((LibraryPage {
            generation: self.generation.clone(),
            revision: 1,
            collections: vec![Collection {
                id: "films".into(),
                title: "Films".into(),
                kind: "film".into(),
            }],
            items: vec![Item {
                id: "film-1".into(),
                collection_id: "films".into(),
                parent_id: String::new(),
                title: "Film".into(),
                kind: "film".into(),
                has_year: false,
                year: 0,
                has_season: false,
                season: 0,
                has_episode: false,
                episode: 0,
                duration_us: 5_400_000_000,
                poster: String::new(),
                fanart: String::new(),
                available: true,
            }],
            next_token: String::new(),
        },))
    }
    fn backend_state(&self) -> String {
        "available".into()
    }
    fn features(&self) -> Vec<String> {
        vec![FEATURE_RESUME.into()]
    }
    fn launch(&self, _generation: &str, _id: &str) -> production::Result<(PlaybackSource,)> {
        Err(ErrorCode::Unsupported.into())
    }
}
#[derive(Clone)]
struct FakeResume {
    generation: String,
    calls: Calls,
}
#[zbus::interface(name = "org.arinn.LibraryResume1")]
impl FakeResume {
    fn get_resumes(&self, _token: &str) -> production::Result<(ResumePage,)> {
        Ok((ResumePage {
            generation: self.generation.clone(),
            revision: 1,
            resumes: Vec::<Resume>::new(),
            next_token: String::new(),
        },))
    }
    fn set_progress(
        &self,
        _generation: &str,
        content_id: &str,
        position_us: i64,
        duration_us: i64,
        ended: bool,
    ) -> production::Result<()> {
        self.calls
            .lock()
            .unwrap()
            .push((content_id.into(), position_us, duration_us, ended));
        Ok(())
    }
}

struct Fixture {
    bus: Child,
    folder: PathBuf,
    _core: Core,
    _providers: Vec<Connection>,
    sessions: Arc<StdMutex<(u64, Vec<Session>)>>,
    calls: Calls,
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
fn provider(
    id: &str,
    capability: &str,
    owner: &Connection,
    generation: &str,
    folder: &std::path::Path,
) -> Provider {
    Provider {
        info: ProviderInfo {
            id: id.into(),
            plugin_id: id.into(),
            source_path: String::new(),
            name: id.into(),
            version: "0.0.1".into(),
            path: format!("/org/arinn/Daemon1/providers/p_{id}")
                .try_into()
                .unwrap(),
            capabilities: vec![capability.into()],
            runtime_state: "starting".into(),
            backend_state: "unavailable".into(),
            generation: generation.into(),
            revision: 0,
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
    async fn new(initial: Vec<Session>) -> Self {
        let mut bus = Command::new("dbus-daemon")
            .args(["--session", "--nofork", "--print-address=1"])
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let mut address = String::new();
        BufReader::new(bus.stdout.take().unwrap())
            .read_line(&mut address)
            .unwrap();
        let server = connect(&address).await;
        let library = connect(&address).await;
        let player = connect(&address).await;
        server.request_name(production::SERVICE).await.unwrap();
        let folder = std::env::temp_dir().join(format!("arinn-progress-{}", Uuid::new_v4()));
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
        let (lg, pg) = (Uuid::new_v4().to_string(), Uuid::new_v4().to_string());
        let calls: Calls = Arc::default();
        let sessions = Arc::new(StdMutex::new((1, initial)));
        library
            .object_server()
            .at(
                production::PROVIDER_PATH,
                FakeLibrary {
                    generation: lg.clone(),
                },
            )
            .await
            .unwrap();
        library
            .object_server()
            .at(
                production::PROVIDER_PATH,
                FakeResume {
                    generation: lg.clone(),
                    calls: calls.clone(),
                },
            )
            .await
            .unwrap();
        player
            .object_server()
            .at(
                production::PROVIDER_PATH,
                FakePlayer {
                    generation: pg.clone(),
                    sessions: sessions.clone(),
                },
            )
            .await
            .unwrap();
        {
            let mut state = core.state.lock().await;
            state.providers.insert(
                "library".into(),
                provider("library", "library", &library, &lg, &folder),
            );
            state.providers.insert(
                "player".into(),
                provider("player", "media", &player, &pg, &folder),
            );
        }
        // The library must be ready before the player's first import, or the
        // first forward would find no resume store.
        let watcher = core.clone();
        let owner = library.unique_name().unwrap().to_string();
        tokio::spawn(async move { watcher.watch_provider("library".into(), owner, lg).await });
        for _ in 0..150 {
            if core.state.lock().await.providers["library"]
                .info
                .runtime_state
                == "ready"
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        let watcher = core.clone();
        let owner = player.unique_name().unwrap().to_string();
        tokio::spawn(async move { watcher.watch_provider("player".into(), owner, pg).await });
        Self {
            bus,
            folder,
            _core: core,
            _providers: vec![library, player],
            sessions,
            calls,
        }
    }
    /// Replace the player's sessions; the 1 s watcher tick picks them up.
    fn set(&self, sessions: Vec<Session>) {
        let mut guard = self.sessions.lock().unwrap();
        guard.0 += 1;
        guard.1 = sessions;
    }
    async fn wait_calls(&self, count: usize) -> Vec<(String, i64, i64, bool)> {
        for _ in 0..200 {
            if self.calls.lock().unwrap().len() >= count {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        self.calls.lock().unwrap().clone()
    }
}
const S: i64 = 1_000_000;

#[tokio::test]
async fn pause_seek_and_stop_are_forwarded_and_ended_is_never_claimed() {
    let f = Fixture::new(vec![session("s1", "paused", 60 * S, true)]).await;
    // First sight of a linked session.
    let calls = f.wait_calls(1).await;
    assert_eq!(calls[0], ("film-1".into(), 60 * S, 5_400 * S, false));
    // Nothing changed: no further calls over two watcher ticks.
    tokio::time::sleep(Duration::from_millis(2_300)).await;
    assert_eq!(f.calls.lock().unwrap().len(), 1);
    // A seek while paused.
    f.set(vec![session("s1", "paused", 600 * S, true)]);
    let calls = f.wait_calls(2).await;
    assert_eq!(calls[1].1, 600 * S);
    // Stop: the session disappears; its last position is forwarded.
    f.set(Vec::new());
    let calls = f.wait_calls(3).await;
    assert_eq!(calls[2].1, 600 * S);
    assert!(calls.iter().all(|c| !c.3), "a pause or stop is never ended");
    assert_eq!(calls.len(), 3);
}

#[tokio::test]
async fn a_confirmed_end_is_forwarded_as_ended() {
    let f = Fixture::new(vec![session("s1", "playing", 5_390 * S, true)]).await;
    let calls = f.wait_calls(1).await;
    assert!(!calls[0].3);
    // The player confirms the end of the file.
    f.set(vec![session("s1", "ended", 5_400 * S, true)]);
    let calls = f.wait_calls(2).await;
    assert_eq!(calls[1], ("film-1".into(), 5_400 * S, 5_400 * S, true));
    // Staying ended sends nothing more.
    tokio::time::sleep(Duration::from_millis(2_300)).await;
    assert_eq!(f.calls.lock().unwrap().len(), 2);
}

#[tokio::test]
async fn a_playing_session_is_forwarded_periodically_from_its_anchor() {
    let mut playing = session("s1", "playing", 60 * S, true);
    // Sampled 10 s ago: the forwarded position is extrapolated.
    playing.anchor_us = monotonic_us() - 10 * S as u64;
    let f = Fixture::new(vec![playing]).await;
    let calls = f.wait_calls(1).await;
    assert!((calls[0].1 - 70 * S).abs() < 2 * S, "{}", calls[0].1);
    // While playing, at least every PROGRESS_INTERVAL.
    let calls = f.wait_calls(2).await;
    let deadline = Instant::now() + PROGRESS_INTERVAL + Duration::from_secs(3);
    let mut calls = calls;
    while calls.len() < 2 && Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(100)).await;
        calls = f.calls.lock().unwrap().clone();
    }
    assert!(calls.len() >= 2);
    assert!(calls[1].1 > calls[0].1);
}

#[tokio::test]
async fn sessions_without_library_content_are_not_forwarded() {
    let f = Fixture::new(vec![session("s1", "playing", 60 * S, false)]).await;
    tokio::time::sleep(Duration::from_millis(2_500)).await;
    assert!(f.calls.lock().unwrap().is_empty());
}
