// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The remote's power key on KDE (spec §5.7 *Power button*, ADR-0111, #481).
//!
//! The KWin script registers `ArinnPower` without a key. When the adapter
//! connects, after the Home and quick-menu keys, the daemon gives it the
//! power keys (`key_sequence::power`). PowerDevil holds them through its own
//! KGlobalAccel actions, so for this KGlobalAccel run the daemon marks those
//! actions not present (`setInactive`), as KGlobalAccel does when an
//! application quits. That writes nothing to `kglobalshortcutsrc`: the user's
//! keys stay as they are, and a new login starts with them active. A key
//! `ArinnPower` does not get is given back to PowerDevil at once. While
//! `ArinnPower` holds a key, the daemon also holds logind's
//! `handle-power-key` inhibitor.
//!
//! `release` gives the keys back; `arinnd --release-power-key` runs it when
//! the daemon or the KWin adapter stops.
use crate::{
    key_sequence::{self, Sequence},
    power_inhibitor::{self, Inhibitor},
};
use std::{sync::Arc, time::Duration};
use zbus::Connection;

const SERVICE: &str = "org.kde.kglobalaccel";
const PATH: &str = "/kglobalaccel";
const INTERFACE: &str = "org.kde.KGlobalAccel";
/// `registerShortcut("ArinnPower", "Arinn: Power menu", "", …)`.
pub const ACTION: [&str; 4] = ["kwin", "ArinnPower", "KWin", "Arinn: Power menu"];
/// The desktop's power-key actions Arinn takes the key from. Only the unique
/// names count for KGlobalAccel; the friendly names are left empty.
const DESKTOP: [[&str; 4]; 2] = [
    ["org_kde_powerdevil", "PowerOff", "", ""],
    ["org_kde_powerdevil", "PowerDown", "", ""],
];
/// `KGlobalAccel::SetPresent`; without `NoAutoloading` KGlobalAccel keeps the
/// keys it stores for an action that is not new and writes nothing.
const SET_PRESENT: u32 = 2;
const TIMEOUT: Duration = Duration::from_secs(2);

/// A `QKeySequence` on the bus: four combined key codes, unused ones zero.
type Keys = Vec<(Vec<i32>,)>;

fn log(level: &str, operation: &str, error: Option<&str>) {
    eprintln!(
        "{}",
        serde_json::json!({"level":level,"component":"arinnd","provider_id":null,"request_id":null,"operation":operation,"error":error,"duration_ms":0})
    );
}

fn encode(sequences: Vec<Sequence>) -> Keys {
    sequences
        .into_iter()
        .map(|sequence| (vec![sequence.qt(), 0, 0, 0],))
        .collect()
}

async fn call<B>(connection: &Connection, method: &str, body: &B) -> Option<zbus::Message>
where
    B: serde::Serialize + zbus::zvariant::DynamicType,
{
    tokio::time::timeout(
        TIMEOUT,
        connection.call_method(Some(SERVICE), PATH, Some(INTERFACE), method, body),
    )
    .await
    .ok()?
    .ok()
}

/// The action KGlobalAccel names for `key`, if any.
async fn holder(connection: &Connection, key: &(Vec<i32>,)) -> Option<Vec<String>> {
    let reply = call(connection, "actionList", key).await?;
    let owner: Vec<String> = reply.body().deserialize().ok()?;
    (owner.len() == 4).then_some(owner)
}

fn is_desktop(action: &[String]) -> bool {
    DESKTOP.iter().any(|desktop| action[..2] == desktop[..2])
}

async fn stored(connection: &Connection, action: &[String]) -> Keys {
    match call(connection, "shortcutKeys", &(action,)).await {
        Some(reply) => reply.body().deserialize().unwrap_or_default(),
        None => Vec::new(),
    }
}

/// Makes a desktop action present again with the keys KGlobalAccel stores.
async fn give_back(connection: &Connection, action: &[String]) -> bool {
    let keys = stored(connection, action).await;
    call(connection, "setShortcutKeys", &(action, keys, SET_PRESENT))
        .await
        .is_some()
}

