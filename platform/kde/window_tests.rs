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
        kwin.request_name(
            crate::platform::adapter_service().unwrap_or("org.arinn.InvalidPlatformBackend"),
        )
        .await
        .unwrap();
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
for filename,interface in [('platform-windows1.xml','org.arinn.PlatformWindows1'),('kwin-bridge1.xml','org.arinn.KWinBridge1'),('return-association1.xml','org.arinn.ReturnAssociation1'),('kwin-return1.xml','org.arinn.KWinReturn1')]:
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

/// AC-18: presentation intent is distinct from Home state synchronization.
#[tokio::test]
async fn explicit_home_requests_repeat_but_state_refreshes_do_not() {
    use arinn_protocol::production::ProductionDaemonProxy;
    let f = Fixture::new().await;
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
    let shell = ProductionDaemonProxy::new(&f.stranger).await.unwrap();
    let adapter = ProductionDaemonProxy::new(&f.kwin).await.unwrap();
    let mut requests = shell.receive_home_requested().await.unwrap();
    let daemon = zbus::Proxy::new(
        &f.stranger,
        production::SERVICE,
        production::PATH,
        production::SERVICE,
    )
    .await
    .unwrap();
    let mut changes = daemon.receive_signal("Changed").await.unwrap();
    for _ in 0..2 {
        adapter
            .report_home_request("activity", "screen")
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(2), changes.next())
            .await
            .unwrap()
            .unwrap();
        let event = tokio::time::timeout(Duration::from_secs(2), requests.next())
            .await
            .unwrap()
            .unwrap();
        let args = event.args().unwrap();
        assert_eq!(args.epoch, f.core.epoch);
        assert_eq!(args.target, "activity");
        assert_eq!(args.output, "screen");
        let (status,) = shell.status().await.unwrap();
        assert!(status.home_visible);
        assert_eq!(status.return_window, "activity");
        assert_eq!(status.home_output, "screen");
    }
    // Disappearance, remap, reconnect state restoration, and dismissal all
    // publish Changed, but none may steal focus through HomeRequested.
    for (visible, target) in [
        (true, ""),
        (true, "activity"),
        (true, "activity"),
        (false, ""),
    ] {
        f.kwin
            .call_method(
                Some(production::SERVICE),
                production::PATH,
                Some(production::SERVICE),
                "ReportHome",
                &(visible, target, "screen"),
            )
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(2), changes.next())
            .await
            .unwrap()
            .unwrap();
        shell.status().await.unwrap();
    }
    assert!(
        tokio::time::timeout(Duration::from_millis(50), requests.next())
            .await
            .is_err()
    );
    // A late subscriber receives no historical intent. A new Home without a
    // return Activity is nevertheless an explicit, separately delivered event.
    let mut late = shell.receive_home_requested().await.unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(50), late.next())
            .await
            .is_err()
    );
    adapter.report_home_request("", "screen").await.unwrap();
    let event = tokio::time::timeout(Duration::from_secs(2), late.next())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(event.args().unwrap().target, "");
    assert!(shell.status().await.unwrap().0.home_visible);
}

#[tokio::test]
async fn home_requests_reject_untrusted_invalid_and_obsolete_adapter_calls() {
    use arinn_protocol::production::ProductionDaemonProxy;
    let f = Fixture::new().await;
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
    let shell = ProductionDaemonProxy::new(&f.stranger).await.unwrap();
    let adapter = ProductionDaemonProxy::new(&f.kwin).await.unwrap();
    let mut requests = shell.receive_home_requested().await.unwrap();
    assert_eq!(
        normalize_error(
            shell
                .report_home_request("activity", "screen")
                .await
                .unwrap_err()
        ),
        ErrorCode::PermissionDenied
    );
    for (target, output) in [
        ("x".repeat(8193), "screen".into()),
        ("activity".into(), "x".repeat(8193)),
    ] {
        assert_eq!(
            normalize_error(
                adapter
                    .report_home_request(&target, &output)
                    .await
                    .unwrap_err()
            ),
            ErrorCode::InvalidArgument
        );
    }
    // Authentication follows the actual org.kde.KWin unique owner, not a
    // connection that used to own the trusted name.
    f.kwin.release_name("org.kde.KWin").await.unwrap();
    f.provider.request_name("org.kde.KWin").await.unwrap();
    assert_eq!(
        normalize_error(
            adapter
                .report_home_request("activity", "screen")
                .await
                .unwrap_err()
        ),
        ErrorCode::PermissionDenied
    );
    let status = shell.status().await.unwrap().0;
    assert!(!status.home_visible);
    assert!(status.return_window.is_empty());
    assert!(
        tokio::time::timeout(Duration::from_millis(50), requests.next())
            .await
            .is_err()
    );
    let replacement = ProductionDaemonProxy::new(&f.provider).await.unwrap();
    replacement
        .report_home_request("activity", "screen")
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(2), requests.next())
        .await
        .unwrap()
        .unwrap();
}

