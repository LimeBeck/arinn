// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Contract fixtures (ADR-0105): a fixture export of both keyboard
//! interfaces must match the published XML, and the typed proxies must
//! reach it. The production daemon routing is tested in `arinnd`.
use arinn_protocol::{
    keyboard::{self, ExternalKeyboardProxy, KeyboardBridgeProxy, Record},
    production,
};
use futures_util::StreamExt;
use std::{
    io::{BufRead, BufReader, Write},
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex},
    time::Duration,
};
use zbus::{Connection, object_server::SignalEmitter};

const SESSION: &str = "10000000-0000-4000-8000-000000000001";
const GENERATION: &str = "20000000-0000-4000-8000-000000000002";

type Calls = Arc<Mutex<Vec<String>>>;

struct Shell(Calls);
#[zbus::interface(name = "org.arinn.ExternalKeyboard1")]
impl Shell {
    #[zbus(name = "Type")]
    fn type_text(&self, session: &str, text: &str) -> production::Result<()> {
        keyboard::session(session)?;
        keyboard::text(text)?;
        self.0.lock().unwrap().push(format!("type:{}", text.len()));
        Ok(())
    }
    fn press(&self, session: &str, key: &str) -> production::Result<()> {
        keyboard::session(session)?;
        keyboard::press_key(key)?;
        self.0.lock().unwrap().push(format!("press:{key}"));
        Ok(())
    }
    fn close(&self, session: &str) -> production::Result<()> {
        keyboard::session(session)?;
        self.0.lock().unwrap().push("close".into());
        Ok(())
    }
    #[zbus(signal)]
    async fn opened(emitter: &SignalEmitter<'_>, session: &str) -> zbus::Result<()>;
    #[zbus(signal)]
    async fn input(emitter: &SignalEmitter<'_>, session: &str, key: &str) -> zbus::Result<()>;
    #[zbus(signal)]
    async fn closed(emitter: &SignalEmitter<'_>, session: &str) -> zbus::Result<()>;
}

struct Bridge(Calls);
#[zbus::interface(name = "org.arinn.KeyboardBridge1")]
impl Bridge {
    fn request(&self) {
        self.0.lock().unwrap().push("request".into());
    }
    fn connect(&self, instance: &str) -> production::Result<String> {
        keyboard::session(instance)?;
        Ok(GENERATION.into())
    }
    fn next(&self, generation: &str) -> production::Result<Vec<String>> {
        keyboard::session(generation)?;
        Ok(Record::Type(SESSION.into(), "Яa".into()).fields())
    }
    fn opened(&self, generation: &str, session: &str) -> production::Result<()> {
        keyboard::session(generation)?;
        keyboard::session(session)?;
        self.0.lock().unwrap().push("opened".into());
        Ok(())
    }
    fn input(&self, generation: &str, session: &str, key: &str) -> production::Result<()> {
        keyboard::session(generation)?;
        keyboard::session(session)?;
        keyboard::input_key(key)?;
        self.0.lock().unwrap().push(format!("input:{key}"));
        Ok(())
    }
    fn closed(&self, generation: &str, session: &str, reason: &str) -> production::Result<()> {
        keyboard::session(generation)?;
        keyboard::session(session)?;
        keyboard::close_reason(reason)?;
        self.0.lock().unwrap().push(format!("closed:{reason}"));
        Ok(())
    }
}

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

#[tokio::test]
async fn keyboard_interfaces_match_their_xml_and_proxies() {
    tokio::time::timeout(Duration::from_secs(15), contract())
        .await
        .unwrap();
}

async fn contract() {
    let bus = Bus::new();
    let daemon = bus.connect().await;
    let client = bus.connect().await;
    let calls = Calls::default();
    daemon
        .object_server()
        .at(production::PATH, Shell(calls.clone()))
        .await
        .unwrap();
    daemon
        .object_server()
        .at(production::PATH, Bridge(calls.clone()))
        .await
        .unwrap();
    let name = daemon.unique_name().unwrap().to_string();
    let xml = zbus::fdo::IntrospectableProxy::builder(&client)
        .destination(name.as_str())
        .unwrap()
        .path(production::PATH)
        .unwrap()
        .build()
        .await
        .unwrap()
        .introspect()
        .await
        .unwrap();
    compare_xml(&xml, "external-keyboard1.xml", keyboard::SHELL_INTERFACE);
    compare_xml(&xml, "keyboard-bridge1.xml", keyboard::BRIDGE_INTERFACE);

    let shell = ExternalKeyboardProxy::builder(&client)
        .destination(name.as_str())
        .unwrap()
        .build()
        .await
        .unwrap();
    let mut opened = shell.receive_opened().await.unwrap();
    let mut input = shell.receive_input().await.unwrap();
    let mut closed = shell.receive_closed().await.unwrap();
    let emitter = SignalEmitter::new(&daemon, production::PATH).unwrap();
    Shell::opened(&emitter, SESSION).await.unwrap();
    Shell::input(&emitter, SESSION, "select").await.unwrap();
    Shell::closed(&emitter, SESSION).await.unwrap();
    assert_eq!(
        opened.next().await.unwrap().args().unwrap().session(),
        &SESSION
    );
    let key = input.next().await.unwrap();
    assert_eq!(key.args().unwrap().key(), &"select");
    assert_eq!(
        closed.next().await.unwrap().args().unwrap().session(),
        &SESSION
    );
    shell.type_text(SESSION, "Привет").await.unwrap();
    shell.press(SESSION, "backspace").await.unwrap();
    shell.close(SESSION).await.unwrap();
    assert!(shell.type_text(SESSION, "a\nb").await.is_err());
    assert!(shell.press(SESSION, "tab").await.is_err());

    let bridge = KeyboardBridgeProxy::builder(&client)
        .destination(name.as_str())
        .unwrap()
        .build()
        .await
        .unwrap();
    bridge.request().await.unwrap();
    let generation = bridge.connect(SESSION).await.unwrap();
    let record = Record::parse(&bridge.next(&generation).await.unwrap()).unwrap();
    assert_eq!(record, Record::Type(SESSION.into(), "Яa".into()));
    bridge.opened(&generation, SESSION).await.unwrap();
    bridge.input(&generation, SESSION, "up").await.unwrap();
    bridge.closed(&generation, SESSION, "focus").await.unwrap();
    assert!(
        bridge
            .closed(&generation, SESSION, "timeout")
            .await
            .is_err()
    );
    assert_eq!(
        *calls.lock().unwrap(),
        [
            "type:12",
            "press:backspace",
            "close",
            "request",
            "opened",
            "input:up",
            "closed:focus"
        ]
    );
}
