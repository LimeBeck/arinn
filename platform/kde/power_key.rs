// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The remote's power key on KDE (spec §5.7 *Power button*, ADR-0111, #481).
//!
//! The KWin script registers `ArinnPower` without a key. When the adapter
//! connects, after the Home and quick-menu keys, the daemon gives it the
//! power keys (`key_sequence::power`). PowerDevil holds them through its own
//! KGlobalAccel actions. KGlobalAccel stores one key for several actions and
//! triggers the oldest present one only from Plasma 6.7 (kglobalacceld
//! 6.6.90); an older one refuses a key another action stores. The daemon
//! therefore writes the keys to `ArinnPower` first and reads them back: only
//! when they are stored does it mark PowerDevil's actions not present for
//! this KGlobalAccel run (`setInactive`), as KGlobalAccel does when an
//! application quits. That writes nothing to `kglobalshortcutsrc`: the
//! user's keys stay as they are, and a new login starts with them active.
//! Before Plasma 6.7 PowerDevil's actions are never touched and the desktop
//! keeps the key (owner decision of 2026-10-25). While `ArinnPower` holds a
//! key, the daemon also holds logind's `handle-power-key` inhibitor.
//!
//! Arinn holds the key only while something can show the menu: the shell
//! owns `org.arinn.Shell1` and the adapter polls (`watch`). When either is
//! lost, `ArinnPower` is cleared, PowerDevil's actions are present again and
//! then the inhibitor is dropped; when both are back, the key is taken
//! again, as it is after PowerDevil restarts and registers its actions
//! anew. `release` gives the keys back; `arinnd --release-power-key` runs it
//! when the daemon or the KWin adapter stops.
use crate::{
    key_sequence::{self, Sequence},
    power_inhibitor::{self, Inhibitor},
};
use futures_util::StreamExt;
use std::{
    future::Future,
    sync::Arc,
    time::{Duration, Instant},
};
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
/// The shell's bus name; its owner is the live shell (`Type=dbus` unit).
pub const SHELL: &str = "org.arinn.Shell1";
/// How often the adapter's liveness is checked between shell events.
const TICK: Duration = Duration::from_secs(2);
/// PowerDevil's bus name. A new owner registers its actions anew, present,
/// some time after it owns the name.
const POWERDEVIL: &str = "org.kde.Solid.PowerManagement";
/// When the key is taken again after PowerDevil got a new owner.
const POWERDEVIL_RETAKES: [Duration; 3] = [
    Duration::ZERO,
    Duration::from_secs(5),
    Duration::from_secs(30),
];
/// A failed inhibitor waits this long before the next attempt, as on X11.
const RETRY: Duration = Duration::from_secs(30);

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
    /// Wakes the watcher: a new adapter connection wrote its other keys.
    wake: Arc<tokio::sync::Notify>,
    /// The watcher runs once per daemon.
    watching: Arc<std::sync::atomic::AtomicBool>,
}
#[derive(Default)]
struct Inner {
    /// The system bus for the inhibitor; opened on first use.
    system: Option<Connection>,
    inhibitor: Option<Inhibitor>,
    /// The key was taken (or found refused) and not given back since.
    taken: bool,
    /// A new adapter connection registered `ArinnPower` anew, or PowerDevil
    /// registered its actions anew: write again.
    retake: bool,
    /// An adapter connection has written its configured keys, so the power
    /// keys may follow them (they are written last).
    ready: bool,
    /// A failed inhibitor waits until then (review of #517, Codex P2).
    retry_at: Option<Instant>,
    /// `RETRY`, shorter in tests.
    retry: Option<Duration>,
}
/// What `take` found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Taken {
    /// `ArinnPower` holds every power key.
    All,
    /// It holds some or none: another action keeps a key, or KGlobalAccel
    /// is missing.
    Partly,
    /// KGlobalAccel refused a key PowerDevil stores: before Plasma 6.7. The
    /// desktop keeps the key and nothing of its was changed.
    Unsupported,
    /// No logind inhibitor: nothing was taken.
    NoInhibitor,
    /// A KGlobalAccel call failed or timed out: everything was rolled back
    /// and the next attempt follows after the retry delay.
    Failed,
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
    /// A new adapter connection has written its configured keys: the power
    /// keys follow (written by the watcher while the shell is alive).
    pub async fn adapter_connected(&self) {
        let mut inner = self.inner.lock().await;
        inner.retake = true;
        inner.ready = true;
        drop(inner);
        self.wake.notify_one();
    }
    /// PowerDevil registered its actions anew: take the key again.
    async fn desktop_registered(&self) {
        self.inner.lock().await.retake = true;
        self.wake.notify_one();
    }
    /// Whether Arinn holds the key now.
    pub async fn taken(&self) -> bool {
        self.inner.lock().await.taken
    }
    /// Takes the key when `wanted` (a live shell and adapter) and it is not
    /// held, or a new adapter or PowerDevil needs it written again; gives
    /// everything back when not `wanted`.
    pub async fn reconcile(&self, connection: &Connection, wanted: bool) {
        let (take, give_back) = {
            let mut inner = self.inner.lock().await;
            let wanted = wanted && inner.ready;
            if !wanted {
                inner.retry_at = None;
            }
            let due = inner.retry_at.is_none_or(|at| Instant::now() >= at);
            let take = wanted && due && (!inner.taken || inner.retake);
            if take {
                inner.retake = false;
            }
            (take, !wanted && inner.taken)
        };
        if take {
            self.take(connection).await;
        } else if give_back {
            self.give_back(connection).await;
        }
    }
    /// Gives the key back to the desktop, then drops the inhibitor: logind
    /// stays inhibited until `ArinnPower` no longer holds the key (review of
    /// #517, Codex P1).
    ///
    /// A release KGlobalAccel did not answer keeps the key counted as taken
    /// and the inhibitor held, so the next check gives it back again (Codex
    /// P1 on #517).
    pub async fn give_back(&self, connection: &Connection) {
        let mut inner = self.inner.lock().await;
        if release(connection).await || !running(connection).await {
            inner.taken = false;
            inner.inhibitor = None;
        } else {
            log("warn", "power-key-release", Some("Unavailable"));
            inner.taken = true;
        }
    }
    /// Follows the shell's bus name and `adapter_alive` for this daemon:
    /// holds the key only while both are alive (ADR-0111). Also takes the
    /// key again when PowerDevil gets a new owner. Runs once.
    pub fn watch<F, A>(&self, connection: &Connection, adapter_alive: F)
    where
        F: Fn() -> A + Send + 'static,
        A: Future<Output = bool> + Send,
    {
        self.watch_every(connection, adapter_alive, TICK, &POWERDEVIL_RETAKES);
    }
    fn watch_every<F, A>(
        &self,
        connection: &Connection,
        adapter_alive: F,
        tick: Duration,
        retakes: &[Duration],
    ) where
        F: Fn() -> A + Send + 'static,
        A: Future<Output = bool> + Send,
    {
        if self
            .watching
            .swap(true, std::sync::atomic::Ordering::SeqCst)
        {
            return;
        }
        let (key, connection) = (self.clone(), connection.clone());
        let retakes = retakes.to_vec();
        tokio::spawn(async move {
            let Ok(dbus) = zbus::fdo::DBusProxy::new(&connection).await else {
                return;
            };
            let Ok(mut changes) = dbus
                .receive_name_owner_changed_with_args(&[(0, SHELL)])
                .await
            else {
                return;
            };
            let Ok(mut desktop) = dbus
                .receive_name_owner_changed_with_args(&[(0, POWERDEVIL)])
                .await
            else {
                return;
            };
            let shell: zbus::names::BusName = SHELL.try_into().expect("valid name");
            // A key an earlier session left with ArinnPower (kglobalacceld
            // gone before the release at logout) goes back at the first
            // check when no shell is alive.
            key.inner.lock().await.taken = true;
            loop {
                let alive = dbus.name_has_owner(shell.clone()).await.unwrap_or(false)
                    && adapter_alive().await;
                key.reconcile(&connection, alive).await;
                tokio::select! {
                    change = changes.next() => if change.is_none() { break },
                    change = desktop.next() => match change {
                        None => break,
                        // A restarted PowerDevil registers its actions present
                        // again, after it owns its name: take the key again now
                        // and a little later (Codex P1 on #516).
                        Some(change) => if change.args().is_ok_and(|a| a.new_owner.is_some()) {
                            for delay in &retakes {
                                let (key, delay) = (key.clone(), *delay);
                                tokio::spawn(async move {
                                    tokio::time::sleep(delay).await;
                                    key.desktop_registered().await;
                                });
                            }
                        },
                    },
                    _ = tokio::time::sleep(tick) => {},
                    _ = key.wake.notified() => {},
                }
            }
        });
    }
    /// Takes the power keys for `ArinnPower`.
    pub async fn take(&self, connection: &Connection) -> Taken {
        let mut inner = self.inner.lock().await;
        // logind must leave the key alone before Arinn takes it: without
        // the inhibitor the key stays the desktop's (review of #517).
        if inner.inhibitor.is_none() {
            if inner.system.is_none() {
                inner.system = Connection::system().await.ok();
            }
            inner.inhibitor = match inner.system.as_ref() {
                Some(system) => power_inhibitor::inhibit(system).await,
                None => None,
            };
        }
        if inner.inhibitor.is_none() {
            log("warn", "power-key-inhibitor", Some("Unavailable"));
            // Not taken: the watcher tries again after a while.
            inner.taken = false;
            inner.retry_at = Some(Instant::now() + inner.retry.unwrap_or(RETRY));
            release(connection).await;
            return Taken::NoInhibitor;
        }
        inner.taken = true;
        inner.retry_at = None;
        let wanted = encode(key_sequence::power());
        let mut free = Vec::new();
        let mut desktop: Vec<((Vec<i32>,), Vec<String>)> = Vec::new();
        let mut conflict = false;
        for key in &wanted {
            match holder(connection, key).await {
                None => free.push(key.clone()),
                Some(owner) if owner[..2] == ACTION[..2] => free.push(key.clone()),
                Some(owner) if is_desktop(&owner) => {
                    free.push(key.clone());
                    desktop.push((key.clone(), owner));
                }
                // Another action keeps its key (ADR-0094); this includes
                // every other Arinn action, which was written first.
                Some(_) => conflict = true,
            }
        }
        if conflict {
            log("warn", "power-key", Some("Conflict"));
        }
        // Also when no key is free: stale keys of an earlier run go. While
        // PowerDevil's actions are present they are older and still win.
        let written = call(
            connection,
            "setForeignShortcutKeys",
            &(ACTION.to_vec(), free.clone()),
        )
        .await
        .is_some();
        if !written {
            // Timed out or refused: nothing is known about what it stored
            // (Codex P1 on #517). Roll back and try again later.
            log("warn", "power-key", Some("Unavailable"));
            rollback(&mut inner, connection, &desktop).await;
            return Taken::Failed;
        }
        let active = stored(connection, &ACTION.map(String::from)).await;
        if desktop.iter().any(|(key, _)| !active.contains(key)) {
            // KGlobalAccel before Plasma 6.7 refuses a key PowerDevil
            // stores. The desktop keeps every power key; PowerDevil's
            // actions were never touched (owner decision of 2026-10-25).
            log("warn", "power-key", Some("Unsupported"));
            // A free key ArinnPower did get goes too: the desktop keeps
            // every power key. Without an answer, the rollback retries.
            if !rollback(&mut inner, connection, &desktop).await {
                return Taken::Failed;
            }
            inner.taken = true;
            inner.retry_at = None;
            return Taken::Unsupported;
        }
        // ArinnPower stores the keys: PowerDevil's actions are not present
        // for this KGlobalAccel run. Nothing is written, and the stored keys
        // stay the user's. An action that stays present is older and keeps
        // the key, so a press still reaches only one of them.
        // A failed `setInactive` would leave a present PowerDevil action
        // and ArinnPower sharing a key: roll everything back instead
        // (review of #517) and try again later.
        let mut inactive: Vec<&Vec<String>> = Vec::new();
        for (_, owner) in &desktop {
            if !inactive.contains(&owner) {
                if call(connection, "setInactive", &(owner,)).await.is_none() {
                    log("warn", "power-key", Some("Conflict"));
                    rollback(&mut inner, connection, &desktop).await;
                    return Taken::Failed;
                }
                inactive.push(owner);
            }
        }
        let held = wanted.iter().filter(|key| active.contains(key)).count();
        if held < free.len() {
            log("warn", "power-key", Some("NotFound"));
        }
        // No key held: nothing for logind to leave alone.
        if held == 0 {
            inner.inhibitor = None;
        }
        if held == wanted.len() {
            Taken::All
        } else {
            Taken::Partly
        }
    }
}