/// The power keys of one daemon and the inhibitor held for them.
#[derive(Clone, Default)]
pub struct PowerKey {
    inner: Arc<tokio::sync::Mutex<Inner>>,
}
#[derive(Default)]
struct Inner {
    /// The system bus for the inhibitor; opened on first use.
    system: Option<Connection>,
    inhibitor: Option<Inhibitor>,
}
impl PowerKey {
    /// Uses `system` for logind instead of the system bus (tests).
    pub async fn set_system(&self, system: Connection) {
        self.inner.lock().await.system = Some(system);
    }
    /// Whether the logind inhibitor is held.
    pub async fn inhibiting(&self) -> bool {
        self.inner.lock().await.inhibitor.is_some()
    }
    /// A new adapter connection: takes the power keys for `ArinnPower`.
    /// Returns whether it holds every one of them.
    pub async fn connected(&self, connection: &Connection) -> bool {
        let mut inner = self.inner.lock().await;
        let wanted = encode(key_sequence::power());
        let mut free = Vec::new();
        let mut taken: Vec<((Vec<i32>,), Vec<String>)> = Vec::new();
        let mut conflict = false;
        for key in &wanted {
            match holder(connection, key).await {
                None => free.push(key.clone()),
                Some(owner) if owner[..2] == ACTION[..2] => free.push(key.clone()),
                Some(owner) if is_desktop(&owner) => {
                    // Not present for this KGlobalAccel run; nothing is
                    // written, and the stored keys stay the user's.
                    if call(connection, "setInactive", &(&owner,)).await.is_some() {
                        free.push(key.clone());
                        taken.push((key.clone(), owner));
                    } else {
                        conflict = true;
                    }
                }
                // Another action keeps its key (ADR-0094); this includes
                // every other Arinn action, which was written first.
                Some(_) => conflict = true,
            }
        }
        if conflict {
            log("warn", "power-key", Some("Conflict"));
        }
        // Also when no key is free: stale keys of an earlier run go.
        let written = call(
            connection,
            "setForeignShortcutKeys",
            &(ACTION.to_vec(), free.clone()),
        )
        .await
        .is_some();
        let active = if written {
            stored(connection, &ACTION.map(String::from)).await
        } else {
            Vec::new()
        };
        // A key ArinnPower did not get stays the desktop's.
        let mut returned = Vec::new();
        for (key, owner) in &taken {
            if !active.contains(key) && !returned.contains(owner) {
                give_back(connection, owner).await;
                returned.push(owner.clone());
            }
        }
        let held = wanted.iter().filter(|key| active.contains(key)).count();
        if !written {
            log("warn", "power-key", Some("Unavailable"));
        } else if held < free.len() {
            log("warn", "power-key", Some("NotFound"));
        }
        if held == 0 {
            inner.inhibitor = None;
        } else if inner.inhibitor.is_none() {
            if inner.system.is_none() {
                inner.system = Connection::system().await.ok();
            }
            inner.inhibitor = match inner.system.as_ref() {
                Some(system) => power_inhibitor::inhibit(system).await,
                None => None,
            };
            if inner.inhibitor.is_none() {
                log("warn", "power-key-inhibitor", Some("Unavailable"));
            }
        }
        held == wanted.len()
    }
}

