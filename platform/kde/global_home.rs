// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Global Home, quick-menu and on-screen keyboard keys on KDE (#286, #439,
//! #434).
//!
//! The KWin script registers the `ArinnPhase0Home`, `ArinnQuickMenu` and
//! `ArinnKeyboard` actions with one default sequence each, and KGlobalAccel
//! keeps the stored keys on every later registration. When the adapter
//! connects, the daemon therefore writes the active keys itself:
//! `home_shortcut` from `config.toml`, then HomePage, for Home (ADR-0094);
//! `quick_menu_shortcut`, then the remote's Menu keys, for the quick menu
//! (ADR-0106); `keyboard_shortcut`, then Launch1 (the remapped microphone),
//! for the keyboard (ADR-0105). Inside the shell Menu keeps its own meaning,
//! so the Menu keys are active only while another window has focus. Home
//! wins a key any other action wants, and the quick menu wins a key the
//! keyboard wants: the quick-menu action first gives up Home's keys, the
//! keyboard action Home's and the quick menu's, then Home is written, then
//! the quick menu, then the keyboard.
use crate::{
    key_sequence::{self, Sequence},
    storage::Config,
};
use std::{path::PathBuf, sync::Arc};
use zbus::Connection;

const SERVICE: &str = "org.kde.kglobalaccel";
const PATH: &str = "/kglobalaccel";
const INTERFACE: &str = "org.kde.KGlobalAccel";
/// Component and action names, unique then friendly, as KWin registers them
/// for `registerShortcut("ArinnPhase0Home", "Arinn Phase 0: Home", …)`.
const ACTION: [&str; 4] = ["kwin", "ArinnPhase0Home", "KWin", "Arinn Phase 0: Home"];
/// The quick-menu action, `registerShortcut("ArinnQuickMenu", "Arinn: Quick menu", …)`.
const QUICK_MENU_ACTION: [&str; 4] = ["kwin", "ArinnQuickMenu", "KWin", "Arinn: Quick menu"];
/// The keyboard action, `registerShortcut("ArinnKeyboard", "Arinn: On-screen keyboard", …)`.
const KEYBOARD_ACTION: [&str; 4] = ["kwin", "ArinnKeyboard", "KWin", "Arinn: On-screen keyboard"];

/// A `QKeySequence` on the bus: four combined key codes, unused ones zero.
type Keys = Vec<(Vec<i32>,)>;

fn warn(operation: &str, error: &str) {
    eprintln!(
        "{}",
        serde_json::json!({"level":"warn","component":"arinnd","provider_id":null,"request_id":null,"operation":operation,"error":error,"duration_ms":0})
    );
}

/// A new adapter connection polled for the first time. On KDE the KWin
/// script has registered its actions by then; write the configured keys
/// to them in the background (`write_all`), then the power keys, which are
/// fixed and come after every configured key, so each keeps its action
/// (ADR-0111), then prepare KDE for the on-screen keyboard (ADR-0105).
pub fn adapter_connected(
    connection: &Connection,
    config: PathBuf,
    state: PathBuf,
    quick_menu: QuickMenuKeys,
    power: crate::power_key::PowerKey,
) {
    if crate::platform::adapter_service() != Some("org.kde.KWin") {
        return;
    }
    let connection = connection.clone();
    tokio::spawn(async move {
        let directory = config.clone();
        let config = tokio::task::spawn_blocking(move || Config::load(&config))
            .await
            .ok()
            .and_then(Result::ok);
        let shortcuts = config.map_or_else(Shortcuts::default, |config| Shortcuts {
            home: config.home_shortcut,
            quick_menu: config.quick_menu_shortcut,
            keyboard: config.keyboard_shortcut,
        });
        write_all(&connection, &quick_menu, &shortcuts).await;
        power.connected(&connection).await;
        crate::keyboard_config::prepare(&connection, &directory, &state).await;
    });
}

/// The configured `home_shortcut`, `quick_menu_shortcut` and
/// `keyboard_shortcut` values.
struct Shortcuts {
    home: String,
    quick_menu: String,
    keyboard: String,
}
impl Default for Shortcuts {
    fn default() -> Self {
        Self {
            home: key_sequence::DEFAULT_HOME.to_owned(),
            quick_menu: key_sequence::DEFAULT_QUICK_MENU.to_owned(),
            keyboard: key_sequence::DEFAULT_KEYBOARD.to_owned(),
        }
    }
}

