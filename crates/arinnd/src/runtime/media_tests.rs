// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Daemon routing for `media` providers (ADR-0058) on a private bus.
use super::*;
use arinn_protocol::{
    media::MediaApiProxy,
    production::{EntityRef, ProductionOperationProxy},
};
use std::{
    io::{BufRead, BufReader},
    process::{Child, Command, Stdio},
    sync::Mutex as StdMutex,
};

fn session(id: &str, actions: &[&str]) -> Session {
    Session {
        id: id.into(),
        title: "Film".into(),
        subtitle: String::new(),
        artwork: String::new(),
        state: "playing".into(),
        duration_us: 5_400_000_000,
        position_us: 60_000_000,
        anchor_us: 1,
        actions: actions.iter().map(|a| a.to_string()).collect(),
        has_content: false,
        content: EntityRef {
            provider_id: String::new(),
            capability: String::new(),
            local_id: String::new(),
        },
    }
}

#[derive(Clone)]
struct FakeMedia {
    generation: String,
    calls: Arc<StdMutex<Vec<String>>>,
}
#[zbus::interface(name = "org.arinn.MediaProvider1")]
impl FakeMedia {
    fn get_sessions(&self, _token: &str) -> production::Result<(SessionPage,)> {
        Ok((SessionPage {
            generation: self.generation.clone(),
            revision: 1,
            sessions: vec![session("s1", &["play", "pause", "seek"])],
            next_token: String::new(),
        },))
    }
    fn backend_state(&self) -> String {
        "available".into()
    }
    fn features(&self) -> Vec<String> {
        // An unknown flag is ignored by the daemon.
        vec![FEATURE_TRACKS.into(), "FutureFlag9".into()]
    }
    fn play(&self, _generation: &str, id: &str) -> production::Result<()> {
        self.calls.lock().unwrap().push(format!("play {id}"));
        Ok(())
    }
    fn seek(&self, _generation: &str, id: &str, position_us: i64) -> production::Result<()> {
        self.calls
            .lock()
            .unwrap()
            .push(format!("seek {id} {position_us}"));
        Ok(())
    }
    #[zbus(signal)]
    async fn state_changed(
        emitter: &SignalEmitter<'_>,
        generation: &str,
        revision: u64,
    ) -> zbus::Result<()>;
}
#[derive(Clone)]
struct FakeTracks {
    calls: Arc<StdMutex<Vec<String>>>,
}
#[zbus::interface(name = "org.arinn.MediaTracks1")]
impl FakeTracks {
    fn list_tracks(&self, _generation: &str, _id: &str) -> production::Result<Vec<Track>> {
        Ok(vec![Track {
            id: "a1".into(),
            kind: "audio".into(),
            language: "ru".into(),
            label: "Russian".into(),
            codec: "aac".into(),
            channels: 2,
            selected: true,
            external: false,
        }])
    }
    fn select_track(
        &self,
        _generation: &str,
        id: &str,
        kind: &str,
        track_id: &str,
    ) -> production::Result<()> {
        self.calls
            .lock()
            .unwrap()
            .push(format!("select {id} {kind} {track_id}"));
        Ok(())
    }
    fn disable_subtitles(&self, _generation: &str, id: &str) -> production::Result<()> {
        self.calls
            .lock()
            .unwrap()
            .push(format!("subtitles off {id}"));
        Ok(())
    }
}

