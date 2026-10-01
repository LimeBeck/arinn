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
    window_feature: Arc<std::sync::atomic::AtomicBool>,
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
        let mut features = vec![
            FEATURE_TRACKS.into(),
            arinn_protocol::assets::FEATURE_ASSETS.into(),
            "FutureFlag9".into(),
        ];
        if self
            .window_feature
            .load(std::sync::atomic::Ordering::SeqCst)
        {
            features.insert(1, arinn_protocol::media_window::FEATURE.into());
        }
        features
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
/// Serves one PNG and one image that is not what it declares.
#[derive(Clone)]
struct FakeAssets;
#[zbus::interface(name = "org.arinn.AssetProvider1")]
impl FakeAssets {
    fn get_asset(
        &self,
        _generation: &str,
        id: &str,
    ) -> production::Result<(arinn_protocol::assets::Asset,)> {
        let png = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0];
        match id {
            "art-1" => Ok((arinn_protocol::assets::Asset {
                mime: "image/png".into(),
                data: png,
            },)),
            "art-bad" => Ok((arinn_protocol::assets::Asset {
                mime: "image/jpeg".into(),
                data: png,
            },)),
            _ => Err(ErrorCode::NotFound.into()),
        }
    }
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
    window_feature: Arc<std::sync::atomic::AtomicBool>,
    bus: Child,
    folder: PathBuf,
    core: Core,
    client: Connection,
    provider: Option<Connection>,
    calls: Arc<StdMutex<Vec<String>>>,
    generation: String,
    path: OwnedObjectPath,
    window_gate: Arc<WindowGate>,
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
        let window_feature = Arc::new(std::sync::atomic::AtomicBool::new(true));
        provider
            .object_server()
            .at(
                production::PROVIDER_PATH,
                FakeMedia {
                    window_feature: window_feature.clone(),
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
        provider
            .object_server()
            .at(production::PROVIDER_PATH, FakeAssets)
            .await
            .unwrap();
        let window_gate = Arc::new(WindowGate::default());
        provider
            .object_server()
            .at(production::PROVIDER_PATH, FakeWindow(window_gate.clone()))
            .await
            .unwrap();
        core.export_platform().await.unwrap();
        let path: OwnedObjectPath = "/org/arinn/Daemon1/providers/p_media".try_into().unwrap();
        server
            .object_server()
            .at(
                path.clone(),
                super::super::assets::AssetsEndpoint {
                    core: core.clone(),
                    id: "mpv".into(),
                },
            )
            .await
            .unwrap();
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
        server
            .object_server()
            .at(
                path.clone(),
                MediaWindowEndpoint {
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
                settings_schema: None,
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
            window_feature,
            bus,
            folder,
            core,
            client,
            provider: Some(provider),
            calls,
            generation,
            path,
            window_gate,
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
    assert_eq!(
        api.features().await.unwrap(),
        vec![
            FEATURE_TRACKS,
            arinn_protocol::media_window::FEATURE,
            arinn_protocol::assets::FEATURE_ASSETS
        ]
    );
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

async fn assets_api(f: &Fixture) -> arinn_protocol::assets::AssetsApiProxy<'static> {
    arinn_protocol::assets::AssetsApiProxy::builder(&f.client)
        .path(f.path.clone())
        .unwrap()
        .build()
        .await
        .unwrap()
}

#[tokio::test]
async fn assets_are_served_and_validated_again() {
    let f = Fixture::new().await;
    let assets = assets_api(&f).await;
    let (asset,) = assets.get_asset(&f.generation, "art-1").await.unwrap();
    assert_eq!(asset.mime, "image/png");
    assert_eq!(
        error_name(assets.get_asset(&f.generation, "art-404").await),
        "org.arinn.Error1.NotFound"
    );
    // A provider not built on the SDK could send this; the daemon stops it.
    assert_eq!(
        error_name(assets.get_asset(&f.generation, "art-bad").await),
        "org.arinn.Error1.Internal"
    );
    assert_eq!(
        error_name(assets.get_asset("stale", "art-1").await),
        "org.arinn.Error1.Conflict"
    );
    assert_eq!(
        error_name(assets.get_asset(&f.generation, "").await),
        "org.arinn.Error1.InvalidArgument"
    );
}

#[tokio::test]
async fn the_daemon_export_matches_the_published_assets1_xml() {
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
        "/../../docs/plugin-api/assets1.xml"
    ))
    .unwrap();
    assert_eq!(
        methods(&live, "org.arinn.Assets1"),
        methods(&file, "org.arinn.Assets1")
    );
}

#[tokio::test]
async fn removing_a_provider_path_removes_every_daemon_interface() {
    let f = Fixture::new().await;
    f.core.remove_endpoints(&f.path).await;
    let live = zbus::fdo::IntrospectableProxy::builder(&f.client)
        .destination(production::SERVICE)
        .unwrap()
        .path(f.path.clone())
        .unwrap()
        .build()
        .await
        .unwrap()
        .introspect()
        .await;
    // With every interface gone the object itself disappears.
    match live {
        Err(zbus::fdo::Error::UnknownObject(_)) => {}
        Ok(xml) => {
            for interface in ["org.arinn.Media1", "org.arinn.Assets1"] {
                assert!(!xml.contains(interface), "{interface} still exported");
            }
        }
        Err(other) => panic!("{other:?}"),
    }
}

#[derive(Default)]
struct WindowGate {
    blocked: std::sync::atomic::AtomicBool,
    entered: tokio::sync::Notify,
    resume: tokio::sync::Notify,
}
struct FakeWindow(Arc<WindowGate>);
const LAUNCH: &str = "aaaaaaaa-0000-4000-8000-000000000001";
#[zbus::interface(name = "org.arinn.MediaWindowProvider1")]
impl FakeWindow {
    async fn get_launch_id(&self, _generation: &str, _id: &str) -> String {
        if self.0.blocked.load(std::sync::atomic::Ordering::SeqCst) {
            self.0.entered.notify_one();
            self.0.resume.notified().await;
        }
        LAUNCH.into()
    }
}
#[tokio::test]
async fn managed_playback_routes_only_complete_exact_inventory_and_tears_down_after_stop() {
    use arinn_protocol::{
        kwin::KWinBridgeProxy,
        media_window::{KWinMediaProxy, MediaWindowProxy},
    };
    let f = Fixture::new().await;
    f.client.request_name("org.kde.KWin").await.unwrap();
    let bridge = KWinBridgeProxy::new(&f.client).await.unwrap();
    let media = KWinMediaProxy::new(&f.client).await.unwrap();
    let api = MediaWindowProxy::builder(&f.client)
        .path(f.path.clone())
        .unwrap()
        .build()
        .await
        .unwrap();
    let instance = Uuid::new_v4().to_string();
    let g = bridge.connect(&instance).await.unwrap();
    bridge.begin_inventory(&g, 1, 2).await.unwrap();
    bridge
        .put_window(&g, 1, 0, "video", "", "screen", 6)
        .await
        .unwrap();
    bridge
        .put_window(&g, 1, 1, "other", "", "screen", 4)
        .await
        .unwrap();
    bridge.commit_inventory(&g, 1).await.unwrap();
    media.report(&g, 1, "video", LAUNCH).await.unwrap();
    assert!(
        api.get_window(&f.generation, "s1")
            .await
            .unwrap()
            .is_empty()
    );
    media.report(&g, 1, "other", LAUNCH).await.unwrap();
    assert!(
        api.get_window(&f.generation, "s1")
            .await
            .unwrap()
            .is_empty()
    );
    media.report(&g, 1, "other", "").await.unwrap();
    let refs = api.get_window(&f.generation, "s1").await.unwrap();
    assert_eq!(refs[0].token, "video");
    assert_eq!(refs[0].generation, g);
    // Tokio's FIFO state mutex puts this mutation after the second validated
    // read, but before the final association fence. Retained sessions alone
    // cannot authorize an unavailable or no-longer-negotiated provider.
    for (change, expected) in [
        ("backend", ErrorCode::Unavailable),
        ("runtime", ErrorCode::Unavailable),
        ("feature", ErrorCode::Unsupported),
    ] {
        assert_eq!(bridge.connect(&instance).await.unwrap(), g);
        f.window_gate
            .blocked
            .store(true, std::sync::atomic::Ordering::SeqCst);
        let mut read = Box::pin(f.core.playback_window("mpv", &f.generation, "s1"));
        assert!(
            tokio::time::timeout(Duration::from_millis(50), &mut read)
                .await
                .is_err()
        );
        f.window_gate.entered.notified().await;
        let held = f.core.state.lock().await;
        f.window_gate.resume.notify_one();
        assert!(
            tokio::time::timeout(Duration::from_millis(50), &mut read)
                .await
                .is_err()
        );
        let mut mutation = Box::pin(f.core.state.lock());
        std::future::poll_fn(|cx| {
            assert!(std::future::Future::poll(mutation.as_mut(), cx).is_pending());
            std::task::Poll::Ready(())
        })
        .await;
        drop(held);
        std::future::poll_fn(|cx| {
            assert!(std::future::Future::poll(read.as_mut(), cx).is_pending());
            std::task::Poll::Ready(())
        })
        .await;
        let mut state = mutation.await;
        let provider = state.providers.get_mut("mpv").unwrap();
        match change {
            "backend" => provider.info.backend_state = "unavailable".into(),
            "runtime" => provider.info.runtime_state = "restarting".into(),
            _ => provider
                .features
                .retain(|f| f != arinn_protocol::media_window::FEATURE),
        }
        drop(state);
        assert_eq!(read.await.unwrap_err(), expected, "final fence: {change}");
        let mut state = f.core.state.lock().await;
        let provider = state.providers.get_mut("mpv").unwrap();
        provider.info.backend_state = "available".into();
        provider.info.runtime_state = "ready".into();
        if change == "feature" {
            provider
                .features
                .push(arinn_protocol::media_window::FEATURE.into());
        }
        drop(state);
        f.window_gate
            .blocked
            .store(false, std::sync::atomic::Ordering::SeqCst);
    }
    assert!(api.get_window(&f.generation, "missing").await.is_err());
    assert!(
        api.present(&f.generation, "s1", &Uuid::new_v4().to_string())
            .await
            .unwrap_err()
            .to_string()
            .contains("Unsupported")
    );
    let idle_at = tokio::time::Instant::now();
    assert_eq!(bridge.connect(&instance).await.unwrap(), g);
    assert!(media.next_control(&g).await.unwrap().is_empty());
    assert!(
        idle_at.elapsed() >= Duration::from_millis(900),
        "idle poll must not spin"
    );
    let mut poll = Box::pin(media.next_control(&g));
    assert!(
        tokio::time::timeout(Duration::from_millis(50), &mut poll)
            .await
            .is_err()
    );
    media.report(&g, 1, "other", "").await.unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(50), &mut poll)
            .await
            .is_err(),
        "association-only wake must retain the idle poll"
    );
    assert!(
        media
            .next_control(&g)
            .await
            .unwrap_err()
            .to_string()
            .contains("Conflict")
    );
    let request = Uuid::new_v4().to_string();
    let op = api.present(&f.generation, "s1", &request).await.unwrap();
    assert_eq!(
        api.present(&f.generation, "s1", &request).await.unwrap(),
        op
    );
    let command = tokio::time::timeout(Duration::from_millis(500), poll)
        .await
        .expect("enqueue wakes pending poll")
        .unwrap();
    assert_eq!(&command[1..], &["present", "video"]);
    // Present itself changes fullscreen/focus inventory. The same identity
    // must remain acknowledged rather than invalidating its own operation.
    bridge.begin_inventory(&g, 2, 2).await.unwrap();
    bridge
        .put_window(&g, 2, 0, "video", "", "screen", 7)
        .await
        .unwrap();
    bridge
        .put_window(&g, 2, 1, "other", "", "screen", 4)
        .await
        .unwrap();
    bridge.commit_inventory(&g, 2).await.unwrap();
    let acknowledged = std::sync::atomic::AtomicBool::new(false);
    let (ack, ()) = tokio::join!(
        async {
            let result = media.complete_control(&g, &command[0], "").await;
            acknowledged.store(true, std::sync::atomic::Ordering::SeqCst);
            result
        },
        async {
            tokio::time::sleep(Duration::from_millis(30)).await;
            assert!(
                !acknowledged.load(std::sync::atomic::Ordering::SeqCst),
                "success cannot bypass the new identity barrier"
            );
            media.report(&g, 2, "video", LAUNCH).await.unwrap();
            media.report(&g, 2, "other", "").await.unwrap();
        }
    );
    ack.unwrap();
    f.wait(op).await.unwrap();
    // Stop is independent of presentation; a cleared launch still releases
    // the remembered scope. Do not request the provider identity on teardown.
    f.core
        .state
        .lock()
        .await
        .providers
        .get_mut("mpv")
        .unwrap()
        .sessions[0]
        .state = "stopped".into();
    let release = api
        .release(&f.generation, "s1", &Uuid::new_v4().to_string())
        .await
        .unwrap();
    let command = media.next_control(&g).await.unwrap();
    assert_eq!(&command[1..], &["release", "video"]);
    media.complete_control(&g, &command[0], "").await.unwrap();
    f.wait(release).await.unwrap();
    let release = api
        .release(&f.generation, "s1", &Uuid::new_v4().to_string())
        .await
        .unwrap();
    f.wait(release).await.unwrap();
    assert_eq!(bridge.connect(&instance).await.unwrap(), g);
    assert!(media.next_control(&g).await.unwrap().is_empty());
    assert!(media.report(&g, 1, "video", LAUNCH).await.is_err());
    assert!(media.report(&g, 2, "missing", LAUNCH).await.is_err());
    // Release must fence Present while its provider identity read is still
    // blocked, even before it has any queued compositor command to tear down.
    f.window_gate
        .blocked
        .store(true, std::sync::atomic::Ordering::SeqCst);
    f.core
        .state
        .lock()
        .await
        .providers
        .get_mut("mpv")
        .unwrap()
        .sessions[0]
        .state = "playing".into();
    let show_request = Uuid::new_v4().to_string();
    let (late_show, ()) = tokio::join!(api.present(&f.generation, "s1", &show_request), async {
        f.window_gate.entered.notified().await;
        let hide = api
            .release(&f.generation, "s1", &Uuid::new_v4().to_string())
            .await
            .unwrap();
        f.wait(hide).await.unwrap();
        f.window_gate.resume.notify_one();
    });
    assert!(late_show.unwrap_err().to_string().contains("Conflict"));
    assert_eq!(bridge.connect(&instance).await.unwrap(), g);
    assert!(media.next_control(&g).await.unwrap().is_empty());
    f.window_gate
        .blocked
        .store(false, std::sync::atomic::Ordering::SeqCst);
    // A fence must also be captured before the first async admission check.
    // Hold the provider lock so this continuation cannot reach GetLaunchId,
    // then emulate the exit fence already advanced by a validated Release.
    let admission_lock = f.core.state.lock().await;
    let request = Uuid::new_v4().to_string();
    let caller = f.client.unique_name().unwrap().to_string();
    let mut delayed =
        Box::pin(
            f.core
                .playback_command("mpv", &f.generation, "s1", &request, &caller, true),
        );
    std::future::poll_fn(|cx| {
        assert!(std::future::Future::poll(delayed.as_mut(), cx).is_pending());
        std::task::Poll::Ready(())
    })
    .await;
    {
        let mut exits = f.core.playback_exits.lock().await;
        *exits
            .entry(("mpv".into(), f.generation.clone(), "s1".into()))
            .or_default() += 1;
    }
    drop(admission_lock);
    assert_eq!(delayed.await.unwrap_err(), ErrorCode::Conflict);
    assert_eq!(bridge.connect(&instance).await.unwrap(), g);
    assert!(media.next_control(&g).await.unwrap().is_empty());
    // Exit can be admitted before Present reaches the adapter; preserve order.
    f.core
        .state
        .lock()
        .await
        .providers
        .get_mut("mpv")
        .unwrap()
        .sessions[0]
        .state = "playing".into();
    let show = api
        .present(&f.generation, "s1", &Uuid::new_v4().to_string())
        .await
        .unwrap();
    let hide = api
        .release(&f.generation, "s1", &Uuid::new_v4().to_string())
        .await
        .unwrap();
    let command = media.next_control(&g).await.unwrap();
    assert_eq!(command[1], "present");
    media.complete_control(&g, &command[0], "").await.unwrap();
    f.wait(show).await.unwrap();
    let command = media.next_control(&g).await.unwrap();
    assert_eq!(command[1], "release");
    media.complete_control(&g, &command[0], "").await.unwrap();
    f.wait(hide).await.unwrap();
    // A stopped/removed imported session fences delivery and acknowledgement.
    let show = api
        .present(&f.generation, "s1", &Uuid::new_v4().to_string())
        .await
        .unwrap();
    let command = media.next_control(&g).await.unwrap();
    assert_eq!(command[1], "present");
    f.core
        .windows
        .sync_media_sessions(
            &crate::windows::Scope {
                provider: "mpv".into(),
                owner: f
                    .provider
                    .as_ref()
                    .unwrap()
                    .unique_name()
                    .unwrap()
                    .to_string(),
                registration: f.generation.clone(),
            },
            &[],
        )
        .await;
    assert_eq!(f.wait(show).await.unwrap_err(), "Unavailable");
    assert!(media.complete_control(&g, &command[0], "").await.is_err());
    let command = media.next_control(&g).await.unwrap();
    assert_eq!(command[1], "release");
    media.complete_control(&g, &command[0], "").await.unwrap();
    // Stall the final provider-lifetime check while the same generation's
    // inventory removes the resolved video. An obsolete read must not escape.
    f.window_gate
        .blocked
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let (result, ()) = tokio::join!(api.get_window(&f.generation, "s1"), async {
        f.window_gate.entered.notified().await;
        let fence = f.core.state.lock().await;
        f.window_gate.resume.notify_one();
        tokio::time::sleep(Duration::from_millis(50)).await;
        bridge.begin_inventory(&g, 3, 1).await.unwrap();
        bridge
            .put_window(&g, 3, 0, "other", "", "screen", 4)
            .await
            .unwrap();
        bridge.commit_inventory(&g, 3).await.unwrap();
        media.report(&g, 3, "other", "").await.unwrap();
        drop(fence);
    });
    assert!(
        result.unwrap().is_empty(),
        "GetWindow must read the authoritative inventory after its last lifetime wait"
    );
    // A connection that is not the current KWin owner cannot forge reports.
    let stranger = KWinMediaProxy::new(f.provider.as_ref().unwrap())
        .await
        .unwrap();
    assert!(
        stranger
            .report(&g, 2, "video", LAUNCH)
            .await
            .unwrap_err()
            .to_string()
            .contains("PermissionDenied")
    );
    // A delivered presentation loses its window into an empty inventory:
    // no later Report exists to trigger cleanup, so commit must schedule it.
    f.window_gate
        .blocked
        .store(false, std::sync::atomic::Ordering::SeqCst);
    bridge.begin_inventory(&g, 4, 1).await.unwrap();
    bridge
        .put_window(&g, 4, 0, "new-video", "", "screen", 6)
        .await
        .unwrap();
    bridge.commit_inventory(&g, 4).await.unwrap();
    media.report(&g, 4, "new-video", LAUNCH).await.unwrap();
    let show = api
        .present(&f.generation, "s1", &Uuid::new_v4().to_string())
        .await
        .unwrap();
    let command = media.next_control(&g).await.unwrap();
    assert_eq!(command[1], "present");
    bridge.begin_inventory(&g, 5, 0).await.unwrap();
    bridge.commit_inventory(&g, 5).await.unwrap();
    assert_eq!(f.wait(show).await.unwrap_err(), "NotFound");
    assert!(media.complete_control(&g, &command[0], "").await.is_err());
    let cleanup = media.next_control(&g).await.unwrap();
    assert_eq!(&cleanup[1..], &["release", ""]);
    media.complete_control(&g, &cleanup[0], "").await.unwrap();
    bridge.begin_inventory(&g, 6, 1).await.unwrap();
    bridge
        .put_window(&g, 6, 0, "final-video", "", "screen", 6)
        .await
        .unwrap();
    bridge.commit_inventory(&g, 6).await.unwrap();
    media.report(&g, 6, "final-video", LAUNCH).await.unwrap();
    // A failed dispatch may have partially applied flags. Clean exactly that
    // scope once, then leave subsequent explicit Release as a no-op.
    let show = api
        .present(&f.generation, "s1", &Uuid::new_v4().to_string())
        .await
        .unwrap();
    let command = media.next_control(&g).await.unwrap();
    media
        .complete_control(&g, &command[0], "Unavailable")
        .await
        .unwrap();
    assert_eq!(f.wait(show).await.unwrap_err(), "Unavailable");
    let cleanup = media.next_control(&g).await.unwrap();
    assert_eq!(&cleanup[1..], &["release", "final-video"]);
    media.complete_control(&g, &cleanup[0], "").await.unwrap();
    let hide = api
        .release(&f.generation, "s1", &Uuid::new_v4().to_string())
        .await
        .unwrap();
    f.wait(hide).await.unwrap();
    assert_eq!(bridge.connect(&instance).await.unwrap(), g);
    assert!(media.next_control(&g).await.unwrap().is_empty());
    assert_eq!(bridge.connect(&instance).await.unwrap(), g);
    // Provider loss must wake the independently held media poll immediately.
    let mut invalidations = api.receive_changed().await.unwrap();
    let show = api
        .present(&f.generation, "s1", &Uuid::new_v4().to_string())
        .await
        .unwrap();
    let command = media.next_control(&g).await.unwrap();
    media.complete_control(&g, &command[0], "").await.unwrap();
    f.wait(show).await.unwrap();
    // Drain earlier invalidations before observing only provider loss.
    while tokio::time::timeout(Duration::from_millis(20), invalidations.next())
        .await
        .is_ok()
    {}
    let mut poll = Box::pin(media.next_control(&g));
    assert!(
        tokio::time::timeout(Duration::from_millis(50), &mut poll)
            .await
            .is_err()
    );
    f.core
        .windows
        .disconnect(f.provider.as_ref().unwrap().unique_name().unwrap().as_str())
        .await;
    let cleanup = tokio::time::timeout(Duration::from_millis(500), poll)
        .await
        .expect("provider loss wakes media poll")
        .unwrap();
    assert_eq!(&cleanup[1..], &["release", "final-video"]);
    tokio::time::timeout(Duration::from_millis(500), invalidations.next())
        .await
        .expect("provider loss invalidates MediaWindow subscribers")
        .expect("signal stream remains live");
    media.complete_control(&g, &cleanup[0], "").await.unwrap();
}

