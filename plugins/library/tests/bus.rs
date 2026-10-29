// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Private-bus library provider through the SDK. Titles and file URIs are not
//! logged. No network share and no operator session.

use arinn_plugin_library::serve;
use arinn_protocol::{
    library::{
        ImportCandidate, LibraryImportProxy, LibraryProviderProxy, LibraryResumeProxy,
        LibraryRootsProxy, RootState,
    },
    production::{self, EntityRef, Registration, RegistrationReply},
    settings::ProviderSettingsProxy,
};
use std::{
    fs,
    io::{BufRead, BufReader},
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use zbus::{Connection, message::Header};

struct Bus(Child, String);
impl Bus {
    fn start() -> Self {
        let mut child = Command::new("dbus-daemon")
            .args(["--session", "--nofork", "--print-address=1"])
            .stdout(Stdio::piped())
            .spawn()
            .expect("private dbus-daemon");
        let mut address = String::new();
        BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut address)
            .unwrap();
        Self(child, address.trim().into())
    }
    async fn connection(&self) -> Connection {
        zbus::connection::Builder::address(self.1.as_str())
            .unwrap()
            .build()
            .await
            .unwrap()
    }
}
impl Drop for Bus {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[derive(Clone, Default)]
struct Daemon;
#[zbus::interface(name = "org.arinn.Daemon1")]
impl Daemon {
    async fn register(
        &self,
        request: Registration,
        #[zbus(header)] _header: Header<'_>,
    ) -> production::Result<(RegistrationReply,)> {
        assert_eq!(request.capabilities, vec!["library".to_string()]);
        Ok((RegistrationReply {
            epoch: "00000000-0000-4000-8000-000000000001".into(),
            generation: "00000000-0000-4000-8000-000000000002".into(),
            api_minor: 0,
            max_body_bytes: 262144,
            max_page_entries: 200,
            max_operations: 32,
        },))
    }
}

fn scratch() -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "arinn-library-bus-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    fs::create_dir_all(&path).unwrap();
    path
}

async fn scenario() {
    let scratch = scratch();
    let root = scratch.join("media-root");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("QuietTitle (1999).mkv"), b"film").unwrap();
    let home = scratch.join("home");
    let config_home = scratch.join("config");
    let state_home = scratch.join("state");
    fs::create_dir_all(&home).unwrap();
    // SAFETY: the re-exec child is the only thread setting these variables.
    let settings = format!(
        "{{\"roots\":[\"{}\"],\"autoscan\":false,\"probe\":false}}",
        root.display()
    );
    unsafe {
        std::env::set_var("HOME", &home);
        std::env::set_var("XDG_CONFIG_HOME", &config_home);
        std::env::set_var("XDG_STATE_HOME", &state_home);
        std::env::set_var("ARINN_LIBRARY_DEBOUNCE_MS", "200");
        std::env::set_var("ARINN_PROVIDER_SETTINGS", &settings);
    }

    let bus = Bus::start();
    let server = bus.connection().await;
    server
        .object_server()
        .at(production::PATH, Daemon)
        .await
        .unwrap();
    server.request_name(production::SERVICE).await.unwrap();
    let provider = bus.connection().await;
    let destination = provider.unique_name().unwrap().clone();
    let runner = tokio::spawn(async move { serve(provider).await });

    let proxy = LibraryProviderProxy::builder(&server)
        .destination(destination.clone())
        .unwrap()
        .build()
        .await
        .unwrap();
    let (page,) = wait_item(&proxy, "QuietTitle").await;
    assert_eq!(
        proxy.features().await.unwrap(),
        vec![
            "LibraryResume1".to_string(),
            "LibraryImport1".into(),
            "LibraryRoots1".into()
        ]
    );
    let film = page
        .items
        .iter()
        .find(|item| item.title == "QuietTitle")
        .unwrap();
    let id = film.id.clone();
    let (source,) = proxy.launch(&page.generation, &id).await.unwrap();
    source.validate().unwrap();
    assert_eq!(source.local_id, id);
    assert!(source.uri.starts_with("file:///"));
    assert_eq!(source.audio_language, "");
    assert_eq!(source.start_position_us, 0);

    fs::write(root.join("Added (2002).mkv"), b"later").unwrap();
    wait_item(&proxy, "Added").await;

    let resume = LibraryResumeProxy::builder(&server)
        .destination(destination.clone())
        .unwrap()
        .build()
        .await
        .unwrap();
    resume
        .set_progress(&page.generation, &id, 5_000_000, -1, false)
        .await
        .unwrap();
    let (resumes,) = resume.get_resumes("").await.unwrap();
    let row = resumes
        .resumes
        .iter()
        .find(|row| row.content_id == id)
        .unwrap();
    assert_eq!(row.position_us, 5_000_000);
    assert!(!row.completed);
    let (again,) = proxy.launch(&page.generation, &id).await.unwrap();
    assert_eq!(again.start_position_us, 5_000_000);

    // #373 / ADR-0090: launching a completed item over the bus clears its
    // completion and publishes the change.
    resume
        .set_progress(&page.generation, &id, 5_000_000, -1, true)
        .await
        .unwrap();
    let (resumes,) = resume.get_resumes("").await.unwrap();
    assert!(
        resumes
            .resumes
            .iter()
            .any(|row| row.content_id == id && row.completed)
    );
    let (rewatch,) = proxy.launch(&page.generation, &id).await.unwrap();
    assert_eq!(rewatch.start_position_us, 0);
    let (resumes,) = resume.get_resumes("").await.unwrap();
    let row = resumes
        .resumes
        .iter()
        .find(|row| row.content_id == id)
        .unwrap();
    assert!(!row.completed);
    assert_eq!(row.position_us, 0);

    let imported = root.join("Imported.mkv");
    fs::write(&imported, b"imported").unwrap();
    let import = LibraryImportProxy::builder(&server)
        .destination(destination)
        .unwrap()
        .build()
        .await
        .unwrap();
    let candidate = ImportCandidate {
        source: EntityRef {
            provider_id: "downloads".into(),
            capability: "download".into(),
            local_id: "job-1".into(),
        },
        file_uri: arinn_plugin_library::to_file_uri(&imported).unwrap(),
        completion_id: "done-1".into(),
    };
    let first = import
        .import_completed(&page.generation, vec![candidate.clone()])
        .await
        .unwrap();
    let second = import
        .import_completed(&page.generation, vec![candidate])
        .await
        .unwrap();
    assert_eq!(first, second);
    let (page,) = proxy.get_library("").await.unwrap();
    assert_eq!(
        page.items.iter().filter(|item| item.id == first[0]).count(),
        1
    );

    drop(runner);
    let _ = fs::remove_dir_all(&scratch);
}

