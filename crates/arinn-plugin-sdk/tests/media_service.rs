// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! `MediaService` registers `media`, serves the base contract only to the
//! daemon, and exports rich interfaces only when asked (ADR-0054).
mod common;
use arinn_plugin_sdk::runtime::{
    self, AssetHandler, MediaAction, MediaExtensions, MediaHandler, MediaOpenHandler, MediaService,
    MediaTimelineHandler, MediaTracksHandler,
};
use arinn_protocol::{
    assets::{Asset, AssetProviderProxy},
    media::{
        self, MediaOpenProxy, MediaProviderProxy, MediaSource, MediaTimelineProxy,
        MediaTracksProxy, Session, Thumbnail, Timeline, Track,
    },
    production::{self, EntityRef, Registration, RegistrationReply},
    validation::ErrorCode,
};
use std::{
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
    time::Duration,
};
use zbus::message::Header;

#[derive(Clone, Default)]
struct Handler(Arc<Mutex<Vec<String>>>);
impl MediaHandler for Handler {
    fn control(
        &self,
        action: MediaAction,
        id: String,
        position_us: i64,
    ) -> Pin<Box<dyn Future<Output = Result<(), ErrorCode>> + Send + '_>> {
        self.0
            .lock()
            .unwrap()
            .push(format!("{action:?}:{id}:{position_us}"));
        Box::pin(async { Ok(()) })
    }
}
#[derive(Clone, Default)]
/// Serves one valid PNG, one image whose bytes are not what it declares,
/// and nothing else.
struct Assets;
impl AssetHandler for Assets {
    fn asset(
        &self,
        id: String,
    ) -> Pin<Box<dyn Future<Output = Result<Asset, ErrorCode>> + Send + '_>> {
        let png = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0].to_vec();
        Box::pin(async move {
            match id.as_str() {
                "art-1" => Ok(Asset {
                    mime: "image/png".into(),
                    data: png,
                }),
                "art-disguised" => Ok(Asset {
                    mime: "image/jpeg".into(),
                    data: png,
                }),
                _ => Err(ErrorCode::NotFound),
            }
        })
    }
}
struct Open;
impl MediaOpenHandler for Open {
    fn open(
        &self,
        source: MediaSource,
    ) -> Pin<Box<dyn Future<Output = Result<String, ErrorCode>> + Send + '_>> {
        let id = source.content.local_id;
        Box::pin(async move { Ok(format!("session-{id}")) })
    }
}
#[derive(Clone, Default)]
struct Tracks(Arc<Mutex<Vec<String>>>);
impl MediaTracksHandler for Tracks {
    fn list(
        &self,
        id: String,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<Track>, ErrorCode>> + Send + '_>> {
        self.0.lock().unwrap().push(format!("list:{id}"));
        Box::pin(async {
            Ok(vec![Track {
                id: "audio-1".into(),
                kind: media::TRACK_AUDIO.into(),
                language: "en".into(),
                label: "Stereo".into(),
                codec: "aac".into(),
                channels: 2,
                selected: true,
                external: false,
            }])
        })
    }
    fn select(
        &self,
        id: String,
        kind: String,
        track_id: String,
    ) -> Pin<Box<dyn Future<Output = Result<(), ErrorCode>> + Send + '_>> {
        self.0
            .lock()
            .unwrap()
            .push(format!("select:{id}:{kind}:{track_id}"));
        Box::pin(async { Ok(()) })
    }
    fn disable_subtitles(
        &self,
        id: String,
    ) -> Pin<Box<dyn Future<Output = Result<(), ErrorCode>> + Send + '_>> {
        self.0.lock().unwrap().push(format!("off:{id}"));
        Box::pin(async { Ok(()) })
    }
}
#[derive(Clone, Default)]
struct TimelineHandler(Arc<Mutex<Vec<String>>>);
impl MediaTimelineHandler for TimelineHandler {
    fn timeline(
        &self,
        id: String,
    ) -> Pin<Box<dyn Future<Output = Result<Timeline, ErrorCode>> + Send + '_>> {
        self.0.lock().unwrap().push(format!("timeline:{id}"));
        Box::pin(async {
            Ok(Timeline {
                chapters: vec![],
                ranges: vec![],
            })
        })
    }
    fn thumbnail(
        &self,
        _id: String,
        _position_us: i64,
    ) -> Pin<Box<dyn Future<Output = Result<Thumbnail, ErrorCode>> + Send + '_>> {
        Box::pin(async { Err(ErrorCode::Unsupported) })
    }
}