/// ADR-0106: the quick-menu key commits the same Home state as an explicit
/// Home request, but only its own intent: neither request emits the other's
/// signal, and untrusted or invalid calls emit nothing.
#[tokio::test]
async fn quick_menu_requests_are_distinct_from_home_requests() {
    use arinn_protocol::production::ProductionDaemonProxy;
    let f = Fixture::new().await;
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
    let shell = ProductionDaemonProxy::new(&f.stranger).await.unwrap();
    let adapter = ProductionDaemonProxy::new(&f.kwin).await.unwrap();
    let mut menus = shell.receive_quick_menu_requested().await.unwrap();
    let mut homes = shell.receive_home_requested().await.unwrap();
    let quiet = Duration::from_millis(50);
    // Untrusted and invalid calls change no state and emit no intent.
    assert_eq!(
        normalize_error(
            shell
                .report_quick_menu_request("activity", "screen")
                .await
                .unwrap_err()
        ),
        ErrorCode::PermissionDenied
    );
    assert_eq!(
        normalize_error(
            adapter
                .report_quick_menu_request(&"x".repeat(8193), "screen")
                .await
                .unwrap_err()
        ),
        ErrorCode::InvalidArgument
    );
    assert!(!shell.status().await.unwrap().0.home_visible);
    assert!(tokio::time::timeout(quiet, menus.next()).await.is_err());
    // Every press is delivered, identical repeats included.
    for _ in 0..2 {
        adapter
            .report_quick_menu_request("activity", "screen")
            .await
            .unwrap();
        let event = tokio::time::timeout(Duration::from_secs(2), menus.next())
            .await
            .unwrap()
            .unwrap();
        let args = event.args().unwrap();
        assert_eq!(args.epoch, f.core.epoch);
        assert_eq!(args.target, "activity");
        assert_eq!(args.output, "screen");
        let (status,) = shell.status().await.unwrap();
        assert!(status.home_visible);
        assert_eq!(status.return_window, "activity");
        assert_eq!(status.home_output, "screen");
    }
    assert!(tokio::time::timeout(quiet, homes.next()).await.is_err());
    // An explicit Home request is not a quick-menu request.
    adapter
        .report_home_request("activity", "screen")
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(2), homes.next())
        .await
        .unwrap()
        .unwrap();
    assert!(tokio::time::timeout(quiet, menus.next()).await.is_err());
    // A late subscriber receives no historical intent.
    let mut late = shell.receive_quick_menu_requested().await.unwrap();
    assert!(tokio::time::timeout(quiet, late.next()).await.is_err());
}

