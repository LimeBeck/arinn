// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The web-shortcut XML matches a live `org.arinn.WebShortcuts1` export, and
//! the typed proxy round-trips every method and the signal on a private bus.
use arinn_protocol::{
    production::Error,
    validation::ErrorCode,
    web::{WebShortcut, WebShortcutList, WebShortcutsProxy, url},
};
use futures_util::StreamExt;
use std::{
    io::{BufRead, BufReader, Write},
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex},
};
use zbus::{object_server::SignalEmitter, zvariant::OwnedObjectPath};

struct Bus(Child, String);
impl Bus {
    fn start() -> Self {
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

/// Minimal in-memory fixture of the documented semantics; not the daemon backend.
struct Fixture(Arc<Mutex<WebShortcutList>>);
#[zbus::interface(name = "org.arinn.WebShortcuts1")]
impl Fixture {
    fn list(&self) -> (WebShortcutList,) {
        (self.0.lock().unwrap().clone(),)
    }
    async fn add(
        &self,
        title: &str,
        url: &str,
        icon: &str,
        request_id: &str,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> Result<String, Error> {
        let revision = {
            let mut list = self.0.lock().unwrap();
            let shortcut = WebShortcut {
                id: request_id.into(),
                title: title.into(),
                url: url.into(),
                icon: icon.into(),
            };
            shortcut.validate()?;
            list.shortcuts.push(shortcut);
            list.revision += 1;
            list.revision
        };
        Self::changed(&emitter, revision).await?;
        Ok(request_id.into())
    }
    fn update(
        &self,
        id: &str,
        revision: u64,
        _title: &str,
        _url: &str,
        _icon: &str,
    ) -> Result<(), Error> {
        let list = self.0.lock().unwrap();
        if list.revision != revision {
            return Err(ErrorCode::Conflict.into());
        }
        list.shortcuts
            .iter()
            .find(|s| s.id == id)
            .map(|_| ())
            .ok_or(ErrorCode::NotFound.into())
    }
    fn remove(&self, _id: &str, _revision: u64) -> Result<(), Error> {
        Err(ErrorCode::NotFound.into())
    }
    fn open(&self, _id: &str, _request_id: &str) -> OwnedObjectPath {
        "/org/arinn/Daemon1/operations/o_1".try_into().unwrap()
    }
    #[zbus(signal)]
    async fn changed(emitter: &SignalEmitter<'_>, revision: u64) -> zbus::Result<()>;
}

#[tokio::test]
async fn live_xml_typed_proxy_and_signal() {
    let bus = Bus::start();
    let server = bus.connection().await;
    server
        .object_server()
        .at(
            "/org/arinn/Daemon1",
            Fixture(Arc::new(Mutex::new(WebShortcutList {
                revision: 1,
                shortcuts: Vec::new(),
            }))),
        )
        .await
        .unwrap();
    server.request_name("org.arinn.Daemon1").await.unwrap();
    let caller = bus.connection().await;

    let introspection = zbus::fdo::IntrospectableProxy::builder(&caller)
        .destination("org.arinn.Daemon1")
        .unwrap()
        .path("/org/arinn/Daemon1")
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
    interface = root.find("interface[@name='org.arinn.WebShortcuts1']")
    assert interface is not None
    return sorted((node.tag, node.attrib['name'], tuple((a.get('direction', 'out' if node.tag == 'signal' else 'in'), a.attrib['type']) for a in node.findall('arg'))) for node in interface if node.tag in ('method', 'signal'))
assert contract(expected) == contract(actual), (contract(expected), contract(actual))
"#;
    let mut process = Command::new("python3")
        .args(["-c", script])
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../docs/plugin-api/web-shortcuts1.xml"
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

    let proxy = WebShortcutsProxy::new(&caller).await.unwrap();
    let mut changed = proxy.receive_changed().await.unwrap();
    let id = proxy
        .add("Video", "https://video.example/", "", "request-1")
        .await
        .unwrap();
    assert_eq!(id, "request-1");
    assert_eq!(changed.next().await.unwrap().args().unwrap().revision, 2);
    let (list,) = proxy.list().await.unwrap();
    assert_eq!(list.revision, 2);
    list.validate().unwrap();
    assert_eq!(list.shortcuts[0].url, "https://video.example/");

    let error = |result: zbus::Result<()>| match result.map_err(Error::from) {
        Err(Error::Conflict(_)) => "Conflict",
        Err(Error::NotFound(_)) => "NotFound",
        Err(Error::InvalidArgument(_)) => "InvalidArgument",
        other => panic!("unexpected {other:?}"),
    };
    assert_eq!(
        error(proxy.update(&id, 1, "x", "https://a.example", "").await),
        "Conflict"
    );
    assert_eq!(error(proxy.remove("missing", 2).await), "NotFound");
    assert_eq!(
        error(
            proxy
                .add("Bad", "http://insecure.example/", "", "r2")
                .await
                .map(|_| ())
        ),
        "InvalidArgument"
    );
    assert_eq!(
        url("http://insecure.example/"),
        Err(ErrorCode::InvalidArgument)
    );
    let operation = proxy.open(&id, "request-3").await.unwrap();
    assert!(
        operation
            .as_str()
            .starts_with("/org/arinn/Daemon1/operations/")
    );
}