#[derive(Clone, Default)]
struct Daemon(Arc<Mutex<Vec<String>>>);
#[zbus::interface(name = "org.arinn.Daemon1")]
impl Daemon {
    async fn register(
        &self,
        request: Registration,
        #[zbus(header)] _header: Header<'_>,
    ) -> production::Result<(RegistrationReply,)> {
        *self.0.lock().unwrap() = request.capabilities.clone();
        Ok((RegistrationReply {
            epoch: uuid::Uuid::new_v4().to_string(),
            generation: uuid::Uuid::new_v4().to_string(),
            api_minor: 0,
            max_body_bytes: 262144,
            max_page_entries: 200,
            max_operations: 32,
        },))
    }
}

fn session(actions: &[&str]) -> Session {
    Session {
        id: "s1".into(),
        title: "Film".into(),
        subtitle: String::new(),
        artwork: String::new(),
        state: "paused".into(),
        duration_us: 10_000_000,
        position_us: 0,
        anchor_us: 1,
        actions: actions.iter().map(|action| (*action).to_string()).collect(),
        has_content: false,
        content: EntityRef {
            provider_id: String::new(),
            capability: String::new(),
            local_id: String::new(),
        },
    }
}
fn error_name(result: zbus::Result<impl std::fmt::Debug>) -> String {
    match result {
        Err(zbus::Error::MethodError(name, _, _)) => name.to_string(),
        other => panic!("expected an Arinn error, got {other:?}"),
    }
}
fn interface_methods(xml: &str, interface: &str) -> Vec<String> {
    let Some(start) = xml.find(interface) else {
        return Vec::new();
    };
    let interface = &xml[start..xml[start..].find("</interface>").unwrap() + start];
    let mut names: Vec<String> = interface
        .split("<method name=\"")
        .skip(1)
        .chain(interface.split("<signal name=\"").skip(1))
        .map(|chunk| chunk[..chunk.find('"').unwrap()].to_string())
        .collect();
    names.sort();
    names
}