/// ADR-0073 producer: authenticate before validation, publish only exact live
/// context, and never let a rejected report erase an established association.
#[tokio::test]
async fn return_association_production_context_and_invalidation() {
    use arinn_protocol::{
        production::ProductionDaemonProxy,
        return_association::{KWinReturnProxy, ReturnAssociationApiProxy},
    };
    let f = Fixture::new().await;
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
    let home = ProductionDaemonProxy::new(&f.kwin).await.unwrap();
    let report = KWinReturnProxy::new(&f.kwin).await.unwrap();
    let api = ReturnAssociationApiProxy::new(&f.stranger).await.unwrap();
    let initial = api.get().await.unwrap().0;
    assert_eq!(initial.epoch, f.core.epoch);
    assert_eq!(initial.revision, 1);
    error(
        KWinReturnProxy::new(&f.stranger)
            .await
            .unwrap()
            .report("bad", 0, "", "")
            .await,
        "PermissionDenied",
    );
    error(report.report("bad", 0, "", "").await, "PermissionDenied");
    let generation = f.generation().await;
    error(report.report("bad", 0, "", "").await, "InvalidArgument");
    home.report_home_request("opaque-return", "screen")
        .await
        .unwrap();
    error(
        report
            .report(&generation, 1, "opaque-return", "token-0")
            .await,
        "Unavailable",
    );
    f.publish(&generation, 1, 2).await;
    report
        .report(&generation, 1, "opaque-return", "token-0")
        .await
        .unwrap();
    let good = api.get().await.unwrap().0;
    good.validate().unwrap();
    assert_eq!(good.window[0].generation, generation);
    assert_eq!(good.window[0].token, "token-0");
    assert_ne!(good.target, good.window[0].token);
    let mut signals = api.receive_changed().await.unwrap();
    for (g, r, t, w, e) in [
        (generation.as_str(), 2, "opaque-return", "", "Conflict"),
        (generation.as_str(), 1, "old-target", "", "Conflict"),
        (initial.epoch.as_str(), 1, "opaque-return", "", "Conflict"),
        (
            generation.as_str(),
            1,
            "opaque-return",
            "unknown",
            "NotFound",
        ),
    ] {
        error(report.report(g, r, t, w).await, e);
        assert_eq!(api.get().await.unwrap().0, good);
    }
    report
        .report(&generation, 1, "opaque-return", "token-0")
        .await
        .unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(80), signals.next())
            .await
            .is_err()
    );
    // Focus-only inventory updates preserve exact identity and public revision.
    let bridge = KWinBridgeProxy::new(&f.kwin).await.unwrap();
    bridge.begin_inventory(&generation, 2, 2).await.unwrap();
    for i in 0..2 {
        bridge
            .put_window(
                &generation,
                2,
                i,
                &format!("token-{i}"),
                "fixture.desktop",
                "",
                12,
            )
            .await
            .unwrap();
    }
    bridge.commit_inventory(&generation, 2).await.unwrap();
    assert_eq!(api.get().await.unwrap().0, good);
    // Same-target Home refresh revokes evidence before its acknowledgement.
    home.inner()
        .call::<_, _, ()>("ReportHome", &(true, "opaque-return", "screen"))
        .await
        .unwrap();
    let cleared = api.get().await.unwrap().0;
    assert!(cleared.window.is_empty());
    assert_eq!(cleared.target, good.target);
    let changed = tokio::time::timeout(Duration::from_secs(2), signals.next())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(changed.args().unwrap().revision, cleared.revision);
    report
        .report(&generation, 2, "opaque-return", "token-0")
        .await
        .unwrap();
    home.report_home_request("opaque-return", "screen")
        .await
        .unwrap();
    assert!(api.get().await.unwrap().0.window.is_empty());
    report
        .report(&generation, 2, "opaque-return", "token-0")
        .await
        .unwrap();
    home.inner()
        .call::<_, _, ()>("ReportHome", &(false, "", "screen"))
        .await
        .unwrap();
    error(
        report
            .report(&initial.epoch, 99, "old-target", "token-0")
            .await,
        "Unavailable",
    );
    home.inner()
        .call::<_, _, ()>("ReportHome", &(true, "opaque-return", "screen"))
        .await
        .unwrap();
    report
        .report(&generation, 2, "opaque-return", "token-0")
        .await
        .unwrap();
    f.publish(&generation, 3, 0).await;
    assert!(api.get().await.unwrap().0.window.is_empty());
    assert_eq!(api.get().await.unwrap().0.target, "opaque-return");
    error(
        report
            .report(&generation, 3, "opaque-return", "token-0")
            .await,
        "NotFound",
    );
    // Remap/session replacement requires new generation and explicit evidence.
    let replacement = f.generation().await;
    f.publish(&replacement, 1, 1).await;
    error(
        report
            .report(&generation, 3, "opaque-return", "token-0")
            .await,
        "Conflict",
    );
    report
        .report(&replacement, 1, "opaque-return", "token-0")
        .await
        .unwrap();
    f.kwin.release_name("org.kde.KWin").await.unwrap();
    f.provider.request_name("org.kde.KWin").await.unwrap();
    error(
        report
            .report(&replacement, 1, "opaque-return", "token-0")
            .await,
        "PermissionDenied",
    );
    error(
        KWinReturnProxy::new(&f.provider)
            .await
            .unwrap()
            .report(&replacement, 1, "opaque-return", "token-0")
            .await,
        "PermissionDenied",
    );
    assert!(api.get().await.unwrap().0.window.is_empty());
    assert_eq!(api.get().await.unwrap().0.target, "opaque-return");
}

