// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use arinn_protocol::{web::WebShortcut, web_icons::WebShortcutIconsProxy};
use std::{
    io::{BufRead, BufReader},
    process::{Child, Command, Stdio},
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};

struct IconState {
    calls: AtomicUsize,
    hold: AtomicBool,
    invalid: AtomicBool,
    seen: Mutex<Vec<(String, String, String)>>,
    gate: Semaphore,
}
struct IconProvider {
    state: Arc<IconState>,
    caller: String,
    generation: String,
}
#[zbus::interface(name = "org.arinn.WebIconProvider1")]
impl IconProvider {
    async fn get_icon(
        &self,
        generation: &str,
        origin: &str,
        #[zbus(header)] header: Header<'_>,
    ) -> production::Result<(Asset,)> {
        if header.sender().map(|s| s.as_str()) != Some(self.caller.as_str()) {
            return Err(ErrorCode::PermissionDenied.into());
        }
        if generation != self.generation {
            return Err(ErrorCode::Conflict.into());
        }
        self.state
            .seen
            .lock()
            .await
            .push((generation.into(), origin.into(), self.caller.clone()));
        self.state.calls.fetch_add(1, Ordering::SeqCst);
        if self.state.hold.load(Ordering::SeqCst) {
            self.state.gate.acquire().await.unwrap().forget();
        }
        // Synthetic one-pixel PNG, not downloaded artwork.
        let mut bytes = vec![
            137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1,
            8, 6, 0, 0, 0, 31, 21, 196, 137, 0, 0, 0, 13, 73, 68, 65, 84, 120, 156, 99, 248, 207,
            192, 240, 31, 0, 5, 0, 1, 255, 137, 153, 61, 29, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66,
            96, 130,
        ];
        if self.state.invalid.load(Ordering::SeqCst) {
            bytes = vec![0; 128 * 1024 + 1];
        }
        Ok((Asset {
            mime: "image/png".into(),
            data: bytes,
        },))
    }
}
struct Absent;
#[zbus::interface(name = "org.arinn.IconFixture1")]
impl Absent {
    fn ping(&self) {}
}

