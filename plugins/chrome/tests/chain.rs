// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The real `arinn-chrome-host` between a fake extension (this test, on the
//! host's stdin/stdout) and the provider's bridge on a private bus.
use arinn_plugin_chrome::{
    BRIDGE_NAME, BRIDGE_PATH, EXTENSION_ID,
    bridge::{Bridge, HostProxy},
    framing,
    processes::Processes,
};
use arinn_plugin_sdk::runtime::RegistrationContext;
use arinn_protocol::{media_association::BrowserProcessProxy, production, validation::ErrorCode};
use std::{
    io::{BufRead, BufReader},
    process::{Child, Command as StdCommand, Stdio},
    time::Duration,
};
use tokio::process::{Child as TokioChild, ChildStdin, ChildStdout, Command};

#[derive(Clone)]
struct FakeWindows {
    page: std::sync::Arc<std::sync::Mutex<arinn_protocol::platform::WindowPage>>,
    raised: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
    delay: std::sync::Arc<std::sync::Mutex<Duration>>,
}
#[zbus::interface(name = "org.arinn.PlatformWindows1")]
impl FakeWindows {
    async fn get_windows(&self, _token: &str) -> (arinn_protocol::platform::WindowPage,) {
        let delay = *self.delay.lock().unwrap();
        tokio::time::sleep(delay).await;
        (self.page.lock().unwrap().clone(),)
    }
    fn activate(
        &self,
        generation: &str,
        token: &str,
        request_id: &str,
    ) -> production::Result<zbus::zvariant::OwnedObjectPath> {
        arinn_protocol::platform::validate_command(generation, token, request_id)?;
        let page = self.page.lock().unwrap();
        if page.generation != generation {
            return Err(ErrorCode::Conflict.into());
        }
        if !page.windows.iter().any(|w| w.token == token) {
            return Err(ErrorCode::NotFound.into());
        }
        self.raised.lock().unwrap().push(token.into());
        Ok("/org/arinn/Daemon1/operations/open".try_into().unwrap())
    }
}
struct OpenDone(std::sync::Arc<std::sync::Mutex<Duration>>);
#[zbus::interface(name = "org.arinn.Operation1")]
impl OpenDone {
    async fn get(&self) -> (production::OperationInfo,) {
        let delay = *self.0.lock().unwrap();
        tokio::time::sleep(delay).await;
        (production::OperationInfo {
            state: "succeeded".into(),
            error: String::new(),
            retryable: false,
            started_at: 1,
            ended_at: 2,
            results: Vec::new(),
            cancellable: false,
        },)
    }
}

struct SelectedHost {
    started: std::sync::Arc<tokio::sync::Notify>,
    answer: std::sync::Arc<tokio::sync::Notify>,
}
#[zbus::interface(name = "org.arinn.ChromeHost1")]
impl SelectedHost {
    async fn command(&self, _action: &str, _tab: i64, _url: &str) -> (String, i64, bool) {
        self.started.notify_one();
        self.answer.notified().await;
        (String::new(), 7, false)
    }
}

#[tokio::test]
async fn open_result_keeps_the_exact_connection_across_repeated_host_attach() {
    use arinn_plugin_chrome::{HOST_PATH, bridge::BridgeClientProxy};
    use std::sync::Arc;
    let bus = Bus::start();
    let provider = bus.connection().await;
    let bridge = Bridge::default();
    provider
        .object_server()
        .at(BRIDGE_PATH, bridge.clone())
        .await
        .unwrap();
    provider.request_name(BRIDGE_NAME).await.unwrap();
    let host = bus.connection().await;
    let started = Arc::new(tokio::sync::Notify::new());
    let answer = Arc::new(tokio::sync::Notify::new());
    host.object_server()
        .at(
            HOST_PATH,
            SelectedHost {
                started: started.clone(),
                answer: answer.clone(),
            },
        )
        .await
        .unwrap();
    let client = BridgeClientProxy::new(&host).await.unwrap();
    let first = client.attach().await.unwrap();
    let selected = client.attach().await.unwrap();
    assert_ne!(first, selected);
    client
        .publish(&selected, vec![(7, true)], vec![])
        .await
        .unwrap();
    for vanished in [false, true] {
        let request = {
            let (bridge, provider) = (bridge.clone(), provider.clone());
            tokio::spawn(async move { bridge.open(&provider, "https://service.example/").await })
        };
        started.notified().await;
        if vanished {
            bridge.drop_host(host.unique_name().unwrap().as_str());
        }
        client.attach().await.unwrap();
        answer.notify_one();
        let result = request.await.unwrap();
        if vanished {
            assert_eq!(result, Err(ErrorCode::Unavailable));
        } else {
            assert_eq!(result, Ok((format!("{selected}:w7"), false)));
        }
    }
}