#[tokio::test]
async fn return_association_heartbeat_expiry_clears_without_erasing_return() {
    use arinn_protocol::{
        production::ProductionDaemonProxy,
        return_association::{KWinReturnProxy, ReturnAssociationApiProxy},
    };
    let f = Fixture::new().await;
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
    let generation = f.generation().await;
    f.publish(&generation, 1, 1).await;
    ProductionDaemonProxy::new(&f.kwin)
        .await
        .unwrap()
        .inner()
        .call::<_, _, ()>("ReportHome", &(true, "target", ""))
        .await
        .unwrap();
    KWinReturnProxy::new(&f.kwin)
        .await
        .unwrap()
        .report(&generation, 1, "target", "token-0")
        .await
        .unwrap();
    let api = ReturnAssociationApiProxy::new(&f.stranger).await.unwrap();
    let mut signals = api.receive_changed().await.unwrap();
    tokio::time::timeout(Duration::from_secs(7), signals.next())
        .await
        .unwrap()
        .unwrap();
    let value = api.get().await.unwrap().0;
    assert!(value.window.is_empty());
    assert_eq!(value.target, "target");
}

#[tokio::test]
async fn exported_browser_links_require_live_inventory_and_current_registration() {
    use arinn_protocol::browser::BrowserApiProxy;
    let f = Fixture::new().await;
    let generation = f.generation().await;
    f.publish(&generation, 1, 1).await;
    let path = "/org/arinn/Daemon1/providers/fixture";
    f.core
        .connection
        .object_server()
        .at(
            path,
            BrowserEndpoint {
                core: f.core.clone(),
                id: "fixture".into(),
            },
        )
        .await
        .unwrap();
    {
        let mut state = f.core.state.lock().await;
        let p = state.providers.get_mut("fixture").unwrap();
        p.windows = vec![BrowserWindow {
            id: "browser-window".into(),
            window_token: "token-0".into(),
            focused: false,
        }];
        let mut cache = TabSnapshots::new(&p.info.generation).unwrap();
        cache.import(1, p.windows.clone(), vec![]).unwrap();
        p.tab_cache = Some(cache);
    }
    let api = BrowserApiProxy::builder(&f.stranger)
        .destination(production::SERVICE)
        .unwrap()
        .path(path)
        .unwrap()
        .build()
        .await
        .unwrap();
    assert_eq!(
        api.get_tabs("").await.unwrap().0.windows[0].window_token,
        "token-0"
    );
    {
        let mut state = f.core.state.lock().await;
        failed(
            &mut state.providers.get_mut("fixture").unwrap().info,
            ErrorCode::Timeout,
        );
    }
    error(api.get_tabs("").await, "Unavailable");
    f.core
        .state
        .lock()
        .await
        .providers
        .get_mut("fixture")
        .unwrap()
        .info
        .last_error
        .clear();
    f.publish(&generation, 2, 0).await;
    error(api.get_tabs("").await, "Unavailable");
    // Unlinked/offline browser snapshots still work without platform evidence.
    {
        let mut state = f.core.state.lock().await;
        let p = state.providers.get_mut("fixture").unwrap();
        p.windows[0].window_token.clear();
        p.info.revision = 2;
        p.tab_cache
            .as_mut()
            .unwrap()
            .import(2, p.windows.clone(), vec![])
            .unwrap();
        p.info.backend_state = "unavailable".into();
        failed(&mut p.info, ErrorCode::Timeout);
    }
    assert!(
        api.get_tabs("").await.unwrap().0.windows[0]
            .window_token
            .is_empty()
    );
    f.core
        .state
        .lock()
        .await
        .providers
        .get_mut("fixture")
        .unwrap()
        .info
        .generation = Uuid::new_v4().to_string();
    error(api.get_tabs("").await, "Conflict");
}

#[tokio::test]
async fn slow_home_authorization_does_not_hold_inventory_lock() {
    let f = Fixture::new().await;
    let windows = f.core.windows.clone();
    let platform = f.core.platform.clone();
    let connection = f.core.connection.clone();
    let (started, ready) = tokio::sync::oneshot::channel();
    let (release, delayed) = tokio::sync::oneshot::channel();
    let task = tokio::spawn(async move {
        windows
            .commit_home_after(&platform, &connection, true, "target", "", async {
                started.send(()).unwrap();
                delayed.await.unwrap();
                Err(ErrorCode::PermissionDenied.into())
            })
            .await
    });
    ready.await.unwrap();
    let result = tokio::time::timeout(
        Duration::from_millis(200),
        f.core.windows.current_generation(),
    )
    .await;
    release.send(()).unwrap();
    assert!(
        result.is_ok(),
        "inventory must remain responsive during authorization"
    );
    assert!(matches!(
        task.await.unwrap(),
        Err(production::Error::PermissionDenied(_))
    ));
    assert!(
        !f.core
            .platform
            .status(&f.core.epoch, true)
            .await
            .home_visible
    );
}

