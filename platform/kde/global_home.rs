// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Global Home keys on KDE (#286).
//!
//! The KWin script registers the `ArinnPhase0Home` action with one default
//! sequence, and KGlobalAccel keeps the stored keys on every later
//! registration. When the adapter connects, the daemon therefore writes the
//! active keys itself: `home_shortcut` from `config.toml`, then HomePage.
use crate::{key_sequence, storage::Config};
use std::path::PathBuf;
use zbus::Connection;

const SERVICE: &str = "org.kde.kglobalaccel";
const PATH: &str = "/kglobalaccel";
const INTERFACE: &str = "org.kde.KGlobalAccel";
/// Component and action names, unique then friendly, as KWin registers them
/// for `registerShortcut("ArinnPhase0Home", "Arinn Phase 0: Home", …)`.
const ACTION: [&str; 4] = ["kwin", "ArinnPhase0Home", "KWin", "Arinn Phase 0: Home"];

/// A `QKeySequence` on the bus: four combined key codes, unused ones zero.
type Keys = Vec<(Vec<i32>,)>;

fn warn(error: &str) {
    eprintln!(
        "{}",
        serde_json::json!({"level":"warn","component":"arinnd","provider_id":null,"request_id":null,"operation":"global-home-shortcut","error":error,"duration_ms":0})
    );
}

/// A new adapter connection polled for the first time. On KDE the KWin
/// script has registered its Home action by then; write the configured keys
/// to it in the background.
pub fn adapter_connected(connection: &Connection, config: PathBuf) {
    if crate::platform::adapter_service() != Some("org.kde.KWin") {
        return;
    }
    let connection = connection.clone();
    tokio::spawn(async move {
        let configured = tokio::task::spawn_blocking(move || Config::load(&config))
            .await
            .ok()
            .and_then(Result::ok)
            .map_or_else(
                || key_sequence::DEFAULT_HOME.to_owned(),
                |config| config.home_shortcut,
            );
        apply(&connection, &configured).await;
    });
}

/// The keys written for a `home_shortcut` value, and whether the value was
/// replaced by the default.
pub fn keys(configured: &str) -> (Keys, bool) {
    let (sequences, fallback) = key_sequence::home(configured);
    let keys = sequences
        .into_iter()
        .map(|sequence| (vec![sequence.qt(), 0, 0, 0],))
        .collect();
    (keys, fallback)
}

/// Sets the Home action's active keys. Returns whether every key is active:
/// KGlobalAccel drops a key another action already holds.
pub async fn apply(connection: &Connection, configured: &str) -> bool {
    let (keys, fallback) = keys(configured);
    if fallback {
        warn("InvalidArgument");
    }
    let set = connection
        .call_method(
            Some(SERVICE),
            PATH,
            Some(INTERFACE),
            "setForeignShortcutKeys",
            &(ACTION.to_vec(), keys.clone()),
        )
        .await;
    if set.is_err() {
        warn("Unavailable");
        return false;
    }
    let active: Option<Keys> = match connection
        .call_method(
            Some(SERVICE),
            PATH,
            Some(INTERFACE),
            "shortcutKeys",
            &(ACTION.to_vec(),),
        )
        .await
    {
        Ok(reply) => reply.body().deserialize().ok(),
        Err(_) => None,
    };
    let applied = active.is_some_and(|active| keys.iter().all(|key| active.contains(key)));
    if !applied {
        // Usually a conflict with another global shortcut.
        warn("Conflict");
    }
    applied
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{BufRead, BufReader},
        process::{Child, Command, Stdio},
        sync::{Arc, Mutex},
    };

    struct Bus(Child);
    impl Drop for Bus {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    type Writes = Arc<Mutex<Vec<(Vec<String>, Keys)>>>;

    /// Records writes and, like KGlobalAccel, drops keys another action holds.
    struct FakeAccel {
        writes: Writes,
        taken: Vec<i32>,
        active: Keys,
    }
    #[zbus::interface(name = "org.kde.KGlobalAccel")]
    impl FakeAccel {
        #[zbus(name = "setForeignShortcutKeys")]
        fn set_foreign_shortcut_keys(&mut self, action: Vec<String>, keys: Keys) {
            self.writes.lock().unwrap().push((action, keys.clone()));
            self.active = keys
                .into_iter()
                .filter(|(codes,)| !self.taken.contains(&codes[0]))
                .collect();
        }
        #[zbus(name = "shortcutKeys")]
        fn shortcut_keys(&self, _action: Vec<String>) -> Keys {
            self.active.clone()
        }
    }

    async fn bus() -> (Bus, String) {
        let mut child = Command::new("dbus-daemon")
            .args(["--session", "--nofork", "--print-address=1"])
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let mut address = String::new();
        BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut address)
            .unwrap();
        (Bus(child), address.trim().to_owned())
    }

    async fn connect(address: &str) -> Connection {
        zbus::connection::Builder::address(address)
            .unwrap()
            .build()
            .await
            .unwrap()
    }

    /// A bus with a fake KGlobalAccel; returns the daemon's connection.
    async fn fixture(taken: Vec<i32>) -> (Bus, Connection, Connection, Writes) {
        let (bus, address) = bus().await;
        let writes = Writes::default();
        let accel = FakeAccel {
            writes: Arc::clone(&writes),
            taken,
            active: Vec::new(),
        };
        let service = zbus::connection::Builder::address(address.as_str())
            .unwrap()
            .name(SERVICE)
            .unwrap()
            .serve_at(PATH, accel)
            .unwrap()
            .build()
            .await
            .unwrap();
        (bus, service, connect(&address).await, writes)
    }

    #[tokio::test]
    async fn writes_configured_key_and_home_page() {
        let (_bus, _service, daemon, writes) = fixture(Vec::new()).await;
        assert!(apply(&daemon, "Ctrl+Alt+H").await);
        let writes = writes.lock().unwrap();
        assert_eq!(writes.len(), 1);
        assert_eq!(writes[0].0, ACTION);
        assert_eq!(
            writes[0].1,
            [(vec![0x0C00_0048, 0, 0, 0],), (vec![0x0100_0090, 0, 0, 0],)]
        );
    }

    #[tokio::test]
    async fn invalid_value_writes_the_default() {
        let (_bus, _service, daemon, writes) = fixture(Vec::new()).await;
        assert!(apply(&daemon, "Backspace").await);
        assert_eq!(writes.lock().unwrap()[0].1, keys("Meta+F12").0);
    }

    #[tokio::test]
    async fn reports_a_key_taken_by_another_action() {
        let (_bus, _service, daemon, _writes) = fixture(vec![0x1100_003b]).await;
        assert!(!apply(&daemon, "Meta+F12").await);
    }

    #[tokio::test]
    async fn missing_kglobalaccel_is_reported_not_fatal() {
        let (_bus, address) = bus().await;
        assert!(!apply(&connect(&address).await, "Meta+F12").await);
    }
}