#[tokio::test]
async fn unchanged_media_poll_does_not_invalidate_window() {
    let f = Fixture::new().await;
    let api = arinn_protocol::media_window::MediaWindowProxy::builder(&f.client)
        .path(f.path.clone())
        .unwrap()
        .build()
        .await
        .unwrap();
    let mut changes = api.receive_changed().await.unwrap();
    while tokio::time::timeout(Duration::from_millis(30), changes.next())
        .await
        .is_ok()
    {}
    assert!(
        tokio::time::timeout(Duration::from_millis(2200), changes.next())
            .await
            .is_err(),
        "unchanged provider polling must not invalidate the window"
    );
}

#[tokio::test]
async fn feature_removal_cleans_pending_and_active_presentation() {
    use arinn_protocol::{
        kwin::KWinBridgeProxy,
        media_window::{KWinMediaProxy, MediaWindowProxy},
    };
    for acknowledged in [false, true] {
        let f = Fixture::new().await;
        f.client.request_name("org.kde.KWin").await.unwrap();
        let bridge = KWinBridgeProxy::new(&f.client).await.unwrap();
        let media = KWinMediaProxy::new(&f.client).await.unwrap();
        let api = MediaWindowProxy::builder(&f.client)
            .path(f.path.clone())
            .unwrap()
            .build()
            .await
            .unwrap();
        let g = bridge.connect(&rid()).await.unwrap();
        bridge.begin_inventory(&g, 1, 1).await.unwrap();
        bridge
            .put_window(&g, 1, 0, "video", "", "screen", 6)
            .await
            .unwrap();
        bridge.commit_inventory(&g, 1).await.unwrap();
        media.report(&g, 1, "video", LAUNCH).await.unwrap();
        assert!(media.next_control(&g).await.unwrap().is_empty());
        let operation = api.present(&f.generation, "s1", &rid()).await.unwrap();
        let present = media.next_control(&g).await.unwrap();
        assert_eq!(present[1], "present");
        if acknowledged {
            media.complete_control(&g, &present[0], "").await.unwrap();
            f.wait(operation.clone()).await.unwrap();
        }
        f.window_feature
            .store(false, std::sync::atomic::Ordering::SeqCst);
        for _ in 0..100 {
            if !f.core.state.lock().await.providers["mpv"]
                .features
                .iter()
                .any(|v| v == arinn_protocol::media_window::FEATURE)
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        let cleanup = media.next_control(&g).await.unwrap();
        assert_eq!(
            cleanup.len(),
            3,
            "feature removal must release exact presentation"
        );
        assert_eq!(&cleanup[1..], &["release", "video"]);
        media.complete_control(&g, &cleanup[0], "").await.unwrap();
        if !acknowledged {
            assert_eq!(f.wait(operation).await.unwrap_err(), "Unavailable");
        }
        assert!(
            f.calls.lock().unwrap().is_empty(),
            "cleanup must not send Stop"
        );
    }
}
