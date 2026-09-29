// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! `LibraryService` registers `library` and keeps resume and import optional
//! (ADR-0054). Launch's file URI is not logged.
mod common;
use arinn_plugin_sdk::runtime::{
    self, LibraryExtensions, LibraryHandler, LibraryImportHandler, LibraryResumeHandler,
    LibraryService,
};
use arinn_protocol::{
    library::{
        Collection, ImportCandidate, Item, LibraryImportProxy, LibraryProviderProxy,
        LibraryResumeProxy,
    },
    media::PlaybackSource,
    production::{self, EntityRef, Registration, RegistrationReply},
    validation::ErrorCode,
};
use futures_util::StreamExt;
use std::{
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
    time::Duration,
};
use zbus::message::Header;

#[derive(Clone, Default)]
struct Handler(Arc<Mutex<Vec<String>>>);
impl LibraryHandler for Handler {
    fn launch(
        &self,
        id: String,
    ) -> Pin<Box<dyn Future<Output = Result<PlaybackSource, ErrorCode>> + Send + '_>> {
        self.0.lock().unwrap().push(id.clone());
        Box::pin(async move {
            Ok(PlaybackSource {
                local_id: id,
                uri: "file:///media/film.mkv".into(),
                start_position_us: 5_000_000,
                audio_language: String::new(),
                subtitle_language: "en".into(),
            })
        })
    }
}
#[derive(Clone, Default)]
struct Resume(Arc<Mutex<Vec<String>>>);
impl LibraryResumeHandler for Resume {
    fn set_progress(
        &self,
        id: String,
        position_us: i64,
        _duration_us: i64,
        ended: bool,
    ) -> Pin<Box<dyn Future<Output = Result<(), ErrorCode>> + Send + '_>> {
        self.0
            .lock()
            .unwrap()
            .push(format!("progress:{id}:{position_us}:{ended}"));
        Box::pin(async { Ok(()) })
    }
    fn restart(
        &self,
        id: String,
    ) -> Pin<Box<dyn Future<Output = Result<(), ErrorCode>> + Send + '_>> {
        self.0.lock().unwrap().push(format!("restart:{id}"));
        Box::pin(async { Ok(()) })
    }
}
#[derive(Clone, Default)]
struct Import(Arc<Mutex<usize>>);
impl LibraryImportHandler for Import {
    fn import(
        &self,
        candidates: Vec<ImportCandidate>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<String>, ErrorCode>> + Send + '_>> {
        *self.0.lock().unwrap() += 1;
        let ids = candidates
            .iter()
            .map(|candidate| candidate.completion_id.clone())
            .collect();
        Box::pin(async move { Ok(ids) })
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

fn collection() -> Collection {
    Collection {
        id: "films".into(),
        title: "Films".into(),
        kind: "film".into(),
    }
}
fn film(available: bool) -> Item {
    Item {
        id: "film-1".into(),
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
        duration_us: -1,
        poster: String::new(),
        fanart: String::new(),
        available,
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

async fn scenario() {
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
    let resume = Resume::default();
    let import = Import::default();
    let extensions = LibraryExtensions::default()
        .resume(resume.clone())
        .import(import.clone());
    let service = LibraryService::with_extensions(
        handler.clone(),
        vec![collection()],
        vec![film(true), {
            let mut missing = film(false);
            missing.id = "gone".into();
            missing
        }],
        "available",
        extensions,
    )
    .unwrap();
    let mut registrations = service.registrations();
    let runner = tokio::spawn(runtime::run_library_on(provider.clone(), service.clone()));
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
    assert_eq!(*daemon.0.lock().unwrap(), vec!["library".to_string()]);
    let destination = provider.unique_name().unwrap().clone();
    let proxy = LibraryProviderProxy::builder(&server)
        .destination(destination.clone())
        .unwrap()
        .build()
        .await
        .unwrap();
    let (page,) = proxy.get_library("").await.unwrap();
    page.validate().unwrap();
    assert_eq!(page.items.len(), 2);
    assert_eq!(
        proxy.features().await.unwrap(),
        vec!["LibraryResume1".to_string(), "LibraryImport1".to_string()]
    );
    let (item,) = proxy.get_item(&context.generation, "film-1").await.unwrap();
    assert!(item.available);
    let (source,) = proxy.launch(&context.generation, "film-1").await.unwrap();
    source.validate().unwrap();
    assert_eq!(source.local_id, "film-1");
    assert_eq!(
        error_name(proxy.launch(&context.generation, "gone").await),
        "org.arinn.Error1.Unavailable"
    );
    assert_eq!(
        error_name(proxy.launch(&context.generation, "missing").await),
        "org.arinn.Error1.NotFound"
    );

    let resumes = LibraryResumeProxy::builder(&server)
        .destination(destination.clone())
        .unwrap()
        .build()
        .await
        .unwrap();
    let mut changes = resumes.receive_resume_state_changed().await.unwrap();
    assert_eq!(resumes.get_resumes("").await.unwrap().0.resumes.len(), 0);
    resumes
        .set_progress(&context.generation, "film-1", 1_000, -1, false)
        .await
        .unwrap();
    resumes
        .restart_from_beginning(&context.generation, "gone")
        .await
        .unwrap();
    assert_eq!(
        error_name(
            resumes
                .set_progress(&context.generation, "film-1", -1, -1, false)
                .await
        ),
        "org.arinn.Error1.InvalidArgument"
    );
    service
        .replace_resumes(vec![arinn_protocol::library::Resume {
            content_id: "film-1".into(),
            position_us: 1_000,
            duration_us: -1,
            completed: false,
            observed_at_us: 10,
        }])
        .await
        .unwrap();
    let signal = tokio::time::timeout(Duration::from_secs(3), changes.next())
        .await
        .unwrap()
        .unwrap();
    assert!(signal.args().unwrap().revision >= 1);
    assert_eq!(resumes.get_resumes("").await.unwrap().0.resumes.len(), 1);

    let imports = LibraryImportProxy::builder(&server)
        .destination(destination.clone())
        .unwrap()
        .build()
        .await
        .unwrap();
    let candidate = ImportCandidate {
        source: EntityRef {
            provider_id: "downloads".into(),
            capability: "download".into(),
            local_id: "job-1".into(),
        },
        file_uri: "file:///media/film.mkv".into(),
        completion_id: "done-1".into(),
    };
    assert_eq!(
        imports
            .import_completed(&context.generation, vec![candidate.clone()])
            .await
            .unwrap(),
        vec!["done-1".to_string()]
    );
    assert_eq!(
        imports
            .import_completed(&context.generation, vec![candidate])
            .await
            .unwrap(),
        vec!["done-1".to_string()]
    );
    assert_eq!(*import.0.lock().unwrap(), 2);

    let stranger = LibraryProviderProxy::builder(&bus.connection().await)
        .destination(destination)
        .unwrap()
        .build()
        .await
        .unwrap();
    assert_eq!(
        error_name(stranger.get_library("").await),
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
    for (interface, file) in [
        ("org.arinn.LibraryProvider1", "library-provider1.xml"),
        ("org.arinn.LibraryResume1", "library-resume1.xml"),
        ("org.arinn.LibraryImport1", "library-import1.xml"),
    ] {
        let xml = std::fs::read_to_string(format!(
            "{}/../../docs/plugin-api/{file}",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap();
        assert_eq!(
            interface_methods(&live, interface),
            interface_methods(&xml, interface),
            "{interface}\n{live}"
        );
    }
    runner.abort();
}

#[test]
fn library_service_registers_and_checks_commands() {
    if std::env::var_os("ARINN_LIBRARY_SERVICE_CHILD").is_some() {
        tokio::runtime::Runtime::new().unwrap().block_on(scenario());
        return;
    }
    let token = std::env::temp_dir().join(format!("arinn-library-{}", uuid::Uuid::new_v4()));
    std::fs::write(&token, "fixture-token").unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "library_service_registers_and_checks_commands",
            "--nocapture",
        ])
        .env("ARINN_LIBRARY_SERVICE_CHILD", "1")
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
