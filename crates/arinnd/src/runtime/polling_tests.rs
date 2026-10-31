// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use crate::test_support::{Bounded, served};
use std::sync::{
    Mutex as StdMutex,
    atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
};

#[derive(Clone)]
struct SnapshotFixture {
    generation: String,
    calls: Arc<AtomicUsize>,
    revision: Arc<AtomicU64>,
    mixed: Arc<AtomicBool>,
    backend: Arc<StdMutex<String>>,
}
impl SnapshotFixture {
    fn new() -> Self {
        Self {
            generation: Uuid::new_v4().to_string(),
            calls: Arc::default(),
            revision: Arc::new(AtomicU64::new(1)),
            mixed: Arc::default(),
            backend: Arc::new(StdMutex::new("available".into())),
        }
    }
    fn page(&self, token: &str) -> (u64, String) {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let revision = self.revision.load(Ordering::SeqCst)
            + u64::from(!token.is_empty() && self.mixed.load(Ordering::SeqCst));
        (
            revision,
            if token.is_empty() {
                "next".into()
            } else {
                String::new()
            },
        )
    }
    fn take_calls(&self) -> usize {
        self.calls.swap(0, Ordering::SeqCst)
    }
}

macro_rules! fixture_entries {
    (applications, $token:expr) => {
        vec![Application {
            id: if $token.is_empty() { "first" } else { "last" }.into(),
            title: "Synthetic application".into(),
            can_launch: true,
            group: "other".into(),
            icon: String::new(),
        }]
    };
    ($field:ident, $token:expr) => {
        vec![]
    };
}