#[tokio::test]
async fn shortcut_results_raise_the_selected_window_with_stale_browser_focus() {
    use arinn_plugin_chrome::opening::OpenWindows;
    use arinn_protocol::{
        browser::BrowserWindow,
        platform::{Window, WindowPage},
    };
    use std::sync::{Arc, Mutex};
    let bus = Bus::start();
    let provider = bus.connection().await;
    let bridge = Bridge::default();
    provider
        .object_server()
        .at(BRIDGE_PATH, bridge.clone())
        .await
        .unwrap();
    provider.request_name(BRIDGE_NAME).await.unwrap();
    let daemon = bus.connection().await;
    daemon.request_name(production::SERVICE).await.unwrap();
    let generation = uuid::Uuid::new_v4().to_string();
    let platform = FakeWindows {
        page: Arc::new(Mutex::new(WindowPage {
            generation: generation.clone(),
            revision: 1,
            windows: ["screen-a", "screen-b"]
                .into_iter()
                .map(|token| Window {
                    token: token.into(),
                    desktop_id: String::new(),
                    output: String::new(),
                    active: false,
                    fullscreen: true,
                    can_activate: true,
                    can_close: true,
                })
                .collect(),
            next_token: String::new(),
        })),
        raised: Arc::default(),
        delay: Arc::default(),
    };
    daemon
        .object_server()
        .at(production::PATH, platform.clone())
        .await
        .unwrap();
    let completion_delay = Arc::new(Mutex::new(Duration::ZERO));
    daemon
        .object_server()
        .at(
            "/org/arinn/Daemon1/operations/open",
            OpenDone(completion_delay.clone()),
        )
        .await
        .unwrap();
    let context = RegistrationContext {
        connection: provider.clone(),
        daemon_owner: daemon.unique_name().unwrap().to_string(),
        daemon_epoch: uuid::Uuid::new_v4().to_string(),
        generation: uuid::Uuid::new_v4().to_string(),
    };
    let (registrations, current) = tokio::sync::watch::channel(Some(context.clone()));
    let opening = Arc::new(OpenWindows::default());
    opening.bind(current);
    opening.publish(
        Some(&context),
        &generation,
        &[
            BrowserWindow {
                id: "c1:w1".into(),
                window_token: "screen-a".into(),
                focused: false,
            },
            BrowserWindow {
                id: "c1:w2".into(),
                window_token: "screen-b".into(),
                focused: true,
            },
        ],
    );
    let mut child = host(&bus, &origin());
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = child.stdout.take().unwrap();
    until(|| bridge.snapshot().2).await;
    send(&mut stdin, r#"{"type":"snapshot","windows":[{"id":1,"focused":false},{"id":2,"focused":true}],"tabs":[]}"#).await;
    until(|| bridge.snapshot().0.len() == 2).await;

    // Native Messaging result -> host -> provider -> PlatformWindows1. Focus
    // stays on B throughout; a newly created C has no guessed platform target.
    for (id, created) in [
        (1, false),
        (2, false),
        (1, false),
        (3, true),
        (1, false),
        (2, false),
    ] {
        let request = {
            let (opening, provider, bridge) = (opening.clone(), provider.clone(), bridge.clone());
            tokio::spawn(async move {
                opening
                    .open(&provider, &bridge, "https://service.example/")
                    .await
            })
        };
        let message = receive(&mut stdout).await;
        let serial = message["serial"].as_u64().unwrap();
        send(&mut stdin, &format!(r#"{{"type":"result","serial":{serial},"error":"","windowId":{id},"created":{created}}}"#)).await;
        assert_eq!(request.await.unwrap(), Ok(()));
    }
    assert_eq!(
        *platform.raised.lock().unwrap(),
        ["screen-a", "screen-b", "screen-a", "screen-a", "screen-b"]
    );

    // Every individual stage fits its cap, but their sum exceeds the complete
    // provider budget: native result 4s + snapshot 2s + operation result 3s.
    // The activation already dispatched at ~6s is uncertain, never replayed.
    *platform.delay.lock().unwrap() = Duration::from_secs(2);
    *completion_delay.lock().unwrap() = Duration::from_secs(3);
    let request = {
        let (opening, provider, bridge) = (opening.clone(), provider.clone(), bridge.clone());
        tokio::spawn(async move {
            opening
                .open(&provider, &bridge, "https://service.example/")
                .await
        })
    };
    let message = receive(&mut stdout).await;
    let serial = message["serial"].as_u64().unwrap();
    tokio::time::sleep(Duration::from_secs(4)).await;
    send(
        &mut stdin,
        &format!(
            r#"{{"type":"result","serial":{serial},"error":"","windowId":1,"created":false}}"#
        ),
    )
    .await;
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(6), request)
            .await
            .unwrap()
            .unwrap(),
        Err(ErrorCode::Timeout)
    );
    assert_eq!(platform.raised.lock().unwrap().len(), 6);
    *platform.delay.lock().unwrap() = Duration::ZERO;
    *completion_delay.lock().unwrap() = Duration::ZERO;

    // Reused windows without a settled link fail instead of raising B.
    for case in [
        "unlinked",
        "platform-restarted",
        "registration-changed",
        "old-extension",
        "snapshot-timeout",
        "gone-host",
    ] {
        let request = {
            let (opening, provider, bridge) = (opening.clone(), provider.clone(), bridge.clone());
            tokio::spawn(async move {
                opening
                    .open(&provider, &bridge, "https://service.example/")
                    .await
            })
        };
        let message = receive(&mut stdout).await;
        let serial = message["serial"].as_u64().unwrap();
        let mut id = 1;
        match case {
            "unlinked" => id = 4,
            "platform-restarted" => {
                platform.page.lock().unwrap().generation = uuid::Uuid::new_v4().to_string()
            }
            "registration-changed" => {
                registrations.send_replace(None);
            }
            "old-extension" => id = -1,
            "snapshot-timeout" => *platform.delay.lock().unwrap() = Duration::from_secs(9),
            "gone-host" => bridge.drop_host(&bridge.default_host().unwrap()),
            _ => unreachable!(),
        }
        send(&mut stdin, &format!(r#"{{"type":"result","serial":{serial},"error":"","windowId":{id},"created":false}}"#)).await;
        assert!(request.await.unwrap().is_err(), "{case}");
        platform.page.lock().unwrap().generation = generation.clone();
        *platform.delay.lock().unwrap() = Duration::ZERO;
        registrations.send_replace(Some(context.clone()));
    }
    assert_eq!(platform.raised.lock().unwrap().len(), 6);
}

struct Bus(Child, String);
impl Bus {
    fn start() -> Self {
        let mut child = StdCommand::new("dbus-daemon")
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
    async fn connection(&self) -> zbus::Connection {
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

fn host(bus: &Bus, origin: &str) -> TokioChild {
    Command::new(env!("CARGO_BIN_EXE_arinn-chrome-host"))
        .arg(origin)
        .env("DBUS_SESSION_BUS_ADDRESS", &bus.1)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap()
}
fn origin() -> String {
    format!("chrome-extension://{EXTENSION_ID}/")
}
async fn send(stdin: &mut ChildStdin, json: &str) {
    framing::write(stdin, json.as_bytes()).await.unwrap();
}
async fn receive(stdout: &mut ChildStdout) -> serde_json::Value {
    let frame = tokio::time::timeout(Duration::from_secs(5), framing::read(stdout))
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    serde_json::from_slice(&frame).unwrap()
}
async fn until<F: Fn() -> bool>(condition: F) {
    tokio::time::timeout(Duration::from_secs(5), async {
        while !condition() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("condition timed out");
}

const SNAPSHOT: &str = r#"{"type":"snapshot","windows":[{"id":1,"focused":true}],
  "tabs":[{"id":7,"windowId":1,"title":"Video","url":"https://video.example/","active":true,"audible":true},
          {"id":8,"windowId":1,"title":"Mail","url":"chrome://newtab/","active":false,"audible":false}]}"#;

#[tokio::test(flavor = "multi_thread")]
async fn host_links_the_extension_and_the_provider_bridge() {
    let bus = Bus::start();
    let provider = bus.connection().await;
    let bridge = Bridge::default();
    provider
        .object_server()
        .at(BRIDGE_PATH, bridge.clone())
        .await
        .unwrap();

    // Without the provider the host exits; the extension retries later.
    let status = host(&bus, &origin()).wait().await.unwrap();
    assert!(!status.success());
    provider.request_name(BRIDGE_NAME).await.unwrap();
    // Only the packaged extension may start it.
    let mut stranger = host(&bus, "chrome-extension://aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/");
    assert_eq!(stranger.wait().await.unwrap().code(), Some(2));

    let mut child = host(&bus, &origin());
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = child.stdout.take().unwrap();
    until(|| bridge.snapshot().2).await;
    send(&mut stdin, SNAPSHOT).await;
    until(|| bridge.snapshot().1.len() == 2).await;
    let (windows, tabs, _) = bridge.snapshot();
    assert_eq!(windows[0].id, "c1:w1");
    assert!(windows[0].focused);
    // ADR-0096: the process that started the host stands for the browser.
    assert_eq!(bridge.browser_process("c1:w1").unwrap(), std::process::id());
    // The exported interface answers the registered daemon only, and checks
    // the caller before the arguments. Before registration every caller is
    // refused alike.
    let daemon = bus.connection().await;
    let idle = bus.connection().await;
    let (_unregistered, nobody) = tokio::sync::watch::channel(None);
    idle.object_server()
        .at(
            production::PROVIDER_PATH,
            Processes::new(bridge.clone(), nobody),
        )
        .await
        .unwrap();
    let early = BrowserProcessProxy::builder(&daemon)
        .destination(idle.unique_name().unwrap().to_string())
        .unwrap()
        .build()
        .await
        .unwrap();
    match early
        .get_process_id("30000000-0000-4000-8000-000000000003", "c1:w1")
        .await
        .unwrap_err()
    {
        zbus::Error::MethodError(name, _, _) => {
            assert_eq!(name.as_str(), "org.arinn.Error1.PermissionDenied")
        }
        other => panic!("unexpected error: {other}"),
    }
    let generation = "30000000-0000-4000-8000-000000000003";
    let (_registered, registrations) = tokio::sync::watch::channel(Some(RegistrationContext {
        connection: provider.clone(),
        daemon_owner: daemon.unique_name().unwrap().to_string(),
        daemon_epoch: "10000000-0000-4000-8000-000000000001".into(),
        generation: generation.into(),
    }));
    provider
        .object_server()
        .at(
            production::PROVIDER_PATH,
            Processes::new(bridge.clone(), registrations),
        )
        .await
        .unwrap();
    let proxy = |connection: zbus::Connection| {
        let destination = provider.unique_name().unwrap().to_string();
        async move {
            BrowserProcessProxy::builder(&connection)
                .destination(destination)
                .unwrap()
                .build()
                .await
                .unwrap()
        }
    };
    let trusted = proxy(daemon.clone()).await;
    assert_eq!(
        trusted.get_process_id(generation, "c1:w1").await.unwrap(),
        std::process::id()
    );
    let stranger_bus = bus.connection().await;
    let snoop = proxy(stranger_bus.clone()).await;
    for (generation, window) in [(generation, "c1:w1"), ("", "")] {
        match snoop.get_process_id(generation, window).await.unwrap_err() {
            zbus::Error::MethodError(name, _, _) => {
                assert_eq!(name.as_str(), "org.arinn.Error1.PermissionDenied")
            }
            other => panic!("unexpected error: {other}"),
        }
    }
    assert_eq!(
        tabs.iter()
            .map(|t| (t.id.as_str(), t.active, t.audible))
            .collect::<Vec<_>>(),
        vec![("c1:t7", true, true), ("c1:t8", false, false)]
    );

    // A provider command reaches the extension and its result comes back.
    let route = bridge.route("c1:t7").unwrap();
    let command = {
        let provider = provider.clone();
        let host = route.host.clone();
        tokio::spawn(async move { Bridge::command(&provider, &host, "activate", 7, "").await })
    };
    let message = receive(&mut stdout).await;
    assert_eq!(message["type"], "command");
    assert_eq!(message["action"], "activate");
    assert_eq!(message["tabId"], 7);
    let serial = message["serial"].as_u64().unwrap();
    send(
        &mut stdin,
        &format!(r#"{{"type":"result","serial":{serial},"error":""}}"#),
    )
    .await;
    assert_eq!(command.await.unwrap(), Ok(()));

    // Errors from the extension map to codes; open carries the URL.
    let host_name = bridge.default_host().unwrap();
    let command = {
        let provider = provider.clone();
        let host = host_name.clone();
        tokio::spawn(async move {
            Bridge::command(&provider, &host, "open", 0, "https://example.org/").await
        })
    };
    let message = receive(&mut stdout).await;
    assert_eq!(
        (message["action"].as_str(), message["url"].as_str()),
        (Some("open"), Some("https://example.org/"))
    );
    let serial = message["serial"].as_u64().unwrap();
    send(
        &mut stdin,
        &format!(r#"{{"type":"result","serial":{serial},"error":"weird"}}"#),
    )
    .await;
    assert_eq!(command.await.unwrap(), Err(ErrorCode::Unavailable));

    // Invalid commands never reach the extension.
    for (action, url) in [
        ("eval", ""),
        ("open", "file:///etc/passwd"),
        ("close", "https://x.org/"),
    ] {
        assert_eq!(
            Bridge::command(&provider, &host_name, action, 1, url).await,
            Err(ErrorCode::InvalidArgument)
        );
    }
    // Only the provider may command the host.
    let other = bus.connection().await;
    let proxy = HostProxy::builder(&other)
        .destination(host_name.clone())
        .unwrap()
        .build()
        .await
        .unwrap();
    assert!(proxy.command("close", 7, "").await.is_err());

    // A malformed message ends the connection and the bridge forgets it.
    send(&mut stdin, r#"{"type":"eval","code":"x"}"#).await;
    assert!(
        !tokio::time::timeout(Duration::from_secs(5), child.wait())
            .await
            .unwrap()
            .unwrap()
            .success()
    );
    until(|| !bridge.snapshot().2).await;

    // Closing the port ends the host cleanly; losing the provider does too.
    let mut child = host(&bus, &origin());
    until(|| bridge.snapshot().2).await;
    drop(child.stdin.take());
    assert!(
        tokio::time::timeout(Duration::from_secs(5), child.wait())
            .await
            .unwrap()
            .unwrap()
            .success()
    );
    until(|| !bridge.snapshot().2).await;
    let mut child = host(&bus, &origin());
    // The port stays open: only the provider's loss may end this host.
    let _port = child.stdin.take();
    until(|| bridge.snapshot().2).await;
    provider.release_name(BRIDGE_NAME).await.unwrap();
    assert!(
        tokio::time::timeout(Duration::from_secs(5), child.wait())
            .await
            .unwrap()
            .unwrap()
            .success()
    );
}

// #356: when the bridge name moves straight to a new provider (one queued
// behind the old one), the host's provider is gone and it must exit too.
#[tokio::test(flavor = "multi_thread")]
async fn host_exits_when_the_bridge_moves_to_a_new_provider() {
    let bus = Bus::start();
    let old = bus.connection().await;
    let bridge = Bridge::default();
    old.object_server()
        .at(BRIDGE_PATH, bridge.clone())
        .await
        .unwrap();
    old.request_name(BRIDGE_NAME).await.unwrap();
    let mut child = host(&bus, &origin());
    // Keep the browser's port open: `wait` would otherwise close stdin, and
    // the host would exit for that reason instead.
    let _port = child.stdin.take();
    until(|| bridge.snapshot().2).await;

    // The restarted provider queues for the name while the old one still
    // holds it; releasing hands the name over without an ownerless moment.
    let new = bus.connection().await;
    new.object_server()
        .at(BRIDGE_PATH, Bridge::default())
        .await
        .unwrap();
    let queued = zbus::fdo::DBusProxy::new(&new)
        .await
        .unwrap()
        .request_name(
            BRIDGE_NAME.try_into().unwrap(),
            // No DoNotQueue: wait in line, as a provider restarted early does.
            zbus::fdo::RequestNameFlags::AllowReplacement.into(),
        )
        .await
        .unwrap();
    assert_eq!(queued, zbus::fdo::RequestNameReply::InQueue);
    assert!(
        tokio::time::timeout(Duration::from_millis(500), child.wait())
            .await
            .is_err(),
        "the host keeps running while its provider owns the name"
    );
    old.release_name(BRIDGE_NAME).await.unwrap();
    assert!(
        tokio::time::timeout(Duration::from_secs(5), child.wait())
            .await
            .expect("the host outlived its provider")
            .unwrap()
            .success()
    );
}