/// Whether KGlobalAccel runs; never starts it.
async fn running(connection: &Connection) -> bool {
    let Ok(dbus) = zbus::fdo::DBusProxy::new(connection).await else {
        return false;
    };
    dbus.name_has_owner(SERVICE.try_into().expect("valid name"))
        .await
        .unwrap_or(false)
}

/// Undoes a take that did not finish: clears `ArinnPower`, makes `desktop`'s
/// actions present again, and drops the inhibitor only once `ArinnPower` is
/// known to be empty. Without that answer the inhibitor stays and the key
/// stays counted as taken, so the next check gives it back or takes it
/// again. Either way a new attempt waits for the retry delay. Returns
/// whether `ArinnPower` was cleared.
async fn rollback(
    inner: &mut Inner,
    connection: &Connection,
    desktop: &[((Vec<i32>,), Vec<String>)],
) -> bool {
    let cleared = call(
        connection,
        "setForeignShortcutKeys",
        &(ACTION.to_vec(), Keys::new()),
    )
    .await
    .is_some()
        // Without KGlobalAccel nothing can hold the key.
        || !running(connection).await;
    let mut returned: Vec<&Vec<String>> = Vec::new();
    for (_, owner) in desktop {
        if !returned.contains(&owner) {
            give_back(connection, owner).await;
            returned.push(owner);
        }
    }
    inner.retry_at = Some(Instant::now() + inner.retry.unwrap_or(RETRY));
    if cleared {
        inner.inhibitor = None;
        inner.taken = false;
    } else {
        log("warn", "power-key-rollback", Some("Unavailable"));
        inner.taken = true;
        inner.retake = true;
    }
    cleared
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
    /// A KGlobalAccel with shared keys, as kglobalacceld 6.6.90 (Plasma
    /// 6.7) and later: an action stores every key it is given, `actionList`
    /// names the oldest action storing a key, and a press goes to the oldest
    /// present one.
    #[derive(Default)]
    struct Accel {
        shortcuts: Vec<Shortcut>,
        /// Calls that change state, in order.
        writes: Vec<String>,
        /// Before Plasma 6.7: a key another action stores is dropped.
        exclusive: bool,
        /// logind, to see whether it is still inhibited when `ArinnPower`
        /// is cleared.
        login: Option<Shared>,
        /// Open inhibitors at each clearing of `ArinnPower`'s keys.
        cleared_open: Vec<usize>,
        /// `setInactive` fails.
        fail_inactive: bool,
        /// `setForeignShortcutKeys` fails (a timeout, for the daemon).
        fail_writes: bool,
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
        fn set_inactive(&self, action: Vec<String>) -> zbus::fdo::Result<()> {
            let mut accel = self.0.lock().unwrap();
            if accel.fail_inactive {
                return Err(zbus::fdo::Error::Failed("fixture".into()));
            }
            accel.writes.push(format!("inactive {}", action[1]));
            if let Some(s) = accel.find(&action) {
                s.present = false;
            }
            Ok(())
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
        fn set_foreign_shortcut_keys(
            &self,
            action: Vec<String>,
            keys: Keys,
        ) -> zbus::fdo::Result<()> {
            let mut accel = self.0.lock().unwrap();
            if accel.fail_writes {
                return Err(zbus::fdo::Error::Failed("fixture".into()));
            }
            accel
                .writes
                .push(format!("keys {} {}", action[1], keys.len()));
            if keys.is_empty()
                && let Some(login) = accel.login.clone()
            {
                let open = login.lock().unwrap().open();
                accel.cleared_open.push(open);
            }
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
            Ok(())
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
        accel.lock().unwrap().login = Some(login.clone());
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
        assert_eq!(f.key.take(&f.daemon).await, Taken::All);
        assert_eq!(f.pressed(POWER_OFF).as_deref(), Some("ArinnPower"));
        assert_eq!(f.pressed(POWER_DOWN).as_deref(), Some("ArinnPower"));
        // The keys are stored first; only then is PowerDevil set aside.
        assert_eq!(
            f.writes(),
            [
                "keys ArinnPower 2",
                "inactive PowerOff",
                "inactive PowerDown"
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
        assert_eq!(f.key.take(&f.daemon).await, Taken::All);
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

    /// Owner decision of 2026-10-25 (review of #516): KGlobalAccel before
    /// Plasma 6.7 refuses a key another action stores. The desktop keeps
    /// every power key, PowerDevil's actions are never set aside, and logind
    /// is not inhibited for a key Arinn does not own. A later attempt (a new
    /// adapter connection) changes nothing either.
    #[tokio::test]
    async fn before_plasma_6_7_the_desktop_keeps_the_key() {
        let f = fixture(plasma()).await;
        f.accel.lock().unwrap().exclusive = true;
        for _ in 0..2 {
            assert_eq!(f.key.take(&f.daemon).await, Taken::Unsupported);
            assert_eq!(f.pressed(POWER_OFF).as_deref(), Some("PowerOff"));
            assert_eq!(f.pressed(POWER_DOWN).as_deref(), Some("PowerDown"));
            assert!(!f.writes().iter().any(|w| w.starts_with("inactive")));
            // The inhibitor taken first is dropped: no key, nothing to inhibit.
            assert!(!f.key.inhibiting().await);
            assert!(eventually(async || f.login.lock().unwrap().open() == 0).await);
        }
        // PowerDevil's keys are as they were.
        let accel = f.accel.lock().unwrap().shortcuts.clone();
        assert_eq!(accel[0].keys, [key(POWER_OFF)]);
        assert!(accel.iter().all(|s| s.present));
        assert!(accel[2].keys.is_empty());
    }

    /// A third-party action keeps its key (ADR-0094); a free key is taken.
    #[tokio::test]
    async fn another_action_keeps_its_key() {
        let f = fixture(vec![
            shortcut(["kwin", "Other", "KWin", "Other"], &[POWER_OFF], 2),
            shortcut(ACTION, &[], 90),
        ])
        .await;
        assert_eq!(f.key.take(&f.daemon).await, Taken::Partly);
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
        assert_eq!(f.key.take(&f.daemon).await, Taken::All);
        assert_eq!(f.pressed(POWER_OFF).as_deref(), Some("ArinnPower"));
        assert!(f.key.inhibiting().await);
        // release names PowerDevil's actions; unknown ones change nothing.
        assert!(release(&f.daemon).await);
        assert_eq!(f.pressed(POWER_OFF), None);
    }

    /// Polls `check` until it holds or 3 s pass.
    async fn eventually<F: AsyncFnMut() -> bool>(mut check: F) -> bool {
        for _ in 0..150 {
            if check().await {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        false
    }

    /// Review of #516: Arinn holds the key only while the shell owns
    /// `org.arinn.Shell1` and the adapter polls. A shell at its start limit
    /// or an adapter that stopped gives PowerDevil the key back and drops
    /// the inhibitor; when both are back the key is taken again, and a new
    /// adapter connection writes it again.
    #[tokio::test]
    async fn the_key_follows_a_live_shell_and_adapter() {
        use std::sync::atomic::{AtomicBool, Ordering};
        let f = fixture(plasma()).await;
        let adapter = Arc::new(AtomicBool::new(true));
        let alive = adapter.clone();
        f.key.watch_every(
            &f.daemon,
            move || {
                let alive = alive.clone();
                async move { alive.load(Ordering::SeqCst) }
            },
            Duration::from_millis(50),
            &[],
        );
        f.key.adapter_connected().await;
        // No shell yet: the desktop keeps the key, nothing is inhibited, and
        // the first check only makes sure PowerDevil's actions are present.
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert_eq!(f.pressed(POWER_OFF).as_deref(), Some("PowerOff"));
        assert!(!f.key.inhibiting().await);
        assert!(f.login.lock().unwrap().calls.is_empty());
        assert!(
            f.writes().iter().all(|w| w.starts_with("present ")),
            "{:?}",
            f.writes()
        );
        let shell = connect(&f._bus.1).await;
        shell.request_name(SHELL).await.unwrap();
        assert!(eventually(async || f.pressed(POWER_OFF).as_deref() == Some("ArinnPower")).await);
        assert!(eventually(async || f.key.inhibiting().await).await);
        // The shell is gone: everything goes back at once.
        shell.release_name(SHELL).await.unwrap();
        assert!(eventually(async || f.pressed(POWER_OFF).as_deref() == Some("PowerOff")).await);
        assert_eq!(f.pressed(POWER_DOWN).as_deref(), Some("PowerDown"));
        assert!(!f.key.inhibiting().await);
        assert!(!f.key.taken().await);
        assert!(eventually(async || f.login.lock().unwrap().open() == 0).await);
        // The shell is back.
        shell.request_name(SHELL).await.unwrap();
        assert!(eventually(async || f.pressed(POWER_OFF).as_deref() == Some("ArinnPower")).await);
        // The adapter stops polling (an unloaded KWin script), and returns.
        adapter.store(false, Ordering::SeqCst);
        assert!(eventually(async || f.pressed(POWER_OFF).as_deref() == Some("PowerOff")).await);
        assert!(!f.key.inhibiting().await);
        adapter.store(true, Ordering::SeqCst);
        assert!(eventually(async || f.pressed(POWER_OFF).as_deref() == Some("ArinnPower")).await);
        // A new adapter connection registered ArinnPower anew: written again.
        let written = |f: &Fixture| {
            f.writes()
                .iter()
                .filter(|w| w.as_str() == "keys ArinnPower 2")
                .count()
        };
        let before = written(&f);
        f.key.adapter_connected().await;
        assert!(eventually(async || written(&f) == before + 1).await);
        // An unchanged state writes nothing more.
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert_eq!(written(&f), before + 1);
    }

    /// Review of #517 (Codex P1): without logind's inhibitor Arinn does not
    /// take the key, so logind and Arinn never both act on one press. The
    /// key is not counted as taken (Codex P2): the next attempt follows
    /// after the retry delay, not only after the shell or adapter flapped.
    #[tokio::test]
    async fn without_the_inhibitor_the_desktop_keeps_the_key_and_retries() {
        let f = fixture(plasma()).await;
        f.key.inner.lock().await.retry = Some(Duration::from_millis(200));
        f.key.adapter_connected().await;
        f.login.lock().unwrap().refuse = true;
        f.key.reconcile(&f.daemon, true).await;
        assert!(!f.key.taken().await);
        assert_eq!(f.pressed(POWER_OFF).as_deref(), Some("PowerOff"));
        assert_eq!(f.pressed(POWER_DOWN).as_deref(), Some("PowerDown"));
        assert!(!f.writes().iter().any(|w| w.starts_with("inactive")));
        assert!(!f.key.inhibiting().await);
        // logind answers again; the retry waits for its delay.
        f.login.lock().unwrap().refuse = false;
        f.key.reconcile(&f.daemon, true).await;
        assert_eq!(f.login.lock().unwrap().calls.len(), 1);
        assert_eq!(f.pressed(POWER_OFF).as_deref(), Some("PowerOff"));
        tokio::time::sleep(Duration::from_millis(250)).await;
        f.key.reconcile(&f.daemon, true).await;
        assert_eq!(f.pressed(POWER_OFF).as_deref(), Some("ArinnPower"));
        assert!(f.key.inhibiting().await);
    }

    /// Review of #517 (Codex P1): giving the key back clears `ArinnPower`
    /// while logind is still inhibited, and drops the inhibitor only then.
    #[tokio::test]
    async fn the_inhibitor_goes_after_the_key() {
        let f = fixture(plasma()).await;
        assert_eq!(f.key.take(&f.daemon).await, Taken::All);
        f.key.give_back(&f.daemon).await;
        assert_eq!(f.accel.lock().unwrap().cleared_open, [1]);
        assert!(!f.key.inhibiting().await);
        assert!(eventually(async || f.login.lock().unwrap().open() == 0).await);
        assert_eq!(f.pressed(POWER_OFF).as_deref(), Some("PowerOff"));
    }

    /// Codex P1 on #516: a restarted PowerDevil registers its actions
    /// present again, after it owns its name. The watcher takes the key
    /// again, also when the registration comes a little later.
    #[tokio::test]
    async fn a_powerdevil_restart_gives_arinn_the_key_again() {
        let f = fixture(plasma()).await;
        f.key.watch_every(
            &f.daemon,
            || async { true },
            Duration::from_secs(60),
            &[Duration::ZERO, Duration::from_millis(300)],
        );
        f.key.adapter_connected().await;
        let shell = connect(&f._bus.1).await;
        shell.request_name(SHELL).await.unwrap();
        assert!(eventually(async || f.pressed(POWER_OFF).as_deref() == Some("ArinnPower")).await);
        // PowerDevil restarts: it owns its name, then registers its actions.
        let powerdevil = connect(&f._bus.1).await;
        powerdevil.request_name(POWERDEVIL).await.unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
        for s in &mut f.accel.lock().unwrap().shortcuts {
            s.present = true;
        }
        assert_eq!(f.pressed(POWER_OFF).as_deref(), Some("PowerOff"));
        assert!(eventually(async || f.pressed(POWER_OFF).as_deref() == Some("ArinnPower")).await);
        assert_eq!(f.pressed(POWER_DOWN).as_deref(), Some("ArinnPower"));
        assert!(f.key.inhibiting().await);
    }

    /// A new adapter connection that gets no key (here a KGlobalAccel that
    /// refuses shared keys, as one restarted after a downgrade) drops the
    /// inhibitor.
    #[tokio::test]
    async fn a_reconnect_without_a_key_drops_the_inhibitor() {
        let f = fixture(plasma()).await;
        assert_eq!(f.key.take(&f.daemon).await, Taken::All);
        assert!(f.key.inhibiting().await);
        {
            let mut accel = f.accel.lock().unwrap();
            accel.exclusive = true;
            for s in &mut accel.shortcuts {
                if s.action[1] == "ArinnPower" {
                    s.keys.clear();
                }
            }
        }
        assert_eq!(f.key.take(&f.daemon).await, Taken::Unsupported);
        assert!(!f.key.inhibiting().await);
        assert!(eventually(async || f.login.lock().unwrap().open() == 0).await);
        assert_eq!(f.pressed(POWER_OFF).as_deref(), Some("PowerOff"));
    }

    /// Review of #517 (mutation K1): before Plasma 6.7 a free key that
    /// `ArinnPower` did get goes too, so the desktop keeps every power key.
    #[tokio::test]
    async fn before_plasma_6_7_a_free_key_is_not_kept_either() {
        let f = fixture(vec![
            powerdevil("PowerOff", POWER_OFF, 3),
            shortcut(ACTION, &[], 90),
        ])
        .await;
        f.accel.lock().unwrap().exclusive = true;
        assert_eq!(f.key.take(&f.daemon).await, Taken::Unsupported);
        assert_eq!(f.pressed(POWER_OFF).as_deref(), Some("PowerOff"));
        assert_eq!(f.pressed(POWER_DOWN), None);
        assert!(f.accel.lock().unwrap().shortcuts[1].keys.is_empty());
        assert!(!f.key.inhibiting().await);
    }

    /// Review of #517 (mutation M10): third-party actions hold every power
    /// key, so Arinn holds none and logind is not inhibited.
    #[tokio::test]
    async fn no_key_held_drops_the_inhibitor() {
        let f = fixture(vec![
            shortcut(
                ["kwin", "Other", "KWin", "Other"],
                &[POWER_OFF, POWER_DOWN],
                2,
            ),
            shortcut(ACTION, &[], 90),
        ])
        .await;
        assert_eq!(f.key.take(&f.daemon).await, Taken::Partly);
        assert_eq!(f.pressed(POWER_OFF).as_deref(), Some("Other"));
        assert!(!f.key.inhibiting().await);
        assert!(eventually(async || f.login.lock().unwrap().open() == 0).await);
    }

    /// Review of #517: a failed `setInactive` would leave a present
    /// PowerDevil action sharing a key with `ArinnPower`. Everything rolls
    /// back (PowerDevil keeps the key, `ArinnPower` is empty, the inhibitor
    /// goes), and the next attempt after the delay takes the key.
    #[tokio::test]
    async fn a_failed_set_inactive_rolls_back_and_retries() {
        let f = fixture(plasma()).await;
        f.key.inner.lock().await.retry = Some(Duration::from_millis(200));
        f.key.adapter_connected().await;
        f.accel.lock().unwrap().fail_inactive = true;
        f.key.reconcile(&f.daemon, true).await;
        assert_eq!(f.pressed(POWER_OFF).as_deref(), Some("PowerOff"));
        assert!(f.accel.lock().unwrap().shortcuts[2].keys.is_empty());
        assert!(!f.key.inhibiting().await);
        assert!(!f.key.taken().await);
        f.accel.lock().unwrap().fail_inactive = false;
        f.key.reconcile(&f.daemon, true).await;
        assert_eq!(f.pressed(POWER_OFF).as_deref(), Some("PowerOff"));
        tokio::time::sleep(Duration::from_millis(250)).await;
        f.key.reconcile(&f.daemon, true).await;
        assert_eq!(f.pressed(POWER_OFF).as_deref(), Some("ArinnPower"));
    }

    /// Codex P1 on #517: a write KGlobalAccel did not answer leaves nothing
    /// counted as done. PowerDevil keeps the key, the inhibitor stays while
    /// `ArinnPower` could not be cleared, and the next attempt after the
    /// delay takes the key.
    #[tokio::test]
    async fn a_failed_write_is_tried_again() {
        let f = fixture(plasma()).await;
        f.key.inner.lock().await.retry = Some(Duration::from_millis(200));
        f.key.adapter_connected().await;
        f.accel.lock().unwrap().fail_writes = true;
        f.key.reconcile(&f.daemon, true).await;
        assert_eq!(f.pressed(POWER_OFF).as_deref(), Some("PowerOff"));
        assert!(!f.writes().iter().any(|w| w.starts_with("inactive")));
        assert!(f.key.inhibiting().await);
        f.accel.lock().unwrap().fail_writes = false;
        tokio::time::sleep(Duration::from_millis(250)).await;
        f.key.reconcile(&f.daemon, true).await;
        assert_eq!(f.pressed(POWER_OFF).as_deref(), Some("ArinnPower"));
        assert!(f.key.inhibiting().await);
    }

    /// Codex P1 on #517: a release KGlobalAccel did not answer keeps the
    /// inhibitor and the key counted as taken; the next check releases it.
    #[tokio::test]
    async fn an_unanswered_release_is_tried_again() {
        let f = fixture(plasma()).await;
        assert_eq!(f.key.take(&f.daemon).await, Taken::All);
        f.accel.lock().unwrap().fail_writes = true;
        f.key.give_back(&f.daemon).await;
        assert!(f.key.taken().await);
        assert!(f.key.inhibiting().await);
        f.accel.lock().unwrap().fail_writes = false;
        f.key.inner.lock().await.ready = true;
        f.key.reconcile(&f.daemon, false).await;
        assert!(!f.key.taken().await);
        assert!(!f.key.inhibiting().await);
        assert!(f.accel.lock().unwrap().shortcuts[2].keys.is_empty());
        assert_eq!(f.pressed(POWER_OFF).as_deref(), Some("PowerOff"));
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
        assert_eq!(key.take(&daemon).await, Taken::Failed);
        assert!(!key.inhibiting().await);
        assert!(!key.taken().await);
        assert!(!release(&daemon).await);
        assert!(eventually(async || login.lock().unwrap().open() == 0).await);
    }
}