#[tokio::test]
async fn slow_browser_checks_release_registry_and_revalidate_identity() {
    for replace_registration in [true, false] {
        let f = Fixture::new().await;
        {
            let mut state = f.core.state.lock().await;
            let p = state.providers.get_mut("fixture").unwrap();
            p.windows = vec![BrowserWindow {
                id: "w".into(),
                window_token: "token".into(),
                focused: false,
            }];
            let mut cache = TabSnapshots::new(&p.info.generation).unwrap();
            cache.import(1, p.windows.clone(), vec![]).unwrap();
            p.tab_cache = Some(cache);
        }
        let endpoint = BrowserEndpoint {
            core: f.core.clone(),
            id: "fixture".into(),
        };
        let (started, ready) = tokio::sync::oneshot::channel();
        let (release, delayed) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(async move {
            endpoint
                .checked_tabs(":fixture", "", |_, linked| async move {
                    assert!(linked);
                    started.send(()).unwrap();
                    delayed.await.unwrap();
                    Ok(HashSet::from(["token".into()]))
                })
                .await
        });
        ready.await.unwrap();
        {
            let mut state = tokio::time::timeout(Duration::from_millis(200), f.core.state.lock())
                .await
                .expect("unrelated provider operations must not wait for the bus lookup");
            let p = state.providers.get_mut("fixture").unwrap();
            if replace_registration {
                p.info.generation = Uuid::new_v4().to_string();
            } else {
                p.windows[0].window_token.clear();
            }
        }
        release.send(()).unwrap();
        let result = task.await.unwrap();
        if replace_registration {
            assert!(matches!(result, Err(production::Error::Conflict(_))));
        } else {
            assert!(matches!(result, Err(production::Error::Unavailable(_))));
        }
    }
}

#[tokio::test]
async fn late_authorized_home_cannot_overwrite_a_new_owners_commit() {
    use arinn_protocol::production::ProductionDaemonProxy;
    // Include identical content: public association revision is not a sufficient
    // fence because an accepted identical Home report need not change it.
    for newest_target in ["current", "new-target"] {
        let f = Fixture::new().await;
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
        let a = ProductionDaemonProxy::new(&f.kwin).await.unwrap();
        a.inner()
            .call::<_, _, ()>("ReportHome", &(true, "current", "new-output"))
            .await
            .unwrap();
        let windows = f.core.windows.clone();
        let platform = f.core.platform.clone();
        let connection = f.core.connection.clone();
        let (authorized, ready) = tokio::sync::oneshot::channel();
        let (release, delayed) = tokio::sync::oneshot::channel();
        let pending = tokio::spawn(async move {
            windows
                .commit_home_after(
                    &platform,
                    &connection,
                    false,
                    "stale-a",
                    "old-output",
                    async {
                        // A's authorization succeeded before losing the name; delay its
                        // continuation until B has really committed through Daemon1.
                        authorized.send(()).unwrap();
                        delayed.await.unwrap();
                        Ok(())
                    },
                )
                .await
        });
        ready.await.unwrap();
        f.kwin.release_name("org.kde.KWin").await.unwrap();
        f.provider.request_name("org.kde.KWin").await.unwrap();
        // No inventory session: Home must work before Connect. This isolates
        // the Home revision fence from inventory-generation reset protection.
        let b = ProductionDaemonProxy::new(&f.provider).await.unwrap();
        b.inner()
            .call::<_, _, ()>("ReportHome", &(true, newest_target, "new-output"))
            .await
            .unwrap();
        release.send(()).unwrap();
        assert!(matches!(
            pending.await.unwrap(),
            Err(production::Error::Conflict(_))
        ));
        let status = f.core.platform.status(&f.core.epoch, true).await;
        assert!(status.home_visible);
        assert_eq!(status.return_window, newest_target);
        assert_eq!(status.home_output, "new-output");
        let association =
            arinn_protocol::return_association::ReturnAssociationApiProxy::new(&f.stranger)
                .await
                .unwrap()
                .get()
                .await
                .unwrap()
                .0;
        assert_eq!(association.target, newest_target);
        assert!(association.window.is_empty());
    }
}

