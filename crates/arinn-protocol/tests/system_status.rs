// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Contract fixtures only (ADR-0100): no production producer or shell
//! consumer. The live export must match the published XML.
use arinn_protocol::{
    production,
    system_status::{self, Audio, Link, Network, SystemStatus, SystemStatusApiProxy},
};
use futures_util::StreamExt;
use std::{
    io::{BufRead, BufReader, Write},
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex},
    time::Duration,
};
use zbus::{Connection, object_server::SignalEmitter};

const EPOCH: &str = "10000000-0000-4000-8000-000000000001";

struct Legacy;
#[zbus::interface(name = "org.arinn.ContractFixture1")]
impl Legacy {
    fn ping(&self) {}
}

struct Daemon(Arc<Mutex<SystemStatus>>);
#[zbus::interface(name = "org.arinn.SystemStatus1")]
impl Daemon {
    fn get(&self) -> production::Result<(SystemStatus,)> {
        Ok((self.0.lock().unwrap().clone(),))
    }
    #[zbus(signal)]
    async fn changed(emitter: &SignalEmitter<'_>, epoch: &str, revision: u64) -> zbus::Result<()>;
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
async fn daemon_root_interface_wire_and_change() {
    tokio::time::timeout(Duration::from_secs(15), contract())
        .await
        .unwrap();
}

async fn contract() {
    let bus = Bus::new();
    let daemon = bus.connect().await;
    let shell = bus.connect().await;
    daemon
        .object_server()
        .at(production::PATH, Legacy)
        .await
        .unwrap();
    let daemon_name = daemon.unique_name().unwrap().to_string();
    let public = SystemStatusApiProxy::builder(&shell)
        .destination(daemon_name.as_str())
        .unwrap()
        .build()
        .await
        .unwrap();
    // A daemon without the interface: the shell shows neither icon.
    assert!(public.get().await.is_err());

    // A platform without a producer publishes everything unknown.
    let state = Arc::new(Mutex::new(SystemStatus::unknown(EPOCH, 1)));
    daemon
        .object_server()
        .at(production::PATH, Daemon(state.clone()))
        .await
        .unwrap();
    let xml = zbus::fdo::IntrospectableProxy::builder(&shell)
        .destination(daemon_name.as_str())
        .unwrap()
        .path(production::PATH)
        .unwrap()
        .build()
        .await
        .unwrap()
        .introspect()
        .await
        .unwrap();
    compare_xml(&xml, "system-status1.xml", system_status::INTERFACE);
    let (unknown,) = public.get().await.unwrap();
    unknown.validate().unwrap();
    assert_eq!(
        (unknown.audio(), unknown.network(), unknown.link()),
        (Audio::Unknown, Network::Unknown, Link::Unknown)
    );

    // A change invalidates readers through Changed; the shell re-reads.
    // The match is installed before the change, as readers must.
    let mut changes = public.receive_changed().await.unwrap();
    {
        let mut snapshot = state.lock().unwrap();
        snapshot.revision = 2;
        snapshot.audio = "muted".into();
        snapshot.network = "connected".into();
        snapshot.link = "wifi".into();
        snapshot.signal = 2;
        snapshot.validate().unwrap();
    }
    let iface = daemon
        .object_server()
        .interface::<_, Daemon>(production::PATH)
        .await
        .unwrap();
    Daemon::changed(iface.signal_emitter(), EPOCH, 2)
        .await
        .unwrap();
    let signal = changes.next().await.unwrap();
    let args = signal.args().unwrap();
    assert_eq!((args.epoch(), *args.revision()), (&EPOCH, 2));
    let (known,) = public.get().await.unwrap();
    known.validate().unwrap();
    assert!(known.revision > unknown.revision);
    assert_eq!(
        (known.audio(), known.network(), known.link(), known.signal()),
        (Audio::Muted, Network::Connected, Link::Wifi, 2)
    );
}
