// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use arinn_plugin_desktop::launcher::launch_on;
use arinn_protocol::validation::ErrorCode;
use std::{
    collections::HashMap,
    io::{BufRead, BufReader},
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicI32, AtomicU64, Ordering},
    },
};
use zbus::{
    Connection,
    zvariant::{OwnedObjectPath, OwnedValue},
};
struct Bus(Child, String);
impl Bus {
    fn start() -> Self {
        let mut c = Command::new("dbus-daemon")
            .args(["--session", "--nofork", "--print-address=1"])
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let mut address = String::new();
        BufReader::new(c.stdout.take().unwrap())
            .read_line(&mut address)
            .unwrap();
        Self(c, address.trim().into())
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
#[derive(Clone, Default)]
struct Service {
    ended: Arc<AtomicU64>,
    status: Arc<AtomicI32>,
}
#[zbus::interface(name = "org.freedesktop.systemd1.Service")]
impl Service {
    #[zbus(property)]
    fn exec_main_exit_timestamp_monotonic(&self) -> u64 {
        self.ended.load(Ordering::SeqCst)
    }
    #[zbus(property)]
    fn exec_main_code(&self) -> i32 {
        libc::CLD_EXITED
    }
    #[zbus(property)]
    fn exec_main_status(&self) -> i32 {
        self.status.load(Ordering::SeqCst)
    }
}
struct Manager {
    bus_address: String,
    expected: PathBuf,
    service: Service,
}
#[zbus::interface(name = "org.freedesktop.systemd1.Manager")]
impl Manager {
    async fn start_transient_unit(
        &self,
        name: &str,
        mode: &str,
        properties: Vec<(String, OwnedValue)>,
        auxiliary: Vec<(String, Vec<(String, OwnedValue)>)>,
    ) -> zbus::fdo::Result<OwnedObjectPath> {
        assert!(name.starts_with("app-arinn-"));
        assert_eq!(mode, "fail");
        assert!(auxiliary.is_empty());
        let mut p: HashMap<_, _> = properties.into_iter().collect();
        for (key, value) in [
            ("Type", "exec"),
            ("ExitType", "cgroup"),
            ("Slice", "app.slice"),
            ("CollectMode", "inactive-or-failed"),
            ("StandardOutput", "null"),
            ("StandardError", "null"),
        ] {
            assert_eq!(String::try_from(p.remove(key).unwrap()).unwrap(), value);
        }
        assert!(bool::try_from(p.remove("AddRef").unwrap()).unwrap());
        assert!(!p.contains_key("Environment"));
        assert!(!p.contains_key("PartOf"));
        assert!(!p.contains_key("BindsTo"));
        let command: Vec<(String, Vec<String>, Vec<String>)> =
            p.remove("ExecStartEx").unwrap().try_into().unwrap();
        assert_eq!(command.len(), 1);
        let (executable, args, flags) = &command[0];
        assert_eq!(executable, "/usr/bin/gio");
        assert_eq!(
            args,
            &["/usr/bin/gio", "launch", self.expected.to_str().unwrap()]
        );
        assert_eq!(flags, &["no-env-expand"]);
        self.service.ended.store(0, Ordering::SeqCst);
        let service = self.service.clone();
        let path = self.expected.clone();
        let bus_address = self.bus_address.clone();
        tokio::task::spawn_blocking(move || {
            let status = Command::new("/usr/bin/gio")
                .env("DBUS_SESSION_BUS_ADDRESS", bus_address)
                .arg("launch")
                .arg(path)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .unwrap();
            service
                .status
                .store(status.code().unwrap_or(1), Ordering::SeqCst);
            service.ended.store(1, Ordering::SeqCst);
        });
        Ok("/org/freedesktop/systemd1/job/test".try_into().unwrap())
    }
    fn get_unit(&self, _name: &str) -> OwnedObjectPath {
        "/org/freedesktop/systemd1/unit/test".try_into().unwrap()
    }
    fn unref_unit(&self, _name: &str) {}
}
struct Tree(PathBuf);
impl Drop for Tree {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
#[tokio::test]
async fn system_launcher_dispatch_failure_and_argument_isolation() {
    let dir = Tree(std::env::temp_dir().join(format!("arinn-launch-{}", uuid::Uuid::new_v4())));
    std::fs::create_dir(&dir.0).unwrap();
    let desktop = dir.0.join("literal $HOME ; test.desktop");
    let marker = dir.0.join("marker");
    let helper = dir.0.join("helper.sh");
    std::fs::write(
        &helper,
        format!("#!/bin/sh\nprintf '%s' \"$1\" > '{}'\n", marker.display()),
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&helper, std::fs::Permissions::from_mode(0o700)).unwrap();
    std::fs::write(
        &desktop,
        format!(
            "[Desktop Entry]\nType=Application\nName=Fixture\nExec={} %c %U\n",
            helper.display()
        ),
    )
    .unwrap();
    let bus = Bus::start();
    let server = bus.connection().await;
    let client = bus.connection().await;
    let service = Service::default();
    server
        .object_server()
        .at("/org/freedesktop/systemd1/unit/test", service.clone())
        .await
        .unwrap();
    server
        .object_server()
        .at(
            "/org/freedesktop/systemd1",
            Manager {
                bus_address: bus.1.clone(),
                expected: desktop.clone(),
                service,
            },
        )
        .await
        .unwrap();
    server
        .request_name("org.freedesktop.systemd1")
        .await
        .unwrap();
    assert_eq!(launch_on(&client, &desktop).await, Ok(()));
    for _ in 0..100 {
        if marker.exists() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert_eq!(std::fs::read_to_string(&marker).unwrap(), "Fixture");
    std::fs::write(
        &desktop,
        "[Desktop Entry]\nType=Application\nName=Broken\nExec=/arinn-test-missing-program\n",
    )
    .unwrap();
    assert_eq!(
        launch_on(&client, &desktop).await,
        Err(ErrorCode::Unavailable)
    );
    assert_eq!(
        launch_on(&client, std::path::Path::new("relative.desktop")).await,
        Err(ErrorCode::InvalidArgument)
    );
}

struct Activation(Arc<AtomicU64>);
#[zbus::interface(name = "org.freedesktop.Application")]
impl Activation {
    fn activate(&self, _platform_data: HashMap<String, OwnedValue>) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

#[tokio::test]
async fn dbus_activatable_entry_uses_private_session_application() {
    let dir = Tree(std::env::temp_dir().join(format!("arinn-activation-{}", uuid::Uuid::new_v4())));
    std::fs::create_dir(&dir.0).unwrap();
    let desktop = dir.0.join("org.arinn.DesktopFixture.desktop");
    std::fs::write(
        &desktop,
        "[Desktop Entry]\nType=Application\nName=Fixture\nDBusActivatable=true\n",
    )
    .unwrap();
    let bus = Bus::start();
    let application = bus.connection().await;
    let activated = Arc::new(AtomicU64::new(0));
    application
        .object_server()
        .at("/org/arinn/DesktopFixture", Activation(activated.clone()))
        .await
        .unwrap();
    application
        .request_name("org.arinn.DesktopFixture")
        .await
        .unwrap();
    let server = bus.connection().await;
    let service = Service::default();
    server
        .object_server()
        .at("/org/freedesktop/systemd1/unit/test", service.clone())
        .await
        .unwrap();
    server
        .object_server()
        .at(
            "/org/freedesktop/systemd1",
            Manager {
                bus_address: bus.1.clone(),
                expected: desktop.clone(),
                service,
            },
        )
        .await
        .unwrap();
    server
        .request_name("org.freedesktop.systemd1")
        .await
        .unwrap();
    let provider = bus.connection().await;
    assert_eq!(launch_on(&provider, &desktop).await, Ok(()));
    drop(provider);
    assert_eq!(activated.load(Ordering::SeqCst), 1);
    // Activation belongs to the application's own bus identity and survives
    // the caller connection. This is a lifetime assertion, not a cgroup claim.
    assert!(
        zbus::fdo::DBusProxy::new(&server)
            .await
            .unwrap()
            .name_has_owner("org.arinn.DesktopFixture".try_into().unwrap())
            .await
            .unwrap()
    );
}
