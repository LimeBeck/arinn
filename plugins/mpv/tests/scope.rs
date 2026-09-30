// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The mpv unit is its own application scope and does not reference the plugin.

use arinn_plugin_mpv::{UNIT, arguments, ensure_unit, socket_is_private};
use arinn_protocol::validation::ErrorCode;
use std::{
    collections::HashMap,
    io::{BufRead, BufReader},
    path::Path,
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};
use zbus::{Connection, zvariant::OwnedValue};

struct Bus(std::process::Child, String);
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
    async fn connection(&self) -> Connection {
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

struct Manager {
    starts: Arc<AtomicUsize>,
    active: Arc<AtomicBool>,
    socket: String,
}

#[zbus::interface(name = "org.freedesktop.systemd1.Manager")]
impl Manager {
    async fn start_transient_unit(
        &self,
        name: &str,
        mode: &str,
        properties: Vec<(String, OwnedValue)>,
        auxiliary: Vec<(String, Vec<(String, OwnedValue)>)>,
    ) -> zbus::fdo::Result<zbus::zvariant::OwnedObjectPath> {
        assert_eq!(name, UNIT);
        assert_eq!(mode, "fail");
        assert!(auxiliary.is_empty());
        let mut properties: HashMap<_, _> = properties.into_iter().collect();
        for (key, value) in [
            ("Type", "exec"),
            ("ExitType", "cgroup"),
            ("Slice", "app.slice"),
            ("CollectMode", "inactive-or-failed"),
            ("StandardOutput", "null"),
            ("StandardError", "null"),
            ("Description", "Arinn playback"),
        ] {
            assert_eq!(
                String::try_from(properties.remove(key).unwrap()).unwrap(),
                value,
                "{key}"
            );
        }
        assert!(!bool::try_from(properties.remove("AddRef").unwrap()).unwrap());
        assert!(!properties.contains_key("Environment"));
        assert!(!properties.contains_key("PartOf"));
        assert!(!properties.contains_key("BindsTo"));
        let command: Vec<(String, Vec<String>, Vec<String>)> = properties
            .remove("ExecStartEx")
            .unwrap()
            .try_into()
            .unwrap();
        let (executable, args, flags) = &command[0];
        assert_eq!(executable, "/usr/bin/mpv");
        assert_eq!(args[0], "/usr/bin/mpv");
        assert!(args.iter().any(|arg| arg == "--input-default-bindings=no"));
        assert!(args.iter().any(|arg| arg == "--osc=no"));
        assert!(args.iter().any(|arg| arg == "--osd-level=0"));
        assert!(args.iter().any(|arg| arg == "--no-config"));
        assert!(
            args.iter()
                .any(|arg| arg == &format!("--input-ipc-server={}", self.socket))
        );
        assert!(args.iter().all(|arg| !arg.contains("clip.mkv")));
        assert_eq!(flags, &vec!["no-env-expand".to_string()]);
        self.starts.fetch_add(1, Ordering::SeqCst);
        Ok("/org/freedesktop/systemd1/job/1".try_into().unwrap())
    }

    fn get_unit(&self, name: &str) -> zbus::fdo::Result<zbus::zvariant::OwnedObjectPath> {
        assert_eq!(name, UNIT);
        if self.active.load(Ordering::SeqCst) {
            Ok("/org/freedesktop/systemd1/unit/mpv".try_into().unwrap())
        } else {
            Err(zbus::fdo::Error::Failed("no unit".into()))
        }
    }

    fn reset_failed_unit(&self, _name: &str) {}
}

struct Unit {
    active: Arc<AtomicBool>,
}
#[zbus::interface(name = "org.freedesktop.systemd1.Unit")]
impl Unit {
    #[zbus(property)]
    fn active_state(&self) -> String {
        if self.active.load(Ordering::SeqCst) {
            "active".into()
        } else {
            "inactive".into()
        }
    }
}

#[tokio::test]
async fn transient_unit_is_unreferenced_and_not_restarted_while_active() {
    assert!(!socket_is_private(Path::new("/tmp/ipc")));
    assert!(
        arguments(Path::new("/run/user/1000/arinn/mpv/ipc"))
            .iter()
            .any(|arg| arg.starts_with("--input-ipc-server=/run/user/1000/"))
    );
    let bus = Bus::start();
    let server = bus.connection().await;
    let client = bus.connection().await;
    let starts = Arc::new(AtomicUsize::new(0));
    let active = Arc::new(AtomicBool::new(false));
    let socket = "/run/user/1000/arinn/mpv/ipc".to_string();
    server
        .object_server()
        .at(
            "/org/freedesktop/systemd1",
            Manager {
                starts: Arc::clone(&starts),
                active: Arc::clone(&active),
                socket: socket.clone(),
            },
        )
        .await
        .unwrap();
    server
        .object_server()
        .at(
            "/org/freedesktop/systemd1/unit/mpv",
            Unit {
                active: Arc::clone(&active),
            },
        )
        .await
        .unwrap();
    server
        .request_name("org.freedesktop.systemd1")
        .await
        .unwrap();
    assert_eq!(
        ensure_unit(&client, Path::new("/tmp/ipc"), Path::new("/tmp/sock")).await,
        Err(ErrorCode::InvalidArgument)
    );
    assert_eq!(starts.load(Ordering::SeqCst), 0);
    ensure_unit(&client, Path::new("/usr/bin/mpv"), Path::new(&socket))
        .await
        .unwrap();
    assert_eq!(starts.load(Ordering::SeqCst), 1);
    active.store(true, Ordering::SeqCst);
    ensure_unit(&client, Path::new("/usr/bin/mpv"), Path::new(&socket))
        .await
        .unwrap();
    assert_eq!(starts.load(Ordering::SeqCst), 1);
}

#[test]
fn arguments_disable_player_input() {
    let args = arguments(Path::new("/run/user/1000/arinn/mpv/ipc"));
    for flag in [
        "--no-config",
        "--load-scripts=no",
        "--input-default-bindings=no",
        "--input-vo-keyboard=no",
        "--input-media-keys=no",
        "--no-input-terminal",
        "--osc=no",
        "--osd-level=0",
        "--no-resume-playback",
        "--save-position-on-quit=no",
    ] {
        assert!(args.iter().any(|arg| arg == flag), "{flag}");
    }
}
