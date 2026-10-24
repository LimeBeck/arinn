// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The daemon's launch guard with a provider double on a private bus
//! (spec §6.5, ADR-0110): a Cancel never reaches a launch that is not its
//! own, also after `Cancel` timed out and for the daemon's own browser launch.
use super::*;
use arinn_protocol::production::ProductionOperationProxy;
use std::{
    io::{BufRead, BufReader},
    process::{Child, Command, Stdio},
    sync::Mutex as StdMutex,
};

/// Launches stay pending; `CancelLaunch` answers only when released.
#[derive(Clone, Default)]
struct SlowApps {
    log: Arc<StdMutex<Vec<String>>>,
    release_cancel: Arc<tokio::sync::Notify>,
}
#[zbus::interface(name = "org.arinn.Provider1")]
impl SlowApps {
    async fn launch(&self, _generation: &str, id: &str) -> production::Result<()> {
        self.log.lock().unwrap().push(format!("launch {id}"));
        std::future::pending::<()>().await;
        Ok(())
    }
    async fn cancel_launch(&self, _generation: &str, id: &str) -> production::Result<()> {
        self.log.lock().unwrap().push(format!("cancel {id}"));
        self.release_cancel.notified().await;
        self.log.lock().unwrap().push(format!("cancelled {id}"));
        Ok(())
    }
}

