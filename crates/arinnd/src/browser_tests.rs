// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Daemon routing for `browser` providers (ADR-0048, #83): snapshot import,
//! `Browser1`, tab commands with compositor activation and web shortcut open.
use super::*;
use arinn_protocol::{
    browser::BrowserApiProxy,
    kwin::KWinBridgeProxy,
    production::{Error, ProductionOperationProxy},
};
use std::{
    io::{BufRead, BufReader},
    process::{Child, Command, Stdio},
    sync::Mutex as StdMutex,
};

#[derive(Clone)]
struct FakeBrowser {
    generation: String,
    backend: Arc<StdMutex<String>>,
    calls: Arc<StdMutex<Vec<String>>>,
}
#[zbus::interface(name = "org.arinn.BrowserProvider1")]
impl FakeBrowser {
    fn get_tabs(&self, _token: &str) -> production::Result<(TabPage,)> {
        Ok((TabPage {
            generation: self.generation.clone(),
            revision: 1,
            windows: vec![
                BrowserWindow {
                    id: "w1".into(),
                    window_token: "token-0".into(),
                    focused: true,
                },
                BrowserWindow {
                    id: "w2".into(),
                    window_token: String::new(),
                    focused: false,
                },
            ],
            tabs: vec![
                Tab {
                    id: "t1".into(),
                    window_id: "w1".into(),
                    title: "Video".into(),
                    url: "https://video.example/".into(),
                    active: true,
                    audible: true,
                },
                Tab {
                    id: "t2".into(),
                    window_id: "w2".into(),
                    title: "Mail".into(),
                    url: "https://mail.example/".into(),
                    active: true,
                    audible: false,
                },
            ],
            next_token: String::new(),
        },))
    }
    fn backend_state(&self) -> String {
        self.backend.lock().unwrap().clone()
    }
    fn open_url(&self, _generation: &str, url: &str) -> production::Result<()> {
        self.calls.lock().unwrap().push(format!("open {url}"));
        Ok(())
    }
    fn activate_tab(&self, _generation: &str, id: &str) -> production::Result<()> {
        self.calls.lock().unwrap().push(format!("activate {id}"));
        Ok(())
    }
    fn close_tab(&self, _generation: &str, id: &str) -> production::Result<()> {
        self.calls.lock().unwrap().push(format!("close {id}"));
        Ok(())
    }
    #[zbus(signal)]
    async fn state_changed(
        emitter: &SignalEmitter<'_>,
        generation: &str,
        revision: u64,
    ) -> zbus::Result<()>;
}

