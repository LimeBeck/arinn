// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use crate::test_support::served;
use arinn_protocol::{
    download::{DownloadsApiProxy, JobPage, Output},
    production::ProductionOperationProxy,
};
use std::{
    io::{BufRead, BufReader},
    process::{Child, Command, Stdio},
    sync::Mutex as StdMutex,
};

fn job(id: &str, completed: bool, outputs: usize) -> Job {
    Job {
        id: id.into(),
        title: "Synthetic download".into(),
        state: if completed { "completed" } else { "paused" }.into(),
        completed_bytes: 0,
        has_total: false,
        total_bytes: 0,
        rate_bytes_per_second: 0,
        eta_seconds: -1,
        has_progress: false,
        progress: 0.0,
        actions: vec![
            "pause".into(),
            "resume".into(),
            "start-now".into(),
            "retry".into(),
        ],
        error_code: String::new(),
        error_message: String::new(),
        outputs: (0..outputs)
            .map(|i| Output {
                id: format!("out-{i}"),
                title: "Fixture".into(),
                file_uri: format!("file:///synthetic/output-{i}.mkv"),
            })
            .collect(),
    }
}
#[derive(Clone)]
struct FakeDownload {
    generation: String,
    jobs: Arc<StdMutex<Vec<Job>>>,
    backend: Arc<StdMutex<String>>,
    revision: Arc<StdMutex<u64>>,
    calls: Arc<StdMutex<Vec<String>>>,
}
#[zbus::interface(name = "org.arinn.DownloadProvider1")]
impl FakeDownload {
    fn get_jobs(&self, _token: &str) -> (JobPage,) {
        (JobPage {
            generation: self.generation.clone(),
            revision: *self.revision.lock().unwrap(),
            jobs: self.jobs.lock().unwrap().clone(),
            next_token: String::new(),
        },)
    }
    fn backend_state(&self) -> String {
        self.backend.lock().unwrap().clone()
    }
    fn features(&self) -> Vec<String> {
        vec![]
    }
    fn pause(&self, _generation: &str, _id: &str) {
        self.calls.lock().unwrap().push("pause".into());
    }
    fn resume(&self, _generation: &str, _id: &str) {
        self.calls.lock().unwrap().push("resume".into());
    }
    fn start_now(&self, _generation: &str, _id: &str) {
        self.calls.lock().unwrap().push("start-now".into());
    }
    fn retry(&self, _generation: &str, _id: &str) -> production::Result<()> {
        self.calls.lock().unwrap().push("retry".into());
        Err(ErrorCode::Timeout.into())
    }
    #[zbus(signal)]
    async fn state_changed(
        emitter: &SignalEmitter<'_>,
        generation: &str,
        revision: u64,
    ) -> zbus::Result<()>;
}
#[derive(Default)]
struct Ledger {
    entries: BTreeMap<(String, String, String), String>,
    calls: usize,
    batch_sizes: Vec<usize>,
    fail_after_commit: bool,
    empty: bool,
    unsupported: bool,
}
#[derive(Clone)]
struct FakeImport {
    ledger: Arc<StdMutex<Ledger>>,
}
#[zbus::interface(name = "org.arinn.LibraryImport1")]
impl FakeImport {
    fn import_completed(
        &self,
        _generation: &str,
        candidates: Vec<ImportCandidate>,
    ) -> production::Result<Vec<String>> {
        library::validate_imports(&candidates)?;
        let mut ledger = self.ledger.lock().unwrap();
        ledger.calls += 1;
        ledger.batch_sizes.push(candidates.len());
        if ledger.unsupported {
            return Err(ErrorCode::Unsupported.into());
        }
        if ledger.empty {
            return Ok(vec![]);
        }
        let mut result = vec![];
        for candidate in candidates {
            assert_eq!(candidate.source.capability, "download");
            let next = format!("item-{}", ledger.entries.len());
            let value = ledger
                .entries
                .entry((
                    candidate.source.provider_id,
                    candidate.source.local_id,
                    candidate.completion_id,
                ))
                .or_insert(next)
                .clone();
            result.push(value);
        }
        if ledger.fail_after_commit {
            ledger.fail_after_commit = false;
            return Err(ErrorCode::Timeout.into());
        }
        Ok(result)
    }
}
struct Fixture {
    bus: Child,
    folder: PathBuf,
    core: Core,
    client: Connection,
    provider: Connection,
    fake: FakeDownload,
    ledger: Arc<StdMutex<Ledger>>,
    generation: String,
    path: OwnedObjectPath,
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
fn provider(
    id: &str,
    capability: &str,
    owner: &str,
    generation: &str,
    path: OwnedObjectPath,
) -> Provider {
    Provider {
        info: ProviderInfo {
            id: id.into(),
            plugin_id: id.into(),
            source_path: String::new(),
            name: "Fixture".into(),
            version: "0.0.1".into(),
            path,
            capabilities: vec![capability.into()],
            runtime_state: "ready".into(),
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
impl Fixture {
    async fn new(jobs: Vec<Job>) -> Self {
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
        let provider_bus = connect(&address).await;
        let client = connect(&address).await;
        server.request_name(production::SERVICE).await.unwrap();
        let folder = std::env::temp_dir().join(format!("arinn-download-{}", Uuid::new_v4()));
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
        let generation = Uuid::new_v4().to_string();
        let fake = FakeDownload {
            generation: generation.clone(),
            jobs: Arc::new(StdMutex::new(jobs)),
            backend: Arc::new(StdMutex::new("available".into())),
            revision: Arc::new(StdMutex::new(1)),
            calls: Arc::default(),
        };
        let ledger = Arc::default();
        provider_bus
            .object_server()
            .at(production::PROVIDER_PATH, fake.clone())
            .await
            .unwrap();
        provider_bus
            .object_server()
            .at(
                production::PROVIDER_PATH,
                FakeImport {
                    ledger: Arc::clone(&ledger),
                },
            )
            .await
            .unwrap();
        let path: OwnedObjectPath = "/org/arinn/Daemon1/providers/p_download"
            .try_into()
            .unwrap();
        server
            .object_server()
            .at(
                path.clone(),
                DownloadsEndpoint {
                    core: core.clone(),
                    id: "downloads".into(),
                },
            )
            .await
            .unwrap();
        core.state.lock().await.providers.insert(
            "downloads".into(),
            provider(
                "downloads",
                "download",
                provider_bus.unique_name().unwrap().as_str(),
                &generation,
                path.clone(),
            ),
        );
        let mut library = provider(
            "library",
            "library",
            provider_bus.unique_name().unwrap().as_str(),
            &generation,
            "/org/arinn/Daemon1/providers/p_library".try_into().unwrap(),
        );
        library.features.push(library::FEATURE_IMPORT.into());
        core.state
            .lock()
            .await
            .providers
            .insert("library".into(), library);
        // The tests call the provider and the daemon right after their first
        // exports (#528).
        served(&client, &provider_bus).await;
        served(&client, &server).await;
        Self {
            bus,
            folder,
            core,
            client,
            provider: provider_bus,
            fake,
            ledger,
            generation,
            path,
        }
    }
    fn owner(&self) -> String {
        self.provider.unique_name().unwrap().to_string()
    }
    async fn sync(&self) {
        let proxy = DownloadProviderProxy::builder(&self.client)
            .destination(self.provider.unique_name().unwrap())
            .unwrap()
            .build()
            .await
            .unwrap();
        let (revision, jobs, backend) = read_jobs(&proxy, &self.generation, None).await.unwrap();
        {
            let jobs = jobs.unwrap();
            let mut state = self.core.state.lock().await;
            let p = state.providers.get_mut("downloads").unwrap();
            let mut downloads = self.core.downloads.lock().await;
            let d = downloads.entry("downloads".into()).or_default();
            d.source
                .get_or_insert_with(|| JobSnapshots::new(&self.generation).unwrap())
                .import(revision, jobs.clone())
                .unwrap();
            d.jobs = jobs;
            p.info.backend_state = backend;
            d.next_import = None;
        }
        self.core
            .refresh_download_imports("downloads", &self.owner(), &self.generation)
            .await
            .unwrap();
        self.core
            .publish_downloads("downloads", &self.owner(), &self.generation)
            .await;
    }
    async fn api(&self) -> DownloadsApiProxy<'static> {
        DownloadsApiProxy::builder(&self.client)
            .path(self.path.clone())
            .unwrap()
            .build()
            .await
            .unwrap()
    }
    async fn row(&self) -> Download {
        self.api()
            .await
            .get_download(&self.generation, "job")
            .await
            .unwrap()
            .0
    }
    async fn wait(&self, path: OwnedObjectPath) -> Result<Vec<EntityRef>, String> {
        let op = ProductionOperationProxy::builder(&self.client)
            .path(path)
            .unwrap()
            .build()
            .await
            .unwrap();
        for _ in 0..100 {
            let info = op.get().await.unwrap().0;
            match info.state.as_str() {
                "succeeded" => return Ok(info.results),
                "failed" | "cancelled" => return Err(info.error),
                _ => tokio::time::sleep(Duration::from_millis(20)).await,
            }
        }
        panic!("operation did not finish");
    }
    async fn forget_runtime(&mut self) {
        // A new Core and generation restore only the persisted checkpoint.
        let core = Core::new(
            &self.core.connection,
            self.core.paths.clone(),
            self.folder.clone(),
        );
        self.generation = Uuid::new_v4().to_string();
        self.fake.generation = self.generation.clone();
        self.provider
            .object_server()
            .remove::<FakeDownload, _>(production::PROVIDER_PATH)
            .await
            .unwrap();
        self.provider
            .object_server()
            .at(production::PROVIDER_PATH, self.fake.clone())
            .await
            .unwrap();
        let mut providers = std::mem::take(&mut self.core.state.lock().await.providers);
        for p in providers.values_mut() {
            p.info.generation = self.generation.clone();
            p.info.revision = 0;
        }
        core.state.lock().await.providers = providers;
        self.core
            .connection
            .object_server()
            .remove::<DownloadsEndpoint, _>(self.path.clone())
            .await
            .unwrap();
        self.core
            .connection
            .object_server()
            .at(
                self.path.clone(),
                DownloadsEndpoint {
                    core: core.clone(),
                    id: "downloads".into(),
                },
            )
            .await
            .unwrap();
        self.core = core;
    }
}
fn rid() -> String {
    Uuid::new_v4().to_string()
}
fn error<T: std::fmt::Debug>(result: zbus::Result<T>) -> String {
    match result {
        Err(zbus::Error::MethodError(name, _, _)) => name.to_string(),
        other => format!("{other:?}"),
    }
}

#[tokio::test]
async fn ac17_completed_import_is_deduplicated_and_survives_daemon_restart() {
    let mut f = Fixture::new(vec![job("job", true, 40)]).await;
    f.sync().await;
    let row = f.row().await;
    assert_eq!(row.import_state, "imported");
    assert_eq!(row.library_refs.len(), 40);
    assert_eq!(f.ledger.lock().unwrap().batch_sizes, [32, 8]);
    let (page,) = f.api().await.get_downloads("").await.unwrap();
    page.validate().unwrap();
    let json = serde_json::to_string(&page).unwrap();
    assert!(!json.contains("file:"));
    assert!(!json.contains("synthetic/output"));
    f.sync().await;
    assert_eq!(f.ledger.lock().unwrap().calls, 2);
    f.forget_runtime().await;
    f.sync().await;
    assert_eq!(f.ledger.lock().unwrap().calls, 2);
    assert_eq!(f.row().await.library_refs, row.library_refs);
}
#[tokio::test]
async fn ac17_ambiguous_import_replays_stable_completion_keys_after_restart() {
    let mut f = Fixture::new(vec![job("job", true, 2)]).await;
    f.ledger.lock().unwrap().fail_after_commit = true;
    f.sync().await;
    assert_eq!(f.row().await.import_state, "pending");
    assert_eq!(f.ledger.lock().unwrap().entries.len(), 2);
    f.forget_runtime().await;
    f.sync().await;
    assert_eq!(f.row().await.import_state, "imported");
    assert_eq!(f.ledger.lock().unwrap().entries.len(), 2);
    assert_eq!(f.ledger.lock().unwrap().calls, 2);
}
#[tokio::test]
async fn unavailable_library_stays_pending_then_updates_shell_revision() {
    let f = Fixture::new(vec![job("job", true, 1)]).await;
    f.core
        .state
        .lock()
        .await
        .providers
        .get_mut("library")
        .unwrap()
        .info
        .backend_state = "unavailable".into();
    f.sync().await;
    assert_eq!(f.row().await.import_state, "pending");
    assert_eq!(f.ledger.lock().unwrap().calls, 0);
    let before = f.api().await.get_downloads("").await.unwrap().0.revision;
    f.core
        .state
        .lock()
        .await
        .providers
        .get_mut("library")
        .unwrap()
        .info
        .backend_state = "available".into();
    f.sync().await;
    assert_eq!(f.row().await.import_state, "imported");
    assert!(f.api().await.get_downloads("").await.unwrap().0.revision > before);
}
#[tokio::test]
async fn unknown_mapping_unsupported_and_incomplete_are_distinct() {
    let f = Fixture::new(vec![job("job", true, 0)]).await;
    f.sync().await;
    assert_eq!(f.row().await.import_state, "unavailable");
    *f.fake.jobs.lock().unwrap() = vec![job("job", false, 1)];
    *f.fake.revision.lock().unwrap() += 1;
    f.sync().await;
    assert_eq!(f.row().await.import_state, "none");
    *f.fake.jobs.lock().unwrap() = vec![job("job", true, 1)];
    *f.fake.revision.lock().unwrap() += 1;
    f.ledger.lock().unwrap().empty = true;
    f.sync().await;
    assert_eq!(f.row().await.import_state, "unsupported");
    f.sync().await;
    assert_eq!(f.ledger.lock().unwrap().calls, 1);
}
#[tokio::test]
async fn unavailable_snapshot_cannot_prune_checkpoints_and_commands_are_refused() {
    let f = Fixture::new(vec![job("job", true, 1)]).await;
    f.sync().await;
    *f.fake.backend.lock().unwrap() = "unavailable".into();
    f.sync().await;
    assert_eq!(f.row().await.import_state, "imported");
    assert_eq!(
        error(f.api().await.pause(&f.generation, "job", &rid()).await),
        "org.arinn.Error1.Unavailable"
    );
    f.fake.jobs.lock().unwrap().clear();
    *f.fake.revision.lock().unwrap() += 1;
    f.sync().await;
    assert_eq!(
        Store::open(&f.core.paths.state)
            .unwrap()
            .download_imports("downloads")
            .unwrap()
            .len(),
        1
    );
    *f.fake.backend.lock().unwrap() = "available".into();
    f.sync().await;
    assert!(
        Store::open(&f.core.paths.state)
            .unwrap()
            .download_imports("downloads")
            .unwrap()
            .is_empty()
    );
}
#[tokio::test]
async fn commands_check_generation_actions_and_request_identity_without_retry() {
    let f = Fixture::new(vec![job("job", false, 0)]).await;
    f.sync().await;
    let api = f.api().await;
    assert_eq!(
        error(api.pause("old", "job", &rid()).await),
        "org.arinn.Error1.Conflict"
    );
    assert_eq!(
        error(api.pause(&f.generation, "missing", &rid()).await),
        "org.arinn.Error1.NotFound"
    );
    let request = rid();
    let path = api.pause(&f.generation, "job", &request).await.unwrap();
    f.wait(path.clone()).await.unwrap();
    assert_eq!(
        api.pause(&f.generation, "job", &request).await.unwrap(),
        path
    );
    assert_eq!(
        error(api.resume(&f.generation, "job", &request).await),
        "org.arinn.Error1.Conflict"
    );
    f.wait(api.resume(&f.generation, "job", &rid()).await.unwrap())
        .await
        .unwrap();
    f.wait(api.start_now(&f.generation, "job", &rid()).await.unwrap())
        .await
        .unwrap();
    assert_eq!(
        f.wait(api.retry(&f.generation, "job", &rid()).await.unwrap())
            .await
            .unwrap_err(),
        "Timeout"
    );
    assert_eq!(
        *f.fake.calls.lock().unwrap(),
        ["pause", "resume", "start-now", "retry"]
    );
    f.core
        .downloads
        .lock()
        .await
        .get_mut("downloads")
        .unwrap()
        .jobs[0]
        .actions
        .clear();
    assert_eq!(
        error(api.pause(&f.generation, "job", &rid()).await),
        "org.arinn.Error1.Unsupported"
    );
}
#[tokio::test]
async fn real_watch_path_exports_snapshot_and_matches_downloads_xml() {
    let f = Fixture::new(vec![job("job", false, 0)]).await;
    let core = f.core.clone();
    let owner = f.owner();
    let generation = f.generation.clone();
    let watcher = tokio::spawn(async move {
        core.watch_provider("downloads".into(), owner, generation)
            .await
    });
    let api = f.api().await;
    for _ in 0..100 {
        if api.get_downloads("").await.is_ok() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(
        api.get_downloads("").await.unwrap().0.downloads[0].import_state,
        "none"
    );
    let xml = zbus::fdo::IntrospectableProxy::builder(&f.client)
        .destination(production::SERVICE)
        .unwrap()
        .path(f.path.clone())
        .unwrap()
        .build()
        .await
        .unwrap()
        .introspect()
        .await
        .unwrap();
    let expected = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/plugin-api/downloads1.xml"
    ));
    assert_eq!(
        super::super::media_routing::tests::methods(&xml, "org.arinn.Downloads1"),
        super::super::media_routing::tests::methods(expected, "org.arinn.Downloads1")
    );
    watcher.abort();
    f.core.remove_endpoints(&f.path).await;
    assert!(api.get_downloads("").await.is_err());
}

/// An unchanged job snapshot is re-read on every tick and provider signal but
/// announces nothing; a backend change still emits `ProvidersChanged` (#442).
#[tokio::test]
async fn unchanged_snapshot_emits_no_providers_changed() {
    let f = Fixture::new(vec![job("job", false, 0)]).await;
    let daemon = arinn_protocol::production::ProductionDaemonProxy::new(&f.client)
        .await
        .unwrap();
    let mut changes = daemon.receive_providers_changed().await.unwrap();
    let core = f.core.clone();
    let owner = f.owner();
    let generation = f.generation.clone();
    let watcher = tokio::spawn(async move {
        core.watch_provider("downloads".into(), owner, generation)
            .await
    });
    // The first read publishes revision 1.
    tokio::time::timeout(Duration::from_secs(5), changes.next())
        .await
        .unwrap()
        .unwrap();
    // Provider signals and at least one tick re-read the same snapshot.
    let emitter = SignalEmitter::new(&f.provider, production::PROVIDER_PATH).unwrap();
    for _ in 0..3 {
        FakeDownload::state_changed(&emitter, &f.generation, 1)
            .await
            .unwrap();
    }
    assert!(
        tokio::time::timeout(Duration::from_millis(1500), changes.next())
            .await
            .is_err(),
        "an unchanged snapshot emitted ProvidersChanged"
    );
    *f.fake.backend.lock().unwrap() = "degraded".into();
    FakeDownload::state_changed(&emitter, &f.generation, 1)
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(5), changes.next())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        f.core.state.lock().await.providers["downloads"]
            .info
            .backend_state,
        "degraded"
    );
    watcher.abort();
}
#[tokio::test]
async fn pending_retry_is_throttled_and_keeps_its_library_after_default_changes() {
    let f = Fixture::new(vec![job("job", true, 1)]).await;
    f.ledger.lock().unwrap().fail_after_commit = true;
    f.sync().await;
    f.core
        .refresh_download_imports("downloads", &f.owner(), &f.generation)
        .await
        .unwrap();
    assert_eq!(f.ledger.lock().unwrap().calls, 1);
    let mut config = Config::default();
    config.defaults.insert("library".into(), "other".into());
    config.save(&f.core.paths.config).unwrap();
    let mut other = provider(
        "other",
        "library",
        &f.owner(),
        &f.generation,
        "/org/arinn/Daemon1/providers/p_other".try_into().unwrap(),
    );
    other.features.push(library::FEATURE_IMPORT.into());
    f.core
        .state
        .lock()
        .await
        .providers
        .insert("other".into(), other);
    f.core
        .state
        .lock()
        .await
        .providers
        .get_mut("library")
        .unwrap()
        .info
        .backend_state = "unavailable".into();
    f.sync().await;
    assert_eq!(f.ledger.lock().unwrap().calls, 1);
    assert_eq!(f.row().await.import_state, "pending");
    f.core
        .state
        .lock()
        .await
        .providers
        .get_mut("library")
        .unwrap()
        .info
        .backend_state = "available".into();
    f.sync().await;
    assert_eq!(f.row().await.library_refs[0].provider_id, "library");
    assert_eq!(f.ledger.lock().unwrap().entries.len(), 1);
}
#[tokio::test]
async fn unsupported_reply_is_terminal_but_changed_mapping_is_reconsidered() {
    let f = Fixture::new(vec![job("job", true, 1)]).await;
    f.ledger.lock().unwrap().unsupported = true;
    f.sync().await;
    assert_eq!(f.row().await.import_state, "unsupported");
    f.sync().await;
    assert_eq!(f.ledger.lock().unwrap().calls, 1);
    f.ledger.lock().unwrap().unsupported = false;
    f.fake.jobs.lock().unwrap()[0].outputs[0].file_uri = "file:///synthetic/changed.mkv".into();
    *f.fake.revision.lock().unwrap() += 1;
    f.sync().await;
    assert_eq!(f.row().await.import_state, "imported");
}
#[tokio::test]
async fn malformed_or_stale_snapshot_is_rejected_before_publication() {
    let f = Fixture::new(vec![job("job", false, 0), job("job", false, 0)]).await;
    let proxy = DownloadProviderProxy::builder(&f.client)
        .destination(f.provider.unique_name().unwrap())
        .unwrap()
        .build()
        .await
        .unwrap();
    assert_eq!(
        read_jobs(&proxy, &f.generation, None).await.unwrap_err(),
        ErrorCode::InvalidArgument
    );
    f.fake.jobs.lock().unwrap().pop();
    assert_eq!(
        read_jobs(&proxy, &rid(), None).await.unwrap_err(),
        ErrorCode::Conflict
    );
    *f.fake.backend.lock().unwrap() = "unknown".into();
    assert_eq!(
        read_jobs(&proxy, &f.generation, None).await.unwrap_err(),
        ErrorCode::Unsupported
    );
}

#[test]
fn long_import_paths_obey_the_dbus_body_limit_as_well_as_the_count_limit() {
    use zbus::zvariant::{LE, serialized::Context, to_bytes};
    let generation = rid();
    let candidates: Vec<_> = (0..32)
        .map(|i| ImportCandidate {
            source: EntityRef {
                provider_id: "p".repeat(256),
                capability: "download".into(),
                local_id: "j".repeat(256),
            },
            file_uri: format!("file:///synthetic/{}.mkv", "x".repeat(8100)),
            completion_id: format!("out-{i}"),
        })
        .collect();
    assert!(
        to_bytes(Context::new_dbus(LE, 0), &(&generation, &candidates))
            .unwrap()
            .len()
            > validation::MAX_BODY_BYTES
    );
    let batches = import_batches(&generation, &candidates).unwrap();
    assert!(batches.len() > 1);
    assert_eq!(
        batches.iter().map(Vec::len).sum::<usize>(),
        candidates.len()
    );
    for batch in batches {
        library::validate_imports(&batch).unwrap();
        assert!(
            to_bytes(Context::new_dbus(LE, 0), &(&generation, &batch))
                .unwrap()
                .len()
                <= validation::MAX_BODY_BYTES
        );
    }
}
