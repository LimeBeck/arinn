// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! `BrowserService` registers with the `browser` capability, serves the
//! `org.arinn.BrowserProvider1` contract only to the daemon and checks commands
//! before its handler runs (ADR-0048). The XML matches the live export.
mod common;
use arinn_plugin_sdk::runtime::{self, BrowserHandler, BrowserService, TabAction};
use arinn_protocol::{
    browser::{BrowserProviderProxy, BrowserWindow, Tab},
    production::{self, Registration, RegistrationReply},
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
impl BrowserHandler for Handler {
    fn tab(
        &self,
        action: TabAction,
        id: String,
    ) -> Pin<Box<dyn Future<Output = Result<(), ErrorCode>> + Send + '_>> {
        self.0.lock().unwrap().push(format!("{action:?}:{id}"));
        Box::pin(async { Ok(()) })
    }
    fn open(
        &self,
        url: String,
    ) -> Pin<Box<dyn Future<Output = Result<(), ErrorCode>> + Send + '_>> {
        self.0.lock().unwrap().push(format!("Open:{url}"));
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
        #[zbus(header)] _h: Header<'_>,
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

fn window() -> BrowserWindow {
    BrowserWindow {
        id: "c1:w1".into(),
        window_token: String::new(),
        focused: true,
    }
}
fn tab(id: &str, active: bool) -> Tab {
    Tab {
        id: id.into(),
        window_id: "c1:w1".into(),
        title: "Tab".into(),
        url: "https://example.org/".into(),
        active,
        audible: false,
    }
}
fn error_name(result: zbus::Result<impl std::fmt::Debug>) -> String {
    match result {
        Err(zbus::Error::MethodError(name, _, _)) => name.to_string(),
        other => panic!("expected an Arinn error, got {other:?}"),
    }
}
/// The first `StateChanged` revision at or above `at_least`. The signal of
/// an earlier revision (the registration's) may still be in flight when the
/// test subscribes, so older revisions are skipped rather than failing.
async fn next_revision(
    changes: &mut (
             impl futures_util::Stream<Item = arinn_protocol::browser::provider::StateChanged> + Unpin
         ),
    at_least: u64,
) -> u64 {
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let revision = changes.next().await.unwrap().args().unwrap().revision;
            if revision >= at_least {
                return revision;
            }
        }
    })
    .await
    .unwrap()
}
fn methods(xml: &str) -> Vec<String> {
    let start = xml.find("org.arinn.BrowserProvider1").expect("interface");
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
    let service = BrowserService::new(handler.clone(), vec![], vec![], "unavailable").unwrap();
    let mut registrations = service.registrations();
    let runner = tokio::spawn(runtime::run_browser_on(provider.clone(), service.clone()));
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
    assert_eq!(*daemon.0.lock().unwrap(), vec!["browser".to_string()]);
    let generation = context.generation;

    let proxy = BrowserProviderProxy::builder(&server)
        .destination(provider.unique_name().unwrap().clone())
        .unwrap()
        .build()
        .await
        .unwrap();
    let mut changes = proxy.receive_provider_state_changed().await.unwrap();
    // No browser connected: an empty snapshot and Unavailable commands.
    let (page,) = proxy.get_tabs("").await.unwrap();
    assert_eq!((page.revision, page.tabs.len()), (1, 0));
    assert_eq!(proxy.backend_state().await.unwrap(), "unavailable");
    assert_eq!(
        error_name(proxy.open_url(&generation, "https://example.org/").await),
        "org.arinn.Error1.Unavailable"
    );

    // A browser connects: new entries and backend state in one revision.
    service
        .replace(
            vec![window()],
            vec![tab("c1:1", true), tab("c1:2", false)],
            "available",
        )
        .await
        .unwrap();
    assert_eq!(next_revision(&mut changes, 2).await, 2);
    let (page,) = proxy.get_tabs("").await.unwrap();
    page.validate().unwrap();
    assert_eq!((page.revision, page.tabs.len()), (2, 2));
    assert_eq!(proxy.backend_state().await.unwrap(), "available");

    // Only the backend state changes: still a new revision (Store::touch).
    service
        .replace(
            vec![window()],
            vec![tab("c1:1", true), tab("c1:2", false)],
            "degraded",
        )
        .await
        .unwrap();
    assert_eq!(next_revision(&mut changes, 3).await, 3);
    service
        .replace(
            vec![window()],
            vec![tab("c1:1", true), tab("c1:2", false)],
            "available",
        )
        .await
        .unwrap();

    // Commands are checked before the handler runs.
    proxy.activate_tab(&generation, "c1:2").await.unwrap();
    proxy.close_tab(&generation, "c1:1").await.unwrap();
    proxy
        .open_url(&generation, "http://example.org/watch")
        .await
        .unwrap();
    assert_eq!(
        error_name(proxy.activate_tab(&generation, "c1:9").await),
        "org.arinn.Error1.NotFound"
    );
    assert_eq!(
        error_name(
            proxy
                .activate_tab(&uuid::Uuid::new_v4().to_string(), "c1:1")
                .await
        ),
        "org.arinn.Error1.Conflict"
    );
    for bad in [
        "file:///etc/passwd",
        "javascript:alert(1)",
        "chrome://settings",
    ] {
        assert_eq!(
            error_name(proxy.open_url(&generation, bad).await),
            "org.arinn.Error1.InvalidArgument"
        );
    }
    assert_eq!(
        *handler.0.lock().unwrap(),
        vec![
            "Activate:c1:2".to_string(),
            "Close:c1:1".to_string(),
            "Open:http://example.org/watch".to_string(),
        ]
    );
    // An invalid snapshot is rejected and changes nothing.
    assert_eq!(
        service
            .replace(vec![], vec![tab("orphan", false)], "available")
            .await,
        Err(ErrorCode::InvalidArgument)
    );

    // Anyone but the daemon is refused.
    let stranger = BrowserProviderProxy::builder(&bus.connection().await)
        .destination(provider.unique_name().unwrap().clone())
        .unwrap()
        .build()
        .await
        .unwrap();
    assert_eq!(
        error_name(stranger.get_tabs("").await),
        "org.arinn.Error1.PermissionDenied"
    );

    // The XML contract matches the live export.
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
        "/../../docs/plugin-api/browser-provider1.xml"
    ))
    .unwrap();
    assert_eq!(methods(&live), methods(&file));
    for signature in ["(sta(ssb)a(ssssbb)s)", "name=\"url\" type=\"s\""] {
        assert!(live.contains(signature), "{signature}");
        assert!(file.contains(signature), "{signature}");
    }
    runner.abort();
}

#[test]
fn browser_service_registers_serves_and_checks_commands() {
    // Launch environment only in a child process, as in registration_context.rs.
    if std::env::var_os("ARINN_BROWSER_SERVICE_CHILD").is_some() {
        tokio::runtime::Runtime::new().unwrap().block_on(scenario());
        return;
    }
    let token = std::env::temp_dir().join(format!("arinn-browser-{}", uuid::Uuid::new_v4()));
    std::fs::write(&token, "fixture-token").unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "browser_service_registers_serves_and_checks_commands",
            "--nocapture",
        ])
        .env("ARINN_BROWSER_SERVICE_CHILD", "1")
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
