// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `org.arinn.Settings1` (ADR-0065) on a private bus, with a provider served
//! by the SDK's `serve_settings`.
use super::*;
use crate::test_support::served;
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

fn translations() -> settings::Translations {
    settings::Translations::default().with(
        "ru",
        "roots",
        settings::Text {
            title: Some("Папки медиатеки".into()),
            ..settings::Text::default()
        },
    )
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
    fn translations(&self) -> settings::Translations {
        translations()
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
    address: String,
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
        // The tests call the daemon, and the daemon the library provider,
        // right after their first exports (#528).
        served(&client, &server).await;
        served(&client, &library).await;
        // `plain` serves no ProviderSettings1, but its object server must
        // run to answer UnknownInterface rather than leave calls unanswered.
        let _ = plain.object_server();
        served(&client, &plain).await;
        Self {
            bus,
            address,
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

/// ADR-0112: the shell's presentation preferences under `arinn`, with their
/// defaults, stored in `config.toml`.
#[tokio::test]
async fn the_shell_preferences_are_core_settings() {
    let f = Fixture::new().await;
    let api = f.api().await;
    let (generation, settings) = api.get_settings("arinn").await.unwrap();
    settings::validate_schema(&settings).unwrap();
    let find =
        |settings: &[Setting], id: &str| settings.iter().find(|s| s.id == id).cloned().unwrap();
    let light = find(&settings, "light_field");
    assert_eq!(
        (
            light.category.as_str(),
            light.kind.as_str(),
            light.value.as_str(),
            light.description.as_str()
        ),
        (
            "appearance",
            "boolean",
            "true",
            "A soft glow along the bottom of the screen and behind the item in focus. Off turns the glow off"
        )
    );
    let ambient = find(&settings, "ambient_timeout");
    assert_eq!(
        (
            ambient.category.as_str(),
            ambient.kind.as_str(),
            ambient.value.as_str()
        ),
        ("appearance", "enum", r#""10""#)
    );
    for value in ["5", "10", "15", "30", "60"] {
        assert!(
            ambient
                .constraints
                .contains(&format!(r#""value":"{value}""#)),
            "{value}"
        );
    }
    let sounds = find(&settings, "interface_sounds");
    assert_eq!(
        (
            sounds.category.as_str(),
            sounds.kind.as_str(),
            sounds.value.as_str()
        ),
        ("sound", "enum", r#""quiet""#)
    );
    for value in ["off", "quiet", "normal"] {
        assert!(
            sounds
                .constraints
                .contains(&format!(r#""value":"{value}""#)),
            "{value}"
        );
    }

    let path = api
        .set_value("arinn", &generation, "light_field", "false", &rid())
        .await
        .unwrap();
    f.wait(path).await.unwrap();
    let path = api
        .set_value("arinn", &generation, "ambient_timeout", r#""30""#, &rid())
        .await
        .unwrap();
    f.wait(path).await.unwrap();
    let path = api
        .set_value(
            "arinn",
            &generation,
            "interface_sounds",
            r#""normal""#,
            &rid(),
        )
        .await
        .unwrap();
    f.wait(path).await.unwrap();
    let config = Config::load(&f.folder.join("config")).unwrap();
    assert_eq!(
        (
            config.light_field,
            config.ambient_timeout,
            config.interface_sounds.as_str()
        ),
        (false, 30, "normal")
    );
    let (_, settings) = api.get_settings("arinn").await.unwrap();
    assert_eq!(find(&settings, "light_field").value, "false");
    assert_eq!(find(&settings, "ambient_timeout").value, r#""30""#);
    assert_eq!(find(&settings, "interface_sounds").value, r#""normal""#);
    // Only the offered timeouts; nothing is stored for another one.
    for bad in [r#""7""#, "30", r#""never""#] {
        assert_eq!(
            error_name(
                api.set_value("arinn", &generation, "ambient_timeout", bad, &rid())
                    .await
            ),
            "org.arinn.Error1.InvalidArgument",
            "{bad}"
        );
    }
    for bad in [r#""loud""#, "quiet", r#""maybe""#] {
        assert_eq!(
            error_name(
                api.set_value("arinn", &generation, "interface_sounds", bad, &rid())
                    .await
            ),
            "org.arinn.Error1.InvalidArgument",
            "{bad}"
        );
    }
    assert_eq!(
        Config::load(&f.folder.join("config"))
            .unwrap()
            .ambient_timeout,
        30
    );
}

#[tokio::test]
async fn ui_language_discovers_partial_files_and_keeps_a_removed_choice() {
    let f = Fixture::new().await;
    let api = f.api().await;
    let (generation, schema) = api.get_settings("arinn").await.unwrap();
    let language = schema.iter().find(|s| s.id == "ui_language").unwrap();
    assert_eq!(
        (
            language.category.as_str(),
            language.kind.as_str(),
            language.value.as_str()
        ),
        ("appearance", "enum", r#""system""#)
    );
    for value in ["system", "en", "ru"] {
        assert!(
            language
                .constraints
                .contains(&format!(r#""value":"{value}""#))
        );
    }
    let directory = f.folder.join("data/translations/overrides/de/shell");
    std::fs::create_dir_all(&directory).unwrap();
    let file = directory.join("arinn.qm");
    std::fs::write(&file, b"opaque local catalogue").unwrap();
    let (_, schema) = api.get_settings("arinn").await.unwrap();
    assert!(
        schema
            .iter()
            .find(|s| s.id == "ui_language")
            .unwrap()
            .constraints
            .contains(r#""value":"de""#)
    );
    let operation = api
        .set_value("arinn", &generation, "ui_language", r#""de""#, &rid())
        .await
        .unwrap();
    f.wait(operation).await.unwrap();
    assert_eq!(
        Config::load(&f.folder.join("config")).unwrap().ui_language,
        "de"
    );
    std::fs::remove_file(file).unwrap();
    let (_, schema) = api.get_settings("arinn").await.unwrap();
    let language = schema.iter().find(|s| s.id == "ui_language").unwrap();
    assert_eq!(language.value, r#""de""#);
    assert!(language.constraints.contains(r#""value":"de""#));
    for invalid in [r#""../ru""#, r#""en_US""#, "42", r#""unavailable""#] {
        assert_eq!(
            error_name(
                api.set_value("arinn", &generation, "ui_language", invalid, &rid())
                    .await
            ),
            "org.arinn.Error1.InvalidArgument"
        );
    }
    assert_eq!(
        Config::load(&f.folder.join("config")).unwrap().ui_language,
        "de"
    );
}

/// ADR-0136: Appearance › Light follows the time of day, a boolean of the
/// `arinn` source, on by default and stored in `config.toml`.
#[tokio::test]
async fn light_follows_the_time_of_day_is_a_core_setting() {
    let f = Fixture::new().await;
    let api = f.api().await;
    let (generation, settings) = api.get_settings("arinn").await.unwrap();
    settings::validate_schema(&settings).unwrap();
    let setting = settings
        .iter()
        .find(|s| s.id == "light_follows_time")
        .cloned()
        .unwrap();
    assert_eq!(
        (
            setting.category.as_str(),
            setting.kind.as_str(),
            setting.value.as_str(),
            setting.title.as_str()
        ),
        (
            "appearance",
            "boolean",
            "true",
            "Light follows the time of day"
        )
    );
    let path = api
        .set_value("arinn", &generation, "light_follows_time", "false", &rid())
        .await
        .unwrap();
    f.wait(path).await.unwrap();
    let config = Config::load(&f.folder.join("config")).unwrap();
    assert!(!config.light_follows_time);
    let (_, settings) = api.get_settings("arinn").await.unwrap();
    assert_eq!(
        settings
            .iter()
            .find(|s| s.id == "light_follows_time")
            .unwrap()
            .value,
        "false"
    );
    assert_eq!(
        error_name(
            api.set_value(
                "arinn",
                &generation,
                "light_follows_time",
                r#""yes""#,
                &rid()
            )
            .await
        ),
        "org.arinn.Error1.InvalidArgument"
    );
    assert!(
        !Config::load(&f.folder.join("config"))
            .unwrap()
            .light_follows_time
    );
}

/// Owner decision on #488: a hand-edited invalid timeout does not break the
/// daemon's settings; it reads as the default.
#[tokio::test]
async fn an_invalid_hand_edited_timeout_reads_as_the_default() {
    let f = Fixture::new().await;
    let api = f.api().await;
    api.get_settings("arinn").await.unwrap();
    let path = f.folder.join("config").join("config.toml");
    let text = std::fs::read_to_string(&path).unwrap();
    let text: String = text
        .lines()
        .filter(|line| !line.starts_with("ambient_timeout"))
        .map(|line| format!("{line}\n"))
        .collect();
    // Top-level keys come before the first table.
    std::fs::write(&path, format!("ambient_timeout = 7\n{text}")).unwrap();
    let (_, settings) = api.get_settings("arinn").await.unwrap();
    let ambient = settings.iter().find(|s| s.id == "ambient_timeout").unwrap();
    assert_eq!(ambient.value, r#""10""#);
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
    assert_eq!(file.len(), 4);
}

/// ADR-0129: the provider's table is passed through with its generation;
/// the daemon's own source and an unknown provider have none to give.
#[tokio::test]
async fn translations_are_passed_through() {
    let f = Fixture::new().await;
    let api = f.api().await;
    let (generation, json) = api.get_translations("library").await.unwrap();
    assert_eq!(generation, f.generation);
    assert_eq!(
        settings::Translations::from_json(&json).unwrap(),
        translations()
    );
    let (_, json) = api.get_translations("arinn").await.unwrap();
    assert_eq!(json, "{}");
    // A provider without ProviderSettings1.
    let (_, json) = api.get_translations("plain").await.unwrap();
    assert_eq!(json, "{}");
    assert_eq!(
        error_name(api.get_translations("nobody").await),
        "org.arinn.Error1.NotFound"
    );
}

/// A provider that answers GetSchema late and never answers
/// GetTranslations.
struct Slow;
#[zbus::interface(name = "org.arinn.ProviderSettings1")]
impl Slow {
    async fn get_schema(&self, _generation: &str) -> Vec<Setting> {
        tokio::time::sleep(Duration::from_millis(1500)).await;
        schema()
    }
    async fn get_translations(&self, _generation: &str) -> String {
        tokio::time::sleep(Duration::from_secs(30)).await;
        "{}".into()
    }
}

/// ADR-0129: the schema and the translations share one SCHEMA_TIMEOUT
/// deadline. Two serial deadlines let one fetch take almost 4 s, past the
/// shell's 3 s call timeout, and the whole Settings load failed.
#[tokio::test]
async fn schema_and_translations_share_one_deadline() {
    let f = Fixture::new().await;
    let slow = connect(&f.address).await;
    slow.object_server()
        .at("/org/arinn/Provider1", Slow)
        .await
        .unwrap();
    served(&f.client, &slow).await;
    {
        let mut state = f._core.state.lock().await;
        state.providers.insert(
            "slow".into(),
            provider("slow", &slow, &f.generation, &f.folder),
        );
    }
    let api = f.api().await;
    let started = Instant::now();
    let (generation, json) = api.get_translations("slow").await.unwrap();
    let elapsed = started.elapsed();
    assert_eq!(generation, f.generation);
    assert_eq!(json, "{}");
    assert!(
        elapsed < SCHEMA_TIMEOUT + Duration::from_millis(500),
        "{elapsed:?}"
    );
    // The schema answered in time and is kept.
    let (_, settings) = api.get_settings("slow").await.unwrap();
    assert_eq!(settings.len(), 1);
}

/// The daemon's own schema obeys the same rules as a provider schema, so
/// the shell shows every row: `search_engine` once used the unknown
/// category `browser` and was dropped (#682). Handoff: a Settings › Apps row.
#[test]
fn the_daemon_schema_is_valid_and_lists_the_search_engine_under_applications() {
    let engines = vec![crate::browse::Engine {
        id: "example".into(),
        title: "Example Search".into(),
        template: "https://search.example/?q={query}".into(),
    }];
    let schema = daemon_schema(&engines, &crate::localization::languages(&[], "system"));
    assert_eq!(settings::validate_schema(&schema), Ok(()));
    let engine = schema.iter().find(|s| s.id == "search_engine").unwrap();
    assert_eq!(engine.category, "applications");
}
