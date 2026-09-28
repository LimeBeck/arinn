// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The production `org.arinn.WebShortcuts1` export on a private bus: live XML,
//! packaged defaults, deduplicated add, revision conflicts, signal and Open.
use arinn_protocol::{
    production::{Error, ProductionOperationProxy},
    web::WebShortcutsProxy,
};
use arinnd::{operations::Operations, shortcuts};
use futures_util::StreamExt;
use std::{
    fs,
    io::{BufRead, BufReader, Write},
    path::PathBuf,
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
struct Tree(PathBuf);
impl Drop for Tree {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn error(result: Result<impl std::fmt::Debug, zbus::Error>) -> &'static str {
    match result.map_err(Error::from) {
        Err(Error::Conflict(_)) => "Conflict",
        Err(Error::NotFound(_)) => "NotFound",
        Err(Error::InvalidArgument(_)) => "InvalidArgument",
        other => panic!("unexpected {other:?}"),
    }
}
async fn export(bus: &Bus, tree: &Tree, roots: &[PathBuf]) -> zbus::Connection {
    let server = bus.connection().await;
    let endpoint = shortcuts::WebShortcuts::new(&tree.0.join("state"), Operations::default());
    endpoint
        .seed(shortcuts::load_defaults(roots))
        .await
        .unwrap();
    server
        .object_server()
        .at("/org/arinn/Daemon1", endpoint)
        .await
        .unwrap();
    server.request_name("org.arinn.Daemon1").await.unwrap();
    server
}

#[tokio::test]
async fn production_export_matches_contract() {
    let tree = Tree(std::env::temp_dir().join(format!("arinn-web-{}", uuid::Uuid::new_v4())));
    let system = tree.0.join("system");
    fs::create_dir_all(system.join("arinn")).unwrap();
    fs::write(
        system.join("arinn/web-shortcuts.toml"),
        "[[shortcut]]\ntitle = \"Video\"\nurl = \"https://video.example/\"\nicon = \"video\"\n\n\
         [[shortcut]]\ntitle = \"Media server\"\nurl = \"http://192.168.1.10:8096/\"\n\n\
         [[shortcut]]\ntitle = \"Public http\"\nurl = \"http://public.example/\"\n",
    )
    .unwrap();
    let roots = vec![tree.0.join("home"), system];
    let bus = Bus::start();
    let server = export(&bus, &tree, &roots).await;
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
    // Valid packaged defaults are seeded; the public http one is skipped.
    let (list,) = proxy.list().await.unwrap();
    assert_eq!(list.revision, 2);
    assert_eq!(
        list.shortcuts
            .iter()
            .map(|s| s.title.as_str())
            .collect::<Vec<_>>(),
        ["Video", "Media server"]
    );
    assert_eq!(list.shortcuts[0].icon, "video");

    let request = uuid::Uuid::new_v4().to_string();
    let id = proxy
        .add("NAS", "http://nas:5000/", "", &request)
        .await
        .unwrap();
    assert_eq!(changed.next().await.unwrap().args().unwrap().revision, 3);
    // Same request: same ID, nothing added. Changed content: Conflict.
    assert_eq!(
        proxy
            .add("NAS", "http://nas:5000/", "", &request)
            .await
            .unwrap(),
        id
    );
    assert_eq!(
        error(proxy.add("Other", "http://nas:5000/", "", &request).await),
        "Conflict"
    );
    assert_eq!(
        error(
            proxy
                .add(
                    "x",
                    "http://public.example/",
                    "",
                    &uuid::Uuid::new_v4().to_string()
                )
                .await
        ),
        "InvalidArgument"
    );
    assert_eq!(
        error(
            proxy
                .add("x", "https://ok.example/", "", "not-a-uuid")
                .await
        ),
        "InvalidArgument"
    );
    let (list,) = proxy.list().await.unwrap();
    assert_eq!((list.revision, list.shortcuts.len()), (3, 3));

    assert_eq!(
        error(proxy.update(&id, 2, "NAS", "http://nas:5001/", "").await),
        "Conflict"
    );
    proxy
        .update(&id, 3, "NAS", "http://nas:5001/", "")
        .await
        .unwrap();
    assert_eq!(changed.next().await.unwrap().args().unwrap().revision, 4);
    assert_eq!(error(proxy.remove("missing", 4).await), "NotFound");
    let video = list.shortcuts[0].id.clone();
    proxy.remove(&video, 4).await.unwrap();
    assert_eq!(changed.next().await.unwrap().args().unwrap().revision, 5);

    // No BrowserProvider exists yet: Open admits an operation that fails
    // Unavailable, and the same request returns the same operation.
    let open = uuid::Uuid::new_v4().to_string();
    let path = proxy.open(&id, &open).await.unwrap();
    assert_eq!(proxy.open(&id, &open).await.unwrap(), path);
    let operation = ProductionOperationProxy::builder(&caller)
        .path(path)
        .unwrap()
        .build()
        .await
        .unwrap();
    let (info,) = operation.get().await.unwrap();
    assert_eq!(
        (info.state.as_str(), info.error.as_str()),
        ("failed", "Unavailable")
    );
    assert_eq!(
        error(proxy.open(&video, &uuid::Uuid::new_v4().to_string()).await),
        "NotFound"
    );

    // A restarted daemon keeps the list and never re-adds the removed default.
    drop(server);
    let _server = export(&bus, &tree, &roots).await;
    let proxy = WebShortcutsProxy::new(&caller).await.unwrap();
    let (list,) = proxy.list().await.unwrap();
    assert_eq!(list.revision, 5);
    assert!(list.shortcuts.iter().all(|s| s.title != "Video"));
}

#[test]
fn packaged_default_set_is_valid() {
    let tree = Tree(std::env::temp_dir().join(format!("arinn-web-{}", uuid::Uuid::new_v4())));
    fs::create_dir_all(tree.0.join("arinn")).unwrap();
    fs::copy(
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../packaging/web-shortcuts/web-shortcuts.toml"
        ),
        tree.0.join("arinn/web-shortcuts.toml"),
    )
    .unwrap();
    let defaults = shortcuts::load_defaults(std::slice::from_ref(&tree.0)).unwrap();
    assert!(!defaults.is_empty());
    for shortcut in &defaults {
        shortcut.validate().unwrap();
    }
}
