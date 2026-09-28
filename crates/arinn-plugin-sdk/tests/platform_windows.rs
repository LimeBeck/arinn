// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Executable contract fixture, not a production platform backend.
use arinn_plugin_sdk::platform::WindowClient;
use arinn_protocol::{
    platform::{self, Window, WindowPage},
    production::{self, Error, OperationInfo},
    validation::ErrorCode,
};
use futures_util::StreamExt;
use std::{
    collections::HashMap,
    io::{BufRead, BufReader, Write},
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use zbus::{Connection, message::Header, object_server::SignalEmitter, zvariant::OwnedObjectPath};

struct Bus(Child, String);
impl Bus {
    fn start() -> Self {
        let mut child = Command::new("dbus-daemon")
            .args(["--session", "--nofork", "--print-address=1"])
            .stdout(Stdio::piped())
            .spawn()
            .expect("private dbus-daemon");
        let mut address = String::new();
        BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut address)
            .unwrap();
        Self(child, address.trim().into())
    }
    async fn connection(&self) -> Connection {
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

#[derive(Clone)]
struct Lease {
    owner: String,
    page: WindowPage,
    expires: Instant,
}
#[derive(Clone)]
struct CommandRecord {
    content: (String, String, bool),
    path: OwnedObjectPath,
}
struct State {
    owner: String,
    pages: Vec<WindowPage>,
    leases: HashMap<String, Lease>,
    commands: HashMap<(String, String), CommandRecord>,
    operations: HashMap<String, OperationInfo>,
    unavailable: bool,
    command_error: String,
    pending: bool,
    delay: Duration,
    calls: usize,
}
#[derive(Clone)]
struct Fixture(Arc<Mutex<State>>);
impl Fixture {
    fn new(owner: String) -> Self {
        Self(Arc::new(Mutex::new(State {
            owner,
            pages: pages(),
            leases: HashMap::new(),
            commands: HashMap::new(),
            operations: HashMap::new(),
            unavailable: false,
            command_error: String::new(),
            pending: false,
            delay: Duration::ZERO,
            calls: 0,
        })))
    }
    fn authorize(&self, header: &Header<'_>) -> production::Result<String> {
        let caller = header
            .sender()
            .ok_or(ErrorCode::PermissionDenied)?
            .to_string();
        if self.0.lock().unwrap().owner != caller {
            return Err(ErrorCode::PermissionDenied.into());
        }
        Ok(caller)
    }
    async fn command(
        &self,
        generation: &str,
        token: &str,
        request_id: &str,
        close: bool,
        header: Header<'_>,
        connection: &Connection,
    ) -> production::Result<OwnedObjectPath> {
        let owner = self.authorize(&header)?;
        platform::validate_command(generation, token, request_id)?;
        let path = {
            let mut state = self.0.lock().unwrap();
            state.calls += 1;
            let key = (owner, request_id.to_owned());
            let content = (generation.to_owned(), token.to_owned(), close);
            if let Some(command) = state.commands.get(&key) {
                if command.content != content {
                    return Err(ErrorCode::Conflict.into());
                }
                return Ok(command.path.clone());
            }
            if state.unavailable {
                return Err(ErrorCode::Unavailable.into());
            }
            if generation != state.pages[0].generation {
                return Err(ErrorCode::Conflict.into());
            }
            let window = state
                .pages
                .iter()
                .flat_map(|p| &p.windows)
                .find(|w| w.token == token)
                .ok_or(ErrorCode::NotFound)?;
            if (close && !window.can_close) || (!close && !window.can_activate) {
                return Err(ErrorCode::Unsupported.into());
            }
            let path: OwnedObjectPath = format!(
                "/org/arinn/Daemon1/operations/o_{}",
                uuid::Uuid::new_v4().simple()
            )
            .try_into()
            .unwrap();
            let error = state.command_error.clone();
            let status = if state.pending {
                "running"
            } else if error.is_empty() {
                "succeeded"
            } else {
                "failed"
            };
            let operation = OperationInfo {
                state: status.into(),
                error,
                retryable: false,
                started_at: 1,
                ended_at: if state.pending { 0 } else { 2 },
                results: vec![],
                cancellable: false,
            };
            state.operations.insert(path.to_string(), operation);
            state.commands.insert(
                key,
                CommandRecord {
                    content,
                    path: path.clone(),
                },
            );
            path
        };
        connection
            .object_server()
            .at(path.clone(), Operation(self.clone(), path.to_string()))
            .await?;
        Ok(path)
    }
}
#[zbus::interface(name = "org.arinn.PlatformWindows1")]
impl Fixture {
    async fn get_windows(
        &self,
        token: &str,
        #[zbus(header)] header: Header<'_>,
    ) -> production::Result<(WindowPage,)> {
        let owner = self.authorize(&header)?;
        let delay = self.0.lock().unwrap().delay;
        tokio::time::sleep(delay).await;
        let mut state = self.0.lock().unwrap();
        if state.unavailable {
            return Err(ErrorCode::Unavailable.into());
        }
        if !token.is_empty() {
            let lease = state.leases.get(token).ok_or(ErrorCode::Conflict)?;
            if lease.owner != owner {
                return Err(ErrorCode::PermissionDenied.into());
            }
            if lease.expires <= Instant::now() {
                return Err(ErrorCode::Conflict.into());
            }
            return Ok((lease.page.clone(),));
        }
        let pages = state.pages.clone();
        for pair in pages.windows(2) {
            state.leases.insert(
                pair[0].next_token.clone(),
                Lease {
                    owner: owner.clone(),
                    page: pair[1].clone(),
                    expires: Instant::now() + Duration::from_secs(30),
                },
            );
        }
        Ok((pages[0].clone(),))
    }
    async fn activate(
        &self,
        generation: &str,
        token: &str,
        request_id: &str,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] connection: &Connection,
    ) -> production::Result<OwnedObjectPath> {
        self.command(generation, token, request_id, false, header, connection)
            .await
    }
    async fn close(
        &self,
        generation: &str,
        token: &str,
        request_id: &str,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] connection: &Connection,
    ) -> production::Result<OwnedObjectPath> {
        self.command(generation, token, request_id, true, header, connection)
            .await
    }
    #[zbus(signal)]
    async fn windows_changed(
        emitter: &SignalEmitter<'_>,
        generation: &str,
        revision: u64,
    ) -> zbus::Result<()>;
}
struct Operation(Fixture, String);
#[zbus::interface(name = "org.arinn.Operation1")]
impl Operation {
    fn get(&self, #[zbus(header)] header: Header<'_>) -> production::Result<(OperationInfo,)> {
        self.0.authorize(&header)?;
        Ok((self.0.0.lock().unwrap().operations[&self.1].clone(),))
    }
    fn cancel(&self) -> production::Result<()> {
        Err(ErrorCode::Unsupported.into())
    }
    #[zbus(signal)]
    async fn changed(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;
}
fn pages() -> Vec<WindowPage> {
    let generation = uuid::Uuid::new_v4().to_string();
    (0..2)
        .map(|i| WindowPage {
            generation: generation.clone(),
            revision: 7,
            windows: vec![Window {
                token: format!("opaque/window {i}"),
                desktop_id: "fixture-app.desktop".into(),
                output: "fixture-output".into(),
                active: i == 0,
                fullscreen: false,
                can_activate: true,
                can_close: i == 0,
            }],
            next_token: if i == 0 {
                "page-two".into()
            } else {
                String::new()
            },
        })
        .collect()
}
fn assert_error<T: std::fmt::Debug>(result: production::Result<T>, code: &str) {
    let error = result.unwrap_err();
    assert_eq!(
        zbus::DBusError::name(&error).as_str(),
        format!("org.arinn.Error1.{code}")
    );
}
async fn setup() -> (Bus, Connection, Connection, Fixture, WindowClient) {
    let bus = Bus::start();
    let server = bus.connection().await;
    let caller = bus.connection().await;
    let fixture = Fixture::new(caller.unique_name().unwrap().to_string());
    server
        .object_server()
        .at(production::PATH, fixture.clone())
        .await
        .unwrap();
    server.request_name(production::SERVICE).await.unwrap();
    let client = WindowClient::new(&caller).await.unwrap();
    (bus, server, caller, fixture, client)
}

#[tokio::test]
async fn g15_typed_pages_live_xml_and_buffered_signal() {
    let (_bus, server, caller, fixture, client) = setup().await;
    let mut changes = client.proxy().receive_windows_changed().await.unwrap();
    let snapshot = client.snapshot().await.unwrap();
    assert_eq!(snapshot.windows.len(), 2);
    assert_eq!(snapshot.revision, 7); // A late subscriber need not start at revision 1.
    assert_eq!(
        snapshot.windows[0].desktop_id,
        snapshot.windows[1].desktop_id
    );
    assert_ne!(snapshot.windows[0].token, snapshot.windows[1].token);
    let emitter = SignalEmitter::new(&server, production::PATH).unwrap();
    Fixture::windows_changed(&emitter, &snapshot.generation, 8)
        .await
        .unwrap();
    let signal = tokio::time::timeout(Duration::from_secs(1), changes.next())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(signal.args().unwrap().revision, 8);

    let introspection = zbus::fdo::IntrospectableProxy::builder(&caller)
        .destination(server.unique_name().unwrap().clone())
        .unwrap()
        .path(production::PATH)
        .unwrap()
        .build()
        .await
        .unwrap()
        .introspect()
        .await
        .unwrap();
    let script = r#"
import sys, xml.etree.ElementTree as ET
expected = ET.parse(sys.argv[1]).getroot()
actual = ET.fromstring(sys.stdin.read())
def contract(root):
    interface = root.find("interface[@name='org.arinn.PlatformWindows1']")
    assert interface is not None
    return sorted((node.tag, node.attrib['name'], tuple((a.get('direction', 'out' if node.tag == 'signal' else 'in'), a.attrib['type']) for a in node.findall('arg'))) for node in interface if node.tag in ('method', 'signal'))
assert contract(expected) == contract(actual), (contract(expected), contract(actual))
"#;
    let mut process = Command::new("python")
        .args(["-c", script])
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../docs/plugin-api/platform-windows1.xml"
        ))
        .stdin(Stdio::piped())
        .spawn()
        .unwrap();
    process
        .stdin
        .take()
        .unwrap()
        .write_all(introspection.as_bytes())
        .unwrap();
    assert!(process.wait().unwrap().success());

    let (first,) = client.proxy().get_windows("").await.unwrap();
    fixture.0.lock().unwrap().pages = pages();
    let (last,) = client.proxy().get_windows(&first.next_token).await.unwrap();
    assert_eq!(last.generation, first.generation); // Lease survives a replacement.
    fixture
        .0
        .lock()
        .unwrap()
        .leases
        .get_mut(&first.next_token)
        .unwrap()
        .expires = Instant::now();
    assert_error(
        client
            .proxy()
            .get_windows(&first.next_token)
            .await
            .map_err(Error::from),
        "Conflict",
    );
}

