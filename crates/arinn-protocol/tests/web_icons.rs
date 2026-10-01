// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Contract fixtures only: no production network client, cache or shell renderer.
use arinn_protocol::{
    assets::Asset,
    production,
    validation::ErrorCode,
    web_icons::{self, WebIconProviderProxy, WebShortcutIconsProxy},
};
use std::{
    io::{BufRead, BufReader, Write},
    process::{Child, Command, Stdio},
    time::Duration,
};
use zbus::{Connection, message::Header};
const EPOCH: &str = "77777777-7777-4777-8777-777777777777";
const GENERATION: &str = "11111111-1111-4111-8111-111111111111";
fn image() -> Asset {
    // Synthetic PNG header fixture; full raster decoding belongs to implementation.
    let mut data = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
    data.extend_from_slice(&1_u32.to_be_bytes());
    data.extend_from_slice(&1_u32.to_be_bytes());
    data.resize(33, 0);
    Asset {
        mime: "image/png".into(),
        data,
    }
}
struct Legacy;
#[zbus::interface(name = "org.arinn.ContractFixture1")]
impl Legacy {
    fn ping(&self) {}
}
struct Public;
#[zbus::interface(name = "org.arinn.WebShortcutIcons1")]
impl Public {
    async fn get(&self, epoch: &str, revision: u64, id: &str) -> production::Result<(Asset,)> {
        web_icons::request(epoch, revision, id)?;
        if epoch != EPOCH || revision != 3 {
            return Err(ErrorCode::Conflict.into());
        }
        if id != "saved" {
            return Err(ErrorCode::NotFound.into());
        }
        Ok((image(),))
    }
}
struct Provider {
    owner: String,
}
#[zbus::interface(name = "org.arinn.WebIconProvider1")]
impl Provider {
    async fn get_icon(
        &self,
        generation: &str,
        origin: &str,
        #[zbus(header)] header: Header<'_>,
    ) -> production::Result<(Asset,)> {
        if header.sender().map(|n| n.as_str()) != Some(&self.owner) {
            return Err(ErrorCode::PermissionDenied.into());
        }
        if generation != GENERATION {
            return Err(ErrorCode::Conflict.into());
        }
        web_icons::origin(origin)?;
        Ok((image(),))
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
async fn optional_interfaces_wire_and_context_checks() {
    tokio::time::timeout(Duration::from_secs(15), contract())
        .await
        .unwrap();
}
async fn contract() {
    let bus = Bus::new();
    let daemon = bus.connect().await;
    let provider = bus.connect().await;
    let shell = bus.connect().await;
    daemon
        .object_server()
        .at(production::PATH, Legacy)
        .await
        .unwrap();
    let daemon_name = daemon.unique_name().unwrap().as_str();
    let provider_name = provider.unique_name().unwrap().as_str();
    let public = WebShortcutIconsProxy::builder(&shell)
        .destination(daemon_name)
        .unwrap()
        .build()
        .await
        .unwrap();
    assert!(public.get(EPOCH, 3, "saved").await.is_err()); // optional absence
    daemon
        .object_server()
        .at(production::PATH, Public)
        .await
        .unwrap();
    provider
        .object_server()
        .at(
            "/org/arinn/Provider1",
            Provider {
                owner: daemon_name.into(),
            },
        )
        .await
        .unwrap();
    for (owner, path, file, interface) in [
        (
            daemon_name,
            production::PATH,
            "web-shortcut-icons1.xml",
            web_icons::INTERFACE,
        ),
        (
            provider_name,
            "/org/arinn/Provider1",
            "web-icon-provider1.xml",
            web_icons::PROVIDER_INTERFACE,
        ),
    ] {
        let xml = zbus::fdo::IntrospectableProxy::builder(&shell)
            .destination(owner)
            .unwrap()
            .path(path)
            .unwrap()
            .build()
            .await
            .unwrap()
            .introspect()
            .await
            .unwrap();
        compare_xml(&xml, file, interface);
    }
    let result = public.get(EPOCH, 3, "saved").await.unwrap().0;
    web_icons::image(&result).unwrap();
    assert_eq!(result, image());
    error(public.get(EPOCH, 2, "saved").await.unwrap_err(), "Conflict");
    error(
        public.get(GENERATION, 3, "saved").await.unwrap_err(),
        "Conflict",
    );
    error(
        public.get(EPOCH, 3, "deleted").await.unwrap_err(),
        "NotFound",
    );
    error(
        public.get(EPOCH, 0, "saved").await.unwrap_err(),
        "InvalidArgument",
    );
    let trusted = WebIconProviderProxy::builder(&daemon)
        .destination(provider_name)
        .unwrap()
        .build()
        .await
        .unwrap();
    assert_eq!(
        trusted
            .get_icon(GENERATION, "https://site.example/")
            .await
            .unwrap()
            .0,
        image()
    );
    error(
        trusted
            .get_icon(EPOCH, "https://site.example/")
            .await
            .unwrap_err(),
        "Conflict",
    );
    error(
        trusted
            .get_icon(GENERATION, "https://site.example/private?secret")
            .await
            .unwrap_err(),
        "InvalidArgument",
    );
    let untrusted = WebIconProviderProxy::builder(&shell)
        .destination(provider_name)
        .unwrap()
        .build()
        .await
        .unwrap();
    error(
        untrusted
            .get_icon(GENERATION, "https://site.example/")
            .await
            .unwrap_err(),
        "PermissionDenied",
    );
}
