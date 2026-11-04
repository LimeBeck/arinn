// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use arinn_protocol::{
    browser::authorization::BrowserAuthProxy, production::ProductionOperationProxy,
};
use std::{
    io::{BufRead, BufReader},
    process::{Child, Command, Stdio},
    sync::Mutex as StdMutex,
};
#[derive(Default)]
struct Entry {
    pending: bool,
    created: bool,
    cancelled: bool,
}
#[derive(Clone, Default)]
struct Browser {
    entries: Arc<StdMutex<HashMap<String, Entry>>>,
    hold: Arc<AtomicBool>,
    release: Arc<tokio::sync::Notify>,
    changed: Arc<tokio::sync::Notify>,
    opened: Arc<StdMutex<Vec<String>>>,
    fail_close: Arc<AtomicBool>,
}
#[zbus::interface(name = "org.arinn.BrowserProvider1")]
impl Browser {
    async fn open_temporary_url(
        &self,
        _generation: &str,
        request_id: &str,
        _url: &str,
    ) -> production::Result<String> {
        self.opened.lock().unwrap().push(request_id.into());
        self.entries
            .lock()
            .unwrap()
            .entry(request_id.into())
            .or_default()
            .pending = true;
        self.changed.notify_waiters();
        if self.hold.load(Ordering::Acquire) {
            self.release.notified().await;
        }
        let mut entries = self.entries.lock().unwrap();
        let entry = entries.get_mut(request_id).unwrap();
        entry.pending = false;
        entry.created = true;
        drop(entries);
        self.changed.notify_waiters();
        Ok(request_id.into())
    }
    async fn close_temporary_window(
        &self,
        _generation: &str,
        request_id: &str,
    ) -> production::Result<()> {
        self.entries
            .lock()
            .unwrap()
            .entry(request_id.into())
            .or_default()
            .cancelled = true;
        if self.fail_close.swap(false, Ordering::AcqRel) {
            return Err(ErrorCode::Unavailable.into());
        }
        loop {
            let changed = self.changed.notified();
            {
                let mut entries = self.entries.lock().unwrap();
                let entry = entries.get_mut(request_id).unwrap();
                if !entry.pending {
                    entry.created = false;
                    return Ok(());
                }
            }
            changed.await;
        }
    }
}
struct Fixture {
    bus: Child,
    folder: PathBuf,
    address: String,
    core: Core,
    client: Connection,
    _provider: Connection,
    browser: Browser,
    sessions: Sessions,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = self.bus.kill();
        let _ = self.bus.wait();
        let _ = std::fs::remove_dir_all(&self.folder);
    }
}
async fn connection(address: &str) -> Connection {
    zbus::connection::Builder::address(address.trim())
        .unwrap()
        .method_timeout(Duration::from_secs(20))
        .build()
        .await
        .unwrap()
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
        let server = connection(&address).await;
        server.request_name(production::SERVICE).await.unwrap();
        let client = connection(&address).await;
        let provider = connection(&address).await;
        let folder = std::env::temp_dir().join(format!("arinn-auth-browser-{}", Uuid::new_v4()));
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
        let browser = Browser::default();
        provider
            .object_server()
            .at(production::PROVIDER_PATH, browser.clone())
            .await
            .unwrap();
        core.state.lock().await.providers.insert(
            "fixture".into(),
            Provider {
                info: ProviderInfo {
                    id: "fixture".into(),
                    plugin_id: "fixture".into(),
                    source_path: String::new(),
                    name: "Fixture".into(),
                    version: "0.0.1".into(),
                    path: "/org/arinn/Daemon1/providers/fixture".try_into().unwrap(),
                    capabilities: vec!["browser".into()],
                    runtime_state: "ready".into(),
                    backend_state: "available".into(),
                    generation: "g1".into(),
                    revision: 1,
                    last_error: String::new(),
                    error_at: 0,
                    recovered_at: 0,
                    restarts_since_daemon_start: 0,
                    cumulative_restart_count: 0,
                },
                manifest: None,
                unit: "fixture.service".into(),
                token_file: folder.join("token"),
                token: String::new(),
                owner: provider.unique_name().unwrap().to_string(),
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
            },
        );
        let sessions = Sessions::default();
        server
            .object_server()
            .at(
                production::PATH,
                BrowserAuthEndpoint {
                    core: core.clone(),
                    sessions: sessions.clone(),
                },
            )
            .await
            .unwrap();
        crate::test_support::served(&client, &server).await;
        crate::test_support::served(&client, &provider).await;
        Self {
            bus,
            folder,
            address,
            core,
            client,
            _provider: provider,
            browser,
            sessions,
        }
    }
    async fn api(&self) -> BrowserAuthProxy<'static> {
        BrowserAuthProxy::builder(&self.client)
            .build()
            .await
            .unwrap()
    }
    async fn wait(&self, path: OwnedObjectPath) -> Result<(), String> {
        let operation = ProductionOperationProxy::builder(&self.client)
            .destination(production::SERVICE)
            .unwrap()
            .path(path)
            .unwrap()
            .build()
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(15), async {
            loop {
                let (info,) = operation.get().await.unwrap();
                match info.state.as_str() {
                    "succeeded" => return Ok(()),
                    "failed" | "cancelled" => return Err(info.error),
                    _ => tokio::time::sleep(Duration::from_millis(5)).await,
                }
            }
        })
        .await
        .unwrap()
    }
}
#[tokio::test]
async fn caller_ids_are_scoped_and_internal_leases_are_unique_without_history() {
    let f = Fixture::new().await;
    let api = f.api().await;
    let request = Uuid::new_v4().to_string();
    let path = api
        .open("https://fixture.invalid/device", &request)
        .await
        .unwrap();
    assert!(f.wait(path.clone()).await.is_ok());
    assert_eq!(
        api.open("https://fixture.invalid/device", &request)
            .await
            .unwrap(),
        path
    );
    assert!(
        api.open("https://different.invalid/device", &request)
            .await
            .is_err()
    );
    let other = connection(&f.address).await;
    let other_api = BrowserAuthProxy::new(&other).await.unwrap();
    assert!(
        f.wait(
            other_api
                .open("https://fixture.invalid/device", &request)
                .await
                .unwrap()
        )
        .await
        .is_ok()
    );
    let leases = f.browser.opened.lock().unwrap().clone();
    assert_eq!(leases.len(), 2);
    assert_ne!(leases[0], leases[1]);
    assert!(!leases.contains(&request));
    other_api.close(&request).await.unwrap();
    assert!(f.browser.entries.lock().unwrap()[&leases[0]].created);
    api.close(&request).await.unwrap();
    assert!(!f.folder.join("state").exists());
}
#[tokio::test]
async fn close_before_dispatch_revokes_the_queued_open() {
    let f = Fixture::new().await;
    let api = f.api().await;
    let request = Uuid::new_v4().to_string();
    let guard = f.core.state.lock().await;
    let path = api
        .open("https://fixture.invalid/device", &request)
        .await
        .unwrap();
    let client = f.client.clone();
    let close_request = request.clone();
    let closed = tokio::spawn(async move {
        BrowserAuthProxy::new(&client)
            .await
            .unwrap()
            .close(&close_request)
            .await
    });
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if f.sessions
                .lock()
                .await
                .values()
                .any(|s| s.launch.cancelled())
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    drop(guard);
    closed.await.unwrap().unwrap();
    assert!(f.wait(path).await.is_err());
    assert!(f.browser.opened.lock().unwrap().is_empty());
}
#[tokio::test]
async fn timed_out_create_is_retained_and_close_waits_for_the_late_window() {
    let f = Fixture::new().await;
    f.browser.hold.store(true, Ordering::Release);
    let api = f.api().await;
    let request = Uuid::new_v4().to_string();
    let path = api
        .open("https://fixture.invalid/device", &request)
        .await
        .unwrap();
    assert!(f.wait(path).await.is_err()); // ten-second transport deadline, mutation still running
    assert_eq!(f.sessions.lock().await.len(), 1);
    let client = f.client.clone();
    let closing = request.clone();
    let closed = tokio::spawn(async move {
        BrowserAuthProxy::new(&client)
            .await
            .unwrap()
            .close(&closing)
            .await
    });
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if f.browser
                .entries
                .lock()
                .unwrap()
                .values()
                .any(|e| e.cancelled)
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert!(!closed.is_finished());
    f.browser.release.notify_waiters();
    closed.await.unwrap().unwrap();
    assert!(
        f.browser
            .entries
            .lock()
            .unwrap()
            .values()
            .all(|e| !e.created)
    );
    assert!(f.sessions.lock().await.is_empty());
}
#[tokio::test]
async fn failed_cleanup_is_retained_until_the_same_identity_is_retried() {
    let f = Fixture::new().await;
    let api = f.api().await;
    let request = Uuid::new_v4().to_string();
    assert!(
        f.wait(
            api.open("https://fixture.invalid/device", &request)
                .await
                .unwrap()
        )
        .await
        .is_ok()
    );
    f.browser.fail_close.store(true, Ordering::Release);
    assert!(api.close(&request).await.is_err());
    assert_eq!(f.sessions.lock().await.len(), 1);
    assert!(
        f.wait(
            api.open(
                "https://fixture.invalid/device",
                &Uuid::new_v4().to_string()
            )
            .await
            .unwrap()
        )
        .await
        .is_err()
    );
    api.close(&request).await.unwrap();
    assert!(f.sessions.lock().await.is_empty());
    assert!(
        f.wait(
            api.open(
                "https://fixture.invalid/device",
                &Uuid::new_v4().to_string()
            )
            .await
            .unwrap()
        )
        .await
        .is_ok()
    );
}
