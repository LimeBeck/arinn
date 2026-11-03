// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: MIT OR Apache-2.0
mod common;
use arinn_plugin_sdk::runtime::{self, MusicCatalogue, MusicHandler, MusicService};
use arinn_protocol::{
    music::{self, Command, Entity, MusicProviderProxy, Page, Query, State},
    production::{self, Registration, RegistrationReply},
    validation::ErrorCode,
};
use std::{
    future::Future,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
#[derive(Clone, Default)]
struct Handler(Arc<AtomicUsize>);
impl MusicHandler for Handler {
    fn supports_assets(&self) -> bool {
        true
    }
    fn asset(
        &self,
        id: String,
    ) -> Pin<Box<dyn Future<Output = Result<arinn_protocol::assets::Asset, ErrorCode>> + Send + '_>>
    {
        Box::pin(async move {
            if id != "cover:fixture" {
                return Err(ErrorCode::NotFound);
            }
            Ok(arinn_protocol::assets::Asset {
                mime: "image/png".into(),
                data: vec![0x89, b'P', b'N', b'G', 13, 10, 26, 10],
            })
        })
    }
    fn state(&self) -> Pin<Box<dyn Future<Output = Result<State, ErrorCode>> + Send + '_>> {
        Box::pin(async move {
            Ok(State {
                revision: self.0.load(Ordering::SeqCst) as u64 + 1,
                actions: vec!["scan".into()],
                ..State::default()
            })
        })
    }
    fn catalogue(
        &self,
        _query: Query,
    ) -> Pin<Box<dyn Future<Output = Result<MusicCatalogue, ErrorCode>> + Send + '_>> {
        Box::pin(async {
            Ok(MusicCatalogue {
                revision: 7,
                entries: (0..150)
                    .map(|i| Entity {
                        id: format!("track-{i}"),
                        kind: "track".into(),
                        title: "Fixture".into(),
                        ..Entity::default()
                    })
                    .collect(),
            })
        })
    }
    fn command(
        &self,
        _command: Command,
    ) -> Pin<Box<dyn Future<Output = Result<State, ErrorCode>> + Send + '_>> {
        Box::pin(async move {
            self.0.fetch_add(1, Ordering::SeqCst);
            self.state().await
        })
    }
    fn reset(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        Box::pin(async {})
    }
}
struct Daemon;
#[zbus::interface(name = "org.arinn.Daemon1")]
impl Daemon {
    async fn register(&self, request: Registration) -> production::Result<(RegistrationReply,)> {
        assert_eq!(request.capabilities, vec!["music"]);
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
fn error(result: zbus::Result<String>) -> String {
    match result {
        Err(zbus::Error::MethodError(name, _, _)) => name.to_string(),
        _ => panic!("expected safe method error"),
    }
}
fn members(xml: &str, interface: &str) -> Vec<String> {
    let start = xml
        .find(&format!("<interface name=\"{interface}\">"))
        .expect("interface present");
    let body = &xml[start..];
    let body = &body[..body.find("</interface>").unwrap()];
    let mut members = Vec::new();
    for kind in ["method", "signal"] {
        let open = format!("<{kind} name=\"");
        let close = format!("</{kind}>");
        for member in body.split(open.as_str()).skip(1) {
            let name = &member[..member.find('"').unwrap()];
            let member = &member[..member.find(close.as_str()).unwrap()];
            let mut args = Vec::new();
            for arg in member.split("<arg ").skip(1) {
                let attr = |key: &str, default: &str| {
                    arg.split(&format!("{key}=\""))
                        .nth(1)
                        .map(|v| v[..v.find('"').unwrap()].to_owned())
                        .unwrap_or_else(|| default.to_owned())
                };
                let direction = if kind == "signal" { "out" } else { "in" };
                args.push(format!(
                    "{}:{}",
                    attr("direction", direction),
                    attr("type", "")
                ));
            }
            members.push(format!("{kind} {name}({})", args.join(",")));
        }
    }
    members.sort();
    members
}
fn error(result: zbus::Result<String>) -> String {
    match result {
        Err(zbus::Error::MethodError(name, _, _)) => name.to_string(),
        _ => panic!("expected safe method error"),
    }
}
async fn scenario() {
    let bus = common::Bus::start();
    let daemon = bus.connection().await;
    daemon
        .object_server()
        .at(production::PATH, Daemon)
        .await
        .unwrap();
    daemon.request_name(production::SERVICE).await.unwrap();
    let provider = bus.connection().await;
    let handler = Handler::default();
    let service = MusicService::new(handler.clone());
    let mut registrations = service.registrations();
    let runner = tokio::spawn(runtime::run_music_on(provider.clone(), service));
    let generation = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Some(context) = registrations.borrow_and_update().clone() {
                break context.generation;
            }
            registrations.changed().await.unwrap();
        }
    })
    .await
    .unwrap();
    let proxy = MusicProviderProxy::builder(&daemon)
        .destination(provider.unique_name().unwrap().clone())
        .unwrap()
        .build()
        .await
        .unwrap();
    // The live provider export matches the published interface file.
    let live = zbus::fdo::IntrospectableProxy::builder(&daemon)
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
    let published = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/plugin-api/music-provider1.xml"
    ))
    .unwrap();
    assert_eq!(
        members(&live, "org.arinn.MusicProvider1"),
        members(&published, "org.arinn.MusicProvider1")
    );
    assert_eq!(
        proxy.features().await.unwrap(),
        vec![arinn_protocol::assets::FEATURE_ASSETS]
    );
    let assets = arinn_protocol::assets::AssetProviderProxy::builder(&daemon)
        .destination(provider.unique_name().unwrap().clone())
        .unwrap()
        .build()
        .await
        .unwrap();
    assets
        .get_asset(&generation, "cover:fixture")
        .await
        .unwrap()
        .0
        .validate()
        .unwrap();
    assert!(
        assets
            .get_asset("stale-generation", "cover:fixture")
            .await
            .is_err()
    );
    let stranger = bus.connection().await;
    let unauthorized_assets = arinn_protocol::assets::AssetProviderProxy::builder(&stranger)
        .destination(provider.unique_name().unwrap().clone())
        .unwrap()
        .build()
        .await
        .unwrap();
    assert!(
        unauthorized_assets
            .get_asset(&generation, "cover:fixture")
            .await
            .is_err()
    );
    let unauthorized = MusicProviderProxy::builder(&stranger)
        .destination(provider.unique_name().unwrap().clone())
        .unwrap()
        .build()
        .await
        .unwrap();
    assert_eq!(
        error(unauthorized.get_state(&generation).await),
        "org.arinn.Error1.PermissionDenied"
    );
    assert_eq!(
        error(proxy.get_state("old-generation").await),
        "org.arinn.Error1.Conflict"
    );
    let client = daemon.unique_name().unwrap().as_str();
    let other = stranger.unique_name().unwrap().as_str();
    let mut query = Query {
        facet: "tracks".into(),
        parent: String::new(),
        search: String::new(),
        token: String::new(),
    };
    let first: Page = music::decode(
        &proxy
            .get_page(&generation, client, &music::encode(&query).unwrap())
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(first.entries.len(), 100);
    assert!(!first.next_token.is_empty());
    query.token = first.next_token;
    let payload = music::encode(&query).unwrap();
    let next = proxy.get_page(&generation, client, &payload).await.unwrap();
    assert_eq!(music::decode::<Page>(&next).unwrap().entries.len(), 50);
    assert_eq!(
        proxy.get_page(&generation, client, &payload).await.unwrap(),
        next
    );
    assert_eq!(
        error(proxy.get_page(&generation, other, &payload).await),
        "org.arinn.Error1.PermissionDenied"
    );
    query.search = "different".into();
    assert_eq!(
        error(
            proxy
                .get_page(&generation, client, &music::encode(&query).unwrap())
                .await
        ),
        "org.arinn.Error1.PermissionDenied"
    );
    let mut command = Command {
        request_id: uuid::Uuid::new_v4().to_string(),
        action: "scan".into(),
        session: String::new(),
        entity: String::new(),
        occurrence: String::new(),
        position_us: 0,
        replaces_provider: String::new(),
        replaces_session: String::new(),
        text: String::new(),
    };
    let payload = music::encode(&command).unwrap();
    let receipt = proxy.command(&generation, client, &payload).await.unwrap();
    assert_eq!(
        proxy.command(&generation, client, &payload).await.unwrap(),
        receipt
    );
    assert_eq!(handler.0.load(Ordering::SeqCst), 1);
    proxy.command(&generation, other, &payload).await.unwrap();
    assert_eq!(handler.0.load(Ordering::SeqCst), 2);
    command.entity = "changed".into();
    assert_eq!(
        error(
            proxy
                .command(&generation, client, &music::encode(&command).unwrap())
                .await
        ),
        "org.arinn.Error1.Conflict"
    );
    daemon.release_name(production::SERVICE).await.unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if registrations.borrow_and_update().is_none() {
                break;
            }
            registrations.changed().await.unwrap();
        }
    })
    .await
    .unwrap();
    assert_eq!(
        error(proxy.get_state(&generation).await),
        "org.arinn.Error1.PermissionDenied"
    );
    runner.abort();
    let _ = runner.await;
}

#[test]
fn private_music_owner_cursors_and_caller_scoped_receipts() {
    if std::env::var_os("ARINN_MUSIC_SERVICE_CHILD").is_some() {
        tokio::runtime::Runtime::new().unwrap().block_on(scenario());
        return;
    }
    let token = std::env::temp_dir().join(format!("arinn-music-token-{}", uuid::Uuid::new_v4()));
    std::fs::write(&token, "fixture-token").unwrap();
    let result = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "private_music_owner_cursors_and_caller_scoped_receipts",
            "--nocapture",
        ])
        .env("ARINN_MUSIC_SERVICE_CHILD", "1")
        .env("ARINN_PROVIDER_ID", "fixture")
        .env("ARINN_PLUGIN_ID", "fixture")
        .env("ARINN_PLUGIN_VERSION", "0.0.1")
        .env("ARINN_TOKEN_FILE", &token)
        .output()
        .unwrap();
    std::fs::remove_file(token).unwrap();
    assert!(
        result.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
}
