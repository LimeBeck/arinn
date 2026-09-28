// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `arinn-plugin-chrome`: the `browser` provider. It owns the bridge name for
//! native hosts and publishes their merged state through the SDK.
use arinn_plugin_chrome::{BRIDGE_NAME, BRIDGE_PATH, bridge::Bridge};
use arinn_plugin_sdk::runtime::{self, BrowserHandler, BrowserService, TabAction};
use arinn_protocol::validation::ErrorCode;
use std::{future::Future, pin::Pin};

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
    let publisher = {
        let service = service.clone();
        let mut changes = bridge.changes();
        tokio::spawn(async move {
            loop {
                let (windows, tabs, connected) = bridge.snapshot();
                let backend = if connected {
                    "available"
                } else {
                    "unavailable"
                };
                if service.replace(windows, tabs, backend).await.is_err() {
                    // The merged snapshot is fitted to the wire rules; this
                    // only fails on an internal inconsistency.
                    eprintln!("arinn-plugin-chrome: rejected an invalid browser snapshot");
                }
                if changes.changed().await.is_err() {
                    break;
                }
            }
        })
    };
    let result = runtime::run_browser_on(connection, service).await;
    publisher.abort();
    result.map_err(|e| anyhow::anyhow!(e))
}
