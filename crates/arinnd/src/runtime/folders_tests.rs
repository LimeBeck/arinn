// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `org.arinn.Folders1` (ADR-0071): mount table parsing, the folder listing
//! on a temporary tree, and the live export on a private bus.
use super::*;
use arinn_protocol::folders::FoldersApiProxy;
use std::{
    io::{BufRead, BufReader},
    os::unix::{ffi::OsStrExt, fs::PermissionsExt},
    process::{Command, Stdio},
};

fn line(point: &str) -> String {
    format!("36 35 98:0 / {point} rw,noatime master:1 - ext3 /dev/root rw,errors=continue")
}

#[test]
fn mounts_are_unescaped_nested_and_ordered() {
    let table = [
        line("/"),
        line("/home"),
        line("/run/media/fixture/Backup\\040Disk"),
        line("/media/usb"),
        line("/mnt/nas/films"),
        line("/mnt"),
        line("/mediathek"),
        line("/run/media/fixture/backup"),
        line("/media/usb"),
        line("/media/\\377bad"),
    ]
    .join("\n");
    assert_eq!(
        mounts(&table),
        vec![
            "/media/usb".to_string(),
            "/mnt/nas/films".to_string(),
            "/run/media/fixture/backup".to_string(),
            "/run/media/fixture/Backup Disk".to_string(),
        ]
    );
}

#[test]
fn places_start_with_home_and_end_with_root() {
    let table = line("/media/usb");
    let list = places(Some(Path::new("/home/fixture")), &table);
    let kinds: Vec<_> = list
        .iter()
        .map(|p| (p.kind.as_str(), p.path.as_str()))
        .collect();
    assert_eq!(
        kinds,
        vec![
            (PLACE_HOME, "/home/fixture"),
            (PLACE_MOUNT, "/media/usb"),
            (PLACE_ROOT, "/")
        ]
    );
    for place in &list {
        place.validate().unwrap();
    }
    // No usable home: the list still has the root.
    assert_eq!(places(None, "").len(), 1);
    assert_eq!(places(Some(Path::new("relative")), "").len(), 1);
}

struct Tree(PathBuf);
impl Tree {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("arinn-folders-{}", Uuid::new_v4()));
        for folder in ["Films", "music", ".hidden", "Series/Season 1"] {
            std::fs::create_dir_all(root.join(folder)).unwrap();
        }
        std::fs::write(root.join("notes.txt"), "file").unwrap();
        std::os::unix::fs::symlink(root.join("Films"), root.join("Linked")).unwrap();
        std::os::unix::fs::symlink(root.join("notes.txt"), root.join("linked-file")).unwrap();
        std::os::unix::fs::symlink(root.join("gone"), root.join("dangling")).unwrap();
        // A name that is not UTF-8 cannot be shown or passed back.
        std::fs::create_dir(root.join(std::ffi::OsStr::from_bytes(b"bad\xff"))).unwrap();
        Self(root)
    }
}
impl Drop for Tree {
    fn drop(&mut self) {
        let _ = std::fs::set_permissions(
            self.0.join("locked"),
            std::fs::Permissions::from_mode(0o755),
        );
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn only_folders_are_listed() {
    let tree = Tree::new();
    let mut names = subfolders(&tree.0).unwrap();
    names.sort_by(|a, b| folders::order(a, b));
    assert_eq!(names, vec!["Films", "Linked", "music", "Series"]);
    assert_eq!(
        subfolders(&tree.0.join("missing")),
        Err(ErrorCode::NotFound)
    );
    assert_eq!(
        subfolders(&tree.0.join("notes.txt")),
        Err(ErrorCode::NotFound)
    );
    let locked = tree.0.join("locked");
    std::fs::create_dir(&locked).unwrap();
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
    // Root reads any folder; the check applies to an ordinary user only.
    if std::fs::read_dir(&locked).is_err() {
        assert_eq!(subfolders(&locked), Err(ErrorCode::PermissionDenied));
    }
}

#[tokio::test]
async fn export_matches_the_published_folders1_xml_and_lists_pages() {
    let tree = Tree::new();
    let mut bus = Command::new("dbus-daemon")
        .args(["--session", "--nofork", "--print-address=1"])
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut address = String::new();
    BufReader::new(bus.stdout.take().unwrap())
        .read_line(&mut address)
        .unwrap();
    let connect = |address: String| async move {
        zbus::connection::Builder::address(address.trim())
            .unwrap()
            .build()
            .await
            .unwrap()
    };
    let server = connect(address.clone()).await;
    let client = connect(address.clone()).await;
    let mountinfo = tree.0.join("mountinfo");
    std::fs::write(&mountinfo, line("/media/usb")).unwrap();
    server
        .object_server()
        .at(
            production::PATH,
            FoldersEndpoint {
                home: Some(tree.0.clone()),
                mountinfo,
            },
        )
        .await
        .unwrap();
    server.request_name(production::SERVICE).await.unwrap();

    let live = zbus::fdo::IntrospectableProxy::builder(&client)
        .destination(production::SERVICE)
        .unwrap()
        .path(production::PATH)
        .unwrap()
        .build()
        .await
        .unwrap()
        .introspect()
        .await
        .unwrap();
    let file = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/plugin-api/folders1.xml"
    ))
    .unwrap();
    let mut live_methods = super::super::media_routing::tests::methods(&live, "org.arinn.Folders1");
    let mut file_methods = super::super::media_routing::tests::methods(&file, "org.arinn.Folders1");
    live_methods.sort();
    file_methods.sort();
    assert_eq!(live_methods, file_methods);

    let proxy = FoldersApiProxy::new(&client).await.unwrap();
    let list = proxy.places().await.unwrap();
    assert_eq!(list.first().unwrap().path, tree.0.to_str().unwrap());
    assert_eq!(list[1].path, "/media/usb");
    assert_eq!(list.last().unwrap().path, "/");

    let root = tree.0.to_str().unwrap();
    assert_eq!(
        proxy.list(root, "", 2).await.unwrap(),
        (vec!["Films".to_string(), "Linked".to_string()], true)
    );
    assert_eq!(
        proxy.list(root, "Linked", 2).await.unwrap(),
        (vec!["music".to_string(), "Series".to_string()], false)
    );
    let name = |result: zbus::Result<(Vec<String>, bool)>| match result {
        Err(zbus::Error::MethodError(name, _, _)) => name.to_string(),
        other => format!("{other:?}"),
    };
    let missing = format!("{root}/missing");
    assert_eq!(
        name(proxy.list(&missing, "", 5).await),
        "org.arinn.Error1.NotFound"
    );
    for (path, after, limit) in [("relative", "", 5), (root, "", 0), (root, ".hidden", 5)] {
        assert_eq!(
            name(proxy.list(path, after, limit).await),
            "org.arinn.Error1.InvalidArgument",
            "{after:?} {limit}"
        );
    }
    let _ = bus.kill();
    let _ = bus.wait();
}
