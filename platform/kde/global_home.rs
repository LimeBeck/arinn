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

/// The action that holds `key` in KGlobalAccel, if another one does. A
/// failed lookup counts as free: the write below still reports a missing
/// service.
async fn held_elsewhere(connection: &Connection, key: &(Vec<i32>,)) -> bool {
    let owner: Vec<String> = match connection
        .call_method(Some(SERVICE), PATH, Some(INTERFACE), "actionList", key)
        .await
    {
        Ok(reply) => reply.body().deserialize().unwrap_or_default(),
        Err(_) => return false,
    };
    owner.len() >= 2 && owner[..2] != ACTION[..2]
}

/// Sets the Home action's active keys. Returns whether every key is active.
///
/// KGlobalAccel stores a key that another action already holds but never
/// triggers Home with it, so such keys are checked with `actionList` first,
/// left out and reported as a conflict. When no key is free, the stored keys
/// stay as they are.
pub async fn apply(connection: &Connection, configured: &str) -> bool {
    let (keys, fallback) = keys(configured);
    if fallback {
        warn("InvalidArgument");
    }
    let total = keys.len();
    let mut free = Vec::new();
    for key in keys {
        if !held_elsewhere(connection, &key).await {
            free.push(key);
        }
    }
    let conflict = free.len() < total;
    if conflict {
        warn("Conflict");
    }
    if free.is_empty() {
        return false;
    }
    let set = connection
        .call_method(
            Some(SERVICE),
            PATH,
            Some(INTERFACE),
            "setForeignShortcutKeys",
            &(ACTION.to_vec(), free.clone()),
        )
        .await;
    if set.is_err() {
        warn("Unavailable");
        return false;
    }
    // KGlobalAccel ignores a write for an action it does not know.
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
    let stored = active.is_some_and(|active| free.iter().all(|key| active.contains(key)));
    if !stored {
        warn("NotFound");
    }
    stored && !conflict
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

    /// Records writes. Like kglobalacceld it stores every key, even one
    /// another action holds; `actionList` names that other action.
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
            self.active = keys;
        }
        #[zbus(name = "actionList")]
        fn action_list(&self, key: (Vec<i32>,)) -> Vec<String> {
            if self.taken.contains(&key.0[0]) {
                ["kwin", "Other Action", "KWin", "Other"]
                    .map(String::from)
                    .to_vec()
            } else if self.active.contains(&key) {
                ACTION.map(String::from).to_vec()
            } else {
                Vec::new()
            }
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
    async fn a_key_held_by_another_action_is_left_out() {
        // kglobalacceld would store Meta+F12 and never trigger Home with it.
        let (_bus, _service, daemon, writes) = fixture(vec![0x1100_003b]).await;
        assert!(!apply(&daemon, "Meta+F12").await);
        assert_eq!(writes.lock().unwrap()[0].1, [(vec![0x0100_0090, 0, 0, 0],)]);
    }

    #[tokio::test]
    async fn keys_this_action_already_holds_are_written_again() {
        let (_bus, _service, daemon, writes) = fixture(Vec::new()).await;
        assert!(apply(&daemon, "Ctrl+Alt+H").await);
        assert!(apply(&daemon, "Ctrl+Alt+H").await);
        assert_eq!(writes.lock().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn nothing_is_written_when_every_key_is_held() {
        let (_bus, _service, daemon, writes) = fixture(vec![0x1100_003b, 0x0100_0090]).await;
        assert!(!apply(&daemon, "Meta+F12").await);
        assert!(writes.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn missing_kglobalaccel_is_reported_not_fatal() {
        let (_bus, address) = bus().await;
        assert!(!apply(&connect(&address).await, "Meta+F12").await);
    }
}
