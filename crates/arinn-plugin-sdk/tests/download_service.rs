// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! `DownloadService` registers `download` and serves `DownloadProvider1`
//! only to the daemon (ADR-0067).
mod common;
use arinn_plugin_sdk::runtime::{self, DownloadAction, DownloadHandler, DownloadService};
use arinn_protocol::{
    download::{self, DownloadProviderProxy, Job, Output},
    production::{self, Registration, RegistrationReply},
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
impl DownloadHandler for Handler {
    fn control(
        &self,
        action: DownloadAction,
        id: String,
    ) -> Pin<Box<dyn Future<Output = Result<(), ErrorCode>> + Send + '_>> {
        self.0.lock().unwrap().push(format!("{action:?}:{id}"));
        Box::pin(async { Ok(()) })
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

fn job(actions: &[&str]) -> Job {
    Job {
        id: "job-1".into(),
        title: "Film".into(),
        state: download::STATE_PAUSED.into(),
        completed_bytes: 10,
        has_total: true,
        total_bytes: 100,
        rate_bytes_per_second: 0,
        eta_seconds: -1,
        has_progress: true,
        progress: 0.1,
        actions: actions.iter().map(|action| (*action).to_string()).collect(),
        error_code: String::new(),
        error_message: String::new(),
        outputs: vec![Output {
            id: "out-1".into(),
            title: "film.mkv".into(),
            file_uri: "file:///srv/downloads/film.mkv".into(),
        }],
    }
}
fn error_name(result: zbus::Result<impl std::fmt::Debug>) -> String {
    match result {
        Err(zbus::Error::MethodError(name, _, _)) => name.to_string(),
        other => panic!("expected an Arinn error, got {other:?}"),
    }
}
fn interface_members(xml: &str, interface: &str) -> Vec<String> {
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
    let service = DownloadService::new(handler.clone(), vec![], "unavailable").unwrap();
    // Invalid jobs never reach the snapshot.
    let mut bad = job(&["pause"]);
    bad.completed_bytes = 101;
    assert_eq!(
        service.replace(vec![bad], "available").await,
        Err(ErrorCode::InvalidArgument)
    );
    let mut registrations = service.registrations();
    let runner = tokio::spawn(runtime::run_download_on(provider.clone(), service.clone()));
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
    assert_eq!(*daemon.0.lock().unwrap(), vec!["download".to_string()]);
    let generation = context.generation;
    let proxy = DownloadProviderProxy::builder(&server)
        .destination(provider.unique_name().unwrap().clone())
        .unwrap()
        .build()
        .await
        .unwrap();
    let (page,) = proxy.get_jobs("").await.unwrap();
    assert_eq!((page.revision, page.jobs.len()), (1, 0));
    assert!(proxy.features().await.unwrap().is_empty());
    assert_eq!(proxy.backend_state().await.unwrap(), "unavailable");

    // Cached rows stay readable while the backend is gone; commands do not run.
    service
        .replace(vec![job(&["resume", "start-now"])], "unavailable")
        .await
        .unwrap();
    assert_eq!(proxy.get_jobs("").await.unwrap().0.jobs.len(), 1);
    assert_eq!(
        error_name(proxy.resume(&generation, "job-1").await),
        "org.arinn.Error1.Unavailable"
    );

    service
        .replace(vec![job(&["resume", "start-now"])], "available")
        .await
        .unwrap();
    let (page,) = proxy.get_jobs("").await.unwrap();
    page.validate().unwrap();
    assert_eq!(page.revision, 3);
    assert_eq!(
        proxy.get_job(&generation, "job-1").await.unwrap().0,
        job(&["resume", "start-now"])
    );
    proxy.resume(&generation, "job-1").await.unwrap();
    proxy.start_now(&generation, "job-1").await.unwrap();
    // An action the job does not list is Unsupported, not attempted.
    assert_eq!(
        error_name(proxy.pause(&generation, "job-1").await),
        "org.arinn.Error1.Unsupported"
    );
    assert_eq!(
        error_name(proxy.retry(&generation, "job-1").await),
        "org.arinn.Error1.Unsupported"
    );
    assert_eq!(
        error_name(proxy.resume(&generation, "missing").await),
        "org.arinn.Error1.NotFound"
    );
    assert_eq!(
        error_name(proxy.resume(&generation, "").await),
        "org.arinn.Error1.InvalidArgument"
    );
    assert_eq!(
        error_name(
            proxy
                .resume(&uuid::Uuid::new_v4().to_string(), "job-1")
                .await
        ),
        "org.arinn.Error1.Conflict"
    );
    assert_eq!(
        *handler.0.lock().unwrap(),
        vec!["Resume:job-1".to_string(), "StartNow:job-1".to_string()]
    );

    let stranger = DownloadProviderProxy::builder(&bus.connection().await)
        .destination(provider.unique_name().unwrap().clone())
        .unwrap()
        .build()
        .await
        .unwrap();
    assert_eq!(
        error_name(stranger.get_jobs("").await),
        "org.arinn.Error1.PermissionDenied"
    );
    assert_eq!(
        error_name(stranger.resume(&generation, "job-1").await),
        "org.arinn.Error1.PermissionDenied"
    );

    let live = zbus::fdo::IntrospectableProxy::builder(&server)
        .destination(provider.unique_name().unwrap().clone())
        .unwrap()
        .path(production::PROVIDER_PATH)
        .unwrap()
        .build()
        .await
        .unwrap()
        .introspect()
        .await
        .unwrap();
    let file = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/plugin-api/download-provider1.xml"
    ))
    .unwrap();
    assert_eq!(
        interface_members(&live, download::PROVIDER_INTERFACE),
        interface_members(&file, download::PROVIDER_INTERFACE)
    );
    assert_eq!(
        interface_members(&file, download::PROVIDER_INTERFACE).len(),
        9
    );
    assert!(live.contains("(sta(ssstbttxbdasssa(sss))s)"));
    runner.abort();
}

#[test]
fn download_service_serves_jobs_to_the_daemon_only() {
    const FLAG: &str = "ARINN_DOWNLOAD_CHILD";
    const NAME: &str = "download_service_serves_jobs_to_the_daemon_only";
    if std::env::var_os(FLAG).is_some() {
        tokio::runtime::Runtime::new().unwrap().block_on(scenario());
        return;
    }
    let token = std::env::temp_dir().join(format!("arinn-download-{}", uuid::Uuid::new_v4()));
    std::fs::write(&token, "fixture-token").unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", NAME, "--nocapture"])
        .env(FLAG, "1")
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
