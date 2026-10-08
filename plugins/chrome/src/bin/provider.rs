// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `arinn-plugin-chrome`: the `browser` provider. It owns the bridge name for
//! native hosts and publishes their merged state through the SDK.
use arinn_plugin_chrome::{
    BRIDGE_NAME, BRIDGE_PATH,
    association::{Association, SETTLE},
    bridge::Bridge,
};
use arinn_plugin_sdk::{
    platform::WindowClient,
    runtime::{self, BrowserHandler, BrowserService, TabAction},
};
use arinn_protocol::validation::ErrorCode;
use futures_util::StreamExt;
use std::{future::Future, pin::Pin, time::Duration};

struct Handler {
    bridge: Bridge,
    connection: zbus::Connection,
}
impl BrowserHandler for Handler {
    fn tab(
        &self,
        action: TabAction,
        id: String,
    ) -> Pin<Box<dyn Future<Output = Result<(), ErrorCode>> + Send + '_>> {
        Box::pin(async move {
            let route = self.bridge.route(&id)?;
            let action = match action {
                TabAction::Activate => "activate",
                TabAction::Close => "close",
            };
            Bridge::command(&self.connection, &route.host, action, route.tab, "").await
        })
    }
    fn open(
        &self,
        url: String,
    ) -> Pin<Box<dyn Future<Output = Result<(), ErrorCode>> + Send + '_>> {
        Box::pin(async move {
            let host = self.bridge.default_host()?;
            Bridge::command(&self.connection, &host, "open", 0, &url).await
        })
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let connection = zbus::connection::Builder::session()?.build().await?;
    let bridge = Bridge::default();
    connection
        .object_server()
        .at(BRIDGE_PATH, bridge.clone())
        .await?;
    // One provider per session: a second instance cannot take the name.
    connection
        .request_name_with_flags(BRIDGE_NAME, zbus::fdo::RequestNameFlags::DoNotQueue.into())
        .await?;
    let service = BrowserService::new(
        Handler {
            bridge: bridge.clone(),
            connection: connection.clone(),
        },
        Vec::new(),
        Vec::new(),
        "unavailable",
    )?;
    connection
        .object_server()
        .at(
            "/org/arinn/Provider1",
            arinn_plugin_chrome::icons::WebIcons::new(service.registrations()).await?,
        )
        .await?;
    connection
        .object_server()
        .at(
            "/org/arinn/Provider1",
            arinn_plugin_chrome::processes::Processes::new(bridge.clone(), service.registrations()),
        )
        .await?;
    let publisher = tokio::spawn(publish(service.clone(), bridge.clone()));
    let result = runtime::run_browser_on(connection, service).await;
    publisher.abort();
    result.map_err(|e| anyhow::anyhow!(e))
}

/// Publish the merged browser snapshot, with window tokens linked through the
/// platform window inventory while the provider is registered (ADR-0052).
async fn publish(service: BrowserService, bridge: Bridge) {
    let mut changes = bridge.changes();
    let mut registrations = service.registrations();
    let mut association = Association::default();
    let mut platform = None;
    let mut generation = String::new();
    loop {
        let context = registrations.borrow_and_update().clone();
        if platform.is_none()
            && let Some(context) = context
            && let Ok(client) = WindowClient::new(&context.connection).await
            && let Ok(stream) = client.proxy().receive_windows_changed().await
        {
            platform = Some((client, stream));
        }
        let (mut windows, tabs, connected) = bridge.snapshot();
        let inventory = match &platform {
            Some((client, _)) => client.snapshot().await.ok(),
            None => None,
        };
        match inventory {
            Some(snapshot) => {
                // Tokens never survive a new platform generation.
                if snapshot.generation != generation {
                    association.clear();
                    generation = snapshot.generation;
                }
                association.observe(&windows, &snapshot.windows, tokio::time::Instant::now());
            }
            None => {
                platform = None;
                association.clear();
            }
        }
        association.apply(&mut windows);
        let backend = if connected {
            "available"
        } else {
            "unavailable"
        };
        if service.replace(windows, tabs, backend).await.is_err() {
            // The merged snapshot is fitted to the wire rules; this only
            // fails on an internal inconsistency.
            eprintln!("arinn-plugin-chrome: rejected an invalid browser snapshot");
        }
        // A pending focus pair is re-checked after it settles; otherwise the
        // platform inventory is polled like the desktop provider does.
        let wait = if association.pending() {
            SETTLE
        } else {
            Duration::from_secs(1)
        };
        let platform_event = async {
            match platform.as_mut() {
                Some((_, stream)) => stream.next().await.is_some(),
                None => std::future::pending().await,
            }
        };
        tokio::select! {
            changed = changes.changed() => if changed.is_err() { break },
            changed = registrations.changed() => {
                if changed.is_err() { break }
                platform = None;
                association.clear();
            }
            alive = platform_event => if !alive { platform = None },
            _ = tokio::time::sleep(wait) => {}
        }
    }
}
