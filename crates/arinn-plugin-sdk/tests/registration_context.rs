// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

mod common;
use arinn_plugin_sdk::runtime::{
    self, Action, ApplicationHandler, ApplicationService, RegistrationContext,
};
use arinn_protocol::{
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

struct Handler;
impl ApplicationHandler for Handler {
    fn execute(
        &self,
        _: Action,
        _: String,
    ) -> Pin<Box<dyn Future<Output = Result<(), ErrorCode>> + Send + '_>> {
        Box::pin(async { Ok(()) })
    }
}
#[derive(Clone)]
struct Daemon {
    epoch: String,
    churn: bool,
    owner: Arc<Mutex<String>>,
}
#[zbus::interface(name = "org.arinn.Daemon1")]
impl Daemon {
    async fn register(
        &self,
        request: Registration,
        #[zbus(header)] h: Header<'_>,
        #[zbus(connection)] connection: &zbus::Connection,
    ) -> production::Result<(RegistrationReply,)> {
        if self.churn {
            // More signals than the provider connection queue can hold while
            // Register is awaiting its reply. Unrelated owners must not stall it.
            for _ in 0..128 {
                connection
                    .request_name("org.arinn.RegistrationNoise")
                    .await
                    .unwrap();
                connection
                    .release_name("org.arinn.RegistrationNoise")
                    .await
                    .unwrap();
            }
        }
        assert_eq!(request.provider_id, "fixture");
        assert_eq!(request.token, "fixture-token");
        *self.owner.lock().unwrap() = h.sender().unwrap().to_string();
        Ok((RegistrationReply {
            epoch: self.epoch.clone(),
            generation: uuid::Uuid::new_v4().to_string(),
            api_minor: 0,
            max_body_bytes: 262144,
            max_page_entries: 200,
            max_operations: 32,
        },))
    }
}
async fn context(
    rx: &mut tokio::sync::watch::Receiver<Option<RegistrationContext>>,
) -> RegistrationContext {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Some(context) = rx.borrow_and_update().clone() {
                return context;
            }
            rx.changed().await.unwrap();
        }
    })
    .await
    .unwrap()
}
async fn scenario() {
    let bus = common::Bus::start();
    let server = bus.connection().await;
    let daemon = Daemon {
        epoch: uuid::Uuid::new_v4().to_string(),
        churn: false,
        owner: Arc::default(),
    };
    server
        .object_server()
        .at(production::PATH, daemon.clone())
        .await
        .unwrap();
    server.request_name(production::SERVICE).await.unwrap();
    let provider = bus.connection().await;
    let service = ApplicationService::new(Handler, vec![], vec![], "available").unwrap();
    let mut registrations = service.registrations();
    assert!(registrations.borrow().is_none());
    let runner = tokio::spawn(runtime::run_on(provider.clone(), service.clone()));
    let first = context(&mut registrations).await;
    assert_eq!(first.connection.unique_name(), provider.unique_name());
    assert_eq!(
        *daemon.owner.lock().unwrap(),
        first.connection.unique_name().unwrap().as_str()
    );
    assert_eq!(first.daemon_owner, server.unique_name().unwrap().as_str());
    assert_eq!(first.daemon_epoch, daemon.epoch);
    assert!(
        runtime::run_on(bus.connection().await, service.clone())
            .await
            .is_err()
    );
    assert_eq!(
        registrations.borrow().as_ref().unwrap().generation,
        first.generation
    );
    server.release_name(production::SERVICE).await.unwrap();
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            registrations.changed().await.unwrap();
            if registrations.borrow_and_update().is_none() {
                break;
            }
        }
    })
    .await
    .unwrap();
    let replacement = bus.connection().await;
    let new_daemon = Daemon {
        epoch: uuid::Uuid::new_v4().to_string(),
        churn: true,
        owner: Arc::default(),
    };
    replacement
        .object_server()
        .at(production::PATH, new_daemon.clone())
        .await
        .unwrap();
    replacement.request_name(production::SERVICE).await.unwrap();
    let second = context(&mut registrations).await;
    assert_ne!(second.generation, first.generation);
    assert_ne!(second.daemon_owner, first.daemon_owner);
    assert_eq!(second.daemon_epoch, new_daemon.epoch);
    assert_eq!(second.connection.unique_name(), provider.unique_name());
    runner.abort();
    assert!(runner.await.unwrap_err().is_cancelled());
    assert!(registrations.borrow().is_none());
    // A retained connection must not leave the stopped service authorized.
    let proxy = production::ApplicationProviderProxy::builder(&replacement)
        .destination(provider.unique_name().unwrap().clone())
        .unwrap()
        .build()
        .await
        .unwrap();
    assert!(
        matches!(proxy.backend_state().await, Err(zbus::Error::MethodError(name, _, _))
        if name.as_str() == "org.arinn.Error1.PermissionDenied")
    );
}

#[test]
fn actual_connection_owner_replacement_and_cancellation() {
    // Set launch environment only on a child process, never mutate environment
    // concurrently with the Rust test runner or Tokio worker threads.
    if std::env::var_os("ARINN_REGISTRATION_CONTEXT_CHILD").is_some() {
        tokio::runtime::Runtime::new().unwrap().block_on(scenario());
        return;
    }
    let token = std::env::temp_dir().join(format!("arinn-context-{}", uuid::Uuid::new_v4()));
    std::fs::write(&token, "fixture-token").unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "actual_connection_owner_replacement_and_cancellation",
            "--nocapture",
        ])
        .env("ARINN_REGISTRATION_CONTEXT_CHILD", "1")
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
