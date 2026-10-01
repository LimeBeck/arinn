// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Reference contract fixtures only, not production daemon/platform/consumer code.
use arinn_protocol::{
    browser::BrowserWindow,
    production,
    return_association::{
        KWinReturnProxy, ReturnAssociation, ReturnAssociationApiProxy, WindowRef, validate_report,
    },
    validation::ErrorCode,
};
use futures_util::StreamExt;
use std::{
    io::{BufRead, BufReader, Write},
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex},
    time::Duration,
};
use zbus::{Connection, message::Header, object_server::SignalEmitter};

const EPOCH: &str = "10000000-0000-4000-8000-000000000001";
const PLATFORM: &str = "20000000-0000-4000-8000-000000000002";
const TARGET: &str = "30000000-0000-4000-8000-000000000003";
const TOKEN: &str = "20000000-0000-4000-8000-000000000002:17";
const REMAP: &str = "20000000-0000-4000-8000-000000000002:18";
const PROVIDER: &str = "40000000-0000-4000-8000-000000000004";

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
}
impl Drop for Bus {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

struct State {
    snapshot: ReturnAssociation,
    adapter_owner: String,
    platform: String,
    inventory_revision: i32,
    tokens: Vec<String>,
    home: bool,
}
impl State {
    fn new(owner: &str) -> Self {
        Self {
            snapshot: ReturnAssociation {
                epoch: EPOCH.into(),
                revision: 1,
                target: TARGET.into(),
                window: vec![],
            },
            adapter_owner: owner.into(),
            platform: PLATFORM.into(),
            inventory_revision: 1,
            tokens: vec![TOKEN.into()],
            home: true,
        }
    }
    fn home_report(&mut self, visible: bool, target: &str) {
        self.home = visible;
        self.snapshot.target = target.into();
        self.snapshot.window.clear();
        self.snapshot.revision += 1;
    }
    fn inventory(&mut self, generation: &str, tokens: &[&str]) {
        self.platform = generation.into();
        self.inventory_revision += 1;
        self.tokens = tokens.iter().map(|s| s.to_string()).collect();
        if self
            .snapshot
            .window
            .iter()
            .any(|w| w.generation != generation || !self.tokens.contains(&w.token))
        {
            self.snapshot.window.clear();
            self.snapshot.revision += 1;
        }
    }
    fn report(
        &mut self,
        generation: &str,
        revision: i32,
        target: &str,
        token: &str,
    ) -> production::Result<()> {
        validate_report(generation, revision, target, token)?;
        if !self.home {
            return Err(ErrorCode::Unavailable.into());
        }
        if generation != self.platform
            || revision != self.inventory_revision
            || target != self.snapshot.target
        {
            return Err(ErrorCode::Conflict.into());
        }
        if !token.is_empty() && !self.tokens.iter().any(|t| t == token) {
            return Err(ErrorCode::NotFound.into());
        }
        let window = if token.is_empty() {
            vec![]
        } else {
            vec![WindowRef {
                generation: generation.into(),
                token: token.into(),
            }]
        };
        if window != self.snapshot.window {
            self.snapshot.window = window;
            self.snapshot.revision += 1;
        }
        Ok(())
    }
}
// A legacy root exists, but has no optional association API.
struct Legacy;
#[zbus::interface(name = "org.arinn.Daemon1")]
impl Legacy {
    fn status(&self) -> (arinn_protocol::Status,) {
        (arinn_protocol::Status {
            epoch: EPOCH.into(),
            provider_available: true,
            generation: EPOCH.into(),
            revision: 0,
            home_visible: true,
            return_window: TARGET.into(),
            active_window: String::new(),
            active_class: String::new(),
            active_fullscreen: false,
            platform_connected: true,
            home_output: String::new(),
            active_output: String::new(),
        },)
    }
}

struct Public(Arc<Mutex<State>>);
#[zbus::interface(name = "org.arinn.ReturnAssociation1")]
impl Public {
    fn get(&self) -> (ReturnAssociation,) {
        (self.0.lock().unwrap().snapshot.clone(),)
    }
    #[zbus(signal)]
    async fn changed(emitter: &SignalEmitter<'_>, epoch: &str, revision: u64) -> zbus::Result<()>;
}
struct Bridge(Arc<Mutex<State>>);
#[zbus::interface(name = "org.arinn.KWinReturn1")]
impl Bridge {
    async fn report(
        &self,
        generation: &str,
        inventory_revision: i32,
        target: &str,
        token: &str,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] connection: &Connection,
    ) -> production::Result<()> {
        let owner = zbus::fdo::DBusProxy::new(connection)
            .await?
            .get_name_owner("org.kde.KWin".try_into().unwrap())
            .await
            .map_err(|_| ErrorCode::PermissionDenied)?;
        if header.sender().map(|s| s.as_str()) != Some(owner.as_str())
            || owner.as_str() != self.0.lock().unwrap().adapter_owner
        {
            return Err(ErrorCode::PermissionDenied.into());
        }
        let (before, value) = {
            let mut state = self.0.lock().unwrap();
            let before = state.snapshot.revision;
            state.report(generation, inventory_revision, target, token)?;
            (before, state.snapshot.clone())
        };
        if before != value.revision {
            let emitter = SignalEmitter::new(connection, production::PATH)?;
            Public::changed(&emitter, &value.epoch, value.revision).await?;
        }
        Ok(())
    }
}
fn error(error: zbus::Error, expected: &str) {
    match error {
        zbus::Error::MethodError(name, _, _) => {
            assert_eq!(name.as_str(), format!("org.arinn.Error1.{expected}"))
        }
        other => panic!("unexpected error: {other}"),
    }
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
    assert!(child.wait().unwrap().success());
}
#[tokio::test]
async fn live_xml_optional_absence_authentication_and_revision_signal() {
    tokio::time::timeout(Duration::from_secs(15), live_contract())
        .await
        .unwrap();
}
async fn live_contract() {
    let bus = Bus::new();
    let server = bus.connect().await;
    let adapter = bus.connect().await;
    let client = bus.connect().await;
    server
        .object_server()
        .at(production::PATH, Legacy)
        .await
        .unwrap();
    server.request_name(production::SERVICE).await.unwrap();
    adapter.request_name("org.kde.KWin").await.unwrap();
    let owner = server.unique_name().unwrap().to_string();
    let api = ReturnAssociationApiProxy::builder(&client)
        .destination(owner.as_str())
        .unwrap()
        .build()
        .await
        .unwrap();
    // Old daemon has no optional interface: callers keep the existing Return.
    assert!(api.get().await.is_err());
    assert_eq!(
        production::ProductionDaemonProxy::new(&client)
            .await
            .unwrap()
            .status()
            .await
            .unwrap()
            .0
            .return_window,
        TARGET
    );
    let state = Arc::new(Mutex::new(State::new(
        adapter.unique_name().unwrap().as_str(),
    )));
    server
        .object_server()
        .at(production::PATH, Public(state.clone()))
        .await
        .unwrap();
    server
        .object_server()
        .at(production::PATH, Bridge(state.clone()))
        .await
        .unwrap();
    let xml = zbus::fdo::IntrospectableProxy::builder(&client)
        .destination(owner.as_str())
        .unwrap()
        .path(production::PATH)
        .unwrap()
        .build()
        .await
        .unwrap()
        .introspect()
        .await
        .unwrap();
    compare_xml(
        &xml,
        "return-association1.xml",
        "org.arinn.ReturnAssociation1",
    );
    compare_xml(&xml, "kwin-return1.xml", "org.arinn.KWinReturn1");
    let mut changes = api.receive_changed().await.unwrap();
    let initial = api.get().await.unwrap().0;
    initial.validate().unwrap();
    assert!(initial.window.is_empty());
    assert_eq!(initial.target, TARGET);
    let bridge = KWinReturnProxy::builder(&adapter)
        .destination(owner.as_str())
        .unwrap()
        .build()
        .await
        .unwrap();
    let untrusted = KWinReturnProxy::new(&client).await.unwrap();
    error(
        untrusted
            .report(PLATFORM, 1, TARGET, TOKEN)
            .await
            .unwrap_err(),
        "PermissionDenied",
    );
    error(
        bridge.report(EPOCH, 1, TARGET, TOKEN).await.unwrap_err(),
        "Conflict",
    );
    error(
        bridge.report(PLATFORM, 0, TARGET, TOKEN).await.unwrap_err(),
        "InvalidArgument",
    );
    error(
        bridge
            .report(PLATFORM, 1, "other-return", TOKEN)
            .await
            .unwrap_err(),
        "Conflict",
    );
    error(
        bridge
            .report(PLATFORM, 1, TARGET, "unknown")
            .await
            .unwrap_err(),
        "NotFound",
    );
    assert_eq!(api.get().await.unwrap().0, initial);
    bridge.report(PLATFORM, 1, TARGET, TOKEN).await.unwrap();
    let event = tokio::time::timeout(Duration::from_secs(2), changes.next())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(event.args().unwrap().epoch, EPOCH);
    assert_eq!(event.args().unwrap().revision, 2);
    let value = api.get().await.unwrap().0;
    value.validate().unwrap();
    assert_eq!(
        value.window[0],
        WindowRef {
            generation: PLATFORM.into(),
            token: TOKEN.into()
        }
    );
    assert_ne!(value.target, value.window[0].token);
    assert_ne!(value.epoch, value.window[0].generation);
    // Wrong/obsolete inventory cannot overwrite a valid association.
    error(
        bridge.report(PLATFORM, 2, TARGET, "").await.unwrap_err(),
        "Conflict",
    );
    assert_eq!(api.get().await.unwrap().0, value);
    bridge.report(PLATFORM, 1, TARGET, "").await.unwrap();
    assert!(api.get().await.unwrap().0.window.is_empty());
    assert_eq!(api.get().await.unwrap().0.target, TARGET);
    adapter.release_name("org.kde.KWin").await.unwrap();
    client.request_name("org.kde.KWin").await.unwrap();
    error(
        bridge.report(PLATFORM, 1, TARGET, TOKEN).await.unwrap_err(),
        "PermissionDenied",
    );
    // Even the new KWin owner must establish its own inventory session first.
    error(
        untrusted
            .report(PLATFORM, 1, TARGET, TOKEN)
            .await
            .unwrap_err(),
        "PermissionDenied",
    );
    // A unique-owner-pinned consumer does not follow a replacement daemon.
    server.release_name(production::SERVICE).await.unwrap();
    let replacement = bus.connect().await;
    replacement
        .object_server()
        .at(production::PATH, Legacy)
        .await
        .unwrap();
    replacement.request_name(production::SERVICE).await.unwrap();
    assert_ne!(replacement.unique_name().unwrap().as_str(), owner);
    assert!(
        ReturnAssociationApiProxy::new(&client)
            .await
            .unwrap()
            .get()
            .await
            .is_err()
    );
}

/// Consumer reference model: joins only complete current provider snapshots;
/// never identifies anything from `focused`, a title, or a Return UUID itself.
fn matched_window<'a>(
    value: &ReturnAssociation,
    current_epoch: &str,
    target: &str,
    received_owner: &str,
    current_owner: &str,
    providers: &'a [(&str, &str, Vec<BrowserWindow>)],
) -> Option<&'a str> {
    if value.validate().is_err()
        || received_owner != current_owner
        || value.epoch != current_epoch
        || value.target != target
        || providers
            .iter()
            .any(|(expected, actual, _)| expected != actual)
    {
        return None;
    }
    let window = value.window.first()?;
    let mut matches = providers
        .iter()
        .flat_map(|(_, _, windows)| windows)
        .filter(|w| w.window_token == window.token);
    let first = matches.next()?;
    if matches.next().is_some() {
        None
    } else {
        Some(first.id.as_str())
    }
}
#[test]
fn identity_lifetimes_ambiguity_and_focus_loss_are_conservative() {
    let mut state = State::new(":1.1");
    state.report(PLATFORM, 1, TARGET, TOKEN).unwrap();
    let window = BrowserWindow {
        id: "connection-7:window-5".into(),
        window_token: TOKEN.into(),
        focused: false,
    };
    let providers = vec![(PROVIDER, PROVIDER, vec![window.clone()])];
    let matched = |value: &ReturnAssociation| {
        matched_window(value, EPOCH, TARGET, ":1.2", ":1.2", &providers)
    };
    assert_eq!(
        matched(&state.snapshot),
        Some(window.id.as_str()),
        "Home removed focus, not identity"
    );
    state.inventory(PLATFORM, &[TOKEN]); // Focus-only inventory update preserves the explicit identity.
    assert_eq!(matched(&state.snapshot), Some(window.id.as_str()));
    assert!(
        state.report(PLATFORM, 1, TARGET, TOKEN).is_err(),
        "old committed revision rejected"
    );
    state.inventory(PLATFORM, &[]);
    assert_eq!(matched(&state.snapshot), None);
    assert_eq!(state.snapshot.target, TARGET);
    state.inventory(PLATFORM, &[REMAP]);
    assert_eq!(
        matched(&state.snapshot),
        None,
        "remap never redirects old token"
    );
    assert!(
        state
            .report(PLATFORM, state.inventory_revision, TARGET, TOKEN)
            .is_err()
    );
    state
        .report(PLATFORM, state.inventory_revision, TARGET, REMAP)
        .unwrap();
    assert_eq!(
        matched(&state.snapshot),
        None,
        "old provider token cannot match fresh mapping"
    );
    let remapped = BrowserWindow {
        window_token: REMAP.into(),
        ..window.clone()
    };
    let fresh = vec![(PROVIDER, PROVIDER, vec![remapped.clone()])];
    assert_eq!(
        matched_window(&state.snapshot, EPOCH, TARGET, ":1.2", ":1.2", &fresh),
        Some(window.id.as_str())
    );
    let restarted = vec![(
        "60000000-0000-4000-8000-000000000006",
        PROVIDER,
        vec![remapped.clone()],
    )];
    assert_eq!(
        matched_window(&state.snapshot, EPOCH, TARGET, ":1.2", ":1.2", &restarted),
        None
    );
    let ambiguous = vec![(
        PROVIDER,
        PROVIDER,
        vec![
            remapped.clone(),
            BrowserWindow {
                id: "different-window".into(),
                ..remapped
            },
        ],
    )];
    assert_eq!(
        matched_window(&state.snapshot, EPOCH, TARGET, ":1.2", ":1.2", &ambiguous),
        None
    );
    assert_eq!(
        matched_window(&state.snapshot, PLATFORM, TARGET, ":1.2", ":1.2", &fresh),
        None,
        "new daemon epoch"
    );
    assert_eq!(
        matched_window(&state.snapshot, EPOCH, TARGET, ":1.2", ":1.3", &fresh),
        None,
        "late old-owner reply"
    );
    assert_eq!(
        matched_window(
            &state.snapshot,
            EPOCH,
            "another-return",
            ":1.2",
            ":1.2",
            &fresh
        ),
        None
    );
    state.inventory("50000000-0000-4000-8000-000000000005", &[REMAP]);
    assert!(
        state.snapshot.window.is_empty(),
        "generation change invalidates even same string token"
    );
    assert_eq!(state.snapshot.target, TARGET);
}

