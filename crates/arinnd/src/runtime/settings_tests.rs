// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `org.arinn.Settings1` (ADR-0065) on a private bus, with a provider served
//! by the SDK's `serve_settings`.
use super::*;
use arinn_plugin_sdk::runtime::{RegistrationContext, SettingsHandler, serve_settings};
use arinn_protocol::{production::ProductionOperationProxy, settings::SettingsApiProxy};
use std::{
    io::{BufRead, BufReader},
    process::{Child, Command, Stdio},
    sync::Mutex as StdMutex,
};

fn schema() -> Vec<Setting> {
    vec![Setting {
        id: "roots".into(),
        category: "media".into(),
        title: "Library folders".into(),
        description: String::new(),
        kind: settings::KIND_PATH.into(),
        value: r#"["/srv/films"]"#.into(),
        constraints: r#"{"directory":true,"multiple":true}"#.into(),
        enabled: true,
    }]
}

#[derive(Clone)]
struct Handler {
    applied: Arc<StdMutex<Vec<Map<String, Value>>>>,
    answer: Arc<StdMutex<Result<(), ErrorCode>>>,
}
impl SettingsHandler for Handler {
    fn schema(&self) -> Vec<Setting> {
        schema()
    }
    fn apply(
        &self,
        values: Map<String, Value>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), ErrorCode>> + Send + '_>>
    {
        self.applied.lock().unwrap().push(values);
        let answer = *self.answer.lock().unwrap();
        Box::pin(async move { answer })
    }
}