/// Writes the three actions for one adapter connection. Home wins a key
/// that another action wants, and the quick menu wins a key the keyboard
/// wants (ADR-0106, ADR-0105), also when the losing action still holds it
/// from its registration default or an earlier configuration: the
/// quick-menu action first releases the keys Home wants, the keyboard
/// action those of Home and the quick menu, then Home is written, then the
/// quick menu, then the keyboard without the keys of both.
async fn write_all(connection: &Connection, quick_menu: &QuickMenuKeys, shortcuts: &Shortcuts) {
    let home = keys(&shortcuts.home).0;
    let reserved = encode(key_sequence::reserved(
        &shortcuts.home,
        &shortcuts.quick_menu,
    ));
    release(connection, &QUICK_MENU_ACTION, "quick-menu-shortcut", &home).await;
    release(connection, &KEYBOARD_ACTION, "keyboard-shortcut", &reserved).await;
    apply(connection, &shortcuts.home).await;
    quick_menu
        .connected(connection, &shortcuts.home, &shortcuts.quick_menu)
        .await;
    apply_keyboard(
        connection,
        &shortcuts.home,
        &shortcuts.quick_menu,
        &shortcuts.keyboard,
    )
    .await;
}

/// Removes `wanted` from `action`'s stored keys. KGlobalAccel keeps each
/// action's keys across registrations, and a key a losing action still
/// holds would otherwise count as another action's and be left out of the
/// winner. Only a stored copy is touched; nothing is written when `action`
/// holds none of them.
async fn release(connection: &Connection, action: &[&str; 4], operation: &str, wanted: &Keys) {
    let stored: Keys = match connection
        .call_method(
            Some(SERVICE),
            PATH,
            Some(INTERFACE),
            "shortcutKeys",
            &(action.to_vec(),),
        )
        .await
    {
        Ok(reply) => reply.body().deserialize().unwrap_or_default(),
        Err(_) => return,
    };
    let kept: Keys = stored
        .iter()
        .filter(|key| !wanted.contains(key))
        .cloned()
        .collect();
    if kept.len() == stored.len() {
        return;
    }
    write(connection, action, operation, kept, true).await;
}

/// Whether a reported window class is the shell's: its Wayland app id, or
/// its X11 class under KDE/X11.
fn is_shell_class(class: &str) -> bool {
    class == "org.arinn.Shell" || class == "arinn-shell"
}

/// The adapter reported the active window. On KDE the remote's Menu keys
/// follow it: off while the shell has focus, on otherwise.
pub fn window_reported(connection: &Connection, quick_menu: &QuickMenuKeys, class: &str) {
    if crate::platform::adapter_service() != Some("org.kde.KWin") {
        return;
    }
    if !quick_menu.focus(is_shell_class(class)) {
        return;
    }
    let (connection, quick_menu) = (connection.clone(), quick_menu.clone());
    tokio::spawn(async move {
        quick_menu.sync(&connection).await;
    });
}