struct Fixture {
    bus: Child,
    folder: PathBuf,
    core: Core,
    client: Connection,
    _provider: Connection,
    apps: SlowApps,
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
fn provider(id: &str, capability: &str, owner: &str, generation: &str) -> Provider {
    Provider {
        info: ProviderInfo {
            id: id.into(),
            plugin_id: id.into(),
            source_path: String::new(),
            name: "Fixture".into(),
            version: "0.0.1".into(),
            path: format!("/org/arinn/Daemon1/providers/p_{id}")
                .try_into()
                .unwrap(),
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
        let provider_bus = connect(&address).await;
        let client = connect(&address).await;
        server.request_name(production::SERVICE).await.unwrap();
        let folder = std::env::temp_dir().join(format!("arinn-launch-{}", Uuid::new_v4()));
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
        let apps = SlowApps::default();
        provider_bus
            .object_server()
            .at(production::PROVIDER_PATH, apps.clone())
            .await
            .unwrap();
        let generation = Uuid::new_v4().to_string();
        let mut desktop = provider(
            "desktop",
            "application",
            provider_bus.unique_name().unwrap().as_str(),
            &generation,
        );
        desktop.applications = ["game.desktop", "browser.desktop"]
            .into_iter()
            .map(|id| Application {
                id: id.into(),
                title: "Fixture".into(),
                can_launch: true,
                group: "other".into(),
                icon: String::new(),
            })
            .collect();
        core.state
            .lock()
            .await
            .providers
            .insert("desktop".into(), desktop);
        Self {
            bus,
            folder,
            core,
            client,
            _provider: provider_bus,
            apps,
            generation,
        }
    }
    /// A tile launch by the client, as the shell starts one.
    async fn launch(&self, id: &str) -> OwnedObjectPath {
        self.core
            .command(
                "desktop",
                "launch",
                &self.generation,
                id,
                &Uuid::new_v4().to_string(),
                self.client.unique_name().unwrap().as_str(),
            )
            .await
            .unwrap()
    }
    async fn cancel(&self, path: &OwnedObjectPath) -> zbus::Result<()> {
        ProductionOperationProxy::builder(&self.client)
            .path(path.clone())
            .unwrap()
            .build()
            .await
            .unwrap()
            .cancel()
            .await
    }
    fn log(&self) -> Vec<String> {
        self.apps.log.lock().unwrap().clone()
    }
    async fn until(&self, what: &str, predicate: impl Fn(&[String]) -> bool) {
        tokio::time::timeout(Duration::from_secs(5), async {
            while !predicate(&self.log()) {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap_or_else(|_| panic!("timed out waiting until {what}: {:?}", self.log()));
    }
}
fn error_name<T: std::fmt::Debug>(result: zbus::Result<T>) -> String {
    match result {
        Err(zbus::Error::MethodError(name, _, _)) => name.to_string(),
        other => format!("{other:?}"),
    }
}
fn launches(log: &[String], id: &str) -> usize {
    log.iter().filter(|e| *e == &format!("launch {id}")).count()
}

#[tokio::test]
async fn a_launch_waits_for_a_slow_cancel_beyond_its_timeout() {
    let f = Fixture::new().await;
    let first = f.launch("game.desktop").await;
    f.until("the first launch is sent", |log| {
        launches(log, "game.desktop") == 1
    })
    .await;
    // The provider takes longer than the daemon's cancel deadline.
    let cancel = {
        let client = f.client.clone();
        let first = first.clone();
        tokio::spawn(async move {
            ProductionOperationProxy::builder(&client)
                .path(first)
                .unwrap()
                .build()
                .await
                .unwrap()
                .cancel()
                .await
        })
    };
    f.until("CancelLaunch reached the provider", |log| {
        log.contains(&"cancel game.desktop".to_string())
    })
    .await;
    // The same tile again while the cancel is on its way.
    let second = f.launch("game.desktop").await;
    // The caller gets Timeout, and the first launch keeps running.
    assert_eq!(
        error_name(cancel.await.unwrap()),
        "org.arinn.Error1.Timeout"
    );
    let state = |path: &OwnedObjectPath| {
        let client = f.client.clone();
        let path = path.clone();
        async move {
            let (info,) = ProductionOperationProxy::builder(&client)
                .path(path)
                .unwrap()
                .build()
                .await
                .unwrap()
                .get()
                .await
                .unwrap();
            info.state
        }
    };
    assert_eq!(state(&first).await, "running");
    // The second launch is still held: the late CancelLaunch would stop it.
    tokio::time::sleep(LAUNCH_CANCEL_DEADLINE).await;
    assert_eq!(launches(&f.log(), "game.desktop"), 1, "{:?}", f.log());
    assert_eq!(state(&second).await, "running");
    // The provider answers at last; only then is the second launch sent.
    f.apps.release_cancel.notify_one();
    f.until("the second launch is sent", |log| {
        launches(log, "game.desktop") == 2
    })
    .await;
    let log = f.log();
    let answered = log
        .iter()
        .position(|e| e == "cancelled game.desktop")
        .unwrap();
    let sent = log
        .iter()
        .rposition(|e| e == "launch game.desktop")
        .unwrap();
    assert!(answered < sent, "{log:?}");
}

#[tokio::test]
async fn the_daemons_browser_launch_counts_for_a_tile_cancel() {
    let f = Fixture::new().await;
    crate::storage::Config {
        browser_application: "browser.desktop".into(),
        ..Default::default()
    }
    .save(&f.core.paths.config)
    .unwrap();
    // A browser provider is registered but not connected yet.
    {
        let mut browser = provider("chrome", "browser", ":1.999", &f.generation);
        browser.info.backend_state = "unavailable".into();
        f.core
            .state
            .lock()
            .await
            .providers
            .insert("chrome".into(), browser);
    }
    // A web shortcut opens with no browser: the daemon launches it, and the
    // open gives up waiting long before the provider answers.
    let open = {
        let core = f.core.clone();
        tokio::spawn(async move {
            let op = crate::operations::Operation::detached(Label::new("browse.open"));
            let deadline = tokio::time::Instant::now() + Duration::from_millis(300);
            core.launch_browser(&op, deadline).await
        })
    };
    f.until("the browser launch is sent", |log| {
        launches(log, "browser.desktop") == 1
    })
    .await;
    // The user also starts the browser from its tile and cancels that.
    let tile = f.launch("browser.desktop").await;
    f.until("the tile launch is sent", |log| {
        launches(log, "browser.desktop") == 2
    })
    .await;
    assert_eq!(
        error_name(f.cancel(&tile).await),
        "org.arinn.Error1.Conflict"
    );
    // The open timed out, but its launch is still unanswered and still counts.
    assert_eq!(open.await.unwrap(), Err(ErrorCode::Timeout));
    assert_eq!(
        error_name(f.cancel(&tile).await),
        "org.arinn.Error1.Conflict"
    );
    assert!(
        !f.log().iter().any(|e| e.starts_with("cancel ")),
        "{:?}",
        f.log()
    );
}