const GENERATION: &str = "00000000-0000-4000-8000-000000000002";

/// The provider on the bus, with the fake daemon owning the daemon name.
async fn start_provider(bus: &Bus) -> (Connection, zbus::names::OwnedUniqueName) {
    let server = bus.connection().await;
    server
        .object_server()
        .at(production::PATH, Daemon)
        .await
        .unwrap();
    server.request_name(production::SERVICE).await.unwrap();
    let provider = bus.connection().await;
    let destination = provider.unique_name().unwrap().clone();
    tokio::spawn(async move { serve(provider).await });
    (server, destination)
}

fn method_error(result: zbus::Result<()>) -> String {
    match result {
        Err(zbus::Error::MethodError(name, _, _)) => name.to_string(),
        other => panic!("expected a method error, got {other:?}"),
    }
}

/// #540 / #551 (spec §5.6, ADR-0071): the folders are a Settings path list
/// applied at the next start, and `GetRoots` reports each configured root as
/// written, with its availability from the scan.
async fn settings_and_roots() {
    let scratch = scratch();
    let films = scratch.join("films-root");
    let share = scratch.join("share-root");
    fs::create_dir(&films).unwrap();
    fs::write(films.join("QuietTitle (1999).mkv"), b"film").unwrap();
    let home = scratch.join("home");
    fs::create_dir_all(&home).unwrap();
    // The films root is written with a trailing slash: GetRoots keeps it.
    let films_text = format!("{}/", films.display());
    let share_text = share.display().to_string();
    let settings = format!(
        "{{\"roots\":[\"{films_text}\",\"{share_text}\"],\"probe\":false,\"ffprobe\":\"ffprobe\"}}"
    );
    // SAFETY: the re-exec child is the only thread setting these variables.
    unsafe {
        std::env::set_var("HOME", &home);
        std::env::set_var("XDG_CONFIG_HOME", scratch.join("config"));
        std::env::set_var("XDG_STATE_HOME", scratch.join("state"));
        std::env::set_var("ARINN_LIBRARY_INTERVAL_MS", "200");
        std::env::set_var("ARINN_PROVIDER_SETTINGS", &settings);
    }
    let bus = Bus::start();
    let (server, destination) = start_provider(&bus).await;
    let library = LibraryProviderProxy::builder(&server)
        .destination(destination.clone())
        .unwrap()
        .build()
        .await
        .unwrap();
    wait_item(&library, "QuietTitle").await;

    // The schema: the folder list and the two scan switches, all media.
    let settings = ProviderSettingsProxy::builder(&server)
        .destination(destination.clone())
        .unwrap()
        .build()
        .await
        .unwrap();
    let schema = settings.get_schema(GENERATION).await.unwrap();
    let ids: Vec<&str> = schema.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(ids, ["roots", "autoscan", "probe"]);
    arinn_protocol::settings::validate_schema(&schema).unwrap();
    let roots = &schema[0];
    assert_eq!(
        (roots.kind.as_str(), roots.category.as_str()),
        ("path", "media")
    );
    assert_eq!(roots.value, "[]");
    let constraints: serde_json::Value = serde_json::from_str(&roots.constraints).unwrap();
    assert_eq!(
        constraints,
        serde_json::json!({"directory": true, "multiple": true, "max_items": 64})
    );
    assert!(
        schema[1..]
            .iter()
            .all(|s| s.kind == "boolean" && s.value == "true")
    );
    // A change applies at the next start; `/` and a relative folder are not
    // accepted. Nothing changes in the running provider.
    let change = format!("{{\"roots\":[\"{share_text}\"]}}");
    assert!(method_error(settings.apply(GENERATION, &change).await).ends_with(".Unsupported"));
    assert!(
        method_error(settings.apply(GENERATION, r#"{"roots":["/"]}"#).await)
            .ends_with(".InvalidArgument")
    );
    assert!(
        method_error(settings.apply(GENERATION, r#"{"roots":["films"]}"#).await)
            .ends_with(".InvalidArgument")
    );
    assert!(
        method_error(
            settings
                .apply("00000000-0000-4000-8000-000000000009", &change)
                .await
        )
        .ends_with(".Conflict")
    );

    // The share is missing: reported unavailable; the films root is not.
    let roots = LibraryRootsProxy::builder(&server)
        .destination(destination.clone())
        .unwrap()
        .build()
        .await
        .unwrap();
    let missing = vec![
        RootState {
            path: films_text.clone(),
            available: true,
        },
        RootState {
            path: share_text.clone(),
            available: false,
        },
    ];
    let before = wait_roots(&roots, &missing).await;
    // Rescans with nothing changed announce nothing.
    tokio::time::sleep(Duration::from_millis(700)).await;
    let (same,) = roots.get_roots(GENERATION).await.unwrap();
    assert_eq!(same.revision, before);
    // The share comes back: a new revision, which the SDK announces with
    // StateChanged.
    fs::create_dir(&share).unwrap();
    let back = vec![
        RootState {
            path: films_text.clone(),
            available: true,
        },
        RootState {
            path: share_text.clone(),
            available: true,
        },
    ];
    let after = wait_roots(&roots, &back).await;
    assert_eq!(after, before + 1);
    let _ = fs::remove_dir_all(&scratch);
}

async fn wait_roots(proxy: &LibraryRootsProxy<'_>, expected: &[RootState]) -> u64 {
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        if let Ok((roots,)) = proxy.get_roots(GENERATION).await
            && roots.roots == expected
        {
            return roots.revision;
        }
        if Instant::now() > deadline {
            panic!("roots did not reach the expected state");
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

async fn wait_item(
    proxy: &LibraryProviderProxy<'_>,
    title: &str,
) -> (arinn_protocol::library::LibraryPage,) {
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        if let Ok(Ok((page,))) =
            tokio::time::timeout(Duration::from_secs(2), proxy.get_library("")).await
            && page.items.iter().any(|item| item.title == title)
        {
            return (page,);
        }
        if Instant::now() > deadline {
            panic!("library item did not appear");
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

fn enter(test: &str) -> bool {
    if std::env::var_os("ARINN_LIBRARY_CHILD").is_some() {
        return true;
    }
    let token = std::env::temp_dir().join(format!(
        "arinn-library-token-{}-{}",
        test,
        std::process::id()
    ));
    fs::write(&token, "fixture-token").unwrap();
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", test, "--nocapture"])
        .env("ARINN_LIBRARY_CHILD", "1")
        .env("ARINN_PROVIDER_ID", "library")
        .env("ARINN_PLUGIN_ID", "library")
        .env("ARINN_PLUGIN_VERSION", "0.0.1")
        .env("ARINN_TOKEN_FILE", &token)
        .output()
        .unwrap();
    let _ = fs::remove_file(&token);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("QuietTitle")
            && !stderr.contains("media-root")
            && !stderr.contains("films-root")
            && !stderr.contains("share-root"),
        "stderr leaked a private name"
    );
    assert!(
        output.status.success(),
        "{}\n{stderr}",
        String::from_utf8_lossy(&output.stdout)
    );
    false
}

#[test]
fn library_provider_on_a_private_bus() {
    if !enter("library_provider_on_a_private_bus") {
        return;
    }
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime
        .block_on(async { tokio::time::timeout(Duration::from_secs(20), scenario()).await })
        .expect("private-bus scenario timed out");
}

#[test]
fn library_settings_and_roots_on_a_private_bus() {
    if !enter("library_settings_and_roots_on_a_private_bus") {
        return;
    }
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime
        .block_on(async {
            tokio::time::timeout(Duration::from_secs(30), settings_and_roots()).await
        })
        .expect("private-bus settings scenario timed out");
}