#[tokio::test]
async fn g13_client_rejects_mixed_duplicate_cyclic_and_invalid_pages() {
    let (_bus, _server, _caller, fixture, client) = setup().await;
    for fault in 0..7 {
        {
            let mut state = fixture.0.lock().unwrap();
            state.pages = pages();
            match fault {
                0 => state.pages[1].generation = uuid::Uuid::new_v4().to_string(),
                1 => state.pages[1].revision += 1,
                2 => state.pages[1].windows[0].token = state.pages[0].windows[0].token.clone(),
                3 => state.pages[1].next_token = state.pages[0].next_token.clone(),
                4 => state.pages[1].windows[0].active = true,
                5 => state.pages[0].windows.clear(),
                _ => state.pages[1].windows[0].output = "x".repeat(257),
            }
        }
        assert!(client.snapshot().await.is_err(), "accepted fault {fault}");
    }
    fixture.0.lock().unwrap().pages = vec![WindowPage {
        generation: uuid::Uuid::new_v4().to_string(),
        revision: 1,
        windows: vec![],
        next_token: String::new(),
    }];
    assert!(client.snapshot().await.unwrap().windows.is_empty());
}

#[tokio::test]
async fn g21_commands_are_targeted_deduplicated_and_report_real_outcomes() {
    let (bus, server, _caller, fixture, client) = setup().await;
    let snapshot = client.snapshot().await.unwrap();
    let generation = &snapshot.generation;
    let token = &snapshot.windows[0].token;
    let request = uuid::Uuid::new_v4().to_string();
    let operation = client.activate(generation, token, &request).await.unwrap();
    client.wait_operation(&operation).await.unwrap(); // Completed before admission reply.
    assert_eq!(
        client.activate(generation, token, &request).await.unwrap(),
        operation
    );
    assert_error(client.close(generation, token, &request).await, "Conflict");
    assert_eq!(fixture.0.lock().unwrap().commands.len(), 1);
    assert_error(
        client
            .activate(
                &uuid::Uuid::new_v4().to_string(),
                token,
                &uuid::Uuid::new_v4().to_string(),
            )
            .await,
        "Conflict",
    );
    assert_error(
        client
            .activate(generation, "missing", &uuid::Uuid::new_v4().to_string())
            .await,
        "NotFound",
    );
    assert_error(
        client
            .close(
                generation,
                &snapshot.windows[1].token,
                &uuid::Uuid::new_v4().to_string(),
            )
            .await,
        "Unsupported",
    );
    let calls = fixture.0.lock().unwrap().calls;
    assert_error(
        client.activate(generation, token, "bad request").await,
        "InvalidArgument",
    );
    assert_eq!(fixture.0.lock().unwrap().calls, calls);

    fixture.0.lock().unwrap().command_error = "PermissionDenied".into();
    let refused = client
        .close(generation, token, &uuid::Uuid::new_v4().to_string())
        .await
        .unwrap();
    assert_error(client.wait_operation(&refused).await, "PermissionDenied");
    fixture.0.lock().unwrap().command_error.clear();
    let closed = client
        .close(generation, token, &uuid::Uuid::new_v4().to_string())
        .await
        .unwrap();
    client.wait_operation(&closed).await.unwrap();
    assert_eq!(client.snapshot().await.unwrap().windows.len(), 2); // Dispatch does not mean gone.

    let stranger = WindowClient::new(&bus.connection().await).await.unwrap();
    assert_error(stranger.snapshot().await, "PermissionDenied");
    fixture.0.lock().unwrap().unavailable = true;
    assert_error(client.snapshot().await, "Unavailable");
    fixture.0.lock().unwrap().unavailable = false;

    fixture.0.lock().unwrap().pending = true;
    let pending = client
        .activate(generation, token, &uuid::Uuid::new_v4().to_string())
        .await
        .unwrap();
    let wait = client.wait_operation(&pending);
    let complete = async {
        tokio::time::sleep(Duration::from_millis(30)).await;
        fixture
            .0
            .lock()
            .unwrap()
            .operations
            .get_mut(pending.as_str())
            .unwrap()
            .state = "succeeded".into();
        let emitter = SignalEmitter::new(&server, pending.as_str()).unwrap();
        Operation::changed(&emitter).await.unwrap();
    };
    let (result, ()) = tokio::join!(wait, complete);
    result.unwrap();

    // A vanished window's token is never redirected to another window.
    fixture.0.lock().unwrap().pages[0].windows[0].token = "replacement".into();
    assert_error(
        client
            .activate(generation, token, &uuid::Uuid::new_v4().to_string())
            .await,
        "NotFound",
    );
    fixture.0.lock().unwrap().pages = pages();
    assert_error(
        client
            .activate(generation, token, &uuid::Uuid::new_v4().to_string())
            .await,
        "Conflict",
    );
}

