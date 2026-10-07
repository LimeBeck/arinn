// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Private-bus library provider through the SDK. Titles and file URIs are not
//! logged. No network share and no operator session.

use arinn_plugin_library::serve;
use arinn_protocol::{
    library::{ImportCandidate, LibraryImportProxy, LibraryProviderProxy, LibraryResumeProxy},
    production::{self, EntityRef, Registration, RegistrationReply},
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
        vec!["LibraryResume1".to_string(), "LibraryImport1".into()]
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
        !stderr.contains("QuietTitle") && !stderr.contains("media-root"),
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