/// Gives the power key back to the desktop: clears `ArinnPower`'s keys and
/// makes PowerDevil's actions present again with their stored keys. Writes
/// nothing for an action KGlobalAccel does not know. Returns whether
/// KGlobalAccel answered.
pub async fn release(connection: &Connection) -> bool {
    let dbus = match zbus::fdo::DBusProxy::new(connection).await {
        Ok(dbus) => dbus,
        Err(_) => return false,
    };
    // Never start KGlobalAccel outside KDE.
    if !dbus
        .name_has_owner(SERVICE.try_into().expect("valid name"))
        .await
        .unwrap_or(false)
    {
        return false;
    }
    let arinn = ACTION.map(String::from);
    let mut answered = true;
    if !stored(connection, &arinn).await.is_empty() {
        answered &= call(
            connection,
            "setForeignShortcutKeys",
            &(ACTION.to_vec(), Keys::new()),
        )
        .await
        .is_some();
    }
    for desktop in DESKTOP {
        answered &= give_back(connection, &desktop.map(String::from)).await;
    }
    answered
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::power_inhibitor::tests::{Fake, Shared, bus, connect, logind};
    use std::sync::Mutex;

    #[derive(Clone, Debug, PartialEq)]
    struct Shortcut {
        action: Vec<String>,
        keys: Keys,
        present: bool,
        serial: u64,
    }
    /// A KGlobalAccel with shared keys, as kglobalacceld 6.4 and later: an
    /// action stores every key it is given, `actionList` names the oldest
    /// action storing a key, and a press goes to the oldest present one.
    #[derive(Default)]
    struct Accel {
        shortcuts: Vec<Shortcut>,
        /// Calls that change state, in order.
        writes: Vec<String>,
        /// An older KGlobalAccel: a key another action stores is dropped.
        exclusive: bool,
    }
    impl Accel {
        fn find(&mut self, action: &[String]) -> Option<&mut Shortcut> {
            self.shortcuts
                .iter_mut()
                .find(|s| s.action[..2] == action[..2])
        }
        /// The action a press of `key` triggers.
        fn pressed(&self, code: i32) -> Option<String> {
            self.shortcuts
                .iter()
                .filter(|s| s.present && s.keys.iter().any(|k| k.0[0] == code))
                .min_by_key(|s| s.serial)
                .map(|s| s.action[1].clone())
        }
    }
    type Shared2 = std::sync::Arc<Mutex<Accel>>;
    struct Service(Shared2);
    #[zbus::interface(name = "org.kde.KGlobalAccel")]
    impl Service {
        #[zbus(name = "actionList")]
        fn action_list(&self, key: (Vec<i32>,)) -> Vec<String> {
            let accel = self.0.lock().unwrap();
            accel
                .shortcuts
                .iter()
                .filter(|s| s.keys.contains(&key))
                .min_by_key(|s| s.serial)
                .map(|s| s.action.clone())
                .unwrap_or_default()
        }
        #[zbus(name = "shortcutKeys")]
        fn shortcut_keys(&self, action: Vec<String>) -> Keys {
            let mut accel = self.0.lock().unwrap();
            accel
                .find(&action)
                .map(|s| s.keys.clone())
                .unwrap_or_default()
        }
        #[zbus(name = "setInactive")]
        fn set_inactive(&self, action: Vec<String>) {
            let mut accel = self.0.lock().unwrap();
            accel.writes.push(format!("inactive {}", action[1]));
            if let Some(s) = accel.find(&action) {
                s.present = false;
            }
        }
        #[zbus(name = "setShortcutKeys")]
        fn set_shortcut_keys(&self, action: Vec<String>, keys: Keys, flags: u32) -> Keys {
            let mut accel = self.0.lock().unwrap();
            accel.writes.push(format!("present {} {flags}", action[1]));
            // Autoloading an existing action: only presence changes.
            match accel.find(&action) {
                Some(s) if flags == SET_PRESENT => {
                    s.present = true;
                    s.keys.clone()
                }
                Some(s) => {
                    s.keys = keys;
                    s.keys.clone()
                }
                None => Keys::new(),
            }
        }
        #[zbus(name = "setForeignShortcutKeys")]
        fn set_foreign_shortcut_keys(&self, action: Vec<String>, keys: Keys) {
            let mut accel = self.0.lock().unwrap();
            accel
                .writes
                .push(format!("keys {} {}", action[1], keys.len()));
            let exclusive = accel.exclusive;
            let others: Vec<(Vec<i32>,)> = accel
                .shortcuts
                .iter()
                .filter(|s| s.action[..2] != action[..2])
                .flat_map(|s| s.keys.clone())
                .collect();
            if let Some(s) = accel.find(&action) {
                s.keys = keys
                    .into_iter()
                    .filter(|key| !exclusive || !others.contains(key))
                    .collect();
            }
        }
    }

    const POWER_OFF: i32 = 0x0100_00b7;
    const POWER_DOWN: i32 = 0x0100_010b;
    fn key(code: i32) -> (Vec<i32>,) {
        (vec![code, 0, 0, 0],)
    }
    fn shortcut(action: [&str; 4], keys: &[i32], serial: u64) -> Shortcut {
        Shortcut {
            action: action.map(String::from).to_vec(),
            keys: keys.iter().map(|code| key(*code)).collect(),
            present: true,
            serial,
        }
    }
    fn powerdevil(name: &str, code: i32, serial: u64) -> Shortcut {
        shortcut(
            ["org_kde_powerdevil", name, "Power Management", name],
            &[code],
            serial,
        )
    }
    /// PowerDevil's two actions, registered long before Arinn's.
    fn plasma() -> Vec<Shortcut> {
        vec![
            powerdevil("PowerOff", POWER_OFF, 3),
            powerdevil("PowerDown", POWER_DOWN, 4),
            shortcut(ACTION, &[], 90),
        ]
    }

    struct Fixture {
        _bus: crate::power_inhibitor::tests::Bus,
        _services: Vec<Connection>,
        daemon: Connection,
        accel: Shared2,
        login: Shared,
        key: PowerKey,
    }
    async fn fixture(shortcuts: Vec<Shortcut>) -> Fixture {
        let bus = bus();
        let accel = Shared2::new(Mutex::new(Accel {
            shortcuts,
            ..Default::default()
        }));
        let service = connect(&bus.1).await;
        service
            .object_server()
            .at(PATH, Service(accel.clone()))
            .await
            .unwrap();
        service.request_name(SERVICE).await.unwrap();
        let login = Shared::new(Mutex::new(Fake::default()));
        let logind = logind(&bus, login.clone()).await;
        let daemon = connect(&bus.1).await;
        let key = PowerKey::default();
        key.set_system(connect(&bus.1).await).await;
        Fixture {
            _bus: bus,
            _services: vec![service, logind],
            daemon,
            accel,
            login,
            key,
        }
    }
    impl Fixture {
        fn pressed(&self, code: i32) -> Option<String> {
            self.accel.lock().unwrap().pressed(code)
        }
        fn writes(&self) -> Vec<String> {
            self.accel.lock().unwrap().writes.clone()
        }
    }

    /// Spec §5.7: while Arinn runs, the power key opens Arinn's power menu
    /// and PowerDevil does not act on it; logind is inhibited. Nothing is
    /// written to PowerDevil's keys, and `release` gives the key back.
    #[tokio::test]
    async fn takes_the_key_from_powerdevil_for_the_session_and_gives_it_back() {
        let f = fixture(plasma()).await;
        assert_eq!(f.pressed(POWER_OFF).as_deref(), Some("PowerOff"));
        assert!(f.key.connected(&f.daemon).await);
        assert_eq!(f.pressed(POWER_OFF).as_deref(), Some("ArinnPower"));
        assert_eq!(f.pressed(POWER_DOWN).as_deref(), Some("ArinnPower"));
        assert_eq!(
            f.writes(),
            [
                "inactive PowerOff",
                "inactive PowerDown",
                "keys ArinnPower 2"
            ]
        );
        // PowerDevil still stores its keys: nothing of the user's changed.
        let accel = f.accel.lock().unwrap().shortcuts.clone();
        assert_eq!(accel[0].keys, [key(POWER_OFF)]);
        assert_eq!(accel[1].keys, [key(POWER_DOWN)]);
        assert!(f.key.inhibiting().await);
        assert_eq!(
            f.login.lock().unwrap().calls[0].0,
            crate::power_inhibitor::WHAT
        );
        assert_eq!(f.login.lock().unwrap().open(), 1);
        // A new adapter connection keeps one inhibitor.
        assert!(f.key.connected(&f.daemon).await);
        assert_eq!(f.login.lock().unwrap().calls.len(), 1);
        assert_eq!(f.pressed(POWER_OFF).as_deref(), Some("ArinnPower"));
        // Arinn stops: the desktop handles the key again.
        assert!(release(&f.daemon).await);
        assert_eq!(f.pressed(POWER_OFF).as_deref(), Some("PowerOff"));
        assert_eq!(f.pressed(POWER_DOWN).as_deref(), Some("PowerDown"));
        let accel = f.accel.lock().unwrap().shortcuts.clone();
        assert!(accel[2].keys.is_empty());
        assert_eq!(accel[0].keys, [key(POWER_OFF)]);
        // Autoloading presence only: flags SetPresent without NoAutoloading.
        assert!(f.writes().ends_with(&[
            "keys ArinnPower 0".to_owned(),
            "present PowerOff 2".to_owned(),
            "present PowerDown 2".to_owned(),
        ]));
    }

    /// An older KGlobalAccel refuses a key another action stores: the desktop
    /// keeps it, and logind is not inhibited for a key Arinn does not own.
    #[tokio::test]
    async fn a_key_arinn_does_not_get_stays_the_desktops() {
        let f = fixture(plasma()).await;
        f.accel.lock().unwrap().exclusive = true;
        assert!(!f.key.connected(&f.daemon).await);
        assert_eq!(f.pressed(POWER_OFF).as_deref(), Some("PowerOff"));
        assert_eq!(f.pressed(POWER_DOWN).as_deref(), Some("PowerDown"));
        assert!(!f.key.inhibiting().await);
        assert!(f.login.lock().unwrap().calls.is_empty());
    }

    /// A third-party action keeps its key (ADR-0094); a free key is taken.
    #[tokio::test]
    async fn another_action_keeps_its_key() {
        let f = fixture(vec![
            shortcut(["kwin", "Other", "KWin", "Other"], &[POWER_OFF], 2),
            shortcut(ACTION, &[], 90),
        ])
        .await;
        assert!(!f.key.connected(&f.daemon).await);
        assert_eq!(f.pressed(POWER_OFF).as_deref(), Some("Other"));
        assert_eq!(f.pressed(POWER_DOWN).as_deref(), Some("ArinnPower"));
        assert!(!f.writes().iter().any(|w| w.starts_with("inactive")));
        assert!(f.key.inhibiting().await);
    }

    /// Without PowerDevil the keys are free; logind is still inhibited, so
    /// its default power-off never runs next to the menu.
    #[tokio::test]
    async fn free_keys_are_taken_and_logind_is_inhibited() {
        let f = fixture(vec![shortcut(ACTION, &[], 90)]).await;
        assert!(f.key.connected(&f.daemon).await);
        assert_eq!(f.pressed(POWER_OFF).as_deref(), Some("ArinnPower"));
        assert!(f.key.inhibiting().await);
        // release names PowerDevil's actions; unknown ones change nothing.
        assert!(release(&f.daemon).await);
        assert_eq!(f.pressed(POWER_OFF), None);
    }

    /// Without KGlobalAccel nothing is taken, started or inhibited.
    #[tokio::test]
    async fn missing_kglobalaccel_is_reported_not_fatal() {
        let bus = bus();
        let login = Shared::new(Mutex::new(Fake::default()));
        let _logind = logind(&bus, login.clone()).await;
        let daemon = connect(&bus.1).await;
        let key = PowerKey::default();
        key.set_system(connect(&bus.1).await).await;
        assert!(!key.connected(&daemon).await);
        assert!(!key.inhibiting().await);
        assert!(!release(&daemon).await);
        assert!(login.lock().unwrap().calls.is_empty());
    }
}