#[tokio::test]
async fn selected_adapter_owner_excludes_the_other_backend() {
    let f = Fixture::new().await;
    let selected = crate::platform::adapter_service().unwrap();
    let other = if selected == "org.arinn.X11" {
        "org.kde.KWin"
    } else {
        "org.arinn.X11"
    };
    f.stranger.request_name(other).await.unwrap();
    let untrusted = KWinBridgeProxy::new(&f.stranger).await.unwrap();
    assert_eq!(
        crate::runtime::normalize_error(
            untrusted
                .connect(&Uuid::new_v4().to_string())
                .await
                .unwrap_err()
        ),
        ErrorCode::PermissionDenied
    );
    let trusted = KWinBridgeProxy::new(&f.kwin).await.unwrap();
    let generation = trusted.connect(&Uuid::new_v4().to_string()).await.unwrap();
    trusted.begin_inventory(&generation, 1, 0).await.unwrap();
    trusted.commit_inventory(&generation, 1).await.unwrap();
    f.kwin.release_name(selected).await.unwrap();
    assert!(trusted.begin_inventory(&generation, 2, 0).await.is_err());
}

#[tokio::test]
async fn invalid_backend_rejects_even_a_claimed_sentinel_name() {
    if crate::platform::adapter_service().is_some() {
        return;
    }
    let f = Fixture::new().await;
    let caller = KWinBridgeProxy::new(&f.kwin).await.unwrap();
    assert_eq!(
        crate::runtime::normalize_error(
            caller
                .connect(&Uuid::new_v4().to_string())
                .await
                .unwrap_err()
        ),
        ErrorCode::PermissionDenied
    );
    f.core
        .connection
        .object_server()
        .at(
            production::PATH,
            crate::runtime::Daemon {
                core: f.core.clone(),
            },
        )
        .await
        .unwrap();
    let home = arinn_protocol::production::ProductionDaemonProxy::new(&f.kwin)
        .await
        .unwrap();
    assert_eq!(
        crate::runtime::normalize_error(home.report_home_request("", "").await.unwrap_err()),
        ErrorCode::PermissionDenied
    );
}
/// Records the keys the daemon writes to KGlobalAccel.
type AccelKeys = Vec<(Vec<i32>,)>;
struct RecordingAccel(Arc<std::sync::Mutex<Vec<AccelKeys>>>);
#[zbus::interface(name = "org.kde.KGlobalAccel")]
impl RecordingAccel {
    #[zbus(name = "setForeignShortcutKeys")]
    fn set_foreign_shortcut_keys(&self, action: Vec<String>, keys: AccelKeys) {
        assert_eq!(action[..2], ["kwin", "ArinnPhase0Home"]);
        self.0.lock().unwrap().push(keys);
    }
    #[zbus(name = "shortcutKeys")]
    fn shortcut_keys(&self, _action: Vec<String>) -> AccelKeys {
        self.0.lock().unwrap().last().cloned().unwrap_or_default()
    }
}
/// `home_shortcut` from config.toml and HomePage reach KDE's global shortcut
/// once per adapter connection (#286). Without the fix the configured value
/// was only validated, and KWin kept its hardcoded Meta+F12.
#[tokio::test]
async fn adapter_connection_applies_configured_home_shortcut() {
    let f = Fixture::new().await;
    let mut config = Config::load(&f.core.paths.config).unwrap();
    config.home_shortcut = "Ctrl+Alt+H".into();
    config.save(&f.core.paths.config).unwrap();
    let writes = Arc::default();
    f.stranger
        .object_server()
        .at("/kglobalaccel", RecordingAccel(Arc::clone(&writes)))
        .await
        .unwrap();
    f.stranger
        .request_name("org.kde.kglobalaccel")
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
    let poll = || {
        let kwin = f.kwin.clone();
        tokio::spawn(async move {
            kwin.call_method(
                Some(production::SERVICE),
                production::PATH,
                Some("org.arinn.Daemon1"),
                "TakeCommand",
                &(),
            )
            .await
        })
    };
    let first = poll();
    for _ in 0..100 {
        if !writes.lock().unwrap().is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(
        *writes.lock().unwrap(),
        [vec![
            (vec![0x0C00_0048, 0, 0, 0],),
            (vec![0x0100_0090, 0, 0, 0],)
        ]]
    );
    // Later polls of the same adapter connection leave the keys alone.
    first.abort();
    let second = poll();
    tokio::time::sleep(Duration::from_millis(300)).await;
    second.abort();
    assert_eq!(writes.lock().unwrap().len(), 1);
}
