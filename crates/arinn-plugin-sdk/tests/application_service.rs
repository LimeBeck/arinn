// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! `ApplicationService` forwards `CancelLaunch` to its handler only for the
//! daemon and the current generation; the default handler cannot cancel
//! (ADR-0110). The XML matches the live export.
mod common;
use arinn_plugin_sdk::runtime::{self, Action, ApplicationHandler, ApplicationService};
use arinn_protocol::{
    application::Application,
    production::{self, ApplicationProviderProxy, Registration, RegistrationReply},
    validation::ErrorCode,
};
use std::{
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
    time::Duration,
};
use zbus::message::Header;

/// Launches stay unfinished until `cancel_launch` stops them.
#[derive(Clone, Default)]
struct Cancelling {
    calls: Arc<Mutex<Vec<String>>>,
    stop: Arc<tokio::sync::Notify>,
}
impl ApplicationHandler for Cancelling {
    fn execute(
        &self,
        action: Action,
        id: String,
    ) -> Pin<Box<dyn Future<Output = Result<(), ErrorCode>> + Send + '_>> {
        self.calls.lock().unwrap().push(format!("{action:?}:{id}"));
        Box::pin(async move {
            self.stop.notified().await;
            Err(ErrorCode::Cancelled)
        })
    }
    fn cancel_launch(
        &self,
        id: String,
    ) -> Pin<Box<dyn Future<Output = Result<(), ErrorCode>> + Send + '_>> {
        self.calls.lock().unwrap().push(format!("Cancel:{id}"));
        self.stop.notify_waiters();
        Box::pin(async { Ok(()) })
    }
}
/// A handler written before ADR-0110: no `cancel_launch`.
struct Plain;
impl ApplicationHandler for Plain {
    fn execute(
        &self,
        _: Action,
        _: String,
    ) -> Pin<Box<dyn Future<Output = Result<(), ErrorCode>> + Send + '_>> {
        Box::pin(async { Ok(()) })
    }
}

#[derive(Clone)]
struct Daemon;
#[zbus::interface(name = "org.arinn.Daemon1")]
impl Daemon {
    async fn register(
        &self,
        _request: Registration,
        #[zbus(header)] _h: Header<'_>,
    ) -> production::Result<(RegistrationReply,)> {
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
fn application(id: &str) -> Application {
    Application {
        id: id.into(),
        title: "Fixture".into(),
        can_launch: true,
        group: "games".into(),
        icon: String::new(),
    }
}
fn error_name(result: zbus::Result<impl std::fmt::Debug>) -> String {
    match result {
        Err(zbus::Error::MethodError(name, _, _)) => name.to_string(),
        other => panic!("expected an Arinn error, got {other:?}"),
    }
}
fn methods(xml: &str) -> Vec<String> {
    let start = xml.find("org.arinn.Provider1").expect("interface");
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
/// Register `service` with a fixture daemon; returns the daemon-side proxy,
/// the generation and the provider task.
async fn register(
    bus: &common::Bus,
    service: ApplicationService,
) -> (
    zbus::Connection,
    ApplicationProviderProxy<'static>,
    String,
    tokio::task::JoinHandle<Result<(), Box<dyn std::error::Error + Send + Sync>>>,
) {
    let server = bus.connection().await;
    server
        .object_server()
        .at(production::PATH, Daemon)
        .await
        .unwrap();
    server.request_name(production::SERVICE).await.unwrap();
    let provider = bus.connection().await;
    let mut registrations = service.registrations();
    let runner = tokio::spawn(runtime::run_on(provider.clone(), service));
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
    let proxy = ApplicationProviderProxy::builder(&server)
        .destination(provider.unique_name().unwrap().to_owned())
        .unwrap()
        .build()
        .await
        .unwrap();
    (server, proxy, context.generation, runner)
}

async fn scenario() {
    let bus = common::Bus::start();
    let handler = Cancelling::default();
    let service = ApplicationService::new(
        handler.clone(),
        vec![application("game")],
        vec![],
        "available",
    )
    .unwrap();
    let (server, proxy, generation, runner) = register(&bus, service.clone()).await;

    // A pending launch ends with Cancelled once CancelLaunch reaches the handler.
    let launch = {
        let proxy = proxy.clone();
        let generation = generation.clone();
        tokio::spawn(async move { proxy.launch(&generation, "game").await })
    };
    tokio::time::timeout(Duration::from_secs(3), async {
        while handler.calls.lock().unwrap().is_empty() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    // Wrong generation and invalid ids never reach the handler.
    assert_eq!(
        error_name(
            proxy
                .cancel_launch(&uuid::Uuid::new_v4().to_string(), "game")
                .await
        ),
        "org.arinn.Error1.Conflict"
    );
    assert_eq!(
        error_name(proxy.cancel_launch(&generation, "").await),
        "org.arinn.Error1.InvalidArgument"
    );
    // Only the daemon may cancel.
    let stranger = ApplicationProviderProxy::builder(&bus.connection().await)
        .destination(proxy.inner().destination().to_owned())
        .unwrap()
        .build()
        .await
        .unwrap();
    assert_eq!(
        error_name(stranger.cancel_launch(&generation, "game").await),
        "org.arinn.Error1.PermissionDenied"
    );
    // Not checked against the catalogue: the entry may have disappeared.
    service.replace(vec![], vec![]).await.unwrap();
    proxy.cancel_launch(&generation, "game").await.unwrap();
    assert_eq!(
        error_name(
            tokio::time::timeout(Duration::from_secs(3), launch)
                .await
                .unwrap()
                .unwrap()
        ),
        "org.arinn.Error1.Cancelled"
    );
    assert_eq!(
        *handler.calls.lock().unwrap(),
        ["Launch:game".to_string(), "Cancel:game".to_string()]
    );

    // The XML contract matches the live export.
    let introspect = zbus::fdo::IntrospectableProxy::builder(&server)
        .destination(proxy.inner().destination().to_owned())
        .unwrap()
        .path(production::PROVIDER_PATH)
        .unwrap()
        .build()
        .await
        .unwrap();
    let live = introspect.introspect().await.unwrap();
    let file = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/plugin-api/provider1.xml"
    ))
    .unwrap();
    assert_eq!(methods(&live), methods(&file));
    assert!(methods(&live).contains(&"CancelLaunch".to_string()));
    runner.abort();
    drop(server);

    // A handler without cancel_launch refuses cancellation.
    let bus = common::Bus::start();
    let service =
        ApplicationService::new(Plain, vec![application("game")], vec![], "available").unwrap();
    let (_server, proxy, generation, runner) = register(&bus, service).await;
    assert_eq!(
        error_name(proxy.cancel_launch(&generation, "game").await),
        "org.arinn.Error1.Unsupported"
    );
    proxy.launch(&generation, "game").await.unwrap();
    runner.abort();
}

#[test]
fn application_service_forwards_cancel_launch() {
    // Launch environment only in a child process, as in registration_context.rs.
    if std::env::var_os("ARINN_APPLICATION_SERVICE_CHILD").is_some() {
        tokio::runtime::Runtime::new().unwrap().block_on(scenario());
        return;
    }
    let token = std::env::temp_dir().join(format!("arinn-application-{}", uuid::Uuid::new_v4()));
    std::fs::write(&token, "fixture-token").unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "application_service_forwards_cancel_launch",
            "--nocapture",
        ])
        .env("ARINN_APPLICATION_SERVICE_CHILD", "1")
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