struct Fixture {
    bus: Child,
    folder: PathBuf,
    _core: Core,
    client: Connection,
    _providers: Vec<Connection>,
    handler: Handler,
    generation: String,
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
fn provider(id: &str, owner: &Connection, generation: &str, folder: &std::path::Path) -> Provider {
    Provider {
        info: ProviderInfo {
            id: id.into(),
            plugin_id: id.into(),
            source_path: String::new(),
            name: format!("{id} name"),
            version: "0.0.1".into(),
            path: format!("/org/arinn/Daemon1/providers/p_{id}")
                .try_into()
                .unwrap(),
            capabilities: vec!["library".into()],
            runtime_state: "ready".into(),
            backend_state: "available".into(),
            generation: generation.into(),
            revision: 1,
            last_error: String::new(),
            error_at: 0,
            recovered_at: 0,
            restarts_since_daemon_start: 0,
            cumulative_restart_count: 0,
        },
        manifest: None,
        unit: format!("{id}.service"),
        token_file: folder.join(format!("{id}.token")),
        token: String::new(),
        owner: owner.unique_name().unwrap().to_string(),
        cache: None,
        applications: Vec::new(),
        running: Vec::new(),
        tab_cache: None,
        session_cache: None,
        sessions: Vec::new(),
        features: Vec::new(),
        library_cache: None,
        collections: Vec::new(),
        items: Vec::new(),
        resume_cache: None,
        resumes: Vec::new(),
        settings_schema: None,
        windows: Vec::new(),
        tabs: Vec::new(),
        started: Instant::now(),
        enabled: true,
    }
}
impl Fixture {
    async fn new() -> Self {
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
        let library = connect(&address).await;
        let plain = connect(&address).await;
        let client = connect(&address).await;
        server.request_name(production::SERVICE).await.unwrap();
        let folder = std::env::temp_dir().join(format!("arinn-settings-{}", Uuid::new_v4()));
        std::fs::create_dir(&folder).unwrap();
        let core = Core::new(
            &server,
            Paths {
                config: folder.join("config"),
                data: folder.join("data"),
                state: folder.join("state"),
            },
            folder.clone(),
        );
        core.export_settings().await.unwrap();
        let generation = Uuid::new_v4().to_string();
        let handler = Handler {
            applied: Arc::default(),
            answer: Arc::new(StdMutex::new(Ok(()))),
        };
        let (_tx, rx) = tokio::sync::watch::channel(Some(RegistrationContext {
            connection: library.clone(),
            daemon_owner: server.unique_name().unwrap().to_string(),
            daemon_epoch: core.epoch.clone(),
            generation: generation.clone(),
        }));
        // The channel sender must outlive the provider: keep it in a task.
        tokio::spawn(async move {
            let _keep = _tx;
            std::future::pending::<()>().await
        });
        serve_settings(&library, handler.clone(), rx).await.unwrap();
        {
            let mut state = core.state.lock().await;
            state.providers.insert(
                "library".into(),
                provider("library", &library, &generation, &folder),
            );
            state.providers.insert(
                "plain".into(),
                provider("plain", &plain, &generation, &folder),
            );
        }
        Self {
            bus,
            folder,
            _core: core,
            client,
            _providers: vec![library, plain],
            handler,
            generation,
        }
    }
    async fn api(&self) -> SettingsApiProxy<'static> {
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
    fn stored(&self) -> String {
        std::fs::read_to_string(self.folder.join("config").join("config.toml")).unwrap_or_default()
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
async fn only_providers_with_a_schema_are_listed() {
    let f = Fixture::new().await;
    let providers = f.api().await.list_providers().await.unwrap();
    assert_eq!(
        providers,
        vec![
            ("arinn".into(), "Arinn".into()),
            ("library".into(), "library name".into())
        ]
    );
}

#[tokio::test]
async fn a_value_is_stored_applied_and_signalled() {
    let f = Fixture::new().await;
    let api = f.api().await;
    let mut changes = api.receive_settings_changed().await.unwrap();
    let (generation, settings) = api.get_settings("library").await.unwrap();
    assert_eq!(generation, f.generation);
    assert_eq!(settings[0].value, r#"["/srv/films"]"#);

    let path = api
        .set_value(
            "library",
            &generation,
            "roots",
            r#"["/mnt/nas/films"]"#,
            &rid(),
        )
        .await
        .unwrap();
    f.wait(path).await.unwrap();
    assert!(f.stored().contains("/mnt/nas/films"), "{}", f.stored());
    assert_eq!(
        Value::Object(f.handler.applied.lock().unwrap()[0].clone()),
        serde_json::json!({"roots":["/mnt/nas/films"]})
    );
    let signal = tokio::time::timeout(Duration::from_secs(3), changes.next())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(signal.args().unwrap().provider_id, "library");
    let (_, settings) = api.get_settings("library").await.unwrap();
    assert_eq!(settings[0].value, r#"["/mnt/nas/films"]"#);
}

#[tokio::test]
async fn bad_requests_are_refused_before_anything_is_stored() {
    let f = Fixture::new().await;
    let api = f.api().await;
    let g = f.generation.as_str();
    assert_eq!(
        error_name(
            api.set_value("library", g, "roots", r#"["relative"]"#, &rid())
                .await
        ),
        "org.arinn.Error1.InvalidArgument"
    );
    assert_eq!(
        error_name(
            api.set_value("library", "stale", "roots", "[]", &rid())
                .await
        ),
        "org.arinn.Error1.Conflict"
    );
    assert_eq!(
        error_name(api.set_value("library", g, "nope", "[]", &rid()).await),
        "org.arinn.Error1.NotFound"
    );
    assert_eq!(
        error_name(api.get_settings("missing").await),
        "org.arinn.Error1.NotFound"
    );
    assert!(!f.stored().contains("roots"));
    assert!(f.handler.applied.lock().unwrap().is_empty());
}

#[tokio::test]
async fn a_rejected_value_restores_the_file_and_unsupported_keeps_it() {
    let f = Fixture::new().await;
    let api = f.api().await;
    let g = f.generation.clone();
    *f.handler.answer.lock().unwrap() = Err(ErrorCode::InvalidArgument);
    let path = api
        .set_value("library", &g, "roots", r#"["/refused"]"#, &rid())
        .await
        .unwrap();
    assert_eq!(f.wait(path).await, Err("InvalidArgument".into()));
    assert!(!f.stored().contains("/refused"), "{}", f.stored());

    // Only applies at the next start: stored, operation reports Unsupported.
    *f.handler.answer.lock().unwrap() = Err(ErrorCode::Unsupported);
    let path = api
        .set_value("library", &g, "roots", r#"["/later"]"#, &rid())
        .await
        .unwrap();
    assert_eq!(f.wait(path).await, Err("Unsupported".into()));
    assert!(f.stored().contains("/later"));
}

#[tokio::test]
async fn the_daemon_settings_live_under_arinn() {
    let f = Fixture::new().await;
    let api = f.api().await;
    let (generation, settings) = api.get_settings("arinn").await.unwrap();
    assert_eq!(settings[0].id, "browser_application");
    let path = api
        .set_value(
            "arinn",
            &generation,
            "browser_application",
            r#""chromium.desktop""#,
            &rid(),
        )
        .await
        .unwrap();
    f.wait(path).await.unwrap();
    assert_eq!(
        Config::load(&f.folder.join("config"))
            .unwrap()
            .browser_application,
        "chromium.desktop"
    );
}

#[tokio::test]
async fn the_daemon_export_matches_the_published_settings1_xml() {
    let f = Fixture::new().await;
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
        "/../../docs/plugin-api/settings1.xml"
    ))
    .unwrap();
    let mut live = super::super::media_routing::tests::methods(&live, "org.arinn.Settings1");
    let mut file = super::super::media_routing::tests::methods(&file, "org.arinn.Settings1");
    live.sort();
    file.sort();
    assert_eq!(live, file);
    assert_eq!(file.len(), 3);
}
