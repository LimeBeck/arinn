// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `org.arinn.Browse1` (ADR-0066) on a private bus.
use super::*;
use arinn_protocol::{
    browse::{BrowseApiProxy, KIND_ADDRESS, KIND_SEARCH},
    production::ProductionOperationProxy,
    settings::SettingsApiProxy,
};
use std::{
    io::{BufRead, BufReader},
    process::{Child, Command, Stdio},
};

struct Fixture {
    bus: Child,
    folder: PathBuf,
    _core: Core,
    client: Connection,
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
fn engines() -> Vec<Engine> {
    ["one", "two"]
        .into_iter()
        .map(|id| Engine {
            id: id.into(),
            title: format!("Engine {id}"),
            template: format!("https://{id}.example/search?q={{query}}"),
        })
        .collect()
}
impl Fixture {
    async fn new(engines: Vec<Engine>) -> Self {
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
        let client = connect(&address).await;
        server.request_name(production::SERVICE).await.unwrap();
        let folder = std::env::temp_dir().join(format!("arinn-browse-{}", Uuid::new_v4()));
        std::fs::create_dir(&folder).unwrap();
        let core = Core::new(
            &server,
            Paths {
                config: folder.join("config"),
                data: folder.join("data"),
                state: folder.join("state"),
            },
            folder.clone(),
        )
        .with_search_engines(engines);
        core.export_settings().await.unwrap();
        core.export_browse().await.unwrap();
        Self {
            bus,
            folder,
            _core: core,
            client,
        }
    }
    async fn api(&self) -> BrowseApiProxy<'static> {
        BrowseApiProxy::builder(&self.client)
            .destination(production::SERVICE)
            .unwrap()
            .build()
            .await
            .unwrap()
    }
    async fn settings(&self) -> SettingsApiProxy<'static> {
        SettingsApiProxy::builder(&self.client)
            .destination(production::SERVICE)
            .unwrap()
            .build()
            .await
            .unwrap()
    }
    async fn wait(&self, path: OwnedObjectPath) -> Result<(), String> {
        let operation = ProductionOperationProxy::builder(&self.client)
            .path(path)
            .unwrap()
            .build()
            .await
            .unwrap();
        for _ in 0..150 {
            let info = operation.get().await.unwrap().0;
            match info.state.as_str() {
                "succeeded" => return Ok(()),
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
async fn resolve_follows_the_rules_and_the_selected_engine() {
    let f = Fixture::new(engines()).await;
    let api = f.api().await;
    let address = api.resolve("example.org/a").await.unwrap().0;
    assert_eq!(address.kind, KIND_ADDRESS);
    assert_eq!(address.url, "https://example.org/a");
    let search = api.resolve("  cats and dogs ").await.unwrap().0;
    assert_eq!(search.kind, KIND_SEARCH);
    assert_eq!(search.url, "https://one.example/search?q=cats%20and%20dogs");
    assert_eq!(search.engine, "Engine one");
    assert_eq!(api.resolve("ftp://x").await.unwrap().0.kind, KIND_INVALID);

    // The engine is the daemon setting `search_engine`, an enum over the
    // packaged engines whose default is the first one.
    let settings = f.settings().await;
    let (generation, schema) = settings.get_settings("arinn").await.unwrap();
    let engine = schema.iter().find(|s| s.id == "search_engine").unwrap();
    assert_eq!(engine.kind, "enum");
    assert_eq!(engine.value, r#""one""#);
    let path = settings
        .set_value("arinn", &generation, "search_engine", r#""two""#, &rid())
        .await
        .unwrap();
    f.wait(path).await.unwrap();
    let search = api.resolve("cats").await.unwrap().0;
    assert_eq!(search.url, "https://two.example/search?q=cats");
    assert_eq!(search.engine, "Engine two");
    let path = settings
        .set_value("arinn", &generation, "search_engine", r#""three""#, &rid())
        .await;
    assert!(error_name(path).ends_with("InvalidArgument"));
}

#[tokio::test]
async fn without_engines_a_search_is_invalid_and_there_is_no_engine_setting() {
    let f = Fixture::new(Vec::new()).await;
    let api = f.api().await;
    assert_eq!(api.resolve("cats").await.unwrap().0.kind, KIND_INVALID);
    assert_eq!(
        api.resolve("example.org").await.unwrap().0.kind,
        KIND_ADDRESS
    );
    let (_, schema) = f.settings().await.get_settings("arinn").await.unwrap();
    assert!(schema.iter().all(|s| s.id != "search_engine"));
}

#[tokio::test]
async fn open_records_recent_entries_newest_first() {
    let f = Fixture::new(engines()).await;
    let api = f.api().await;
    let mut changes = api.receive_recent_changed().await.unwrap();
    assert!(api.get_recent().await.unwrap().is_empty());

    // Invalid text fails at once and is not recorded.
    assert!(error_name(api.open("ftp://x", &rid()).await).ends_with("InvalidArgument"));
    assert!(api.get_recent().await.unwrap().is_empty());

    for text in ["a.org", "b.org", " a.org ", "c", "d", "e", "f"] {
        let path = api.open(text, &rid()).await.unwrap();
        // No browser provider in the fixture: the open itself is Unavailable.
        assert_eq!(f.wait(path).await, Err("Unavailable".into()));
    }
    assert!(changes.next().await.is_some());
    // Deduplicated on the trimmed text and capped at five.
    assert_eq!(
        api.get_recent().await.unwrap(),
        ["f", "e", "d", "c", "a.org"]
    );

    // The same request is not recorded twice.
    let request = rid();
    let first = api.open("g", &request).await.unwrap();
    let again = api.open("g", &request).await.unwrap();
    assert_eq!(first, again);
    assert!(error_name(api.open("h", &request).await).ends_with("Conflict"));
    assert_eq!(api.get_recent().await.unwrap(), ["g", "f", "e", "d", "c"]);

    api.clear_recent().await.unwrap();
    assert!(api.get_recent().await.unwrap().is_empty());
}

#[tokio::test]
async fn the_daemon_export_matches_the_published_browse1_xml() {
    let f = Fixture::new(engines()).await;
    let live = zbus::fdo::IntrospectableProxy::builder(&f.client)
        .destination(production::SERVICE)
        .unwrap()
        .path(production::PATH)
        .unwrap()
        .build()
        .await
        .unwrap()
        .introspect()
        .await
        .unwrap();
    let file = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/plugin-api/browse1.xml"
    ))
    .unwrap();
    let mut live = super::super::media_routing::tests::methods(&live, "org.arinn.Browse1");
    let mut file = super::super::media_routing::tests::methods(&file, "org.arinn.Browse1");
    live.sort();
    file.sort();
    assert_eq!(live, file);
    assert_eq!(file.len(), 4);
}
