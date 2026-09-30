// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use arinn_plugin_sdk::platform::WindowClient;
use arinn_protocol::{
    kwin::KWinBridgeProxy, platform::PlatformWindowsProxy, production::ProductionOperationProxy,
};
use std::{
    io::{BufRead, BufReader, Write},
    process::{Child, Command, Stdio},
};
struct Fixture {
    bus: Child,
    folder: PathBuf,
    core: Core,
    kwin: Connection,
    provider: Connection,
    stranger: Connection,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = self.bus.kill();
        let _ = self.bus.wait();
        let _ = std::fs::remove_dir_all(&self.folder);
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
        async fn connect(address: &str) -> Connection {
            zbus::connection::Builder::address(address.trim())
                .unwrap()
                .build()
                .await
                .unwrap()
        }
        let server = connect(&address).await;
        let kwin = connect(&address).await;
        let provider = connect(&address).await;
        let stranger = connect(&address).await;
        server.request_name(production::SERVICE).await.unwrap();
        kwin.request_name("org.kde.KWin").await.unwrap();
        let folder = std::env::temp_dir().join(format!("arinn-windows-{}", Uuid::new_v4()));
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
        // Unit fixture installs the same authenticated registry state created by
        // Register. End-to-end token/cgroup authentication has separate coverage.
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
                    capabilities: vec!["application".into()],
                    runtime_state: "ready".into(),
                    backend_state: "available".into(),
                    generation: Uuid::new_v4().to_string(),
                    revision: 1,
                    last_error: String::new(),
                    error_at: 0,
                    recovered_at: 0,
                    restarts_since_daemon_start: 0,
                    cumulative_restart_count: 0,
                },
                manifest: None,
                unit: "fixture.service".into(),
                token_file: folder.join("fixture.token"),
                token: String::new(),
                owner: provider.unique_name().unwrap().to_string(),
                cache: None,
                applications: Vec::new(),
                running: Vec::new(),
                tab_cache: None,
                session_cache: None,
                sessions: Vec::new(),
                features: Vec::new(),
                windows: Vec::new(),
                tabs: Vec::new(),
                started: Instant::now(),
                enabled: true,
            },
        );
        Self {
            bus,
            folder,
            core,
            kwin,
            provider,
            stranger,
        }
    }
    async fn generation(&self) -> String {
        KWinBridgeProxy::new(&self.kwin)
            .await
            .unwrap()
            .connect(&Uuid::new_v4().to_string())
            .await
            .unwrap()
    }
    async fn publish(&self, generation: &str, revision: i32, count: i32) {
        let bridge = KWinBridgeProxy::new(&self.kwin).await.unwrap();
        bridge
            .begin_inventory(generation, revision, count)
            .await
            .unwrap();
        for i in 0..count {
            bridge
                .put_window(
                    generation,
                    revision,
                    i,
                    &format!("token-{i}"),
                    "fixture.desktop",
                    "",
                    if i == 0 { 15 } else { 12 },
                )
                .await
                .unwrap();
        }
        bridge.commit_inventory(generation, revision).await.unwrap();
    }
}
fn error(result: zbus::Result<impl std::fmt::Debug>, code: &str) {
    assert!(
        matches!(result,Err(zbus::Error::MethodError(ref name,_,_)) if name.as_str()==format!("org.arinn.Error1.{code}")),
        "{result:?}"
    );
}
#[tokio::test]
async fn production_inventory_auth_atomicity_leases_and_xml() {
    let f = Fixture::new().await;
    let generation = f.generation().await;
    let bridge = KWinBridgeProxy::new(&f.kwin).await.unwrap();
    let windows = PlatformWindowsProxy::new(&f.provider).await.unwrap();
    error(
        KWinBridgeProxy::new(&f.stranger)
            .await
            .unwrap()
            .connect(&Uuid::new_v4().to_string())
            .await,
        "PermissionDenied",
    );
    error(
        PlatformWindowsProxy::new(&f.stranger)
            .await
            .unwrap()
            .get_windows("")
            .await,
        "PermissionDenied",
    );
    error(windows.get_windows("").await, "Unavailable");
    f.publish(&generation, 1, 201).await;
    let (first,) = windows.get_windows("").await.unwrap();
    assert_eq!(first.windows.len(), 200);
    assert!(!first.next_token.is_empty());
    f.publish(&generation, 2, 1).await;
    assert_eq!(
        windows
            .get_windows(&first.next_token)
            .await
            .unwrap()
            .0
            .revision,
        1
    );
    assert_eq!(windows.get_windows("").await.unwrap().0.revision, 2);
    bridge.begin_inventory(&generation, 3, 2).await.unwrap();
    bridge
        .put_window(&generation, 3, 0, "valid", "fixture.desktop", "", 0)
        .await
        .unwrap();
    error(
        bridge.commit_inventory(&generation, 3).await,
        "InvalidArgument",
    );
    assert_eq!(windows.get_windows("").await.unwrap().0.revision, 2);
    bridge.begin_inventory(&generation, 3, 1).await.unwrap();
    error(
        bridge
            .put_window(&generation, 3, -1, "bad", "", "", 0)
            .await,
        "InvalidArgument",
    );
    error(bridge.commit_inventory(&generation, 3).await, "Conflict");
    bridge.begin_inventory(&generation, 3, 1).await.unwrap();
    bridge
        .put_window(&generation, 3, 0, "token-0", "fixture.desktop", "", 15)
        .await
        .unwrap();
    error(bridge.commit_inventory(&generation, 3).await, "Conflict");
    // A removed identity cannot be reintroduced in this generation.
    bridge.begin_inventory(&generation, 3, 1).await.unwrap();
    bridge
        .put_window(&generation, 3, 0, "token-1", "fixture.desktop", "", 0)
        .await
        .unwrap();
    error(bridge.commit_inventory(&generation, 3).await, "Conflict");
    let new_generation = f.generation().await;
    assert_ne!(generation, new_generation);
    error(
        windows
            .activate(&generation, "token-0", &Uuid::new_v4().to_string())
            .await,
        "Conflict",
    );
    f.publish(&new_generation, 1, 201).await;
    for _ in 0..32 {
        windows.get_windows("").await.unwrap();
    }
    error(windows.get_windows("").await, "Unavailable");
    f.core
        .windows
        .disconnect(f.provider.unique_name().unwrap().as_str())
        .await;
    assert!(windows.get_windows("").await.is_ok());
    let actual = zbus::fdo::IntrospectableProxy::builder(&f.provider)
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
    let mut python=Command::new("python").args(["-c",r#"
import sys,xml.etree.ElementTree as E
actual=E.fromstring(sys.stdin.read())
def canonical(node):
    children=[canonical(c) for c in node]
    if node.tag in ('node','interface'): children.sort()
    return node.tag,tuple(sorted(node.attrib.items())),tuple(children)
for filename,interface in [('platform-windows1.xml','org.arinn.PlatformWindows1'),('kwin-bridge1.xml','org.arinn.KWinBridge1')]:
    expected=E.parse('../../docs/plugin-api/'+filename).getroot().find('interface')
    assert canonical(expected)==canonical(actual.find("interface[@name='%s']"%interface)), filename
"#]).stdin(Stdio::piped()).spawn().unwrap();
    python
        .stdin
        .take()
        .unwrap()
        .write_all(actual.as_bytes())
        .unwrap();
    assert!(python.wait().unwrap().success());
}
#[tokio::test]
async fn acknowledged_operations_deduplication_disconnect_and_timeout() {
    let f = Fixture::new().await;
    let generation = f.generation().await;
    f.publish(&generation, 1, 1).await;
    let bridge = KWinBridgeProxy::new(&f.kwin).await.unwrap();
    let windows = PlatformWindowsProxy::new(&f.provider).await.unwrap();
    let client = WindowClient::new(&f.provider).await.unwrap();
    let request = Uuid::new_v4().to_string();
    let path = windows
        .activate(&generation, "token-0", &request)
        .await
        .unwrap();
    let command = bridge.next_control(&generation).await.unwrap();
    assert_eq!(&command[1..], ["activate", "token-0"]);
    assert_eq!(
        windows
            .activate(&generation, "token-0", &request)
            .await
            .unwrap(),
        path
    );
    error(
        windows.close(&generation, "token-0", &request).await,
        "Conflict",
    );
    bridge
        .complete_control(&generation, &command[0], "")
        .await
        .unwrap();
    client.wait_operation(&path).await.unwrap();
    error(
        bridge.complete_control(&generation, &command[0], "").await,
        "Conflict",
    );
    f.publish(&generation, 2, 0).await;
    assert_eq!(
        windows
            .activate(&generation, "token-0", &request)
            .await
            .unwrap(),
        path
    );
    error(
        windows
            .activate(&generation, "token-0", &Uuid::new_v4().to_string())
            .await,
        "NotFound",
    );
    let generation = f.generation().await;
    f.publish(&generation, 1, 1).await;
    let pending = windows
        .close(&generation, "token-0", &Uuid::new_v4().to_string())
        .await
        .unwrap();
    let command = bridge.next_control(&generation).await.unwrap();
    bridge
        .complete_control(&generation, &command[0], "PermissionDenied")
        .await
        .unwrap();
    assert!(matches!(
        client.wait_operation(&pending).await,
        Err(production::Error::PermissionDenied(_))
    ));
    // Same shared per-provider queue budget as ordinary application operations.
    let mut paths = Vec::new();
    for _ in 0..32 {
        paths.push(
            windows
                .close(&generation, "token-0", &Uuid::new_v4().to_string())
                .await
                .unwrap(),
        );
    }
    error(
        windows
            .close(&generation, "token-0", &Uuid::new_v4().to_string())
            .await,
        "Unavailable",
    );
    f.core
        .unavailable("fixture", f.provider.unique_name().unwrap().as_str())
        .await;
    assert!(matches!(
        client.wait_operation(&paths[0]).await,
        Err(production::Error::Unavailable(_))
    ));
    error(windows.get_windows("").await, "PermissionDenied");
    {
        let mut state = f.core.state.lock().await;
        let p = state.providers.get_mut("fixture").unwrap();
        p.owner = f.provider.unique_name().unwrap().to_string();
        p.info.generation = Uuid::new_v4().to_string();
    }
    let pending = windows
        .activate(&generation, "token-0", &Uuid::new_v4().to_string())
        .await
        .unwrap();
    let command = bridge.next_control(&generation).await.unwrap();
    // Keep the adapter alive while withholding only the operation acknowledgement.
    for _ in 0..6 {
        tokio::time::sleep(Duration::from_secs(1)).await;
        bridge.next_control(&generation).await.unwrap();
    }
    let operation = ProductionOperationProxy::builder(&f.provider)
        .path(pending)
        .unwrap()
        .build()
        .await
        .unwrap();
    assert_eq!(operation.get().await.unwrap().0.error, "Timeout");
    error(
        bridge.complete_control(&generation, &command[0], "").await,
        "Conflict",
    );
    let pending = windows
        .activate(&generation, "token-0", &Uuid::new_v4().to_string())
        .await
        .unwrap();
    f.generation().await;
    assert!(matches!(
        client.wait_operation(&pending).await,
        Err(production::Error::Unavailable(_))
    ));
}

struct LaunchingProvider;
#[zbus::interface(name = "org.arinn.Provider1")]
impl LaunchingProvider {
    fn launch(&self, _generation: &str, _id: &str) -> production::Result<()> {
        Ok(())
    }
}
/// A successful launch hides Home through the platform adapter, so the new
/// application window is not left underneath the shell.
#[tokio::test]
async fn successful_launch_dismisses_home() {
    let f = Fixture::new().await;
    f.provider
        .object_server()
        .at(production::PROVIDER_PATH, LaunchingProvider)
        .await
        .unwrap();
    f.core
        .connection
        .object_server()
        .at(
            production::PATH,
            Daemon {
                core: f.core.clone(),
            },
        )
        .await
        .unwrap();
    let generation = {
        let mut state = f.core.state.lock().await;
        let p = state.providers.get_mut("fixture").unwrap();
        p.applications = vec![Application {
            id: "app".into(),
            title: "App".into(),
            can_launch: true,
            group: "other".into(),
            icon: String::new(),
        }];
        p.info.generation.clone()
    };
    // The KWin adapter long-polls for commands; its first poll marks it live.
    let kwin = f.kwin.clone();
    let poll = tokio::spawn(async move {
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
    });
    tokio::time::sleep(Duration::from_millis(300)).await;
    let path = f
        .core
        .command(
            "fixture",
            "launch",
            &generation,
            "app",
            &Uuid::new_v4().to_string(),
            "caller",
        )
        .await
        .unwrap();
    let command = tokio::time::timeout(Duration::from_secs(10), poll)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(command, "dismiss");
    let operation = ProductionOperationProxy::builder(&f.provider)
        .path(path)
        .unwrap()
        .build()
        .await
        .unwrap();
    // Dismissal is queued as the launch succeeds; the operation completes next.
    let mut state = String::new();
    for _ in 0..50 {
        state = operation.get().await.unwrap().0.state;
        if state != "queued" && state != "running" {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(state, "succeeded");
}
