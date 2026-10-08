// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Contract fixtures only (ADR-0096): no production provider, daemon join or
//! shell consumer. The live exports must match the published XML.
use arinn_protocol::{
    media_association::{
        self, BrowserProcessProxy, MediaAssociation, MediaAssociationApiProxy, MediaProcessProxy,
        MediaTabLink,
    },
    production,
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
const MEDIA_GENERATION: &str = "20000000-0000-4000-8000-000000000002";
const BROWSER_GENERATION: &str = "30000000-0000-4000-8000-000000000003";
const BROWSER_PID: u32 = 4242;

struct Legacy;
#[zbus::interface(name = "org.arinn.ContractFixture1")]
impl Legacy {
    fn ping(&self) {}
}

struct Daemon(Arc<Mutex<MediaAssociation>>);
#[zbus::interface(name = "org.arinn.MediaAssociation1")]
impl Daemon {
    fn get(&self) -> production::Result<(MediaAssociation,)> {
        Ok((self.0.lock().unwrap().clone(),))
    }
    #[zbus(signal)]
    async fn changed(emitter: &SignalEmitter<'_>, epoch: &str, revision: u64) -> zbus::Result<()>;
}

/// Process IDs go to the daemon only.
fn trusted(header: &Header<'_>, daemon: &str) -> Result<(), ErrorCode> {
    if header.sender().map(|n| n.as_str()) == Some(daemon) {
        Ok(())
    } else {
        Err(ErrorCode::PermissionDenied)
    }
}

struct Media {
    daemon: String,
}
#[zbus::interface(name = "org.arinn.MediaProcess1")]
impl Media {
    fn get_process_id(
        &self,
        generation: &str,
        id: &str,
        #[zbus(header)] header: Header<'_>,
    ) -> production::Result<u32> {
        trusted(&header, &self.daemon)?;
        media_association::validate_process_query(generation, id)?;
        if generation != MEDIA_GENERATION {
            return Err(ErrorCode::Conflict.into());
        }
        match id {
            "browser-session" => Ok(BROWSER_PID),
            "unknown-session" => Ok(media_association::UNKNOWN_PROCESS),
            _ => Err(ErrorCode::NotFound.into()),
        }
    }
}

struct Browser {
    daemon: String,
}
#[zbus::interface(name = "org.arinn.BrowserProcess1")]
impl Browser {
    fn get_process_id(
        &self,
        generation: &str,
        window_id: &str,
        #[zbus(header)] header: Header<'_>,
    ) -> production::Result<u32> {
        trusted(&header, &self.daemon)?;
        media_association::validate_process_query(generation, window_id)?;
        if generation != BROWSER_GENERATION {
            return Err(ErrorCode::Conflict.into());
        }
        match window_id {
            "c1:w1" => Ok(BROWSER_PID),
            _ => Err(ErrorCode::NotFound.into()),
        }
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
    assert!(child.wait().unwrap().success(), "{filename}");
}

fn link() -> MediaTabLink {
    MediaTabLink {
        media_provider: "mpris".into(),
        media_generation: MEDIA_GENERATION.into(),
        session: "browser-session".into(),
        browser_provider: "chrome".into(),
        browser_generation: BROWSER_GENERATION.into(),
        tab: "c1:t7".into(),
    }
}

#[tokio::test]
async fn optional_interfaces_wire_and_caller_checks() {
    tokio::time::timeout(Duration::from_secs(15), contract())
        .await
        .unwrap();
}

async fn contract() {
    let bus = Bus::new();
    let daemon = bus.connect().await;
    let media = bus.connect().await;
    let browser = bus.connect().await;
    let shell = bus.connect().await;
    daemon
        .object_server()
        .at(production::PATH, Legacy)
        .await
        .unwrap();
    let daemon_name = daemon.unique_name().unwrap().to_string();
    let public = MediaAssociationApiProxy::builder(&shell)
        .destination(daemon_name.as_str())
        .unwrap()
        .build()
        .await
        .unwrap();
    // A daemon without the optional interface: the shell keeps both cards.
    assert!(public.get().await.is_err());

    let state = Arc::new(Mutex::new(MediaAssociation {
        epoch: EPOCH.into(),
        revision: 1,
        links: vec![],
    }));
    daemon
        .object_server()
        .at(production::PATH, Daemon(state.clone()))
        .await
        .unwrap();
    media
        .object_server()
        .at(
            production::PROVIDER_PATH,
            Media {
                daemon: daemon_name.clone(),
            },
        )
        .await
        .unwrap();
    browser
        .object_server()
        .at(
            production::PROVIDER_PATH,
            Browser {
                daemon: daemon_name.clone(),
            },
        )
        .await
        .unwrap();
    for (owner, path, file, interface) in [
        (
            daemon_name.clone(),
            production::PATH,
            "media-association1.xml",
            media_association::INTERFACE,
        ),
        (
            media.unique_name().unwrap().to_string(),
            production::PROVIDER_PATH,
            "media-process1.xml",
            media_association::MEDIA_PROCESS_INTERFACE,
        ),
        (
            browser.unique_name().unwrap().to_string(),
            production::PROVIDER_PATH,
            "browser-process1.xml",
            media_association::BROWSER_PROCESS_INTERFACE,
        ),
    ] {
        let xml = zbus::fdo::IntrospectableProxy::builder(&shell)
            .destination(owner.as_str())
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

    // The empty snapshot is valid: nothing is linked.
    let (empty,) = public.get().await.unwrap();
    empty.validate().unwrap();
    assert!(empty.links.is_empty());

    // The daemon asks both providers; the same process ID is the evidence.
    let media_process = MediaProcessProxy::builder(&daemon)
        .destination(media.unique_name().unwrap().as_str())
        .unwrap()
        .build()
        .await
        .unwrap();
    let browser_process = BrowserProcessProxy::builder(&daemon)
        .destination(browser.unique_name().unwrap().as_str())
        .unwrap()
        .build()
        .await
        .unwrap();
    let session_pid = media_process
        .get_process_id(MEDIA_GENERATION, "browser-session")
        .await
        .unwrap();
    let window_pid = browser_process
        .get_process_id(BROWSER_GENERATION, "c1:w1")
        .await
        .unwrap();
    assert_eq!(session_pid, window_pid);
    assert_eq!(
        media_process
            .get_process_id(MEDIA_GENERATION, "unknown-session")
            .await
            .unwrap(),
        media_association::UNKNOWN_PROCESS
    );
    error(
        media_process
            .get_process_id(BROWSER_GENERATION, "browser-session")
            .await
            .unwrap_err(),
        "Conflict",
    );
    error(
        browser_process
            .get_process_id(BROWSER_GENERATION, "")
            .await
            .unwrap_err(),
        "InvalidArgument",
    );
    error(
        browser_process
            .get_process_id(BROWSER_GENERATION, "c9:w9")
            .await
            .unwrap_err(),
        "NotFound",
    );
    // Anyone but the daemon is refused, before its arguments are read:
    // process IDs never reach the shell.
    let snoop = MediaProcessProxy::builder(&shell)
        .destination(media.unique_name().unwrap().as_str())
        .unwrap()
        .build()
        .await
        .unwrap();
    error(
        snoop
            .get_process_id(MEDIA_GENERATION, "browser-session")
            .await
            .unwrap_err(),
        "PermissionDenied",
    );
    error(
        snoop.get_process_id("", "").await.unwrap_err(),
        "PermissionDenied",
    );
    let snoop = BrowserProcessProxy::builder(&shell)
        .destination(browser.unique_name().unwrap().as_str())
        .unwrap()
        .build()
        .await
        .unwrap();
    error(
        snoop
            .get_process_id(BROWSER_GENERATION, "c1:w1")
            .await
            .unwrap_err(),
        "PermissionDenied",
    );
    error(
        snoop.get_process_id("not-a-uuid", "").await.unwrap_err(),
        "PermissionDenied",
    );

    // A published link invalidates readers through Changed; the shell
    // re-reads and sees one link with all six opaque fields.
    let mut changes = public.receive_changed().await.unwrap();
    {
        let mut snapshot = state.lock().unwrap();
        snapshot.revision = 2;
        snapshot.links = vec![link()];
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
    let (linked,) = public.get().await.unwrap();
    linked.validate().unwrap();
    assert_eq!(linked.links, vec![link()]);
    assert!(linked.revision > empty.revision);
}