async fn base_scenario() {
    let bus = common::Bus::start();
    let server = bus.connection().await;
    let daemon = Daemon::default();
    server
        .object_server()
        .at(production::PATH, daemon.clone())
        .await
        .unwrap();
    server.request_name(production::SERVICE).await.unwrap();
    let provider = bus.connection().await;
    let handler = Handler::default();
    let service = MediaService::new(handler.clone(), vec![], "unavailable").unwrap();
    let mut registrations = service.registrations();
    let runner = tokio::spawn(runtime::run_media_on(provider.clone(), service.clone()));
    let context = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Some(context) = registrations.borrow_and_update().clone() {
                return context;
            }
            registrations.changed().await.unwrap();
        }
    })
    .await
    .unwrap();
    assert_eq!(*daemon.0.lock().unwrap(), vec!["media".to_string()]);
    let generation = context.generation;
    let proxy = MediaProviderProxy::builder(&server)
        .destination(provider.unique_name().unwrap().clone())
        .unwrap()
        .build()
        .await
        .unwrap();
    let (page,) = proxy.get_sessions("").await.unwrap();
    assert_eq!((page.revision, page.sessions.len()), (1, 0));
    assert!(proxy.features().await.unwrap().is_empty());
    assert_eq!(
        error_name(proxy.play(&generation, "s1").await),
        "org.arinn.Error1.Unavailable"
    );

    service
        .replace(vec![session(&["play", "pause", "seek"])], "available")
        .await
        .unwrap();
    let (page,) = proxy.get_sessions("").await.unwrap();
    page.validate().unwrap();
    assert_eq!(page.revision, 2);
    proxy.play(&generation, "s1").await.unwrap();
    proxy.seek(&generation, "s1", 1_000).await.unwrap();
    assert_eq!(
        error_name(proxy.seek(&generation, "s1", 50_000_000).await),
        "org.arinn.Error1.InvalidArgument"
    );
    assert_eq!(
        error_name(proxy.next(&generation, "s1").await),
        "org.arinn.Error1.Unsupported"
    );
    assert_eq!(
        error_name(proxy.play(&generation, "missing").await),
        "org.arinn.Error1.NotFound"
    );
    assert_eq!(
        error_name(proxy.play(&uuid::Uuid::new_v4().to_string(), "s1").await),
        "org.arinn.Error1.Conflict"
    );
    assert_eq!(
        *handler.0.lock().unwrap(),
        vec!["Play:s1:0".to_string(), "Seek:s1:1000".to_string(),]
    );

    let stranger = MediaProviderProxy::builder(&bus.connection().await)
        .destination(provider.unique_name().unwrap().clone())
        .unwrap()
        .build()
        .await
        .unwrap();
    assert_eq!(
        error_name(stranger.get_sessions("").await),
        "org.arinn.Error1.PermissionDenied"
    );

    let introspect = zbus::fdo::IntrospectableProxy::builder(&server)
        .destination(provider.unique_name().unwrap().clone())
        .unwrap()
        .path(production::PROVIDER_PATH)
        .unwrap()
        .build()
        .await
        .unwrap();
    let live = introspect.introspect().await.unwrap();
    let file = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/plugin-api/media-provider1.xml"
    ))
    .unwrap();
    assert_eq!(
        interface_methods(&live, "org.arinn.MediaProvider1"),
        interface_methods(&file, "org.arinn.MediaProvider1")
    );
    assert!(live.contains("(sta(sssssxxtasb(sss))s)"));
    assert!(!live.contains("org.arinn.MediaOpen1"));
    assert!(!live.contains("org.arinn.MediaTracks1"));
    assert!(!live.contains("org.arinn.MediaTimeline1"));
    assert!(!live.contains("org.arinn.AssetProvider1"));
    runner.abort();
}

