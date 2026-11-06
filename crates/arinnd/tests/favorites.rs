// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Private-bus durable favorites notifications, CAS and uncertain admission.
use arinn_protocol::{
    favorites::FavoritesProxy,
    production::{EntityRef, Error},
    web::WebShortcut,
};
use arinnd::{favorites::Favorites, storage::Store};
use futures_util::StreamExt;
use std::{
    fs,
    io::{BufRead, BufReader},
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::Duration,
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
struct Tree(PathBuf);
impl Drop for Tree {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[tokio::test]
async fn changed_is_post_commit_and_noop_is_silent() {
    let tree = Tree(std::env::temp_dir().join(format!("arinn-favorites-{}", uuid::Uuid::new_v4())));
    let state = tree.0.join("state");
    let mut store = Store::open(&state).unwrap();
    let shortcut = WebShortcut {
        id: "fixture-site".into(),
        title: "Example".into(),
        url: "https://example.org/".into(),
        icon: String::new(),
    };
    store.add_web_shortcut(&shortcut).unwrap();
    let entry = EntityRef {
        provider_id: "arinn".into(),
        capability: "web-shortcut".into(),
        local_id: shortcut.id,
    };
    let bus = Bus::start();
    let server = bus.connection().await;
    server
        .object_server()
        .at("/org/arinn/Daemon1", Favorites::new(&state))
        .await
        .unwrap();
    server.request_name("org.arinn.Daemon1").await.unwrap();
    let caller = bus.connection().await;
    let proxy = FavoritesProxy::new(&caller).await.unwrap();
    let mut changed = proxy.receive_changed().await.unwrap();
    let (initial,) = proxy.list().await.unwrap();
    assert_eq!(initial.revision, 1);
    assert!(initial.entries.is_empty());
    let application = EntityRef {
        provider_id: "fixture.desktop".into(),
        capability: "application".into(),
        local_id: "one".into(),
    };
    assert!(matches!(
        proxy.set(1, &[application]).await.map_err(Error::from),
        Err(Error::Unavailable(_))
    ));
    assert_eq!(proxy.set(1, std::slice::from_ref(&entry)).await.unwrap(), 2);
    let signal = tokio::time::timeout(Duration::from_secs(2), changed.next())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(signal.args().unwrap().revision, 2);
    let durable = Store::open(&state).unwrap().favorite_list().unwrap();
    assert_eq!(durable.revision, 2);
    assert_eq!(durable.entries.as_slice(), std::slice::from_ref(&entry));
    assert_eq!(proxy.set(2, std::slice::from_ref(&entry)).await.unwrap(), 2);
    assert!(matches!(
        proxy.set(1, &[]).await.map_err(Error::from),
        Err(Error::Conflict(_))
    ));
    assert!(
        tokio::time::timeout(Duration::from_millis(100), changed.next())
            .await
            .is_err()
    );
    // Two clients capture one revision; exactly one replacement commits.
    let other = FavoritesProxy::new(&caller).await.unwrap();
    let absent = EntityRef {
        provider_id: "arinn".into(),
        capability: "web-shortcut".into(),
        local_id: "absent".into(),
    };
    assert!(matches!(
        proxy.set(2, &[absent]).await.map_err(Error::from),
        Err(Error::NotFound(_))
    ));
    let (first, second) = tokio::join!(proxy.set(2, &[]), other.set(2, &[]));
    assert_eq!(usize::from(first.is_ok()) + usize::from(second.is_ok()), 1);
    let loser = if first.is_err() { first } else { second };
    assert!(matches!(
        loser.map_err(Error::from),
        Err(Error::Conflict(_))
    ));
    let signal = tokio::time::timeout(Duration::from_secs(2), changed.next())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(signal.args().unwrap().revision, 3);
    assert!(
        Store::open(&state)
            .unwrap()
            .favorite_list()
            .unwrap()
            .entries
            .is_empty()
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(100), changed.next())
            .await
            .is_err()
    );
}

/// ADR-0143: MusicPins1 writes a pin with its favorites target, guarded by the
/// favorites revision, and announces it through Favorites1.Changed.
#[tokio::test]
async fn music_pins_share_the_favorites_revision_and_signal() {
    use arinn_protocol::music_pins::{MusicPin, MusicPinsProxy};
    let tree =
        Tree(std::env::temp_dir().join(format!("arinn-music-pins-{}", uuid::Uuid::new_v4())));
    let state = tree.0.join("state");
    Store::open(&state).unwrap();
    let bus = Bus::start();
    let server = bus.connection().await;
    let favorites = Favorites::new(&state);
    server
        .object_server()
        .at("/org/arinn/Daemon1", favorites.music_pins())
        .await
        .unwrap();
    server
        .object_server()
        .at("/org/arinn/Daemon1", favorites)
        .await
        .unwrap();
    server.request_name("org.arinn.Daemon1").await.unwrap();
    let caller = bus.connection().await;
    // The live interface matches docs/plugin-api/music-pins1.xml.
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
    interface = root.find("interface[@name='org.arinn.MusicPins1']")
    assert interface is not None
    return sorted((node.tag, node.attrib['name'], tuple((a.get('direction', 'out' if node.tag == 'signal' else 'in'), a.attrib['type']) for a in node.findall('arg'))) for node in interface if node.tag in ('method', 'signal'))
assert contract(expected) == contract(actual), (contract(expected), contract(actual))
"#;
    let mut process = Command::new("python3")
        .args(["-c", script])
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../docs/plugin-api/music-pins1.xml"
        ))
        .stdin(Stdio::piped())
        .spawn()
        .unwrap();
    {
        use std::io::Write;
        process
            .stdin
            .take()
            .unwrap()
            .write_all(introspection.as_bytes())
            .unwrap();
    }
    assert!(process.wait().unwrap().success());
    let favorites = FavoritesProxy::new(&caller).await.unwrap();
    let pins = MusicPinsProxy::new(&caller).await.unwrap();
    let mut changed = favorites.receive_changed().await.unwrap();
    let pin = MusicPin {
        provider_id: "fixture.music".into(),
        station_id: "station:library".into(),
        title: "Fixture station".into(),
        artwork: String::new(),
    };
    assert!(matches!(
        pins.pin(7, &pin).await.map_err(Error::from),
        Err(Error::Conflict(_))
    ));
    assert_eq!(pins.pin(1, &pin).await.unwrap(), 2);
    let signal = tokio::time::timeout(Duration::from_secs(2), changed.next())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(signal.args().unwrap().revision, 2);
    let (list,) = pins.list().await.unwrap();
    assert_eq!(list.revision, 2);
    assert_eq!(list.pins, std::slice::from_ref(&pin));
    let (entries,) = favorites.list().await.unwrap();
    assert_eq!(entries.entries, [pin.target()]);
    // The same pin again is silent.
    assert_eq!(pins.pin(2, &pin).await.unwrap(), 2);
    assert!(
        tokio::time::timeout(Duration::from_millis(100), changed.next())
            .await
            .is_err()
    );
    assert_eq!(
        pins.unpin(2, "fixture.music", "station:library")
            .await
            .unwrap(),
        3
    );
    let signal = tokio::time::timeout(Duration::from_secs(2), changed.next())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(signal.args().unwrap().revision, 3);
    assert!(pins.list().await.unwrap().0.pins.is_empty());
    assert!(matches!(
        pins.unpin(3, "fixture.music", "station:library")
            .await
            .map_err(Error::from),
        Err(Error::NotFound(_))
    ));
}