/// The quick-menu keys of one daemon (ADR-0106). The latest focus is kept
/// apart from the writer, so writes that run late still apply it.
#[derive(Clone, Default)]
pub struct QuickMenuKeys {
    shell_focused: Arc<std::sync::Mutex<bool>>,
    state: Arc<tokio::sync::Mutex<QuickMenuState>>,
}
#[derive(Default)]
struct QuickMenuState {
    /// Every quick-menu key, set when the adapter connects.
    keys: Option<key_sequence::QuickMenu>,
    /// The focus the stored keys were written for.
    written: Option<bool>,
}
impl QuickMenuKeys {
    /// Records the focus; returns whether it changed.
    pub fn focus(&self, shell: bool) -> bool {
        std::mem::replace(&mut *self.shell_focused.lock().unwrap(), shell) != shell
    }
    /// A new adapter connection: write every key for the current focus.
    /// Returns whether every key is active.
    pub async fn connected(&self, connection: &Connection, home: &str, configured: &str) -> bool {
        let (keys, fallback) = key_sequence::quick_menu(configured, &key_sequence::home(home).0);
        if fallback {
            warn("quick-menu-shortcut", "InvalidArgument");
        }
        let mut state = self.state.lock().await;
        state.keys = Some(keys);
        state.written = None;
        let shell = *self.shell_focused.lock().unwrap();
        Self::write_state(&mut state, connection, shell).await
    }
    /// Writes the keys for the latest focus if they differ from the stored
    /// ones. Returns whether every key is active.
    pub async fn sync(&self, connection: &Connection) -> bool {
        let mut state = self.state.lock().await;
        let shell = *self.shell_focused.lock().unwrap();
        if state.written == Some(shell) {
            return true;
        }
        Self::write_state(&mut state, connection, shell).await
    }
    async fn write_state(state: &mut QuickMenuState, connection: &Connection, shell: bool) -> bool {
        let Some(keys) = state.keys.as_ref() else {
            return false;
        };
        let keys: Vec<Sequence> = keys.bound(shell);
        state.written = Some(shell);
        if keys.is_empty() && !shell {
            // Every key is one of Home's.
            warn("quick-menu-shortcut", "Conflict");
            return false;
        }
        write(
            connection,
            &QUICK_MENU_ACTION,
            "quick-menu-shortcut",
            encode(keys),
            true,
        )
        .await
    }
}

fn encode(sequences: Vec<Sequence>) -> Keys {
    sequences
        .into_iter()
        .map(|sequence| (vec![sequence.qt(), 0, 0, 0],))
        .collect()
}

/// The keys written for a `home_shortcut` value, and whether the value was
/// replaced by the default.
pub fn keys(configured: &str) -> (Keys, bool) {
    let (sequences, fallback) = key_sequence::home(configured);
    (encode(sequences), fallback)
}

/// The keys written for a `keyboard_shortcut` value next to the Home and
/// quick-menu values, whether the value was replaced by the default, and
/// whether a Home or quick-menu key was left out.
pub fn keyboard_keys(home: &str, quick_menu: &str, configured: &str) -> (Keys, bool, bool) {
    let (sequences, fallback, conflict) =
        key_sequence::keyboard(configured, &key_sequence::reserved(home, quick_menu));
    (encode(sequences), fallback, conflict)
}

/// The action that holds `key` in KGlobalAccel, if another one does. A
/// failed lookup counts as free: the write below still reports a missing
/// service.
async fn held_elsewhere(connection: &Connection, action: &[&str; 4], key: &(Vec<i32>,)) -> bool {
    let owner: Vec<String> = match connection
        .call_method(Some(SERVICE), PATH, Some(INTERFACE), "actionList", key)
        .await
    {
        Ok(reply) => reply.body().deserialize().unwrap_or_default(),
        Err(_) => return false,
    };
    owner.len() >= 2 && owner[..2] != action[..2]
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
        warn("global-home-shortcut", "InvalidArgument");
    }
    write(connection, &ACTION, "global-home-shortcut", keys, false).await
}

/// Sets the keyboard action's active keys (ADR-0105) with the checks of
/// [`apply`]. A configured key that is one of Home's or the quick menu's is
/// left out as a conflict. Returns whether every key is active.
pub async fn apply_keyboard(
    connection: &Connection,
    home: &str,
    quick_menu: &str,
    configured: &str,
) -> bool {
    let (keys, fallback, conflict) = keyboard_keys(home, quick_menu, configured);
    if fallback {
        warn("keyboard-shortcut", "InvalidArgument");
    }
    if conflict {
        warn("keyboard-shortcut", "Conflict");
    }
    write(
        connection,
        &KEYBOARD_ACTION,
        "keyboard-shortcut",
        keys,
        false,
    )
    .await
        && !conflict
}