async fn rich_scenario() {
    let bus = common::Bus::start();
    let server = bus.connection().await;
    let daemon = Daemon::default();
    server
        .object_server()
        .at(production::PATH, daemon)
        .await
        .unwrap();
    server.request_name(production::SERVICE).await.unwrap();
    let provider = bus.connection().await;
    let extensions = MediaExtensions::default()
        .open(Open)
        .tracks(Tracks::default())
        .timeline(TimelineHandler::default())
        .assets(Assets);
    let service = MediaService::with_extensions(
        Handler::default(),
        vec![session(&["play"])],
        "available",
        extensions,
    )
    .unwrap();
    let mut registrations = service.registrations();
    let runner = tokio::spawn(runtime::run_media_on(provider.clone(), service));
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if registrations.borrow_and_update().is_some() {
                return;
            }
            registrations.changed().await.unwrap();
        }
    })
    .await
    .unwrap();
    let destination = provider.unique_name().unwrap().clone();
    let base = MediaProviderProxy::builder(&server)
        .destination(destination.clone())
        .unwrap()
        .build()
        .await
        .unwrap();
    let generation = base.get_sessions("").await.unwrap().0.generation;
    assert_eq!(
        base.features().await.unwrap(),
        vec![
            "MediaOpen1",
            "MediaTracks1",
            "MediaTimeline1",
            "AssetProvider1"
        ]
    );
    let assets = AssetProviderProxy::builder(&server)
        .destination(destination.clone())
        .unwrap()
        .build()
        .await
        .unwrap();
    let (asset,) = assets.get_asset(&generation, "art-1").await.unwrap();
    assert_eq!(asset.mime, "image/png");
    assert_eq!(
        error_name(assets.get_asset(&generation, "art-404").await),
        "org.arinn.Error1.NotFound"
    );
    // The provider's own bad image never reaches the daemon.
    assert_eq!(
        error_name(assets.get_asset(&generation, "art-disguised").await),
        "org.arinn.Error1.Internal"
    );
    assert_eq!(
        error_name(assets.get_asset(&generation, "").await),
        "org.arinn.Error1.InvalidArgument"
    );
    assert_eq!(
        error_name(assets.get_asset("stale", "art-1").await),
        "org.arinn.Error1.Conflict"
    );
    let open = MediaOpenProxy::builder(&server)
        .destination(destination.clone())
        .unwrap()
        .build()
        .await
        .unwrap();
    let source = MediaSource {
        content: EntityRef {
            provider_id: "library".into(),
            capability: "library".into(),
            local_id: "film-1".into(),
        },
        uri: "file:///media/film.mkv".into(),
        start_position_us: 0,
        audio_language: String::new(),
        subtitle_language: String::new(),
    };
    assert_eq!(
        open.open(&generation, source).await.unwrap(),
        "session-film-1"
    );
    let bad = MediaSource {
        content: EntityRef {
            provider_id: "library".into(),
            capability: "library".into(),
            local_id: "film-1".into(),
        },
        uri: "https://example.org/a.mkv".into(),
        start_position_us: 0,
        audio_language: String::new(),
        subtitle_language: String::new(),
    };
    assert_eq!(
        error_name(open.open(&generation, bad).await),
        "org.arinn.Error1.InvalidArgument"
    );
    let tracks = MediaTracksProxy::builder(&server)
        .destination(destination.clone())
        .unwrap()
        .build()
        .await
        .unwrap();
    assert_eq!(
        tracks.list_tracks(&generation, "s1").await.unwrap().len(),
        1
    );
    tracks
        .select_track(&generation, "s1", "audio", "audio-1")
        .await
        .unwrap();
    tracks.disable_subtitles(&generation, "s1").await.unwrap();
    assert_eq!(
        error_name(tracks.select_track(&generation, "s1", "audio", "").await),
        "org.arinn.Error1.InvalidArgument"
    );
    let timeline = MediaTimelineProxy::builder(&server)
        .destination(destination)
        .unwrap()
        .build()
        .await
        .unwrap();
    timeline.get_timeline(&generation, "s1").await.unwrap();
    assert_eq!(
        error_name(timeline.request_thumbnail(&generation, "s1", 0).await),
        "org.arinn.Error1.Unsupported"
    );
    let introspect = zbus::fdo::IntrospectableProxy::builder(&server)
        .destination(provider.unique_name().unwrap().clone())
        .unwrap()
        .path(production::PROVIDER_PATH)
        .unwrap()
        .build()
        .await
        .unwrap();
    let live = introspect.introspect().await.unwrap();
    for (interface, file) in [
        ("org.arinn.MediaOpen1", "media-open1.xml"),
        ("org.arinn.MediaTracks1", "media-tracks1.xml"),
        ("org.arinn.MediaTimeline1", "media-timeline1.xml"),
        ("org.arinn.AssetProvider1", "asset-provider1.xml"),
    ] {
        let xml = std::fs::read_to_string(format!(
            "{}/../../docs/plugin-api/{file}",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap();
        assert_eq!(
            interface_methods(&live, interface),
            interface_methods(&xml, interface),
            "{interface}"
        );
    }
    runner.abort();
}

fn child(flag: &str, name: &str) {
    if std::env::var_os(flag).is_some() {
        return;
    }
    let token = std::env::temp_dir().join(format!("arinn-media-{}", uuid::Uuid::new_v4()));
    std::fs::write(&token, "fixture-token").unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", name, "--nocapture"])
        .env(flag, "1")
        .env("ARINN_PROVIDER_ID", "fixture")
        .env("ARINN_PLUGIN_ID", "fixture")
        .env("ARINN_PLUGIN_VERSION", "0.0.1")
        .env("ARINN_TOKEN_FILE", &token)
        .output()
        .unwrap();
    std::fs::remove_file(token).unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn media_service_base_has_no_rich_interfaces() {
    if std::env::var_os("ARINN_MEDIA_BASE_CHILD").is_some() {
        tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(base_scenario());
        return;
    }
    child(
        "ARINN_MEDIA_BASE_CHILD",
        "media_service_base_has_no_rich_interfaces",
    );
}

#[test]
fn media_service_rich_interfaces_are_optional() {
    if std::env::var_os("ARINN_MEDIA_RICH_CHILD").is_some() {
        tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(rich_scenario());
        return;
    }
    child(
        "ARINN_MEDIA_RICH_CHILD",
        "media_service_rich_interfaces_are_optional",
    );
}