struct Fixture {
    bus: Child,
    folder: PathBuf,
    core: Core,
    client: Connection,
    owners: Vec<Connection>,
    icons: Vec<Arc<IconState>>,
    revision: u64,
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
            capabilities: vec!["browser".into()],
            runtime_state: "ready".into(),
            backend_state: "unavailable".into(),
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
    async fn new(absent: bool) -> Self {
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
        let folder = std::env::temp_dir().join(format!("arinn-web-icons-{}", Uuid::new_v4()));
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
        let shortcuts = WebShortcuts::new(&core.paths.state, core.operations.clone());
        let defaults = vec![
            WebShortcut {
                id: "saved".into(),
                title: "Example".into(),
                url: "https://EXAMPLE.invalid:443/private?q=secret#fragment".into(),
                icon: String::new(),
            },
            WebShortcut {
                id: "local".into(),
                title: "Local".into(),
                url: "http://localhost/path".into(),
                icon: String::new(),
            },
            WebShortcut {
                id: "explicit".into(),
                title: "Explicit".into(),
                url: "https://example.invalid/".into(),
                icon: "packaged".into(),
            },
        ];
        shortcuts.seed(Some(defaults)).await.unwrap();
        let revision = Store::open(&core.paths.state)
            .unwrap()
            .web_shortcuts()
            .unwrap()
            .revision;
        core.export_web_icons(shortcuts.clone()).await.unwrap();
        server
            .object_server()
            .at(production::PATH, shortcuts)
            .await
            .unwrap();
        let mut owners = Vec::new();
        let mut icons = Vec::new();
        for id in ["a", "b"] {
            let owner = connect(&address).await;
            let generation = Uuid::new_v4().to_string();
            let state = Arc::new(IconState {
                calls: AtomicUsize::new(0),
                hold: AtomicBool::new(false),
                invalid: AtomicBool::new(false),
                seen: Mutex::new(Vec::new()),
                gate: Semaphore::new(0),
            });
            if id == "a" && absent {
                owner
                    .object_server()
                    .at(production::PROVIDER_PATH, Absent)
                    .await
                    .unwrap();
            } else {
                owner
                    .object_server()
                    .at(
                        production::PROVIDER_PATH,
                        IconProvider {
                            state: state.clone(),
                            caller: server.unique_name().unwrap().to_string(),
                            generation: generation.clone(),
                        },
                    )
                    .await
                    .unwrap();
            }
            core.state
                .lock()
                .await
                .providers
                .insert(id.into(), provider(id, &owner, &generation, &folder));
            owners.push(owner);
            icons.push(state);
        }
        Self {
            bus,
            folder,
            core,
            client,
            owners,
            icons,
            revision,
        }
    }
    async fn api(&self) -> WebShortcutIconsProxy<'static> {
        WebShortcutIconsProxy::builder(&self.client)
            .destination(self.core.connection.unique_name().unwrap().to_owned())
            .unwrap()
            .build()
            .await
            .unwrap()
    }
    async fn wait_calls(&self, n: usize) {
        tokio::time::timeout(Duration::from_secs(2), async {
            while self.icons[0].calls.load(Ordering::SeqCst) < n {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
    }
}
fn error<T: std::fmt::Debug>(value: zbus::Result<T>) -> String {
    match value {
        Err(zbus::Error::MethodError(name, _, _)) => name.to_string(),
        other => panic!("expected method error, got {other:?}"),
    }
}
fn named(code: &str) -> String {
    format!("org.arinn.Error1.{code}")
}

#[tokio::test]
async fn routes_stored_origin_to_first_ready_browser_without_backend_or_launch() {
    let f = Fixture::new(false).await;
    let api = f.api().await;
    let (asset,) = api.get(&f.core.epoch, f.revision, "saved").await.unwrap();
    web_icons::image(&asset).unwrap();
    assert_eq!(
        f.icons[0].seen.lock().await[0].1,
        "https://example.invalid/"
    );
    assert_eq!(f.icons[1].calls.load(Ordering::SeqCst), 0);
    assert_eq!(
        error(
            api.get(&Uuid::new_v4().to_string(), f.revision, "saved")
                .await
        ),
        named("Conflict")
    );
    assert_eq!(
        error(api.get(&f.core.epoch, 0, "saved").await),
        named("InvalidArgument")
    );
    assert_eq!(
        error(api.get(&f.core.epoch, f.revision, "missing").await),
        named("NotFound")
    );
    for id in ["local", "explicit"] {
        assert_eq!(
            error(api.get(&f.core.epoch, f.revision, id).await),
            named("Unsupported")
        );
    }
    f.icons[0].invalid.store(true, Ordering::SeqCst);
    assert_eq!(
        error(api.get(&f.core.epoch, f.revision, "saved").await),
        named("Internal")
    );
}

#[tokio::test]
async fn absent_optional_interface_never_probes_the_next_browser() {
    let f = Fixture::new(true).await;
    assert_eq!(
        error(f.api().await.get(&f.core.epoch, f.revision, "saved").await),
        named("Unsupported")
    );
    assert_eq!(f.icons[1].calls.load(Ordering::SeqCst), 0);
    f.core.state.lock().await.providers.remove("a");
    assert!(
        f.api()
            .await
            .get(&f.core.epoch, f.revision, "saved")
            .await
            .is_ok()
    );
    assert_eq!(f.icons[1].calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn late_images_are_discarded_after_entry_or_registration_changes() {
    for change in ["edit", "remove", "generation", "owner", "selection"] {
        let f = Fixture::new(false).await;
        f.icons[0].hold.store(true, Ordering::SeqCst);
        let api = f.api().await;
        let epoch = f.core.epoch.clone();
        let revision = f.revision;
        let pending = tokio::spawn(async move { api.get(&epoch, revision, "saved").await });
        f.wait_calls(1).await;
        // Neither shared storage nor registry locks are held across the call.
        match change {
            "edit" => {
                let mut store = Store::open(&f.core.paths.state).unwrap();
                let mut row = store.web_shortcuts().unwrap().shortcuts[0].clone();
                row.url = "https://other.invalid/".into();
                store.update_web_shortcut(revision, &row).unwrap();
            }
            "remove" => {
                Store::open(&f.core.paths.state)
                    .unwrap()
                    .remove_web_shortcut(revision, "saved")
                    .unwrap();
            }
            "generation" => {
                f.core
                    .state
                    .lock()
                    .await
                    .providers
                    .get_mut("a")
                    .unwrap()
                    .info
                    .generation = Uuid::new_v4().to_string()
            }
            "owner" => {
                f.core
                    .state
                    .lock()
                    .await
                    .providers
                    .get_mut("a")
                    .unwrap()
                    .owner = f.owners[1].unique_name().unwrap().to_string()
            }
            _ => {
                f.core.state.lock().await.providers.remove("a");
            }
        }
        f.icons[0].gate.add_permits(1);
        assert_eq!(error(pending.await.unwrap()), named("Conflict"), "{change}");
    }
}

#[tokio::test]
async fn caps_concurrency_and_total_call_deadline() {
    let f = Fixture::new(false).await;
    f.icons[0].hold.store(true, Ordering::SeqCst);
    let mut pending = Vec::new();
    for _ in 0..4 {
        let api = f.api().await;
        let epoch = f.core.epoch.clone();
        let revision = f.revision;
        pending.push(tokio::spawn(async move {
            api.get(&epoch, revision, "saved").await
        }));
    }
    f.wait_calls(4).await;
    assert_eq!(
        error(f.api().await.get(&f.core.epoch, f.revision, "saved").await),
        named("Unavailable")
    );
    for call in pending {
        assert_eq!(error(call.await.unwrap()), named("Unavailable"));
    }
    f.icons[0].hold.store(false, Ordering::SeqCst);
    f.icons[0].gate.add_permits(4);
    assert!(
        f.api()
            .await
            .get(&f.core.epoch, f.revision, "saved")
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn exported_icon_method_matches_the_accepted_wire_contract() {
    let f = Fixture::new(false).await;
    let xml = zbus::fdo::IntrospectableProxy::builder(&f.client)
        .destination(f.core.connection.unique_name().unwrap().to_owned())
        .unwrap()
        .path(production::PATH)
        .unwrap()
        .build()
        .await
        .unwrap()
        .introspect()
        .await
        .unwrap();
    fn signature(xml: &str) -> Vec<(String, Vec<(String, String)>)> {
        let document = introspection(xml).unwrap();
        document
            .root_element()
            .children()
            .find(|n| n.attribute("name") == Some(web_icons::INTERFACE))
            .unwrap()
            .children()
            .filter(|n| n.has_tag_name("method"))
            .map(|n| {
                (
                    n.attribute("name").unwrap().to_owned(),
                    n.children()
                        .filter(|n| n.has_tag_name("arg"))
                        .map(|n| {
                            (
                                n.attribute("direction").unwrap_or("in").to_owned(),
                                n.attribute("type").unwrap().to_owned(),
                            )
                        })
                        .collect(),
                )
            })
            .collect()
    }
    assert_eq!(
        signature(&xml),
        signature(include_str!(
            "../../../../docs/plugin-api/web-shortcut-icons1.xml"
        ))
    );
}

#[test]
fn introspection_rejects_entities_and_unbounded_documents() {
    assert!(introspection("<!DOCTYPE node [<!ENTITY icon 'x'>]><node/>").is_err());
    assert!(introspection(&format!("<node>{}</node>", "<node/>".repeat(5000))).is_err());
    assert!(introspection(&" ".repeat(64 * 1024 + 1)).is_err());
}

#[tokio::test]
async fn initial_request_waits_for_registration_without_launching_a_browser() {
    let f = Fixture::new(false).await;
    for p in f.core.state.lock().await.providers.values_mut() {
        p.info.runtime_state = "starting".into();
    }
    let api = f.api().await;
    let epoch = f.core.epoch.clone();
    let revision = f.revision;
    let pending = tokio::spawn(async move { api.get(&epoch, revision, "saved").await });
    tokio::time::sleep(Duration::from_millis(120)).await;
    assert_eq!(f.icons[0].calls.load(Ordering::SeqCst), 0);
    assert!(!pending.is_finished());
    f.core
        .state
        .lock()
        .await
        .providers
        .get_mut("a")
        .unwrap()
        .info
        .runtime_state = "ready".into();
    assert!(pending.await.unwrap().is_ok());
    assert_eq!(f.icons[0].calls.load(Ordering::SeqCst), 1);
    assert_eq!(f.icons[1].calls.load(Ordering::SeqCst), 0);
}
