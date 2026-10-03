// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The folder-listing XML matches a live `org.arinn.Folders1` export, and the
//! typed proxy round-trips both methods and the documented errors on a
//! private bus (ADR-0071).
use arinn_protocol::{
    folders::{self, FoldersApiProxy, PLACE_HOME, PLACE_MOUNT, PLACE_ROOT, Place},
    production,
    validation::ErrorCode,
};
use std::{
    collections::HashMap,
    io::{BufRead, BufReader, Write},
    process::{Child, Command, Stdio},
};

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

/// Minimal in-memory fixture of the documented semantics; not the daemon
/// backend. `None` marks a folder that cannot be read.
struct Fixture(HashMap<&'static str, Option<Vec<&'static str>>>);
#[zbus::interface(name = "org.arinn.Folders1")]
impl Fixture {
    fn places(&self) -> Vec<Place> {
        vec![
            Place {
                kind: PLACE_HOME.into(),
                path: "/home/fixture".into(),
            },
            Place {
                kind: PLACE_MOUNT.into(),
                path: "/run/media/fixture/Disk".into(),
            },
            Place {
                kind: PLACE_ROOT.into(),
                path: "/".into(),
            },
        ]
    }
    fn list(&self, path: &str, after: &str, limit: u32) -> production::Result<(Vec<String>, bool)> {
        if !arinn_protocol::settings::path(path) {
            return Err(ErrorCode::InvalidArgument.into());
        }
        let names = self.0.get(path).ok_or(ErrorCode::NotFound)?;
        let names = names.as_ref().ok_or(ErrorCode::PermissionDenied)?;
        Ok(folders::page(
            names.iter().map(|n| n.to_string()),
            after,
            limit,
        )?)
    }
}

fn error_name(result: zbus::Result<(Vec<String>, bool)>) -> String {
    match result {
        Err(zbus::Error::MethodError(name, _, _)) => name.to_string(),
        other => format!("{other:?}"),
    }
}

#[tokio::test]
async fn live_xml_typed_proxy_and_errors() {
    let bus = Bus::start();
    let server = bus.connection().await;
    server
        .object_server()
        .at(
            "/org/arinn/Daemon1",
            Fixture(HashMap::from([
                (
                    "/home/fixture",
                    Some(vec!["Videos", ".cache", "music", "Downloads"]),
                ),
                ("/root", None),
            ])),
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
    interface = root.find("interface[@name='org.arinn.Folders1']")
    assert interface is not None
    return sorted((node.tag, node.attrib['name'], tuple((a.get('direction', 'out' if node.tag == 'signal' else 'in'), a.attrib['type']) for a in node.findall('arg'))) for node in interface if node.tag in ('method', 'signal'))
assert contract(expected) == contract(actual), (contract(expected), contract(actual))
"#;
    let mut process = Command::new("python3")
        .args(["-c", script])
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../docs/plugin-api/folders1.xml"
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

    let proxy = FoldersApiProxy::new(&caller).await.unwrap();
    let places = proxy.places().await.unwrap();
    assert_eq!(places.len(), 3);
    for place in &places {
        place.validate().unwrap();
    }
    assert_eq!(
        proxy.list("/home/fixture", "", 2).await.unwrap(),
        (vec!["Downloads".to_string(), "music".to_string()], true)
    );
    assert_eq!(
        proxy.list("/home/fixture", "music", 2).await.unwrap(),
        (vec!["Videos".to_string()], false)
    );
    assert_eq!(
        error_name(proxy.list("/media/gone", "", 10).await),
        "org.arinn.Error1.NotFound"
    );
    assert_eq!(
        error_name(proxy.list("/root", "", 10).await),
        "org.arinn.Error1.PermissionDenied"
    );
    for (path, limit) in [("relative", 10), ("/home/fixture", 0)] {
        assert_eq!(
            error_name(proxy.list(path, "", limit).await),
            "org.arinn.Error1.InvalidArgument",
            "{path} {limit}"
        );
    }
}