macro_rules! snapshot_test {
    ($name:ident, $test:ident, $iface:literal, $proxy:path, $page:path, $read:path, $method:ident, {$($field:ident),+}) => {
        struct $name(SnapshotFixture);
        #[zbus::interface(name = $iface)]
        impl $name {
            fn $method(&self, token: &str) -> ($page,) {
                let (revision, next_token) = self.0.page(token);
                use $page as Page;
                (Page { generation: self.0.generation.clone(), revision, $($field: fixture_entries!($field, token),)+ next_token },)
            }
            fn backend_state(&self) -> String { self.0.backend.lock().unwrap().clone() }
            fn features(&self) -> Vec<String> { vec![] }
            // The watcher test emits the application fixture's signal only.
            #[allow(dead_code)]
            #[zbus(signal)]
            async fn state_changed(emitter: &SignalEmitter<'_>, generation: &str, revision: u64) -> zbus::Result<()>;
        }
        #[tokio::test]
        async fn $test() {
            let bus = PrivateBus::new();
            let client = bus.connect().await;
            let server = bus.connect().await;
            let fixture = SnapshotFixture::new();
            server.object_server().at(production::PROVIDER_PATH, $name(fixture.clone())).await.unwrap();
            served(&client, &server).await;
            let proxy = <$proxy>::builder(&client).destination(server.unique_name().unwrap()).unwrap().build().await.unwrap();
            let generation = &fixture.generation;
            let first = $read(&proxy, generation, None).bounded().await.unwrap();
            assert!(first.1.is_some());
            assert_eq!(fixture.take_calls(), 2);
            let unchanged = $read(&proxy, generation, Some(1)).bounded().await.unwrap();
            assert!(unchanged.1.is_none());
            assert_eq!(fixture.take_calls(), 1);
            // The fast path must still validate the first page's generation.
            assert_eq!($read(&proxy, &Uuid::new_v4().to_string(), Some(1)).bounded().await.unwrap_err(), ErrorCode::Conflict);
            assert_eq!(fixture.take_calls(), 1);
            // New revisions are read completely, even after an unchanged probe.
            fixture.revision.store(2, Ordering::SeqCst);
            let changed = $read(&proxy, generation, Some(1)).bounded().await.unwrap();
            assert_eq!(changed.0, 2);
            assert!(changed.1.is_some());
            assert_eq!(fixture.take_calls(), 2);
            // A mixed revision cannot become the next accepted snapshot.
            fixture.mixed.store(true, Ordering::SeqCst);
            assert_eq!($read(&proxy, generation, Some(1)).bounded().await.unwrap_err(), ErrorCode::Conflict);
            assert_eq!(fixture.take_calls(), 2);
        }
    };
}

snapshot_test!(Applications, applications_unchanged_revision_reads_only_first_page, "org.arinn.Provider1", ApplicationProviderProxy, ApplicationPage, read_snapshot, get_applications, {applications, running});

snapshot_test!(Browser, browser_unchanged_revision_reads_only_first_page, "org.arinn.BrowserProvider1", BrowserProviderProxy, TabPage, read_tabs, get_tabs, {windows, tabs});

snapshot_test!(
    Media,
    media_unchanged_revision_reads_only_first_page,
    "org.arinn.MediaProvider1",
    arinn_protocol::media::MediaProviderProxy,
    arinn_protocol::media::SessionPage,
    media_routing::read_sessions,
    get_sessions,
    { sessions }
);

snapshot_test!(Library, library_unchanged_revision_reads_only_first_page, "org.arinn.LibraryProvider1", arinn_protocol::library::LibraryProviderProxy, arinn_protocol::library::LibraryPage, library_routing::read_library, get_library, {collections, items});

snapshot_test!(
    Resumes,
    resumes_unchanged_revision_reads_only_first_page,
    "org.arinn.LibraryResume1",
    arinn_protocol::library::LibraryResumeProxy,
    arinn_protocol::library::ResumePage,
    library_routing::read_resumes,
    get_resumes,
    { resumes }
);

snapshot_test!(
    Downloads,
    downloads_unchanged_revision_reads_only_first_page,
    "org.arinn.DownloadProvider1",
    arinn_protocol::download::DownloadProviderProxy,
    arinn_protocol::download::JobPage,
    download_routing::read_jobs,
    get_jobs,
    { jobs }
);

#[test]
fn revision_hints_ignore_redundant_and_foreign_signals() {
    assert!(new_revision_hint("g", None, "g", 1));
    assert!(new_revision_hint("g", Some(1), "g", 2));
    assert!(!new_revision_hint("g", Some(1), "g", 1));
    assert!(!new_revision_hint("g", Some(2), "g", 1));
    assert!(!new_revision_hint("g", None, "other", 1));
}

struct PrivateBus(std::process::Child, String);
impl PrivateBus {
    fn new() -> Self {
        use std::io::{BufRead, BufReader};
        let mut child = std::process::Command::new("dbus-daemon")
            .args(["--session", "--nofork", "--print-address=1"])
            .stdout(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let mut address = String::new();
        BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut address)
            .unwrap();
        Self(child, address.trim().into())
    }
    async fn connect(&self) -> Connection {
        zbus::connection::Builder::address(self.1.as_str())
            .unwrap()
            .build()
            .bounded()
            .await
            .unwrap()
    }
}
impl Drop for PrivateBus {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[tokio::test]
async fn application_watcher_coalesces_hints_and_keeps_backend_refreshes() {
    let bus = PrivateBus::new();
    let client = bus.connect().await;
    let server = bus.connect().await;
    let fixture = SnapshotFixture::new();
    server
        .object_server()
        .at(production::PROVIDER_PATH, Applications(fixture.clone()))
        .await
        .unwrap();
    served(&client, &server).await;
    let directory = std::env::temp_dir().join(format!("arinn-poll-watcher-{}", Uuid::new_v4()));
    let core = Core::new(
        &client,
        Paths {
            config: directory.join("config"),
            state: directory.join("state"),
            data: directory.join("data"),
        },
        directory.join("runtime"),
    );
    let owner = server.unique_name().unwrap().to_string();
    core.state
        .lock()
        .await
        .providers
        .insert("fixture".into(), test_provider(&owner, &fixture.generation));
    let watcher = {
        let core = core.clone();
        let owner = owner.clone();
        let generation = fixture.generation.clone();
        tokio::spawn(async move {
            core.watch_applications("fixture".into(), owner, generation)
                .await
        })
    };
    async {
        while core.state.lock().await.providers["fixture"].info.revision != 1 {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    }
    .bounded()
    .await;
    assert!(fixture.take_calls() >= 2);
    let emitter = SignalEmitter::new(&server, production::PROVIDER_PATH).unwrap();
    for _ in 0..40 {
        Applications::state_changed(&emitter, &fixture.generation, 1)
            .await
            .unwrap();
    }
    *fixture.backend.lock().unwrap() = "unavailable".into();
    async {
        while core.state.lock().await.providers["fixture"]
            .info
            .backend_state
            != "unavailable"
        {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    }
    .bounded()
    .await;
    // At least one fallback refresh observes metadata independently of the
    // catalogue revision. Exact per-read counts and duplicate-hint rejection
    // are tested separately, without depending on CI scheduling latency.
    assert!(fixture.take_calls() >= 1);
    let state = core.state.lock().await;
    let p = &state.providers["fixture"];
    assert_eq!(p.info.revision, 1);
    assert!(p.cache.is_some());
    assert_eq!(
        p.applications
            .iter()
            .map(|a| a.id.as_str())
            .collect::<Vec<_>>(),
        ["first", "last"]
    );
    drop(state);
    watcher.abort();
    let _ = watcher.await;
    let _ = std::fs::remove_dir_all(directory);
}

fn test_provider(owner: &str, generation: &str) -> Provider {
    Provider {
        info: ProviderInfo {
            id: "fixture".into(),
            plugin_id: "fixture".into(),
            source_path: String::new(),
            name: "Fixture".into(),
            version: "0.0.1".into(),
            path: "/org/arinn/Daemon1/providers/p_fixture".try_into().unwrap(),
            capabilities: vec!["application".into()],
            runtime_state: "starting".into(),
            backend_state: "available".into(),
            generation: generation.into(),
            revision: 0,
            last_error: String::new(),
            error_at: 0,
            recovered_at: 0,
            restarts_since_daemon_start: 0,
            cumulative_restart_count: 0,
        },
        manifest: None,
        unit: String::new(),
        token_file: PathBuf::new(),
        token: String::new(),
        owner: owner.into(),
        cache: None,
        applications: vec![],
        running: vec![],
        tab_cache: None,
        windows: vec![],
        tabs: vec![],
        session_cache: None,
        sessions: vec![],
        features: vec![],
        library_cache: None,
        collections: vec![],
        items: vec![],
        resume_cache: None,
        resumes: vec![],
        settings_schema: None,
        started: Instant::now(),
        enabled: true,
    }
}
