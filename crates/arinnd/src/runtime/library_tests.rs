// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Daemon routing for `library` providers and library launch through
//! `MediaOpen1` (ADR-0058) on a private bus.
use super::*;
use arinn_protocol::{
    library::LibraryApiProxy,
    media::{PlaybackSource, Session, SessionPage},
    production::{EntityRef, ProductionOperationProxy},
};
use std::{
    io::{BufRead, BufReader},
    process::{Child, Command, Stdio},
    sync::Mutex as StdMutex,
};

const URI: &str = "file:///srv/films/film.mkv";

fn item(id: &str, available: bool) -> Item {
    Item {
        id: id.into(),
        collection_id: "films".into(),
        parent_id: String::new(),
        title: "Film".into(),
        kind: "film".into(),
        has_year: true,
        year: 2024,
        has_season: false,
        season: 0,
        has_episode: false,
        episode: 0,
        duration_us: 5_400_000_000,
        poster: String::new(),
        fanart: String::new(),
        available,
    }
}

#[derive(Clone)]
struct FakeLibrary {
    generation: String,
    resume: bool,
    calls: Arc<StdMutex<Vec<String>>>,
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
            items: vec![item("film-1", true), item("film-2", false)],
            next_token: String::new(),
        },))
    }
    fn backend_state(&self) -> String {
        "available".into()
    }
    fn features(&self) -> Vec<String> {
        if self.resume {
            vec![FEATURE_RESUME.into()]
        } else {
            Vec::new()
        }
    }
    fn launch(&self, _generation: &str, id: &str) -> production::Result<(PlaybackSource,)> {
        self.calls.lock().unwrap().push(format!("launch {id}"));
        Ok((PlaybackSource {
            local_id: id.into(),
            uri: URI.into(),
            start_position_us: 60_000_000,
            audio_language: "ru".into(),
            subtitle_language: String::new(),
        },))
    }
    #[zbus(signal)]
    async fn state_changed(
        emitter: &SignalEmitter<'_>,
        generation: &str,
        revision: u64,
    ) -> zbus::Result<()>;
}
#[derive(Clone)]
struct FakeResume {
    generation: String,
    calls: Arc<StdMutex<Vec<String>>>,
}
#[zbus::interface(name = "org.arinn.LibraryResume1")]
impl FakeResume {
    fn get_resumes(&self, _token: &str) -> production::Result<(ResumePage,)> {
        Ok((ResumePage {
            generation: self.generation.clone(),
            revision: 1,
            resumes: vec![Resume {
                content_id: "film-1".into(),
                position_us: 60_000_000,
                duration_us: 5_400_000_000,
                completed: false,
                observed_at_us: 1,
            }],
            next_token: String::new(),
        },))
    }
    fn restart_from_beginning(&self, _generation: &str, id: &str) -> production::Result<()> {
        self.calls.lock().unwrap().push(format!("restart {id}"));
        Ok(())
    }
}

#[derive(Clone)]
struct FakePlayer {
    generation: String,
    open: bool,
}
#[zbus::interface(name = "org.arinn.MediaProvider1")]
impl FakePlayer {
    fn get_sessions(&self, _token: &str) -> production::Result<(SessionPage,)> {
        Ok((SessionPage {
            generation: self.generation.clone(),
            revision: 1,
            sessions: Vec::<Session>::new(),
            next_token: String::new(),
        },))
    }
    fn backend_state(&self) -> String {
        "available".into()
    }
    fn features(&self) -> Vec<String> {
        if self.open {
            vec![FEATURE_OPEN.into()]
        } else {
            Vec::new()
        }
    }
    #[zbus(signal)]
    async fn state_changed(
        emitter: &SignalEmitter<'_>,
        generation: &str,
        revision: u64,
    ) -> zbus::Result<()>;
}
#[derive(Clone)]
struct FakeOpen {
    opened: Arc<StdMutex<Vec<MediaSource>>>,
}
#[zbus::interface(name = "org.arinn.MediaOpen1")]
impl FakeOpen {
    fn open(&self, _generation: &str, source: MediaSource) -> production::Result<String> {
        self.opened.lock().unwrap().push(source);
        Ok("session-7".into())
    }
}