#[tokio::test]
async fn g13_read_and_control_waits_are_bounded_without_retries() {
    let (_bus, _server, _caller, fixture, client) = setup().await;
    let snapshot = client.snapshot().await.unwrap();
    fixture.0.lock().unwrap().delay = Duration::from_secs(4);
    assert_error(client.snapshot().await, "Timeout");
    fixture.0.lock().unwrap().pending = true;
    let path = client
        .activate(
            &snapshot.generation,
            &snapshot.windows[0].token,
            &uuid::Uuid::new_v4().to_string(),
        )
        .await
        .unwrap();
    assert_error(client.wait_operation(&path).await, "Timeout");
    assert_eq!(fixture.0.lock().unwrap().commands.len(), 1);
}

#[tokio::test]
async fn g23_daemon_owner_loss_invalidates_clients_and_pending_operations() {
    let (bus, server, caller, fixture, client) = setup().await;
    let snapshot = client.snapshot().await.unwrap();
    fixture.0.lock().unwrap().pending = true;
    let path = client
        .activate(
            &snapshot.generation,
            &snapshot.windows[0].token,
            &uuid::Uuid::new_v4().to_string(),
        )
        .await
        .unwrap();
    let wait = client.wait_operation(&path);
    let replace = async {
        tokio::time::sleep(Duration::from_millis(30)).await;
        server.release_name(production::SERVICE).await.unwrap();
        let replacement = bus.connection().await;
        replacement
            .object_server()
            .at(
                production::PATH,
                Fixture::new(caller.unique_name().unwrap().to_string()),
            )
            .await
            .unwrap();
        replacement.request_name(production::SERVICE).await.unwrap();
        replacement
    };
    let (result, replacement) = tokio::join!(wait, replace);
    assert_error(result, "Unavailable");
    assert_error(client.snapshot().await, "Unavailable");
    let calls = fixture.0.lock().unwrap().calls;
    assert_error(
        client
            .close(
                &snapshot.generation,
                &snapshot.windows[0].token,
                &uuid::Uuid::new_v4().to_string(),
            )
            .await,
        "Unavailable",
    );
    assert_eq!(fixture.0.lock().unwrap().calls, calls);
    let fresh = WindowClient::new(&caller).await.unwrap();
    assert_ne!(
        fresh.snapshot().await.unwrap().generation,
        snapshot.generation
    );
    replacement.release_name(production::SERVICE).await.unwrap();
    assert_error(fresh.snapshot().await, "Unavailable");
}