struct Fixture {
    bus: Child,
    folder: PathBuf,
    core: Core,
    client: Connection,
    provider: Option<Connection>,
    calls: Arc<StdMutex<Vec<String>>>,
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
impl Fixture {
    async fn new() -> Self {
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
        let provider = connect(&address).await;
        let client = connect(&address).await;
        server.request_name(production::SERVICE).await.unwrap();
        let folder = std::env::temp_dir().join(format!("arinn-media-{}", Uuid::new_v4()));
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
        let calls: Arc<StdMutex<Vec<String>>> = Arc::default();
        provider
            .object_server()
            .at(
                production::PROVIDER_PATH,
                FakeMedia {
                    generation: generation.clone(),
                    calls: calls.clone(),
                },
            )
            .await
            .unwrap();
        provider
            .object_server()
            .at(
                production::PROVIDER_PATH,
                FakeTracks {
                    calls: calls.clone(),
                },
            )
            .await
            .unwrap();
        let path: OwnedObjectPath = "/org/arinn/Daemon1/providers/p_media".try_into().unwrap();
        server
            .object_server()
            .at(
                path.clone(),
                MediaEndpoint {
                    core: core.clone(),
                    id: "mpv".into(),
                },
            )
            .await
            .unwrap();
        core.state.lock().await.providers.insert(
            "mpv".into(),
            Provider {
                info: ProviderInfo {
                    id: "mpv".into(),
                    plugin_id: "mpv".into(),
                    source_path: String::new(),
                    name: "Player".into(),
                    version: "0.0.1".into(),
                    path: path.clone(),
                    capabilities: vec!["media".into()],
                    runtime_state: "starting".into(),
                    backend_state: "unavailable".into(),
                    generation: generation.clone(),
                    revision: 0,
                    last_error: String::new(),
                    error_at: 0,
                    recovered_at: 0,
                    restarts_since_daemon_start: 0,
                    cumulative_restart_count: 0,
                },
                manifest: None,
                unit: "mpv.service".into(),
                token_file: folder.join("mpv.token"),
                token: String::new(),
                owner: provider.unique_name().unwrap().to_string(),
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
            },
        );
        let watcher = core.clone();
        let (owner, registration) = (
            provider.unique_name().unwrap().to_string(),
            generation.clone(),
        );
        tokio::spawn(async move {
            watcher
                .watch_provider("mpv".into(), owner, registration)
                .await
        });
        let f = Self {
            bus,
            folder,
            core,
            client,
            provider: Some(provider),
            calls,
            generation,
            path,
        };
        for _ in 0..100 {
            if f.core.state.lock().await.providers["mpv"].sessions.len() == 1 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        f
    }
    async fn api(&self) -> MediaApiProxy<'static> {
        MediaApiProxy::builder(&self.client)
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
        for _ in 0..100 {
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
async fn sessions_are_served_and_commands_reach_the_provider() {
    let f = Fixture::new().await;
    let api = f.api().await;
    let (page,) = api.get_sessions("").await.unwrap();
    assert_eq!(page.sessions.len(), 1);
    assert_eq!(page.generation, f.generation);
    assert_eq!(api.features().await.unwrap(), vec![FEATURE_TRACKS]);
    let (one,) = api.get_session(&f.generation, "s1").await.unwrap();
    assert_eq!(one.actions, ["play", "pause", "seek"]);

    let path = api.play(&f.generation, "s1", &rid()).await.unwrap();
    let results = f.wait(path).await.unwrap();
    assert_eq!(results[0].local_id, "s1");
    let path = api
        .seek(&f.generation, "s1", 120_000_000, &rid())
        .await
        .unwrap();
    f.wait(path).await.unwrap();
    assert_eq!(*f.calls.lock().unwrap(), ["play s1", "seek s1 120000000"]);
}

#[tokio::test]
async fn missing_actions_and_features_are_unsupported() {
    let f = Fixture::new().await;
    let api = f.api().await;
    let g = f.generation.as_str();
    // `next` is not in the session's actions: rejected before any call.
    assert_eq!(
        error_name(api.next(g, "s1", &rid()).await),
        "org.arinn.Error1.Unsupported"
    );
    assert_eq!(
        error_name(api.seek(g, "s1", -1, &rid()).await),
        "org.arinn.Error1.InvalidArgument"
    );
    assert_eq!(
        error_name(api.play(g, "s404", &rid()).await),
        "org.arinn.Error1.NotFound"
    );
    assert_eq!(
        error_name(api.play("stale", "s1", &rid()).await),
        "org.arinn.Error1.Conflict"
    );
    // No MediaTimeline1 flag.
    assert_eq!(
        error_name(api.get_timeline(g, "s1").await),
        "org.arinn.Error1.Unsupported"
    );
    assert_eq!(
        error_name(api.request_thumbnail(g, "s1", 0, &rid()).await),
        "org.arinn.Error1.Unsupported"
    );
    assert!(f.calls.lock().unwrap().is_empty());
}

#[tokio::test]
async fn tracks_are_listed_and_selected_through_the_extension() {
    let f = Fixture::new().await;
    let api = f.api().await;
    let g = f.generation.as_str();
    let tracks = api.list_tracks(g, "s1").await.unwrap();
    assert_eq!(tracks[0].id, "a1");
    assert_eq!(
        error_name(api.select_track(g, "s1", "video", "a1", &rid()).await),
        "org.arinn.Error1.InvalidArgument"
    );
    let path = api
        .select_track(g, "s1", "audio", "a1", &rid())
        .await
        .unwrap();
    f.wait(path).await.unwrap();
    let path = api.disable_subtitles(g, "s1", &rid()).await.unwrap();
    f.wait(path).await.unwrap();
    assert_eq!(
        *f.calls.lock().unwrap(),
        ["select s1 audio a1", "subtitles off s1"]
    );
}

#[tokio::test]
async fn a_lost_provider_makes_media_unavailable() {
    let mut f = Fixture::new().await;
    let owner = f
        .provider
        .as_ref()
        .unwrap()
        .unique_name()
        .unwrap()
        .to_string();
    drop(f.provider.take());
    f.core.unavailable("mpv", &owner).await;
    let api = f.api().await;
    assert_eq!(
        error_name(api.play(&f.generation, "s1", &rid()).await),
        "org.arinn.Error1.Unavailable"
    );
    assert!(
        f.core.state.lock().await.providers["mpv"]
            .sessions
            .is_empty()
    );
}

/// Methods of one interface as `name(direction:type, …)`, in document order.
pub(crate) fn methods(xml: &str, interface: &str) -> Vec<String> {
    let start = xml
        .find(&format!("<interface name=\"{interface}\">"))
        .expect("interface present");
    let body = &xml[start..];
    let body = &body[..body.find("</interface>").unwrap()];
    body.split("<method name=\"")
        .skip(1)
        .map(|method| {
            let name = &method[..method.find('"').unwrap()];
            let method = &method[..method.find("</method>").unwrap_or(method.len())];
            let args: Vec<String> = method
                .split("<arg ")
                .skip(1)
                .map(|arg| {
                    let attr = |key: &str| {
                        arg.split(&format!("{key}=\""))
                            .nth(1)
                            .map(|v| v[..v.find('"').unwrap()].to_owned())
                            .unwrap_or_else(|| "in".into())
                    };
                    format!("{}:{}", attr("direction"), attr("type"))
                })
                .collect();
            format!("{name}({})", args.join(","))
        })
        .collect()
}

#[tokio::test]
async fn the_daemon_export_matches_the_published_media1_xml() {
    let f = Fixture::new().await;
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
        "/../../docs/plugin-api/media1.xml"
    ))
    .unwrap();
    let mut live = methods(&live, "org.arinn.Media1");
    let mut file = methods(&file, "org.arinn.Media1");
    live.sort();
    file.sort();
    assert_eq!(live, file);
    assert!(file.len() >= 14);
}