struct Fixture {
    bus: Child,
    folder: PathBuf,
    core: Core,
    kwin: Connection,
    client: Connection,
    browser: FakeBrowser,
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
        let kwin = connect(&address).await;
        let provider = connect(&address).await;
        let client = connect(&address).await;
        server.request_name(production::SERVICE).await.unwrap();
        kwin.request_name("org.kde.KWin").await.unwrap();
        let folder = std::env::temp_dir().join(format!("arinn-browser-{}", Uuid::new_v4()));
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
        core.export_platform().await.unwrap();
        server
            .object_server()
            .at(production::PATH, Daemon { core: core.clone() })
            .await
            .unwrap();
        let generation = Uuid::new_v4().to_string();
        let browser = FakeBrowser {
            generation: generation.clone(),
            backend: Arc::new(StdMutex::new("available".into())),
            calls: Arc::default(),
        };
        provider
            .object_server()
            .at(production::PROVIDER_PATH, browser.clone())
            .await
            .unwrap();
        let path: OwnedObjectPath = "/org/arinn/Daemon1/providers/p_browser".try_into().unwrap();
        server
            .object_server()
            .at(
                path.clone(),
                BrowserEndpoint {
                    core: core.clone(),
                    id: "chrome".into(),
                },
            )
            .await
            .unwrap();
        // The registry state Register creates; registration itself has its
        // own end-to-end coverage.
        core.state.lock().await.providers.insert(
            "chrome".into(),
            Provider {
                info: ProviderInfo {
                    id: "chrome".into(),
                    plugin_id: "chrome".into(),
                    source_path: String::new(),
                    name: "Chrome".into(),
                    version: "0.0.1".into(),
                    path,
                    capabilities: vec!["browser".into()],
                    runtime_state: "starting".into(),
                    backend_state: "unavailable".into(),
                    generation: generation.clone(),
                    revision: 0,
                    last_error: String::new(),
                    error_at: 0,
                    recovered_at: 0,
                    restarts_since_daemon_start: 0,
                    cumulative_restart_count: 0,
                },
                manifest: None,
                unit: "chrome.service".into(),
                token_file: folder.join("chrome.token"),
                token: String::new(),
                owner: provider.unique_name().unwrap().to_string(),
                cache: None,
                applications: Vec::new(),
                running: Vec::new(),
                tab_cache: None,
                windows: Vec::new(),
                tabs: Vec::new(),
                started: Instant::now(),
                enabled: true,
            },
        );
        let watcher = core.clone();
        let (owner, registration) = (
            provider.unique_name().unwrap().to_string(),
            generation.clone(),
        );
        tokio::spawn(async move {
            watcher
                .watch_provider("chrome".into(), owner, registration)
                .await
        });
        let f = Self {
            bus,
            folder,
            core,
            kwin,
            client,
            browser,
            generation,
        };
        // The watcher imports the provider snapshot within its first tick.
        for _ in 0..100 {
            if f.core.state.lock().await.providers["chrome"].tabs.len() == 2 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        f
    }
    async fn platform(&self) -> String {
        let bridge = KWinBridgeProxy::new(&self.kwin).await.unwrap();
        let generation = bridge.connect(&Uuid::new_v4().to_string()).await.unwrap();
        bridge.begin_inventory(&generation, 1, 1).await.unwrap();
        bridge
            .put_window(&generation, 1, 0, "token-0", "browser.desktop", "", 12)
            .await
            .unwrap();
        bridge.commit_inventory(&generation, 1).await.unwrap();
        generation
    }
    async fn wait(&self, path: OwnedObjectPath) -> Result<(), String> {
        let operation = ProductionOperationProxy::builder(&self.client)
            .path(path)
            .unwrap()
            .build()
            .await
            .unwrap();
        for _ in 0..100 {
            let info = operation.get().await.unwrap().0;
            match info.state.as_str() {
                "succeeded" => return Ok(()),
                "failed" | "cancelled" => return Err(info.error),
                _ => tokio::time::sleep(Duration::from_millis(20)).await,
            }
        }
        panic!("operation did not finish");
    }
    fn take_command(&self) -> tokio::task::JoinHandle<String> {
        let kwin = self.kwin.clone();
        tokio::spawn(async move {
            kwin.call_method(
                Some(production::SERVICE),
                production::PATH,
                Some("org.arinn.Daemon1"),
                "TakeCommand",
                &(),
            )
            .await
            .unwrap()
            .body()
            .deserialize::<String>()
            .unwrap()
        })
    }
}

