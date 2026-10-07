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
    // the caller before the arguments.
    let daemon = bus.connection().await;
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
