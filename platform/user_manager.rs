// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Whether the session bus exposes a working systemd user manager.
use zbus::Connection;

/// `true` when `org.freedesktop.systemd1` is owned and its Manager object answers.
pub async fn available(connection: &Connection) -> bool {
    let bus = match zbus::fdo::DBusProxy::new(connection).await {
        Ok(bus) => bus,
        Err(_) => return false,
    };
    let systemd = match "org.freedesktop.systemd1".try_into() {
        Ok(name) => name,
        Err(_) => return false,
    };
    if !bus.name_has_owner(systemd).await.unwrap_or(false) {
        return false;
    }
    zbus::Proxy::new(
        connection,
        "org.freedesktop.systemd1",
        "/org/freedesktop/systemd1",
        "org.freedesktop.systemd1.Manager",
    )
    .await
    .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader};
    use std::process::{Command, Stdio};

    struct Bus(std::process::Child, String);
    impl Drop for Bus {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    fn bus() -> Bus {
        let mut child = Command::new("dbus-daemon")
            .args(["--session", "--nofork", "--print-address=1"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let mut address = String::new();
        BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut address)
            .unwrap();
        Bus(child, address.trim().to_owned())
    }
    async fn connect(bus: &Bus) -> Connection {
        zbus::connection::Builder::address(bus.1.as_str())
            .unwrap()
            .build()
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn absent_without_systemd_on_the_bus() {
        let bus = bus();
        let connection = connect(&bus).await;
        assert!(!available(&connection).await);
    }

    #[tokio::test]
    async fn present_when_manager_object_is_served() {
        let bus = bus();
        let server = connect(&bus).await;
        let client = connect(&bus).await;
        server
            .object_server()
            .at("/org/freedesktop/systemd1", ManagerImpl)
            .await
            .unwrap();
        server
            .request_name("org.freedesktop.systemd1")
            .await
            .unwrap();
        assert!(available(&client).await);
    }

    struct ManagerImpl;
    #[zbus::interface(name = "org.freedesktop.systemd1.Manager")]
    impl ManagerImpl {
        fn noop(&self) {}
    }
}