#[tokio::test]
async fn browser_snapshot_is_served_and_tab_commands_raise_the_window() {
    let f = Fixture::new().await;
    let api = BrowserApiProxy::builder(&f.client)
        .path("/org/arinn/Daemon1/providers/p_browser")
        .unwrap()
        .build()
        .await
        .unwrap();
    let (page,) = api.get_tabs("").await.unwrap();
    assert_eq!(page.generation, f.generation);
    assert_eq!(page.tabs.len(), 2);
    {
        let state = f.core.state.lock().await;
        let p = &state.providers["chrome"];
        assert_eq!(
            (p.info.runtime_state.as_str(), p.info.backend_state.as_str()),
            ("ready", "available")
        );
    }

    // A tab in a linked window: the browser selects it, then the platform
    // activates exactly that compositor window.
    let platform = f.platform().await;
    let bridge = KWinBridgeProxy::new(&f.kwin).await.unwrap();
    let path = api
        .activate_tab(&f.generation, "t1", &Uuid::new_v4().to_string())
        .await
        .unwrap();
    let control = tokio::time::timeout(Duration::from_secs(5), bridge.next_control(&platform))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(&control[1..], ["activate", "token-0"]);
    bridge
        .complete_control(&platform, &control[0], "")
        .await
        .unwrap();
    assert_eq!(f.wait(path).await, Ok(()));

    // A tab in an unlinked window: Home is hidden and the browser's own focus
    // request decides.
    let poll = f.take_command();
    tokio::time::sleep(Duration::from_millis(200)).await;
    let path = api
        .activate_tab(&f.generation, "t2", &Uuid::new_v4().to_string())
        .await
        .unwrap();
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(5), poll)
            .await
            .unwrap()
            .unwrap(),
        "dismiss"
    );
    assert_eq!(f.wait(path).await, Ok(()));

    let path = api
        .close_tab(&f.generation, "t2", &Uuid::new_v4().to_string())
        .await
        .unwrap();
    assert_eq!(f.wait(path).await, Ok(()));
    assert_eq!(
        *f.browser.calls.lock().unwrap(),
        vec!["activate t1", "activate t2", "close t2"]
    );

    // Unknown tabs, stale generations and application actions are refused.
    assert!(matches!(
        api.activate_tab(&f.generation, "t9", &Uuid::new_v4().to_string()).await,
        Err(zbus::Error::MethodError(name, _, _)) if name.as_str() == "org.arinn.Error1.NotFound"
    ));
    assert!(matches!(
        f.core
            .command(
                "chrome",
                "activate_tab",
                &Uuid::new_v4().to_string(),
                "t1",
                &Uuid::new_v4().to_string(),
                "caller"
            )
            .await,
        Err(ErrorCode::Conflict)
    ));
    assert!(matches!(
        f.core
            .command(
                "chrome",
                "launch",
                &f.generation,
                "t1",
                &Uuid::new_v4().to_string(),
                "caller"
            )
            .await,
        Err(ErrorCode::NotFound)
    ));

    // The daemon export matches the published XML contract.
    let live = zbus::fdo::IntrospectableProxy::builder(&f.client)
        .destination(production::SERVICE)
        .unwrap()
        .path("/org/arinn/Daemon1/providers/p_browser")
        .unwrap()
        .build()
        .await
        .unwrap()
        .introspect()
        .await
        .unwrap();
    let file = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/plugin-api/browser1.xml"
    ))
    .unwrap();
    for method in ["GetTabs", "ActivateTab", "CloseTab"] {
        assert!(
            live.contains(&format!("<method name=\"{method}\">")),
            "{method}"
        );
        assert!(
            file.contains(&format!("<method name=\"{method}\">")),
            "{method}"
        );
    }
    assert!(live.contains("(sta(ssb)a(ssssbb)s)") && file.contains("(sta(ssb)a(ssssbb)s)"));
}

#[tokio::test]
async fn urls_open_in_the_connected_browser_and_raise_its_focused_window() {
    let f = Fixture::new().await;
    let platform = f.platform().await;
    let bridge = KWinBridgeProxy::new(&f.kwin).await.unwrap();
    let core = f.core.clone();
    let open = tokio::spawn(async move { core.open_url("https://example.org/watch").await });
    let control = tokio::time::timeout(Duration::from_secs(5), bridge.next_control(&platform))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(&control[1..], ["activate", "token-0"]);
    bridge
        .complete_control(&platform, &control[0], "")
        .await
        .unwrap();
    assert_eq!(open.await.unwrap(), Ok(()));
    assert_eq!(
        *f.browser.calls.lock().unwrap(),
        vec!["open https://example.org/watch"]
    );

    assert_eq!(
        f.core.open_url("file:///etc/passwd").await,
        Err(ErrorCode::InvalidArgument)
    );
    // With no browser connected there is nothing to open the URL in.
    *f.browser.backend.lock().unwrap() = "unavailable".into();
    f.core
        .state
        .lock()
        .await
        .providers
        .get_mut("chrome")
        .unwrap()
        .info
        .backend_state = "unavailable".into();
    assert_eq!(
        f.core.open_url("https://example.org/").await,
        Err(ErrorCode::Unavailable)
    );
    let _: Option<Error> = None;
}