#[test]
fn context_refresh_dismissal_and_cross_provider_ambiguity_never_remove_return() {
    let mut state = State::new(":1.1");
    state.report(PLATFORM, 1, TARGET, TOKEN).unwrap();
    state.home_report(true, TARGET);
    assert!(
        state.snapshot.window.is_empty(),
        "even same-target Home refresh needs fresh evidence"
    );
    assert_eq!(state.snapshot.target, TARGET);
    state.report(PLATFORM, 1, TARGET, TOKEN).unwrap();
    let one = BrowserWindow {
        id: "provider-local-window".into(),
        window_token: TOKEN.into(),
        focused: false,
    };
    let providers = vec![
        (PROVIDER, PROVIDER, vec![one.clone()]),
        (
            "60000000-0000-4000-8000-000000000006",
            "60000000-0000-4000-8000-000000000006",
            vec![one],
        ),
    ];
    assert_eq!(
        matched_window(&state.snapshot, EPOCH, TARGET, ":1.2", ":1.2", &providers),
        None
    );
    let incomplete = vec![
        (
            PROVIDER,
            PROVIDER,
            vec![BrowserWindow {
                id: "known".into(),
                window_token: TOKEN.into(),
                focused: false,
            }],
        ),
        ("60000000-0000-4000-8000-000000000006", PROVIDER, vec![]),
    ];
    assert_eq!(
        matched_window(&state.snapshot, EPOCH, TARGET, ":1.2", ":1.2", &incomplete),
        None,
        "another ready provider awaiting a current snapshot leaves ambiguity unresolved"
    );
    state.home_report(false, "");
    assert!(state.snapshot.window.is_empty());
    assert!(
        state.report(PLATFORM, 1, "", "").is_err(),
        "hidden Home cannot admit a report"
    );
    state.home_report(true, "different-return");
    assert!(state.report(PLATFORM, 1, TARGET, TOKEN).is_err());
    assert_eq!(state.snapshot.target, "different-return");
}

#[test]
fn hidden_home_precedes_obsolete_context_in_reference_report() {
    let mut state = State::new(":1.2");
    state.home_report(false, "");
    let before = state.snapshot.clone();
    assert!(matches!(
        state.report(EPOCH, 99, TARGET, TOKEN),
        Err(production::Error::Unavailable(_))
    ));
    assert_eq!(state.snapshot, before);
}