/// Writes `keys` to `action` after the conflict checks. With `clear`, an
/// empty list clears the action; otherwise nothing is written when no key
/// is free.
async fn write(
    connection: &Connection,
    action: &[&str; 4],
    operation: &str,
    keys: Keys,
    clear: bool,
) -> bool {
    let total = keys.len();
    let mut free = Vec::new();
    for key in keys {
        if !held_elsewhere(connection, action, &key).await {
            free.push(key);
        }
    }
    let conflict = free.len() < total;
    if conflict {
        warn(operation, "Conflict");
    }
    if free.is_empty() && !(clear && total == 0) {
        return false;
    }
    let set = connection
        .call_method(
            Some(SERVICE),
            PATH,
            Some(INTERFACE),
            "setForeignShortcutKeys",
            &(action.to_vec(), free.clone()),
        )
        .await;
    if set.is_err() {
        warn(operation, "Unavailable");
        return false;
    }
    // KGlobalAccel ignores a write for an action it does not know.
    let active: Option<Keys> = match connection
        .call_method(
            Some(SERVICE),
            PATH,
            Some(INTERFACE),
            "shortcutKeys",
            &(action.to_vec(),),
        )
        .await
    {
        Ok(reply) => reply.body().deserialize().ok(),
        Err(_) => None,
    };
    let stored = active.is_some_and(|active| free.iter().all(|key| active.contains(key)));
    if !stored {
        warn(operation, "NotFound");
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
    /// another action holds; `actionList` names the action holding a key.
    struct FakeAccel {
        writes: Writes,
        taken: Vec<i32>,
        active: Vec<(Vec<String>, Keys)>,
    }
    impl FakeAccel {
        fn keys(&self, action: &[String]) -> Keys {
            self.active
                .iter()
                .find(|(name, _)| name[..2] == action[..2])
                .map(|(_, keys)| keys.clone())
                .unwrap_or_default()
        }
    }
    #[zbus::interface(name = "org.kde.KGlobalAccel")]
    impl FakeAccel {
        #[zbus(name = "setForeignShortcutKeys")]
        fn set_foreign_shortcut_keys(&mut self, action: Vec<String>, keys: Keys) {
            self.writes
                .lock()
                .unwrap()
                .push((action.clone(), keys.clone()));
            self.active.retain(|(name, _)| name[..2] != action[..2]);
            self.active.push((action, keys));
        }
        #[zbus(name = "actionList")]
        fn action_list(&self, key: (Vec<i32>,)) -> Vec<String> {
            if self.taken.contains(&key.0[0]) {
                return ["kwin", "Other Action", "KWin", "Other"]
                    .map(String::from)
                    .to_vec();
            }
            self.active
                .iter()
                .find(|(_, keys)| keys.contains(&key))
                .map(|(name, _)| name.clone())
                .unwrap_or_default()
        }
        #[zbus(name = "shortcutKeys")]
        fn shortcut_keys(&self, action: Vec<String>) -> Keys {
            self.keys(&action)
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
        fixture_with(taken, Vec::new()).await
    }
    /// The same, with keys KGlobalAccel already stores per action.
    async fn fixture_with(
        taken: Vec<i32>,
        active: Vec<(Vec<String>, Keys)>,
    ) -> (Bus, Connection, Connection, Writes) {
        let (bus, address) = bus().await;
        let writes = Writes::default();
        let accel = FakeAccel {
            writes: Arc::clone(&writes),
            taken,
            active,
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

    fn menu() -> (Vec<i32>,) {
        (vec![0x0100_0055, 0, 0, 0],)
    }
    fn menu_kb() -> (Vec<i32>,) {
        (vec![0x0100_00dc, 0, 0, 0],)
    }

    #[tokio::test]
    async fn writes_the_quick_menu_keys_to_their_own_action() {
        let (_bus, _service, daemon, writes) = fixture(Vec::new()).await;
        assert!(apply(&daemon, "Meta+F12").await);
        let keys = QuickMenuKeys::default();
        assert!(keys.connected(&daemon, "Meta+F12", "Ctrl+Alt+Q").await);
        let writes = writes.lock().unwrap();
        assert_eq!(writes.len(), 2);
        assert_eq!(writes[0].0, ACTION);
        assert_eq!(writes[1].0, QUICK_MENU_ACTION);
        assert_eq!(
            writes[1].1,
            [(vec![0x0C00_0051, 0, 0, 0],), menu(), menu_kb()]
        );
    }

    #[tokio::test]
    async fn invalid_quick_menu_value_writes_its_default() {
        let (_bus, _service, daemon, writes) = fixture(Vec::new()).await;
        let keys = QuickMenuKeys::default();
        assert!(keys.connected(&daemon, "Meta+F12", "Escape").await);
        assert_eq!(
            writes.lock().unwrap()[0].1,
            [(vec![0x1100_003a, 0, 0, 0],), menu(), menu_kb()]
        );
    }

    #[tokio::test]
    async fn a_home_key_is_never_written_to_the_quick_menu() {
        let (_bus, _service, daemon, writes) = fixture(Vec::new()).await;
        assert!(apply(&daemon, "Menu").await);
        let keys = QuickMenuKeys::default();
        assert!(keys.connected(&daemon, "Menu", "Ctrl+Alt+Q").await);
        assert_eq!(
            writes.lock().unwrap()[1].1,
            [(vec![0x0C00_0051, 0, 0, 0],), menu_kb()]
        );
    }

    #[tokio::test]
    async fn a_quick_menu_key_held_by_another_action_is_left_out() {
        let (_bus, _service, daemon, writes) = fixture(vec![0x1100_003a]).await;
        let keys = QuickMenuKeys::default();
        assert!(!keys.connected(&daemon, "Meta+F12", "Meta+F11").await);
        assert_eq!(writes.lock().unwrap()[0].1, [menu(), menu_kb()]);
    }

    /// Owner decision of 2026-10-21 (#439): the remote's Menu opens the
    /// quick menu only outside the shell; inside it Menu stays the shell's.
    #[tokio::test]
    async fn remote_menu_keys_follow_the_shell_focus() {
        let (_bus, _service, daemon, writes) = fixture(Vec::new()).await;
        let keys = QuickMenuKeys::default();
        assert!(keys.focus(true));
        assert!(!keys.focus(true));
        // Focus before the adapter connected writes nothing.
        assert!(!keys.sync(&daemon).await);
        assert!(writes.lock().unwrap().is_empty());
        assert!(keys.connected(&daemon, "Meta+F12", "Meta+F11").await);
        let configured = (vec![0x1100_003a, 0, 0, 0],);
        assert_eq!(
            writes.lock().unwrap()[0].1,
            std::slice::from_ref(&configured)
        );
        // An unchanged focus writes nothing again.
        assert!(keys.sync(&daemon).await);
        assert_eq!(writes.lock().unwrap().len(), 1);
        assert!(keys.focus(false));
        assert!(keys.sync(&daemon).await);
        assert_eq!(
            writes.lock().unwrap()[1].1,
            [configured.clone(), menu(), menu_kb()]
        );
        assert!(keys.focus(true));
        assert!(keys.sync(&daemon).await);
        assert_eq!(writes.lock().unwrap()[2].1, [configured]);
        // With the configured key taken by Home, the shell's focus clears
        // the action instead of leaving Menu bound.
        let keys = QuickMenuKeys::default();
        keys.focus(true);
        assert!(keys.connected(&daemon, "Meta+F11", "Meta+F11").await);
        assert!(writes.lock().unwrap()[3].1.is_empty());
    }

    fn shortcuts(home: &str, quick_menu: &str, keyboard: &str) -> Shortcuts {
        Shortcuts {
            home: home.to_owned(),
            quick_menu: quick_menu.to_owned(),
            keyboard: keyboard.to_owned(),
        }
    }
    fn key(code: i32) -> (Vec<i32>,) {
        (vec![code, 0, 0, 0],)
    }
    const META_F11: i32 = 0x1100_003a;
    const META_F12: i32 = 0x1100_003b;
    const HOME_PAGE: i32 = 0x0100_0090;
    async fn stored(connection: &Connection, action: &[&str; 4]) -> Keys {
        connection
            .call_method(
                Some(SERVICE),
                PATH,
                Some(INTERFACE),
                "shortcutKeys",
                &(action.to_vec(),),
            )
            .await
            .unwrap()
            .body()
            .deserialize()
            .unwrap()
    }
    fn owned(action: &[&str; 4], keys: &[i32]) -> (Vec<String>, Keys) {
        (
            action.map(String::from).to_vec(),
            keys.iter().map(|code| key(*code)).collect(),
        )
    }

    /// Review of #502: the KWin script registers the quick menu with
    /// Meta+F11, and KGlobalAccel keeps it. A `home_shortcut` of Meta+F11
    /// still goes to Home, and the quick menu leaves it out (ADR-0106).
    #[tokio::test]
    async fn home_wins_a_key_the_quick_menu_action_still_holds() {
        let (_bus, _service, daemon, _writes) =
            fixture_with(Vec::new(), vec![owned(&QUICK_MENU_ACTION, &[META_F11])]).await;
        let quick = QuickMenuKeys::default();
        write_all(
            &daemon,
            &quick,
            &shortcuts("Meta+F11", "Meta+F11", "Meta+F10"),
        )
        .await;
        assert_eq!(
            stored(&daemon, &ACTION).await,
            [key(META_F11), key(HOME_PAGE)]
        );
        assert_eq!(
            stored(&daemon, &QUICK_MENU_ACTION).await,
            [menu(), menu_kb()]
        );
    }

    /// Swapping the two keys across a restart gives each action its new key.
    #[tokio::test]
    async fn swapped_home_and_quick_menu_keys_follow_the_configuration() {
        let (_bus, _service, daemon, _writes) = fixture_with(
            Vec::new(),
            vec![
                owned(&ACTION, &[META_F11, HOME_PAGE]),
                owned(&QUICK_MENU_ACTION, &[META_F12, 0x0100_0055, 0x0100_00dc]),
            ],
        )
        .await;
        let quick = QuickMenuKeys::default();
        write_all(
            &daemon,
            &quick,
            &shortcuts("Meta+F12", "Meta+F11", "Meta+F10"),
        )
        .await;
        assert_eq!(
            stored(&daemon, &ACTION).await,
            [key(META_F12), key(HOME_PAGE)]
        );
        assert_eq!(
            stored(&daemon, &QUICK_MENU_ACTION).await,
            [key(META_F11), menu(), menu_kb()]
        );
        // And back again.
        write_all(
            &daemon,
            &quick,
            &shortcuts("Meta+F11", "Meta+F12", "Meta+F10"),
        )
        .await;
        assert_eq!(
            stored(&daemon, &ACTION).await,
            [key(META_F11), key(HOME_PAGE)]
        );
        assert_eq!(
            stored(&daemon, &QUICK_MENU_ACTION).await,
            [key(META_F12), menu(), menu_kb()]
        );
    }

    /// Only the sibling Arinn action gives keys up; a key a third-party
    /// action holds stays a conflict for Home and for the quick menu.
    #[tokio::test]
    async fn a_third_party_key_is_still_a_conflict() {
        let (_bus, _service, daemon, _writes) =
            fixture_with(vec![META_F11], vec![owned(&QUICK_MENU_ACTION, &[META_F12])]).await;
        let quick = QuickMenuKeys::default();
        write_all(
            &daemon,
            &quick,
            &shortcuts("Meta+F11", "Meta+F11", "Meta+F10"),
        )
        .await;
        assert_eq!(stored(&daemon, &ACTION).await, [key(HOME_PAGE)]);
        assert_eq!(
            stored(&daemon, &QUICK_MENU_ACTION).await,
            [menu(), menu_kb()]
        );
    }
    const META_F10: i32 = 0x1100_0039;
    const LAUNCH1: i32 = 0x0100_00a3;

    #[tokio::test]
    async fn writes_the_keyboard_key_and_the_microphone_to_their_own_action() {
        let (_bus, _service, daemon, writes) = fixture(Vec::new()).await;
        assert!(apply(&daemon, "Meta+F12").await);
        assert!(apply_keyboard(&daemon, "Meta+F12", "Meta+F11", "Ctrl+Alt+K").await);
        let writes = writes.lock().unwrap();
        assert_eq!(writes.len(), 2);
        assert_eq!(writes[1].0, KEYBOARD_ACTION);
        assert_eq!(writes[1].1, [(vec![0x0C00_004B, 0, 0, 0],), key(LAUNCH1)]);
    }

    #[tokio::test]
    async fn invalid_keyboard_value_writes_its_default() {
        let (_bus, _service, daemon, writes) = fixture(Vec::new()).await;
        assert!(apply_keyboard(&daemon, "Meta+F12", "Meta+F11", "Backspace").await);
        assert_eq!(writes.lock().unwrap()[0].1, [key(META_F10), key(LAUNCH1)]);
    }

    #[tokio::test]
    async fn a_home_key_is_never_written_to_the_keyboard() {
        let (_bus, _service, daemon, writes) = fixture(Vec::new()).await;
        assert!(apply(&daemon, "Ctrl+Alt+H").await);
        assert!(!apply_keyboard(&daemon, "Ctrl+Alt+H", "Meta+F11", "ctrl+alt+h").await);
        let writes = writes.lock().unwrap();
        // Only the microphone reaches the keyboard action.
        assert_eq!(writes[1].1, [key(LAUNCH1)]);
    }

    /// ADR-0105 after ADR-0106: a quick-menu key, the remote's Menu keys
    /// included, is never a keyboard key.
    #[tokio::test]
    async fn a_quick_menu_key_is_never_written_to_the_keyboard() {
        for configured in ["Meta+F11", "Menu", "MenuKB"] {
            let (_bus, _service, daemon, _writes) = fixture(Vec::new()).await;
            let quick = QuickMenuKeys::default();
            write_all(
                &daemon,
                &quick,
                &shortcuts("Meta+F12", "Meta+F11", configured),
            )
            .await;
            assert_eq!(
                stored(&daemon, &KEYBOARD_ACTION).await,
                [key(LAUNCH1)],
                "{configured}"
            );
            assert_eq!(
                stored(&daemon, &QUICK_MENU_ACTION).await,
                [key(META_F11), menu(), menu_kb()],
                "{configured}"
            );
        }
    }

    /// The KWin script registers the keyboard with Meta+F10, and
    /// KGlobalAccel keeps it. A Home or quick-menu value of Meta+F10 still
    /// goes to that action; the keyboard gives the key up first.
    #[tokio::test]
    async fn home_and_the_quick_menu_win_a_key_the_keyboard_action_still_holds() {
        let (_bus, _service, daemon, _writes) = fixture_with(
            Vec::new(),
            vec![owned(&KEYBOARD_ACTION, &[META_F10, LAUNCH1])],
        )
        .await;
        let quick = QuickMenuKeys::default();
        write_all(
            &daemon,
            &quick,
            &shortcuts("Meta+F10", "Meta+F11", "Meta+F10"),
        )
        .await;
        assert_eq!(
            stored(&daemon, &ACTION).await,
            [key(META_F10), key(HOME_PAGE)]
        );
        assert_eq!(stored(&daemon, &KEYBOARD_ACTION).await, [key(LAUNCH1)]);

        let (_bus, _service, daemon, _writes) = fixture_with(
            Vec::new(),
            vec![owned(&KEYBOARD_ACTION, &[META_F10, LAUNCH1])],
        )
        .await;
        write_all(
            &daemon,
            &quick,
            &shortcuts("Meta+F12", "Meta+F10", "Meta+F10"),
        )
        .await;
        assert_eq!(
            stored(&daemon, &QUICK_MENU_ACTION).await,
            [key(META_F10), menu(), menu_kb()]
        );
        assert_eq!(stored(&daemon, &KEYBOARD_ACTION).await, [key(LAUNCH1)]);
    }

    /// Swapping the quick-menu and keyboard keys across a restart gives each
    /// action its new key.
    #[tokio::test]
    async fn swapped_quick_menu_and_keyboard_keys_follow_the_configuration() {
        let (_bus, _service, daemon, _writes) = fixture_with(
            Vec::new(),
            vec![
                owned(&QUICK_MENU_ACTION, &[META_F11, 0x0100_0055, 0x0100_00dc]),
                owned(&KEYBOARD_ACTION, &[META_F10, LAUNCH1]),
            ],
        )
        .await;
        let quick = QuickMenuKeys::default();
        write_all(
            &daemon,
            &quick,
            &shortcuts("Meta+F12", "Meta+F10", "Meta+F11"),
        )
        .await;
        assert_eq!(
            stored(&daemon, &QUICK_MENU_ACTION).await,
            [key(META_F10), menu(), menu_kb()]
        );
        assert_eq!(
            stored(&daemon, &KEYBOARD_ACTION).await,
            [key(META_F11), key(LAUNCH1)]
        );
    }
}
