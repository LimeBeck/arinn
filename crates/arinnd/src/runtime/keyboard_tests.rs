// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! ADR-0105 routing on private buses: a fake KWin requests sessions, a fake
//! input method delivers, a shell connection types.
use super::*;
use crate::{runtime::Provider, storage::Paths};
use arinn_protocol::{
    keyboard::{ExternalKeyboardProxy, KeyboardBridgeProxy},
    production::ProviderInfo,
};
use futures_util::StreamExt;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};
use zbus::Connection;

struct Bus(Child, String);
impl Bus {
    fn new() -> Self {
        let mut child = Command::new("dbus-daemon")
            .args(["--session", "--nofork", "--print-address=1"])
            .stdout(Stdio::piped())
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
            .await
            .unwrap()
    }
    async fn named(&self, name: &str) -> Connection {
        zbus::connection::Builder::address(self.1.as_str())
            .unwrap()
            .name(name)
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

struct Fixture {
    bus: Bus,
    core: Core,
    daemon: String,
    kwin: Connection,
    deliverer: Connection,
    shell: Connection,
}

impl Fixture {
    async fn new() -> Self {
        let bus = Bus::new();
        let server = bus.connect().await;
        let folder = std::env::temp_dir().join(format!("arinn-keyboard-{}", Uuid::new_v4()));
        let core = Core::new(
            &server,
            Paths {
                config: folder.join("config"),
                data: folder.join("data"),
                state: folder.join("state"),
            },
            folder,
        );
        core.keyboard.trust_children(|_, _| true).await;
        core.export_keyboard().await.unwrap();
        let kwin = bus.named("org.kde.KWin").await;
        let deliverer = bus.named(keyboard::INPUT_METHOD_SERVICE).await;
        let shell = bus.connect().await;
        Self {
            daemon: server.unique_name().unwrap().to_string(),
            bus,
            core,
            kwin,
            deliverer,
            shell,
        }
    }
    async fn bridge<'a>(&self, c: &'a Connection) -> KeyboardBridgeProxy<'a> {
        KeyboardBridgeProxy::builder(c)
            .destination(self.daemon.clone())
            .unwrap()
            .build()
            .await
            .unwrap()
    }
    async fn shell<'a>(&self, c: &'a Connection) -> ExternalKeyboardProxy<'a> {
        ExternalKeyboardProxy::builder(c)
            .destination(self.daemon.clone())
            .unwrap()
            .build()
            .await
            .unwrap()
    }
    /// Connects the deliverer and opens a session; returns the deliverer's
    /// generation and the session.
    async fn open(&self) -> (String, String) {
        let bridge = self.bridge(&self.deliverer).await;
        let generation = bridge.connect(&Uuid::new_v4().to_string()).await.unwrap();
        self.bridge(&self.kwin).await.request().await.unwrap();
        let Record::Open(session) = next(&bridge, &generation).await else {
            panic!("open record expected");
        };
        bridge.opened(&generation, &session).await.unwrap();
        (generation, session)
    }
}

async fn next(bridge: &KeyboardBridgeProxy<'_>, generation: &str) -> Record {
    loop {
        let record = Record::parse(&bridge.next(generation).await.unwrap()).unwrap();
        if record != Record::Idle {
            return record;
        }
    }
}

fn denied(error: zbus::Error) -> bool {
    matches!(error, zbus::Error::MethodError(name, _, _) if name.as_str() == "org.arinn.Error1.PermissionDenied")
}
fn named(error: zbus::Error, wanted: &str) -> bool {
    matches!(error, zbus::Error::MethodError(name, _, _) if name.as_str() == format!("org.arinn.Error1.{wanted}"))
}