struct Fixture {
    bus: Child,
    folder: PathBuf,
    core: Core,
    client: Connection,
    _providers: Vec<Connection>,
    calls: Arc<StdMutex<Vec<String>>>,
    opened: Arc<StdMutex<Vec<MediaSource>>>,
    generation: String,
    path: OwnedObjectPath,
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
    path: &OwnedObjectPath,
    folder: &std::path::Path,
) -> Provider {
    Provider {
        info: ProviderInfo {
            id: id.into(),
            plugin_id: id.into(),
            source_path: String::new(),
            name: id.into(),
            version: "0.0.1".into(),
            path: path.clone(),
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
        windows: Vec::new(),
        tabs: Vec::new(),
        started: Instant::now(),
        enabled: true,
    }
}
impl Fixture {
    async fn new(resume: bool, player_can_open: bool) -> Self {
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
        let client = connect(&address).await;
        server.request_name(production::SERVICE).await.unwrap();
        let folder = std::env::temp_dir().join(format!("arinn-library-{}", Uuid::new_v4()));
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
        let player_generation = Uuid::new_v4().to_string();
        let calls: Arc<StdMutex<Vec<String>>> = Arc::default();
        let opened: Arc<StdMutex<Vec<MediaSource>>> = Arc::default();
        let objects = library.object_server();
        objects
            .at(
                production::PROVIDER_PATH,
                FakeLibrary {
                    generation: generation.clone(),
                    resume,
                    calls: calls.clone(),
                },
            )
            .await
            .unwrap();
        objects
            .at(
                production::PROVIDER_PATH,
                FakeResume {
                    generation: generation.clone(),
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
                    generation: player_generation.clone(),
                    open: player_can_open,
                },
            )
            .await
            .unwrap();
        player
            .object_server()
            .at(
                production::PROVIDER_PATH,
                FakeOpen {
                    opened: opened.clone(),
                },
            )
            .await
            .unwrap();
        let path: OwnedObjectPath = "/org/arinn/Daemon1/providers/p_library".try_into().unwrap();
        let player_path: OwnedObjectPath =
            "/org/arinn/Daemon1/providers/p_player".try_into().unwrap();
        server
            .object_server()
            .at(
                path.clone(),
                LibraryEndpoint {
                    core: core.clone(),
                    id: "library".into(),
                },
            )
            .await
            .unwrap();
        {
            let mut state = core.state.lock().await;
            state.providers.insert(
                "library".into(),
                provider("library", "library", &library, &generation, &path, &folder),
            );
            state.providers.insert(
                "player".into(),
                provider(
                    "player",
                    "media",
                    &player,
                    &player_generation,
                    &player_path,
                    &folder,
                ),
            );
        }
        for (id, conn, generation) in [
            ("library", &library, generation.clone()),
            ("player", &player, player_generation.clone()),
        ] {
            let watcher = core.clone();
            let owner = conn.unique_name().unwrap().to_string();
            tokio::spawn(async move { watcher.watch_provider(id.into(), owner, generation).await });
        }
        let f = Self {
            bus,
            folder,
            core,
            client,
            _providers: vec![library, player],
            calls,
            opened,
            generation,
            path,
        };
        for _ in 0..150 {
            {
                let state = f.core.state.lock().await;
                if state.providers["library"].items.len() == 2
                    && state.providers["player"].info.runtime_state == "ready"
                {
                    break;
                }
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        f
    }
    async fn api(&self) -> LibraryApiProxy<'static> {
        LibraryApiProxy::builder(&self.client)
            .path(self.path.clone())
            .unwrap()
            .build()
            .await
            .unwrap()
    }
    async fn wait(&self, path: OwnedObjectPath) -> Result<Vec<EntityRef>, String> {
        let operation = ProductionOperationProxy::builder(&self.client)
            .path(path)
            .unwrap()
            .build()
            .await
            .unwrap();
        for _ in 0..150 {
            let info = operation.get().await.unwrap().0;
            match info.state.as_str() {
                "succeeded" => return Ok(info.results),
                "failed" | "cancelled" => return Err(info.error),
                _ => tokio::time::sleep(Duration::from_millis(20)).await,
            }
        }
        panic!("operation did not finish");
    }
}
fn rid() -> String {
    Uuid::new_v4().to_string()
}
fn error_name<T: std::fmt::Debug>(result: zbus::Result<T>) -> String {
    match result {
        Err(zbus::Error::MethodError(name, _, _)) => name.to_string(),
        other => format!("{other:?}"),
    }
}

#[tokio::test]
async fn the_catalogue_and_resumes_are_served() {
    let f = Fixture::new(true, true).await;
    let api = f.api().await;
    let (page,) = api.get_library("").await.unwrap();
    assert_eq!((page.collections.len(), page.items.len()), (1, 2));
    assert_eq!(api.features().await.unwrap(), vec![FEATURE_RESUME]);
    let (one,) = api.get_item(&f.generation, "film-2").await.unwrap();
    assert!(!one.available);
    let (resumes,) = api.get_resumes("").await.unwrap();
    assert_eq!(resumes.resumes[0].content_id, "film-1");
    let path = api
        .restart_from_beginning(&f.generation, "film-1", &rid())
        .await
        .unwrap();
    f.wait(path).await.unwrap();
    assert_eq!(*f.calls.lock().unwrap(), ["restart film-1"]);
}

#[tokio::test]
async fn a_launch_opens_the_item_in_the_media_provider() {
    let f = Fixture::new(false, true).await;
    let api = f.api().await;
    let path = api.launch(&f.generation, "film-1", &rid()).await.unwrap();
    let results = f.wait(path).await.unwrap();
    // The shell gets the new media session, never the file URI.
    assert_eq!(
        results,
        [EntityRef {
            provider_id: "player".into(),
            capability: "media".into(),
            local_id: "session-7".into(),
        }]
    );
    assert!(!format!("{results:?}").contains("file://"));
    let opened = f.opened.lock().unwrap().clone();
    assert_eq!(opened.len(), 1);
    assert_eq!(opened[0].uri, URI);
    assert_eq!(opened[0].start_position_us, 60_000_000);
    assert_eq!(opened[0].audio_language, "ru");
    assert_eq!(
        opened[0].content,
        EntityRef {
            provider_id: "library".into(),
            capability: "library".into(),
            local_id: "film-1".into(),
        }
    );
}

#[tokio::test]
async fn launch_and_resume_are_checked_before_any_provider_call() {
    let f = Fixture::new(false, true).await;
    let api = f.api().await;
    let g = f.generation.as_str();
    assert_eq!(
        error_name(api.launch(g, "film-2", &rid()).await),
        "org.arinn.Error1.Unavailable"
    );
    assert_eq!(
        error_name(api.launch(g, "film-404", &rid()).await),
        "org.arinn.Error1.NotFound"
    );
    assert_eq!(
        error_name(api.launch("stale", "film-1", &rid()).await),
        "org.arinn.Error1.Conflict"
    );
    // No LibraryResume1.
    assert_eq!(
        error_name(api.get_resumes("").await),
        "org.arinn.Error1.Unsupported"
    );
    assert_eq!(
        error_name(api.restart_from_beginning(g, "film-1", &rid()).await),
        "org.arinn.Error1.Unsupported"
    );
    assert!(f.calls.lock().unwrap().is_empty());
}

#[tokio::test]
async fn a_launch_without_a_player_that_can_open_fails_unavailable() {
    let f = Fixture::new(false, false).await;
    let api = f.api().await;
    let path = api.launch(&f.generation, "film-1", &rid()).await.unwrap();
    assert_eq!(f.wait(path).await, Err("Unavailable".into()));
    assert!(f.opened.lock().unwrap().is_empty());
}

#[tokio::test]
async fn the_daemon_export_matches_the_published_library1_xml() {
    let f = Fixture::new(true, true).await;
    let live = zbus::fdo::IntrospectableProxy::builder(&f.client)
        .destination(production::SERVICE)
        .unwrap()
        .path(f.path.clone())
        .unwrap()
        .build()
        .await
        .unwrap()
        .introspect()
        .await
        .unwrap();
    let file = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/plugin-api/library1.xml"
    ))
    .unwrap();
    let mut live = super::super::media_routing::tests::methods(&live, "org.arinn.Library1");
    let mut file = super::super::media_routing::tests::methods(&file, "org.arinn.Library1");
    live.sort();
    file.sort();
    assert_eq!(live, file);
    assert!(file.len() >= 6);
}