fn compare_xml(actual: &str, filename: &str, interface: &str) {
    let script = r#"
import sys, xml.etree.ElementTree as ET
def members(root):
    interface = root.find("interface[@name='" + sys.argv[2] + "']")
    assert interface is not None
    return sorted((n.tag,n.get('name'),[(a.get('name'),a.get('type'),a.get('direction')) for a in n.findall('arg')]) for n in interface if n.tag in ('method','signal'))
assert members(ET.parse(sys.argv[1]).getroot()) == members(ET.fromstring(sys.stdin.read()))
"#;
    let file = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/plugin-api")
        .join(filename);
    let mut child = Command::new("python3")
        .args(["-c", script, file.to_str().unwrap(), interface])
        .stdin(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(actual.as_bytes())
        .unwrap();
    assert!(child.wait().unwrap().success(), "{filename}");
}

fn within<F: std::future::Future>(f: F) -> tokio::time::Timeout<F> {
    tokio::time::timeout(Duration::from_secs(20), f)
}

#[tokio::test]
async fn a_session_runs_from_the_key_to_the_shell_and_back() {
    within(async {
        let f = Fixture::new().await;
        let live = zbus::fdo::IntrospectableProxy::builder(&f.shell)
            .destination(f.daemon.clone())
            .unwrap()
            .path(production::PATH)
            .unwrap()
            .build()
            .await
            .unwrap()
            .introspect()
            .await
            .unwrap();
        compare_xml(&live, "external-keyboard1.xml", keyboard::SHELL_INTERFACE);
        compare_xml(&live, "keyboard-bridge1.xml", keyboard::BRIDGE_INTERFACE);

        let shell = f.shell(&f.shell).await;
        let mut opened = shell.receive_opened().await.unwrap();
        let mut input = shell.receive_input().await.unwrap();
        let mut closed = shell.receive_closed().await.unwrap();
        // Without a deliverer the key reports Unavailable.
        assert!(named(
            f.bridge(&f.kwin).await.request().await.unwrap_err(),
            "Unavailable"
        ));

        let (generation, session) = f.open().await;
        assert_eq!(
            opened.next().await.unwrap().args().unwrap().session(),
            &session
        );
        let bridge = f.bridge(&f.deliverer).await;
        shell.type_text(&session, "Привет").await.unwrap();
        shell.press(&session, "backspace").await.unwrap();
        assert_eq!(
            next(&bridge, &generation).await,
            Record::Type(session.clone(), "Привет".into())
        );
        assert_eq!(
            next(&bridge, &generation).await,
            Record::Press(session.clone(), "backspace".into())
        );
        bridge.input(&generation, &session, "left").await.unwrap();
        let key = input.next().await.unwrap();
        assert_eq!(
            (key.args().unwrap().session(), key.args().unwrap().key()),
            (&session.as_str(), &"left")
        );
        // Back in the shell: the deliverer is told, the shell hears Closed.
        shell.close(&session).await.unwrap();
        assert_eq!(
            next(&bridge, &generation).await,
            Record::Close(session.clone())
        );
        assert_eq!(
            closed.next().await.unwrap().args().unwrap().session(),
            &session
        );
        // The ended session takes nothing more.
        assert!(named(
            shell.type_text(&session, "a").await.unwrap_err(),
            "Conflict"
        ));
        assert!(named(shell.close(&session).await.unwrap_err(), "Conflict"));
        assert!(named(
            bridge.input(&generation, &session, "up").await.unwrap_err(),
            "Conflict"
        ));
        // No typed text and no key name ever reached a log record.
        let logged = LOGGED.lock().unwrap().join("\n");
        for secret in ["Привет", "backspace", "\"left\""] {
            assert!(!logged.contains(secret), "{secret} logged");
        }
        drop(f.bus);
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn the_key_toggles_and_the_deliverer_ends_sessions() {
    within(async {
        let f = Fixture::new().await;
        let shell = f.shell(&f.shell).await;
        let mut closed = shell.receive_closed().await.unwrap();
        let bridge = f.bridge(&f.deliverer).await;
        let (generation, session) = f.open().await;
        // A second press hides the panel and tells the deliverer.
        f.bridge(&f.kwin).await.request().await.unwrap();
        assert_eq!(
            next(&bridge, &generation).await,
            Record::Close(session.clone())
        );
        assert_eq!(
            closed.next().await.unwrap().args().unwrap().session(),
            &session
        );

        // The target lost focus: the deliverer ends the session.
        f.bridge(&f.kwin).await.request().await.unwrap();
        let Record::Open(second) = next(&bridge, &generation).await else {
            panic!("open expected");
        };
        bridge.opened(&generation, &second).await.unwrap();
        shell.type_text(&second, "x").await.unwrap();
        bridge.closed(&generation, &second, "focus").await.unwrap();
        assert_eq!(
            closed.next().await.unwrap().args().unwrap().session(),
            &second
        );
        // Its waiting text is dropped and nothing is sent after the end.
        assert_eq!(
            Record::parse(&bridge.next(&generation).await.unwrap()).unwrap(),
            Record::Idle
        );
        assert!(named(
            shell.type_text(&second, "y").await.unwrap_err(),
            "Conflict"
        ));
        assert!(named(
            bridge
                .closed(&generation, &second, "focus")
                .await
                .unwrap_err(),
            "Conflict"
        ));
        assert!(
            bridge
                .closed(&generation, &second, "timeout")
                .await
                .is_err()
        );
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn only_trusted_callers_reach_either_side() {
    within(async {
        let f = Fixture::new().await;
        let stranger = f.bus.connect().await;
        // The key comes only from the platform owner.
        assert!(denied(
            f.bridge(&stranger).await.request().await.unwrap_err()
        ));
        assert!(denied(
            f.bridge(&f.deliverer).await.request().await.unwrap_err()
        ));
        // The deliverer lane belongs to the input method KWin started.
        assert!(denied(
            f.bridge(&stranger)
                .await
                .connect(&Uuid::new_v4().to_string())
                .await
                .unwrap_err()
        ));
        assert!(denied(
            f.bridge(&f.kwin)
                .await
                .connect(&Uuid::new_v4().to_string())
                .await
                .unwrap_err()
        ));
        let (generation, session) = f.open().await;
        assert!(denied(
            f.bridge(&stranger)
                .await
                .next(&generation)
                .await
                .unwrap_err()
        ));
        // Neither the platform adapter, the deliverer nor a provider types.
        for caller in [&f.kwin, &f.deliverer] {
            assert!(denied(
                f.shell(caller)
                    .await
                    .type_text(&session, "a")
                    .await
                    .unwrap_err()
            ));
        }
        let provider = f.bus.connect().await;
        let owner = provider.unique_name().unwrap().to_string();
        f.core
            .state
            .lock()
            .await
            .providers
            .insert("plugin".into(), test_provider("plugin", &owner));
        let as_provider = f.shell(&provider).await;
        assert!(denied(
            as_provider.type_text(&session, "a").await.unwrap_err()
        ));
        assert!(denied(
            as_provider.press(&session, "return").await.unwrap_err()
        ));
        assert!(denied(as_provider.close(&session).await.unwrap_err()));
        // The shell may; bad values are rejected before anything queues.
        let shell = f.shell(&f.shell).await;
        assert!(named(
            shell.type_text(&session, "a\nb").await.unwrap_err(),
            "InvalidArgument"
        ));
        assert!(named(
            shell.press(&session, "tab").await.unwrap_err(),
            "InvalidArgument"
        ));
        shell.type_text(&session, "a").await.unwrap();
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn an_input_method_kwin_did_not_start_is_not_trusted() {
    within(async {
        let f = Fixture::new().await;
        f.core.keyboard.trust_children(|_, _| false).await;
        assert!(denied(
            f.bridge(&f.deliverer)
                .await
                .connect(&Uuid::new_v4().to_string())
                .await
                .unwrap_err()
        ));
        // The production check: this test process did not start the bus's
        // KWin connection, which lives in this same process.
        f.core
            .keyboard
            .trust_children(crate::platform::is_direct_child)
            .await;
        assert!(denied(
            f.bridge(&f.deliverer)
                .await
                .connect(&Uuid::new_v4().to_string())
                .await
                .unwrap_err()
        ));
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn timeouts_replacement_and_the_queue_bound_end_or_refuse() {
    within(async {
        let f = Fixture::new().await;
        f.core
            .keyboard
            .set_timeouts(Duration::from_millis(400), Duration::from_millis(1500))
            .await;
        let shell = f.shell(&f.shell).await;
        let mut closed = shell.receive_closed().await.unwrap();
        let bridge = f.bridge(&f.deliverer).await;
        let generation = bridge.connect(&Uuid::new_v4().to_string()).await.unwrap();
        // An unconfirmed session ends and the deliverer hears close.
        f.bridge(&f.kwin).await.request().await.unwrap();
        let Record::Open(session) = next(&bridge, &generation).await else {
            panic!("open expected");
        };
        tokio::time::sleep(Duration::from_millis(800)).await;
        assert_eq!(
            next(&bridge, &generation).await,
            Record::Close(session.clone())
        );
        assert!(named(
            bridge.opened(&generation, &session).await.unwrap_err(),
            "Conflict"
        ));

        // The queue holds at most 64 records.
        f.bridge(&f.kwin).await.request().await.unwrap();
        let Record::Open(session) = next(&bridge, &generation).await else {
            panic!("open expected");
        };
        bridge.opened(&generation, &session).await.unwrap();
        for _ in 0..keyboard::MAX_QUEUED {
            shell.type_text(&session, "a").await.unwrap();
        }
        assert!(named(
            shell.type_text(&session, "a").await.unwrap_err(),
            "Unavailable"
        ));
        // At most one poll waits per deliverer.
        bridge.next(&generation).await.unwrap();

        // A new deliverer instance replaces the old one and ends the session.
        let generation2 = bridge.connect(&Uuid::new_v4().to_string()).await.unwrap();
        assert_ne!(generation, generation2);
        assert_eq!(
            closed.next().await.unwrap().args().unwrap().session(),
            &session
        );
        assert!(named(
            bridge.next(&generation).await.unwrap_err(),
            "Conflict"
        ));
        assert_eq!(
            Record::parse(&bridge.next(&generation2).await.unwrap()).unwrap(),
            Record::Idle
        );

        // A deliverer that stops polling is gone.
        f.bridge(&f.kwin).await.request().await.unwrap();
        let Record::Open(session) = next(&bridge, &generation2).await else {
            panic!("open expected");
        };
        bridge.opened(&generation2, &session).await.unwrap();
        tokio::time::sleep(Duration::from_millis(2200)).await;
        assert_eq!(
            closed.next().await.unwrap().args().unwrap().session(),
            &session
        );
        assert!(named(
            f.bridge(&f.kwin).await.request().await.unwrap_err(),
            "Unavailable"
        ));
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn a_vanished_deliverer_ends_the_session() {
    within(async {
        let f = Fixture::new().await;
        let shell = f.shell(&f.shell).await;
        let mut closed = shell.receive_closed().await.unwrap();
        let (_, session) = f.open().await;
        drop(f.deliverer);
        assert_eq!(
            closed.next().await.unwrap().args().unwrap().session(),
            &session
        );
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn two_polls_at_once_conflict() {
    within(async {
        let f = Fixture::new().await;
        let bridge = f.bridge(&f.deliverer).await;
        let generation = bridge.connect(&Uuid::new_v4().to_string()).await.unwrap();
        let (first, second) = tokio::join!(bridge.next(&generation), async {
            tokio::time::sleep(Duration::from_millis(200)).await;
            bridge.next(&generation).await
        });
        assert_eq!(first.unwrap(), Vec::<String>::new());
        assert!(named(second.unwrap_err(), "Conflict"));
    })
    .await
    .unwrap();
}

fn test_provider(id: &str, owner: &str) -> Provider {
    Provider {
        info: ProviderInfo {
            id: id.into(),
            plugin_id: id.into(),
            source_path: String::new(),
            name: id.into(),
            version: "0.0.1".into(),
            path: format!("/org/arinn/Daemon1/providers/p_{id}")
                .try_into()
                .unwrap(),
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
        unit: format!("{id}.service"),
        token_file: std::env::temp_dir().join(format!("{id}.token")),
        token: String::new(),
        owner: owner.into(),
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
        started: std::time::Instant::now(),
        enabled: true,
    }
}
