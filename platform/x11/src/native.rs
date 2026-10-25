// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use crate::key_sequence::{self, Sequence};
use anyhow::{Result, bail, ensure};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;
use x11rb::{
    CURRENT_TIME,
    connection::Connection,
    protocol::{Event, xinput, xproto::*},
    rust_connection::RustConnection,
};
/// XInput2's `XIAllMasterDevices`.
const ALL_MASTER_DEVICES: xinput::DeviceId = 1;

/// How the adapter reads the power key now (ADR-0111).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reading {
    /// Not read: off, or held by another client and not readable.
    None,
    /// Grabbed on the root window.
    Grab,
    /// Read from XInput2 raw key presses while another client grabs it.
    Raw,
}

/// A raw key press that is a power key: only the power keycodes are looked
/// at, and a repeat of a held key is not a press. Nothing else of the raw
/// event is kept (§13).
pub(crate) fn raw_power_press(
    codes: &HashSet<Keycode>,
    detail: u32,
    flags: xinput::KeyEventFlags,
) -> bool {
    u8::try_from(detail).is_ok_and(|code| codes.contains(&code))
        && !flags.contains(xinput::KeyEventFlags::KEY_REPEAT)
}

x11rb::atom_manager! {
    pub Atoms: AtomsCookie {
        _NET_SUPPORTING_WM_CHECK, _NET_SUPPORTED, _NET_CLIENT_LIST,
        _NET_ACTIVE_WINDOW, _NET_CLOSE_WINDOW, _NET_WM_STATE, _NET_WM_DESKTOP,
        _NET_WM_STATE_FULLSCREEN, _NET_WM_STATE_ABOVE, _NET_WM_STATE_BELOW,
        _NET_WM_WINDOW_TYPE, _NET_WM_WINDOW_TYPE_NORMAL,
        _NET_WM_DESKTOP_FILE, _KDE_NET_WM_DESKTOP_FILE, _GTK_APPLICATION_ID,
        UTF8_STRING, WM_CLASS,
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub xid: Window,
    pub token: String,
    pub desktop_id: String,
    pub launch: String,
    pub flags: i32,
}
#[derive(Clone, Debug)]
pub struct Snapshot {
    pub rows: Vec<Row>,
    pub shell: Option<Window>,
    pub shell_token: String,
    pub active: Option<Window>,
    /// The latest global key since the last snapshot was Home.
    pub home_key: bool,
    /// The latest global key since the last snapshot was the quick menu
    /// (ADR-0106).
    pub quick_menu_key: bool,
    /// The latest global key since the last snapshot was the power key
    /// (ADR-0111).
    pub power_key: bool,
    /// How Arinn reads the power key now (grabbed, or read from raw events
    /// while the power manager does nothing with it; `None`: not at all).
    /// It changes when a regrab moves the key between the two (ADR-0111).
    pub power_reading: Reading,
    pub compositor: bool,
}
#[derive(Default)]
pub struct Tokens(HashMap<Window, String>);
impl Tokens {
    pub fn forget(&mut self, xid: Window) {
        self.0.remove(&xid);
    }
    pub fn retain(&mut self, live: &HashSet<Window>) {
        self.0.retain(|id, _| live.contains(id));
    }
    pub fn get(&mut self, xid: Window) -> String {
        self.0
            .entry(xid)
            .or_insert_with(|| Uuid::new_v4().to_string())
            .clone()
    }
}
pub fn launch_id(class: &[u8]) -> String {
    let Some(instance) = class.split(|b| *b == 0).next() else {
        return String::new();
    };
    let Ok(instance) = std::str::from_utf8(instance) else {
        return String::new();
    };
    let Some(value) = instance.strip_prefix("org.arinn.playback.") else {
        return String::new();
    };
    match Uuid::parse_str(value) {
        Ok(uuid) if !uuid.is_nil() && uuid.to_string() == value => value.into(),
        _ => String::new(),
    }
}
/// UTF-8/STRING metadata of a property reply; `None` when it is invalid.
fn metadata(r: GetPropertyReply, ty: Atom) -> Option<Vec<u8>> {
    if r.type_ == 0 {
        return Some(Vec::new());
    }
    (r.type_ == ty && r.format == 8 && r.bytes_after == 0).then_some(r.value)
}
pub fn desktop_id(value: &[u8], basename: bool) -> String {
    let Ok(text) = std::str::from_utf8(value) else {
        return String::new();
    };
    let text = text.trim_end_matches('\0');
    if arinn_protocol::validation::id(text).is_err() || text.contains('/') {
        return String::new();
    }
    if text == "org.arinn.Shell" || text.ends_with(".desktop") {
        return text.into();
    }
    if basename {
        let id = format!("{text}.desktop");
        if arinn_protocol::validation::id(&id).is_ok() {
            return id;
        }
    }
    String::new()
}

/// The keyboard and modifier mapping the global grabs need (#286, #434).
pub(crate) struct KeyMap {
    min: Keycode,
    per_key: usize,
    keysyms: Vec<u32>,
    /// Lock modifiers (Caps, Num, Scroll) a grab must ignore.
    pub locks: u16,
    meta: HashSet<u16>,
    alts: HashSet<u16>,
}
impl KeyMap {
    pub fn read(c: &RustConnection) -> Result<Self> {
        let setup = c.setup();
        let count = setup.max_keycode - setup.min_keycode + 1;
        let mapping = c.get_keyboard_mapping(setup.min_keycode, count)?.reply()?;
        let per_key = usize::from(mapping.keysyms_per_keycode);
        ensure!(per_key > 0, "invalid keyboard mapping");
        let mut locks = u16::from(ModMask::LOCK);
        let mut supers = HashSet::new();
        let mut metas = HashSet::new();
        let mut alts = HashSet::new();
        let mods = c.get_modifier_mapping()?.reply()?;
        let per_mod = mods.keycodes.len() / 8;
        for (index, syms) in mapping.keysyms.chunks(per_key).enumerate() {
            let key = setup.min_keycode + index as u8;
            let mask = if per_mod > 0 {
                mods.keycodes
                    .chunks(per_mod)
                    .enumerate()
                    .filter(|(_, codes)| codes.contains(&key))
                    .fold(0u16, |mask, (modifier, _)| mask | (1 << modifier))
            } else {
                0
            };
            if syms.iter().any(|s| [0xffe5, 0xff7f, 0xff14].contains(s)) {
                locks |= mask;
            }
            if mask != 0 {
                if syms.iter().any(|s| [0xffeb, 0xffec].contains(s)) {
                    supers.insert(mask);
                }
                if syms.iter().any(|s| [0xffe7, 0xffe8].contains(s)) {
                    metas.insert(mask);
                }
                if syms.iter().any(|s| [0xffe9, 0xffea].contains(s)) {
                    alts.insert(mask);
                }
            }
        }
        // Qt's Meta transport normally uses Super; fall back to native Meta
        // only when no mapped Super key exists. Do not treat Alt aliases as Meta.
        let meta = if supers.is_empty() { metas } else { supers };
        Ok(Self {
            min: setup.min_keycode,
            per_key,
            keysyms: mapping.keysyms,
            locks,
            meta,
            alts,
        })
    }
    /// Every keycode that carries `keysym` in some column.
    pub fn codes(&self, keysym: u32) -> Vec<Keycode> {
        self.keysyms
            .chunks(self.per_key)
            .enumerate()
            .filter(|(_, syms)| syms.contains(&keysym))
            .map(|(index, _)| self.min + index as u8)
            .collect()
    }
    /// Keycodes without any keysym: free for a temporary mapping.
    pub fn free(&self) -> Vec<Keycode> {
        self.keysyms
            .chunks(self.per_key)
            .enumerate()
            .filter(|(_, syms)| syms.iter().all(|s| *s == 0))
            .map(|(index, _)| self.min + index as u8)
            .collect()
    }
    pub fn per_key(&self) -> usize {
        self.per_key
    }
    /// Modifier masks of `sequence` with every lock variant, per keycode.
    pub fn combinations(&self, sequence: &Sequence) -> Vec<(u16, Keycode)> {
        let choices = |wanted: bool, masks: &HashSet<u16>| -> Vec<u16> {
            if wanted {
                masks.iter().copied().collect()
            } else {
                vec![0]
            }
        };
        let m = sequence.modifiers;
        let fixed = if m.shift {
            u16::from(ModMask::SHIFT)
        } else {
            0
        } | if m.ctrl {
            u16::from(ModMask::CONTROL)
        } else {
            0
        };
        let codes = self.codes(sequence.keysym());
        let mut combinations = Vec::new();
        for meta in choices(m.meta, &self.meta) {
            for alt in choices(m.alt, &self.alts) {
                for key in &codes {
                    for subset in 0u16..256 {
                        if subset & !self.locks == 0 {
                            combinations.push((meta | alt | fixed | subset, *key));
                        }
                    }
                }
            }
        }
        combinations
    }
}

/// Grabs one key combination list on `root`. Returns false, with every
/// partial grab released, when another client holds one of them
/// (BadAccess).
pub(crate) fn grab_combinations(
    c: &RustConnection,
    root: Window,
    combinations: &[(u16, Keycode)],
) -> Result<bool> {
    for (modifiers, key) in combinations {
        let result = c
            .grab_key(
                false,
                root,
                (*modifiers).into(),
                *key,
                GrabMode::ASYNC,
                GrabMode::ASYNC,
            )?
            .check();
        match result {
            Ok(()) => (),
            Err(x11rb::errors::ReplyError::X11Error(e))
                if e.error_kind == x11rb::protocol::ErrorKind::Access =>
            {
                for (modifiers, key) in combinations {
                    c.ungrab_key(*key, root, (*modifiers).into())?.check()?;
                }
                return Ok(false);
            }
            Err(error) => return Err(error.into()),
        }
    }
    Ok(true)
}

/// Whether another client grabs one of the `power` keys on `root`: a grab
/// with any modifiers fails with `BadAccess` then. A free key is let go at
/// once.
pub(crate) fn power_held(c: &RustConnection, root: Window, power: &[Sequence]) -> Result<bool> {
    let map = KeyMap::read(c)?;
    let keys: HashSet<Keycode> = power
        .iter()
        .flat_map(|sequence| map.combinations(sequence))
        .map(|(_, key)| key)
        .collect();
    for key in keys {
        let result = c
            .grab_key(
                false,
                root,
                ModMask::ANY,
                key,
                GrabMode::ASYNC,
                GrabMode::ASYNC,
            )?
            .check();
        match result {
            Ok(()) => c.ungrab_key(key, root, ModMask::ANY)?.check()?,
            Err(x11rb::errors::ReplyError::X11Error(e))
                if e.error_kind == x11rb::protocol::ErrorKind::Access =>
            {
                return Ok(true);
            }
            Err(error) => return Err(error.into()),
        }
    }
    Ok(false)
}

/// `power_held` on the display of this process (`--release-power-key`,
/// after the adapter ended): `None` without a display.
pub fn power_held_on_display() -> Option<bool> {
    let (c, screen) = x11rb::connect(None).ok()?;
    let root = c.setup().roots.get(screen)?.root;
    power_held(&c, root, &key_sequence::power()).ok()
}

/// Grabs every sequence on `root` and returns the grabbed combinations. A
/// sequence whose key or modifier is not mapped is skipped (the next
/// MappingNotify grabs it again); one another client holds costs only that
/// sequence and is logged with `what`.
pub(crate) fn grab_sequences(
    c: &RustConnection,
    root: Window,
    map: &KeyMap,
    sequences: &[Sequence],
    what: &str,
) -> Result<HashSet<(u16, Keycode)>> {
    let mut grabbed = HashSet::new();
    for sequence in sequences {
        let combinations = map.combinations(sequence);
        if combinations.is_empty() {
            continue;
        }
        if grab_combinations(c, root, &combinations)? {
            grabbed.extend(combinations);
        } else {
            eprintln!("arinn-platform-x11: a {what} key is held by another client; skipped");
        }
    }
    Ok(grabbed)
}

/// The desktop identity of `xid`: `_NET_WM_DESKTOP_FILE`,
/// `_KDE_NET_WM_DESKTOP_FILE`, then `_GTK_APPLICATION_ID`, the first
/// non-empty valid one. Invalid or missing metadata counts as empty.
pub(crate) fn identity(c: &RustConnection, atoms: &Atoms, xid: Window) -> String {
    [
        atoms._NET_WM_DESKTOP_FILE,
        atoms._KDE_NET_WM_DESKTOP_FILE,
        atoms._GTK_APPLICATION_ID,
    ]
    .into_iter()
    .map(|atom| {
        let value = c
            .get_property(false, xid, atom, atoms.UTF8_STRING, 0, 128)
            .ok()
            .and_then(|cookie| cookie.reply().ok())
            .and_then(|r| metadata(r, atoms.UTF8_STRING))
            .unwrap_or_default();
        desktop_id(&value, atom != atoms._NET_WM_DESKTOP_FILE)
    })
    .find(|id| !id.is_empty())
    .unwrap_or_default()
}

/// Whether a desktop identity is the shell's (#283).
pub(crate) fn is_shell(identity: &str) -> bool {
    ["org.arinn.Shell", "org.arinn.Shell.desktop"].contains(&identity)
}

/// `_NET_ACTIVE_WINDOW`: one or two CARD32s, as xfwm4 writes two. The first
/// non-zero word is the active window; `[0]` and `[0, 0]` mean none (#379).
pub(crate) fn active_window(
    c: &RustConnection,
    root: Window,
    atom: Atom,
) -> Result<Option<Window>> {
    const NAME: &str = "_NET_ACTIVE_WINDOW";
    let ty = AtomEnum::WINDOW;
    let r = c.get_property(false, root, atom, ty, 0, 2)?.reply()?;
    if r.type_ == 0 {
        return Ok(None);
    }
    ensure!(
        r.type_ == u32::from(ty) && r.format == 32,
        "invalid property type"
    );
    let words: Vec<u32> = r.value32().unwrap().collect();
    let length = words.len() + r.bytes_after as usize / 4;
    if length > 2 || r.bytes_after != 0 {
        bail!("{NAME}: length {length} exceeds maximum 2 CARD32");
    }
    let active = match words.as_slice() {
        [] => bail!("{NAME}: length 0 has no valid window id"),
        // xfwm4 writes [0, 0] while no client is focused (#379).
        [0] | [0, 0] => None,
        [id] => Some(*id),
        [0, id] => Some(*id),
        [id, _] => Some(*id),
        [_, _, _, ..] => bail!("{NAME}: length {length} exceeds maximum 2 CARD32"),
    };
    Ok(active)
}

pub struct Attempt {
    pub saved: Vec<Atom>,
    pub succeeded: bool,
}

pub struct Native {
    c: RustConnection,
    root: Window,
    wm: Window,
    atoms: Atoms,
    supported: HashSet<Atom>,
    compositor: Atom,
    tokens: Tokens,
    watched: HashSet<Window>,
    /// Global Home sequences: `home_shortcut`, then HomePage (#286).
    home: Vec<Sequence>,
    /// Quick-menu sequences: `quick_menu_shortcut` (ADR-0106).
    quick_menu: key_sequence::QuickMenu,
    /// Grabbed quick-menu (modifier mask, keycode) pairs; any other grabbed
    /// press is Home unless it is a power key.
    quick_menu_grabs: HashSet<(u16, Keycode)>,
    /// The power keys (`key_sequence::power`, ADR-0111); not configurable.
    power: Vec<Sequence>,
    /// Grabbed power-key (modifier mask, keycode) pairs.
    power_grabs: HashSet<(u16, Keycode)>,
    /// The desktop's power manager no longer acts on the power key (the
    /// dedicated Arinn XFCE session, owner decision of 2026-10-24): a power
    /// key it still grabs is read from XInput2 raw key events instead.
    raw_power: bool,
    /// Keycodes of power keys read from raw events, and whether raw key
    /// presses are selected on the root window.
    raw_power_codes: HashSet<Keycode>,
    raw_selected: bool,
    /// The power keys are read at all: only while the shell is alive
    /// (ADR-0111). On by default for the isolated tests.
    power_enabled: bool,
    /// The shell has focus: the remote's Menu keys are left to it.
    shell_focused: bool,
}
impl Native {
    pub fn connect(home_shortcut: &str, quick_menu_shortcut: &str) -> Result<Self> {
        ensure!(
            std::env::var("XDG_SESSION_TYPE").as_deref() == Ok("x11"),
            "X11 session required"
        );
        let (home, fallback) = key_sequence::home(home_shortcut);
        if fallback {
            eprintln!("arinn-platform-x11: invalid home_shortcut; using the default");
        }
        let (quick_menu, fallback) = key_sequence::quick_menu(quick_menu_shortcut, &home);
        if fallback {
            eprintln!("arinn-platform-x11: invalid quick_menu_shortcut; using the default");
        } else if quick_menu.keys.is_empty() {
            eprintln!("arinn-platform-x11: quick_menu_shortcut is a Home key; skipped");
        }
        Self::connect_display_with(None, home, quick_menu)
    }
    #[cfg(test)]
    pub(crate) fn connect_display(display: Option<&str>) -> Result<Self> {
        let home = key_sequence::home(key_sequence::DEFAULT_HOME).0;
        let quick_menu = key_sequence::quick_menu(key_sequence::DEFAULT_QUICK_MENU, &home).0;
        Self::connect_display_with(display, home, quick_menu)
    }
    pub(crate) fn connect_display_with(
        display: Option<&str>,
        home: Vec<Sequence>,
        quick_menu: key_sequence::QuickMenu,
    ) -> Result<Self> {
        let (c, screen) = x11rb::connect(display)?;
        let root = c.setup().roots[screen].root;
        let atoms = Atoms::new(&c)?.reply()?;
        let compositor = c
            .intern_atom(false, format!("_NET_WM_CM_S{screen}").as_bytes())?
            .reply()?
            .atom;
        let mut s = Self {
            c,
            root,
            wm: 0,
            atoms,
            supported: HashSet::new(),
            compositor,
            tokens: Tokens::default(),
            watched: HashSet::new(),
            home,
            quick_menu,
            quick_menu_grabs: HashSet::new(),
            power: key_sequence::power(),
            power_grabs: HashSet::new(),
            raw_power: false,
            raw_power_codes: HashSet::new(),
            power_enabled: true,
            raw_selected: false,
            shell_focused: false,
        };
        s.wm = s.wm_owner()?;
        s.supported = s
            .words(root, s.atoms._NET_SUPPORTED, AtomEnum::ATOM.into(), 512)?
            .into_iter()
            .collect();
        ensure!(
            [
                s.atoms._NET_CLIENT_LIST,
                s.atoms._NET_ACTIVE_WINDOW,
                s.atoms._NET_WM_STATE,
                s.atoms._NET_WM_STATE_FULLSCREEN,
                s.atoms._NET_WM_STATE_ABOVE,
                s.atoms._NET_WM_DESKTOP
            ]
            .iter()
            .all(|a| s.supported.contains(a)),
            "EWMH support required"
        );
        s.c.change_window_attributes(
            root,
            &ChangeWindowAttributesAux::new()
                .event_mask(EventMask::SUBSTRUCTURE_NOTIFY | EventMask::PROPERTY_CHANGE),
        )?
        .check()?;
        s.grab_home()?;
        Ok(s)
    }
    fn words(&self, window: Window, property: Atom, ty: Atom, limit: u32) -> Result<Vec<u32>> {
        let r = self
            .c
            .get_property(false, window, property, ty, 0, limit)?
            .reply()?;
        ensure!(r.bytes_after == 0, "property exceeds budget");
        if r.type_ == 0 {
            return Ok(Vec::new());
        }
        ensure!(r.type_ == ty && r.format == 32, "invalid property type");
        Ok(r.value32().unwrap().collect())
    }
    fn is_shell_identity(identity: &str) -> bool {
        ["org.arinn.Shell", "org.arinn.Shell.desktop"].contains(&identity)
    }
    fn identity_atoms(&self) -> [Atom; 3] {
        [
            self.atoms._NET_WM_DESKTOP_FILE,
            self.atoms._KDE_NET_WM_DESKTOP_FILE,
            self.atoms._GTK_APPLICATION_ID,
        ]
    }
    /// The first non-empty desktop identity of the replies, in `identity_atoms`
    /// priority order. Invalid or missing metadata counts as empty.
    fn identity_of(&self, replies: impl IntoIterator<Item = Option<Vec<u8>>>) -> String {
        replies
            .into_iter()
            .zip(self.identity_atoms())
            .map(|(value, atom)| {
                desktop_id(
                    &value.unwrap_or_default(),
                    atom != self.atoms._NET_WM_DESKTOP_FILE,
                )
            })
            .find(|id| !id.is_empty())
            .unwrap_or_default()
    }
    fn desktop_identity(&self, xid: Window) -> String {
        identity(&self.c, &self.atoms, xid)
    }
    fn watch_window(&mut self, xid: Window) -> Result<bool> {
        if self.watched.contains(&xid) {
            return Ok(true);
        }
        if self
            .c
            .change_window_attributes(
                xid,
                &ChangeWindowAttributesAux::new()
                    .event_mask(EventMask::STRUCTURE_NOTIFY | EventMask::PROPERTY_CHANGE),
            )?
            .check()
            .is_err()
        {
            return Ok(false);
        }
        self.watched.insert(xid);
        Ok(true)
    }
    /// Shell windows among `xids`. Requests are pipelined: one batch of
    /// attribute requests, then one batch of identity property requests, so
    /// the cost does not grow by a round trip per window (WM frames included).
    fn shell_candidates(&self, xids: &[Window]) -> Result<Vec<Window>> {
        let attributes = xids
            .iter()
            .map(|&xid| Ok((xid, self.c.get_window_attributes(xid)?)))
            .collect::<Result<Vec<_>>>()?;
        let eligible = attributes
            .into_iter()
            .filter_map(|(xid, cookie)| {
                let attr = cookie.reply().ok()?;
                (!attr.override_redirect && attr.class == WindowClass::INPUT_OUTPUT).then_some(xid)
            })
            .collect::<Vec<_>>();
        let pending = eligible
            .iter()
            .map(|&xid| {
                let cookies = self
                    .identity_atoms()
                    .into_iter()
                    .map(|atom| {
                        self.c
                            .get_property(false, xid, atom, self.atoms.UTF8_STRING, 0, 128)
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                Ok((xid, cookies))
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(pending
            .into_iter()
            .filter_map(|(xid, cookies)| {
                let replies = cookies.into_iter().map(|cookie| {
                    cookie
                        .reply()
                        .ok()
                        .and_then(|r| metadata(r, self.atoms.UTF8_STRING))
                });
                Self::is_shell_identity(&self.identity_of(replies)).then_some(xid)
            })
            .collect())
    }
    fn net_active_window(&self) -> Result<Option<Window>> {
        active_window(&self.c, self.root, self.atoms._NET_ACTIVE_WINDOW)
    }
    fn bytes(&self, window: Window, property: Atom, ty: Atom) -> Result<Vec<u8>> {
        let r = self
            .c
            .get_property(false, window, property, ty, 0, 128)?
            .reply()?;
        metadata(r, ty).ok_or_else(|| anyhow::anyhow!("invalid metadata"))
    }
    fn wm_owner(&self) -> Result<Window> {
        let values = self.words(
            self.root,
            self.atoms._NET_SUPPORTING_WM_CHECK,
            AtomEnum::WINDOW.into(),
            1,
        )?;
        let Some(&wm) = values.first() else {
            bail!("no EWMH window manager");
        };
        ensure!(
            wm != 0
                && self.words(
                    wm,
                    self.atoms._NET_SUPPORTING_WM_CHECK,
                    AtomEnum::WINDOW.into(),
                    1
                )? == [wm],
            "invalid EWMH owner"
        );
        Ok(wm)
    }
    fn grab_home(&mut self) -> Result<()> {
        self.c.ungrab_key(0, self.root, ModMask::ANY)?.check()?;
        let map = KeyMap::read(&self.c)?;
        let home = grab_sequences(&self.c, self.root, &map, &self.home, "Home")?;
        // Inside the shell Menu keeps its own meaning (owner decision of
        // 2026-10-21, #439): the remote's Menu keys are grabbed only while
        // another window has focus.
        let quick_menu = self.quick_menu.bound(self.shell_focused);
        // Only Home is required; a quick-menu key that cannot be grabbed
        // never stops the adapter (ADR-0106).
        self.quick_menu_grabs =
            grab_sequences(&self.c, self.root, &map, &quick_menu, "quick-menu")?;
        // The power keys come last, also while the shell has focus; Home
        // and the quick menu keep a key they name. A power key another
        // client holds (xfce4-power-manager) stays that client's and never
        // stops the adapter (ADR-0111).
        let power: Vec<Sequence> = if self.power_enabled {
            self.power
                .iter()
                .copied()
                .filter(|key| !self.home.contains(key) && !quick_menu.contains(key))
                .collect()
        } else {
            Vec::new()
        };
        // The dedicated Arinn XFCE session reads the power keys from raw
        // events only and never grabs them: xfce4-power-manager's own grab
        // must succeed whenever it starts, since it never grabs again
        // (review of #516). Its action is "do nothing" while Arinn runs.
        let mut raw_power_codes = HashSet::new();
        if self.raw_power {
            self.power_grabs = HashSet::new();
            for sequence in &power {
                raw_power_codes.extend(map.combinations(sequence).iter().map(|(_, key)| *key));
            }
        } else {
            self.power_grabs = grab_sequences(&self.c, self.root, &map, &power, "power")?;
        }
        self.select_raw_power(raw_power_codes)?;
        ensure!(!home.is_empty(), "Home key unavailable");
        self.c.flush()?;
        Ok(())
    }
    /// Reads the power keys (`enabled`) or leaves them alone: grabbed, or
    /// with `raw` from raw events only, never grabbed (the dedicated Arinn
    /// XFCE session, ADR-0111). Returns how the key is read now.
    pub fn set_power(&mut self, enabled: bool, raw: bool) -> Result<Reading> {
        let raw = enabled && raw;
        if self.power_enabled != enabled || self.raw_power != raw {
            self.power_enabled = enabled;
            self.raw_power = raw;
            self.grab_home()?;
        }
        Ok(self.reading())
    }
    /// Whether another client grabs a power key now (xfce4-power-manager
    /// after a successful start). Call it while Arinn grabs none.
    pub fn power_held_elsewhere(&self) -> Result<bool> {
        power_held(&self.c, self.root, &self.power)
    }
    /// How the power key is read now.
    pub fn reading(&self) -> Reading {
        // Any key read from raw events means another client still grabs
        // it: its manager must do nothing with it (xfconf), also when
        // another power key is grabbed (Codex on #517).
        if !self.raw_power_codes.is_empty() {
            Reading::Raw
        } else if !self.power_grabs.is_empty() {
            Reading::Grab
        } else {
            Reading::None
        }
    }
    /// Selects XInput2 raw key presses on the root window while a power key
    /// is read from them; raw events reach every client that selects them,
    /// whoever grabs the key.
    fn select_raw_power(&mut self, codes: HashSet<Keycode>) -> Result<()> {
        use xinput::ConnectionExt as _;
        let wanted = !codes.is_empty();
        if wanted && !self.raw_selected {
            let version = self
                .c
                .xinput_xi_query_version(2, 2)
                .ok()
                .and_then(|cookie| cookie.reply().ok());
            if !version.is_some_and(|v| (v.major_version, v.minor_version) >= (2, 1)) {
                eprintln!(
                    "arinn-platform-x11: XInput 2.1 unavailable; the power key stays the desktop's"
                );
                self.raw_power_codes.clear();
                return Ok(());
            }
        }
        if wanted != self.raw_selected {
            let mask = if wanted {
                vec![xinput::XIEventMask::RAW_KEY_PRESS]
            } else {
                Vec::new()
            };
            self.c
                .xinput_xi_select_events(
                    self.root,
                    &[xinput::EventMask {
                        deviceid: ALL_MASTER_DEVICES,
                        mask,
                    }],
                )?
                .check()?;
            self.raw_selected = wanted;
        }
        self.raw_power_codes = codes;
        Ok(())
    }
    pub fn snapshot(&mut self) -> Result<Snapshot> {
        // Establish an X-server barrier before draining lifecycle events.
        self.c.get_input_focus()?.reply()?;
        let mut home_key = false;
        let mut quick_menu_key = false;
        let mut power_key = false;
        while let Some(event) = self.c.poll_for_event()? {
            match event {
                Event::DestroyNotify(e) => {
                    self.tokens.forget(e.window);
                    self.watched.remove(&e.window);
                }
                // The latest global key wins; a press of an ungrabbed
                // combination never reaches the root window.
                Event::KeyPress(e) => {
                    let pressed = (u16::from(e.state) & 0xff, e.detail);
                    quick_menu_key = self.quick_menu_grabs.contains(&pressed);
                    power_key = self.power_grabs.contains(&pressed);
                    home_key = !quick_menu_key && !power_key;
                }
                // A power key the desktop's power manager grabs but no
                // longer acts on; repeats of a held key are not presses.
                Event::XinputRawKeyPress(e)
                    if raw_power_press(&self.raw_power_codes, e.detail, e.flags) =>
                {
                    power_key = true;
                    quick_menu_key = false;
                    home_key = false;
                }
                Event::MappingNotify(_) => self.grab_home()?,
                _ => (),
            }
        }
        ensure!(self.wm_owner()? == self.wm, "window manager changed");
        let clients = self.words(
            self.root,
            self.atoms._NET_CLIENT_LIST,
            AtomEnum::WINDOW.into(),
            4096,
        )?;
        let live = clients.iter().copied().collect::<HashSet<_>>();
        // Lifetimes end when a window leaves _NET_CLIENT_LIST, the shell
        // included: an unlisted (hidden) shell gets no lifetime token.
        self.tokens.retain(&live);
        self.watched.retain(|id| live.contains(id));
        let active = self.net_active_window()?;
        let mut shell_matches = Vec::new();
        let mut rows = Vec::new();
        for xid in clients {
            let Ok(attr) = self.c.get_window_attributes(xid)?.reply() else {
                continue;
            };
            if attr.override_redirect || attr.class != WindowClass::INPUT_OUTPUT {
                continue;
            }
            if !self.watch_window(xid)? {
                continue;
            }
            let identity = self.desktop_identity(xid);
            if Self::is_shell_identity(&identity) {
                shell_matches.push(xid);
                continue;
            }
            let types = self
                .words(
                    xid,
                    self.atoms._NET_WM_WINDOW_TYPE,
                    AtomEnum::ATOM.into(),
                    32,
                )
                .unwrap_or_default();
            if !types.is_empty() && !types.contains(&self.atoms._NET_WM_WINDOW_TYPE_NORMAL) {
                continue;
            }
            let states = self
                .words(xid, self.atoms._NET_WM_STATE, AtomEnum::ATOM.into(), 32)
                .unwrap_or_default();
            let launch = launch_id(
                &self
                    .bytes(xid, self.atoms.WM_CLASS, AtomEnum::STRING.into())
                    .unwrap_or_default(),
            );
            rows.push(Row {
                xid,
                token: self.tokens.get(xid),
                desktop_id: identity,
                launch,
                flags: i32::from(active == Some(xid))
                    | (i32::from(states.contains(&self.atoms._NET_WM_STATE_FULLSCREEN)) << 1)
                    | 4
                    | if self.supported.contains(&self.atoms._NET_CLOSE_WINDOW) {
                        8
                    } else {
                        0
                    },
            });
        }
        // A hidden shell is withdrawn and therefore absent from
        // _NET_CLIENT_LIST (xfwm4). Find it among direct root children only,
        // for Home; it never enters the inventory.
        let root_children = self.c.query_tree(self.root)?.reply()?.children;
        ensure!(
            root_children.len() <= 4096,
            "root window children exceed budget"
        );
        let unlisted = root_children
            .into_iter()
            .filter(|xid| !live.contains(xid))
            .collect::<Vec<_>>();
        let listed_shell = !shell_matches.is_empty();
        shell_matches.extend(self.shell_candidates(&unlisted)?);
        ensure!(shell_matches.len() <= 1, "ambiguous shell identity");
        let shell = shell_matches.first().copied();
        // Only a listed shell has a lifetime; presentation requires one.
        let shell_token = match shell {
            Some(xid) if listed_shell => self.tokens.get(xid),
            _ => String::new(),
        };
        if let Some(xid) = shell {
            // Observe DestroyNotify for the shell even while it is unlisted.
            self.watch_window(xid)?;
        }
        let shell_focused = shell.is_some() && active == shell;
        if shell_focused != self.shell_focused {
            self.shell_focused = shell_focused;
            self.grab_home()?;
        }
        Ok(Snapshot {
            rows,
            shell,
            shell_token,
            active,
            home_key,
            quick_menu_key,
            power_key,
            power_reading: self.reading(),
            compositor: self.c.get_selection_owner(self.compositor)?.reply()?.owner != 0,
        })
    }
    fn message(&self, xid: Window, ty: Atom, data: [u32; 5]) -> Result<()> {
        ensure!(self.wm_owner()? == self.wm, "window manager changed");
        self.c.get_window_attributes(xid)?.reply()?;
        let event = ClientMessageEvent::new(32, xid, ty, data);
        self.c
            .send_event(
                false,
                self.root,
                EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
                event,
            )?
            .check()?;
        self.c.flush()?;
        Ok(())
    }
    pub fn activate(&self, xid: Window) -> Result<()> {
        self.message(
            xid,
            self.atoms._NET_ACTIVE_WINDOW,
            [2, CURRENT_TIME, 0, 0, 0],
        )
    }
    pub fn close(&self, xid: Window) -> Result<()> {
        ensure!(
            self.supported.contains(&self.atoms._NET_CLOSE_WINDOW),
            "close unsupported"
        );
        self.message(
            xid,
            self.atoms._NET_CLOSE_WINDOW,
            [CURRENT_TIME, 2, 0, 0, 0],
        )
    }
    fn state(&self, xid: Window, atom: Atom, enabled: bool) -> Result<()> {
        self.message(
            xid,
            self.atoms._NET_WM_STATE,
            [u32::from(enabled), atom, 0, 2, 0],
        )
    }
    pub fn raise_shell(&self, xid: Window, above: bool) -> Result<()> {
        if above {
            // Keep Home over the current Activity instead of switching to the
            // virtual desktop on which the shell originally mapped.
            self.message(xid, self.atoms._NET_WM_DESKTOP, [u32::MAX, 2, 0, 0, 0])?;
            self.state(xid, self.atoms._NET_WM_STATE_FULLSCREEN, true)?;
        }
        self.state(xid, self.atoms._NET_WM_STATE_ABOVE, above)?;
        if above {
            self.activate(xid)?;
        }
        Ok(())
    }
    pub fn present(&self, video: Window, shell: Window, launch: &str) -> Result<Attempt> {
        ensure!(
            self.c.get_selection_owner(self.compositor)?.reply()?.owner != 0,
            "compositor required"
        );
        ensure!(
            launch_id(&self.bytes(video, self.atoms.WM_CLASS, AtomEnum::STRING.into())?) == launch
                && !launch.is_empty(),
            "video identity changed"
        );
        let saved = self.words(video, self.atoms._NET_WM_STATE, AtomEnum::ATOM.into(), 32)?;
        // Preserve restoration evidence even if any later request fails.
        let result = (|| -> Result<()> {
            self.state(video, self.atoms._NET_WM_STATE_BELOW, false)?;
            self.state(video, self.atoms._NET_WM_STATE_FULLSCREEN, true)?;
            self.activate(video)?;
            self.raise_shell(shell, true)
        })();
        Ok(Attempt {
            saved,
            succeeded: result.is_ok(),
        })
    }
    pub fn has_launch(&self, video: Window, launch: &str) -> Result<bool> {
        match self.bytes(video, self.atoms.WM_CLASS, AtomEnum::STRING.into()) {
            Ok(value) => Ok(!launch.is_empty() && launch_id(&value) == launch),
            Err(error) => {
                if matches!(error.downcast_ref::<x11rb::errors::ReplyError>(),
                    Some(x11rb::errors::ReplyError::X11Error(e)) if e.error_kind == x11rb::protocol::ErrorKind::Window)
                {
                    Ok(false)
                } else {
                    Err(error)
                }
            }
        }
    }
    pub fn release(&self, video: Window, saved: &[Atom], launch: &str) -> Result<()> {
        ensure!(
            launch_id(&self.bytes(video, self.atoms.WM_CLASS, AtomEnum::STRING.into())?) == launch
                && !launch.is_empty(),
            "video identity changed"
        );
        for atom in [
            self.atoms._NET_WM_STATE_FULLSCREEN,
            self.atoms._NET_WM_STATE_BELOW,
        ] {
            self.state(video, atom, saved.contains(&atom))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_power_keycodes_count_and_never_a_repeat() {
        // Review of #517: raw events report every key press; only the power
        // keycodes are looked at, and a held key's repeats are no presses.
        let codes: HashSet<Keycode> = [124].into_iter().collect();
        let none = xinput::KeyEventFlags::from(0u32);
        assert!(raw_power_press(&codes, 124, none));
        assert!(!raw_power_press(&codes, 38, none));
        assert!(!raw_power_press(&codes, 124 + 256, none));
        assert!(!raw_power_press(
            &codes,
            124,
            xinput::KeyEventFlags::KEY_REPEAT
        ));
        assert!(!raw_power_press(&HashSet::new(), 124, none));
    }
    #[test]
    fn recycled_xids_get_fresh_opaque_tokens() {
        let mut tokens = Tokens::default();
        let original = tokens.get(42);
        assert_eq!(tokens.get(42), original);
        tokens.forget(42);
        assert_ne!(tokens.get(42), original);
        let second = tokens.get(42);
        tokens.retain(&HashSet::new());
        assert_ne!(tokens.get(42), second);
    }
    #[test]
    fn native_instance_requires_exact_canonical_launch_uuid() {
        let id = Uuid::new_v4().to_string();
        assert_eq!(
            launch_id(format!("org.arinn.playback.{id}\0mpv\0").as_bytes()),
            id
        );
        for value in [
            format!("mpv\0org.arinn.playback.{id}\0"),
            format!("org.arinn.Playback.{id}\0mpv\0"),
            "org.arinn.playback.00000000-0000-0000-0000-000000000000\0mpv\0".into(),
            format!("org.arinn.playback.{}\0mpv\0", id.replace('-', "")),
        ] {
            assert!(launch_id(value.as_bytes()).is_empty());
        }
    }
    #[test]
    fn desktop_metadata_never_infers_identity_from_a_path() {
        assert_eq!(
            desktop_id(b"org.example.Player.desktop\0", false),
            "org.example.Player.desktop"
        );
        assert!(desktop_id(b"/home/example/private.desktop", true).is_empty());
        assert!(desktop_id(b"invalid\0identity", true).is_empty());
        assert!(desktop_id(b"org.gnome.Nautilus", false).is_empty());
        assert_eq!(desktop_id(b"org.arinn.Shell", true), "org.arinn.Shell");
        assert_eq!(
            desktop_id(b"org.gnome.Nautilus", true),
            "org.gnome.Nautilus.desktop"
        );
        assert_eq!(
            desktop_id(b"org.gnome.Nautilus.desktop", true),
            "org.gnome.Nautilus.desktop"
        );
        assert!(desktop_id("x".repeat(256).as_bytes(), true).is_empty());
    }
}

#[cfg(test)]
pub(crate) mod fixture {
    use super::*;
    use std::{
        io::{BufRead, BufReader},
        process::{Child, Command, Stdio},
    };
    use x11rb::wrapper::ConnectionExt as _;
    pub struct Desktop {
        server: Child,
        pub c: RustConnection,
        pub root: Window,
        pub atoms: Atoms,
        pub display: String,
        pub wm: Window,
    }
    impl Drop for Desktop {
        fn drop(&mut self) {
            let _ = self.server.kill();
            let _ = self.server.wait();
        }
    }
    impl Desktop {
        pub fn new() -> Result<Self> {
            // Always spawn our own X server. Never inspect or use inherited DISPLAY.
            let executable = std::env::var("ARINN_XVFB").unwrap_or_else(|_| "Xvfb".into());
            let mut server = Command::new(executable)
                .args([
                    "-displayfd",
                    "1",
                    "-screen",
                    "0",
                    "1280x720x24",
                    "-nolisten",
                    "tcp",
                    "-noreset",
                ])
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()?;
            let mut number = String::new();
            BufReader::new(server.stdout.take().unwrap()).read_line(&mut number)?;
            ensure!(
                !number.trim().is_empty() && number.trim().bytes().all(|c| c.is_ascii_digit()),
                "isolated X server did not start"
            );
            let display = format!(":{}", number.trim());
            let (c, screen) = x11rb::connect(Some(&display))?;
            let root = c.setup().roots[screen].root;
            let atoms = Atoms::new(&c)?.reply()?;
            let wm = c.generate_id()?;
            c.create_window(
                0,
                wm,
                root,
                0,
                0,
                1,
                1,
                0,
                WindowClass::INPUT_OUTPUT,
                0,
                &CreateWindowAux::new(),
            )?
            .check()?;
            c.change_property32(
                PropMode::REPLACE,
                root,
                atoms._NET_SUPPORTING_WM_CHECK,
                AtomEnum::WINDOW,
                &[wm],
            )?
            .check()?;
            c.change_property32(
                PropMode::REPLACE,
                wm,
                atoms._NET_SUPPORTING_WM_CHECK,
                AtomEnum::WINDOW,
                &[wm],
            )?
            .check()?;
            c.change_property32(
                PropMode::REPLACE,
                root,
                atoms._NET_SUPPORTED,
                AtomEnum::ATOM,
                &[
                    atoms._NET_CLIENT_LIST,
                    atoms._NET_ACTIVE_WINDOW,
                    atoms._NET_CLOSE_WINDOW,
                    atoms._NET_WM_STATE,
                    atoms._NET_WM_STATE_FULLSCREEN,
                    atoms._NET_WM_STATE_ABOVE,
                    atoms._NET_WM_STATE_BELOW,
                    atoms._NET_WM_DESKTOP,
                ],
            )?
            .check()?;
            c.change_window_attributes(
                root,
                &ChangeWindowAttributesAux::new()
                    .event_mask(EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY),
            )?
            .check()?;
            c.flush()?;
            Ok(Self {
                server,
                c,
                root,
                atoms,
                display,
                wm,
            })
        }
        pub fn client(&self, identity: &str, launch: &str) -> Result<Window> {
            let xid = self.c.generate_id()?;
            self.c
                .create_window(
                    0,
                    xid,
                    self.root,
                    0,
                    0,
                    100,
                    100,
                    0,
                    WindowClass::INPUT_OUTPUT,
                    0,
                    &CreateWindowAux::new(),
                )?
                .check()?;
            self.c
                .change_property8(
                    PropMode::REPLACE,
                    xid,
                    self.atoms._GTK_APPLICATION_ID,
                    self.atoms.UTF8_STRING,
                    identity.as_bytes(),
                )?
                .check()?;
            self.c
                .change_property8(
                    PropMode::REPLACE,
                    xid,
                    self.atoms.WM_CLASS,
                    AtomEnum::STRING,
                    format!("org.arinn.playback.{launch}\0generic\0").as_bytes(),
                )?
                .check()?;
            self.c.flush()?;
            Ok(xid)
        }
        pub fn inventory(&self, windows: &[Window], active: Window) -> Result<()> {
            self.c
                .change_property32(
                    PropMode::REPLACE,
                    self.root,
                    self.atoms._NET_CLIENT_LIST,
                    AtomEnum::WINDOW,
                    windows,
                )?
                .check()?;
            self.set_net_active_window(&[active])?;
            self.c.flush()?;
            Ok(())
        }
        pub fn set_net_active_window(&self, words: &[u32]) -> Result<()> {
            self.c
                .change_property32(
                    PropMode::REPLACE,
                    self.root,
                    self.atoms._NET_ACTIVE_WINDOW,
                    AtomEnum::WINDOW,
                    words,
                )?
                .check()?;
            Ok(())
        }
        pub fn compositing(&self, enabled: bool) -> Result<()> {
            let selection = self.c.intern_atom(false, b"_NET_WM_CM_S0")?.reply()?.atom;
            self.c
                .set_selection_owner(if enabled { self.wm } else { 0 }, selection, CURRENT_TIME)?
                .check()?;
            self.c.flush()?;
            Ok(())
        }
        pub fn home_key(&self) -> Result<()> {
            self.home_key_with(0xffeb)
        }
        pub fn home_key_with(&self, modifier_symbol: u32) -> Result<()> {
            self.press(&[modifier_symbol], 0xffc9)
        }
        /// Presses and releases `symbol` while holding `modifiers`, all keysyms.
        pub fn press(&self, modifiers: &[u32], symbol: u32) -> Result<()> {
            use x11rb::protocol::xtest::ConnectionExt as _;
            let setup = self.c.setup();
            let mapping = self
                .c
                .get_keyboard_mapping(setup.min_keycode, setup.max_keycode - setup.min_keycode + 1)?
                .reply()?;
            let key = |symbol| {
                mapping
                    .keysyms
                    .chunks(usize::from(mapping.keysyms_per_keycode))
                    .position(|syms| syms.contains(&symbol))
                    .map(|i| setup.min_keycode + i as u8)
                    .unwrap()
            };
            let held: Vec<u8> = modifiers.iter().map(|m| key(*m)).collect();
            let target = key(symbol);
            let mut events: Vec<(u8, u8)> = held.iter().map(|code| (2, *code)).collect();
            events.extend([(2, target), (3, target)]);
            events.extend(held.iter().rev().map(|code| (3, *code)));
            for (kind, code) in events {
                self.c
                    .xtest_fake_input(kind, code, CURRENT_TIME, self.root, 0, 0, 0)?
                    .check()?;
            }
            self.c.flush()?;
            Ok(())
        }
        pub fn messages(&self) -> Result<Vec<ClientMessageEvent>> {
            self.c.get_input_focus()?.reply()?;
            let mut messages = Vec::new();
            while let Some(event) = self.c.poll_for_event()? {
                if let Event::ClientMessage(message) = event {
                    messages.push(message);
                }
            }
            Ok(messages)
        }
    }
    #[test]
    #[ignore = "requires Xvfb; spawns an isolated X server, never uses the real DISPLAY"]
    fn configured_home_and_home_page_replace_meta_f12() -> Result<()> {
        // #286: home_shortcut decides the grab, and HomePage is always added.
        let desktop = Desktop::new()?;
        let home = crate::key_sequence::home("Ctrl+Alt+H").0;
        let mut native =
            Native::connect_display_with(Some(&desktop.display), home, Default::default())?;
        native.snapshot()?;
        desktop.press(&[0xffe3, 0xffe9], u32::from(b'h'))?; // Control_L, Alt_L
        assert!(native.snapshot()?.home_key);
        desktop.press(&[], 0x1008_ff18)?; // XF86HomePage
        assert!(native.snapshot()?.home_key);
        desktop.press(&[0xffe9], u32::from(b'h'))?; // Alt+H alone is not Home.
        assert!(!native.snapshot()?.home_key);
        desktop.home_key()?; // Meta+F12 is no longer bound.
        assert!(!native.snapshot()?.home_key);
        Ok(())
    }
    #[test]
    #[ignore = "requires Xvfb; spawns an isolated X server, never uses the real DISPLAY"]
    fn quick_menu_key_is_reported_apart_from_home() -> Result<()> {
        // ADR-0106: quick_menu_shortcut is grabbed next to the Home keys and
        // reported as its own key; the latest press wins.
        let desktop = Desktop::new()?;
        let home = crate::key_sequence::home("Meta+F12").0;
        let quick_menu = crate::key_sequence::quick_menu("Ctrl+Alt+Q", &home).0;
        let mut native = Native::connect_display_with(Some(&desktop.display), home, quick_menu)?;
        native.snapshot()?;
        desktop.press(&[0xffe3, 0xffe9], u32::from(b'q'))?; // Control_L, Alt_L
        let snapshot = native.snapshot()?;
        assert!(snapshot.quick_menu_key && !snapshot.home_key);
        desktop.home_key()?;
        let snapshot = native.snapshot()?;
        assert!(snapshot.home_key && !snapshot.quick_menu_key);
        desktop.home_key()?;
        desktop.press(&[0xffe3, 0xffe9], u32::from(b'q'))?;
        let snapshot = native.snapshot()?;
        assert!(snapshot.quick_menu_key && !snapshot.home_key);
        // The grab survives a mapping change.
        let mapping = desktop.c.get_modifier_mapping()?.reply()?.keycodes;
        assert_eq!(
            desktop.c.set_modifier_mapping(&mapping)?.reply()?.status,
            x11rb::protocol::xproto::MappingStatus::SUCCESS
        );
        native.snapshot()?;
        desktop.press(&[0xffe3, 0xffe9], u32::from(b'q'))?;
        assert!(native.snapshot()?.quick_menu_key);
        Ok(())
    }
    #[test]
    #[ignore = "requires Xvfb; spawns an isolated X server, never uses the real DISPLAY"]
    fn remote_menu_opens_the_quick_menu_only_outside_the_shell() -> Result<()> {
        // Owner decision of 2026-10-21 (#439): the remote's Menu key opens
        // the quick menu over another window; the shell keeps Menu.
        let desktop = Desktop::new()?;
        let shell = desktop.client("org.arinn.Shell", "")?;
        let video = desktop.client("org.example.Player.desktop", "")?;
        desktop.inventory(&[shell, video], video)?;
        let home = crate::key_sequence::home("Meta+F12").0;
        let quick_menu = crate::key_sequence::quick_menu("Ctrl+Alt+Q", &home).0;
        let mut native = Native::connect_display_with(Some(&desktop.display), home, quick_menu)?;
        native.snapshot()?;
        desktop.press(&[], 0xff67)?; // Menu
        assert!(native.snapshot()?.quick_menu_key);
        // The shell takes focus: the next snapshot releases Menu to it.
        desktop.inventory(&[shell, video], shell)?;
        native.snapshot()?;
        desktop.press(&[], 0xff67)?;
        assert!(!native.snapshot()?.quick_menu_key);
        // The configured key still opens the quick menu over the shell.
        desktop.press(&[0xffe3, 0xffe9], u32::from(b'q'))?;
        assert!(native.snapshot()?.quick_menu_key);
        desktop.inventory(&[shell, video], video)?;
        native.snapshot()?;
        desktop.press(&[], 0xff67)?;
        let snapshot = native.snapshot()?;
        assert!(snapshot.quick_menu_key && !snapshot.home_key);
        Ok(())
    }
    #[test]
    #[ignore = "requires Xvfb; spawns an isolated X server, never uses the real DISPLAY"]
    fn held_quick_menu_key_never_stops_the_adapter() -> Result<()> {
        // Another client holds the quick-menu key: Home still works.
        let desktop = Desktop::new()?;
        let other = Native::connect_display_with(
            Some(&desktop.display),
            crate::key_sequence::home("Ctrl+Alt+Q").0,
            Default::default(),
        )?;
        let home = crate::key_sequence::home("Meta+F12").0;
        let quick_menu = crate::key_sequence::quick_menu("Ctrl+Alt+Q", &home).0;
        let mut native = Native::connect_display_with(Some(&desktop.display), home, quick_menu)?;
        native.snapshot()?;
        desktop.press(&[0xffe3, 0xffe9], u32::from(b'q'))?;
        assert!(!native.snapshot()?.quick_menu_key);
        desktop.home_key()?;
        assert!(native.snapshot()?.home_key);
        drop(other);
        Ok(())
    }
    /// ADR-0111: XF86PowerOff is grabbed next to the Home and quick-menu
    /// keys and reported as its own key, also while the shell has focus.
    #[test]
    #[ignore = "requires Xvfb; spawns an isolated X server, never uses the real DISPLAY"]
    fn power_key_is_reported_apart_from_home_and_the_quick_menu() -> Result<()> {
        let desktop = Desktop::new()?;
        let shell = desktop.client("org.arinn.Shell", "")?;
        let video = desktop.client("org.example.Player.desktop", "")?;
        desktop.inventory(&[shell, video], video)?;
        let home = crate::key_sequence::home("Meta+F12").0;
        let quick_menu = crate::key_sequence::quick_menu("Ctrl+Alt+Q", &home).0;
        let mut native = Native::connect_display_with(Some(&desktop.display), home, quick_menu)?;
        assert_eq!(native.snapshot()?.power_reading, Reading::Grab);
        desktop.press(&[], 0x1008_ff2a)?; // XF86PowerOff
        let snapshot = native.snapshot()?;
        assert!(snapshot.power_key && !snapshot.home_key && !snapshot.quick_menu_key);
        desktop.home_key()?;
        let snapshot = native.snapshot()?;
        assert!(snapshot.home_key && !snapshot.power_key);
        desktop.press(&[0xffe3, 0xffe9], u32::from(b'q'))?;
        let snapshot = native.snapshot()?;
        assert!(snapshot.quick_menu_key && !snapshot.power_key);
        // The latest press wins.
        desktop.home_key()?;
        desktop.press(&[], 0x1008_ff2a)?;
        let snapshot = native.snapshot()?;
        assert!(snapshot.power_key && !snapshot.home_key);
        // Unlike Menu, the shell's focus keeps the power key grabbed.
        desktop.inventory(&[shell, video], shell)?;
        native.snapshot()?;
        desktop.press(&[], 0x1008_ff2a)?;
        assert!(native.snapshot()?.power_key);
        // The grab survives a mapping change.
        let mapping = desktop.c.get_modifier_mapping()?.reply()?.keycodes;
        assert_eq!(
            desktop.c.set_modifier_mapping(&mapping)?.reply()?.status,
            x11rb::protocol::xproto::MappingStatus::SUCCESS
        );
        native.snapshot()?;
        desktop.press(&[], 0x1008_ff2a)?;
        let snapshot = native.snapshot()?;
        assert!(snapshot.power_key && snapshot.power_reading == Reading::Grab);
        Ok(())
    }
    /// ADR-0111: a power key another client holds (xfce4-power-manager)
    /// stays that client's; Arinn changes no desktop setting, does not
    /// report the key as grabbed, and Home still works.
    #[test]
    #[ignore = "requires Xvfb; spawns an isolated X server, never uses the real DISPLAY"]
    fn held_power_key_is_read_from_raw_events_only_when_its_manager_does_nothing() -> Result<()> {
        let desktop = Desktop::new()?;
        let setup = desktop.c.setup();
        let mapping = desktop
            .c
            .get_keyboard_mapping(setup.min_keycode, setup.max_keycode - setup.min_keycode + 1)?
            .reply()?;
        let power = mapping
            .keysyms
            .chunks(usize::from(mapping.keysyms_per_keycode))
            .position(|syms| syms.contains(&0x1008_ff2a))
            .map(|i| setup.min_keycode + i as u8)
            .unwrap();
        // The power manager grabs the key without modifiers, as xfpm does.
        desktop
            .c
            .grab_key(
                false,
                desktop.root,
                ModMask::from(0u16),
                power,
                GrabMode::ASYNC,
                GrabMode::ASYNC,
            )?
            .check()?;
        desktop.c.flush()?;
        let mut native = Native::connect_display(Some(&desktop.display))?;
        let snapshot = native.snapshot()?;
        assert_eq!(snapshot.power_reading, Reading::None);
        desktop.press(&[], 0x1008_ff2a)?;
        assert!(!native.snapshot()?.power_key);
        desktop.home_key()?;
        assert!(native.snapshot()?.home_key);
        // The dedicated Arinn XFCE session (owner decision of 2026-10-24):
        // the power manager keeps its grab but does nothing with the key,
        // and Arinn reads it from raw events.
        native.set_power(true, true)?;
        let snapshot = native.snapshot()?;
        assert!(snapshot.power_reading == Reading::Raw && !snapshot.power_key);
        assert_eq!(snapshot.power_reading, Reading::Raw);
        desktop.press(&[], 0x1008_ff2a)?;
        let snapshot = native.snapshot()?;
        assert!(snapshot.power_key && !snapshot.home_key && !snapshot.quick_menu_key);
        // Other keys are not power presses; Home keeps its own grab.
        desktop.press(&[], 0x1008_ff18)?; // XF86HomePage
        let snapshot = native.snapshot()?;
        assert!(snapshot.home_key && !snapshot.power_key);
        // It survives a mapping change.
        let mapping = desktop.c.get_modifier_mapping()?.reply()?.keycodes;
        desktop.c.set_modifier_mapping(&mapping)?.reply()?;
        native.snapshot()?;
        desktop.press(&[], 0x1008_ff2a)?;
        assert!(native.snapshot()?.power_key);
        // Back in an ordinary desktop the key is not read any more.
        native.set_power(true, false)?;
        assert_eq!(native.snapshot()?.power_reading, Reading::None);
        desktop.press(&[], 0x1008_ff2a)?;
        assert!(!native.snapshot()?.power_key);
        Ok(())
    }
    /// Codex P1 on #517 and review of #516: with `XF86PowerDown` mapped as
    /// well, the dedicated session reads both keys from raw events and
    /// grabs neither, while the power manager holds `XF86PowerOff`; the
    /// reading is `Raw`, so the power manager is set to do nothing.
    #[test]
    #[ignore = "requires Xvfb; spawns an isolated X server, never uses the real DISPLAY"]
    fn a_power_key_read_raw_beside_a_grabbed_one_reads_raw() -> Result<()> {
        let desktop = Desktop::new()?;
        let setup = desktop.c.setup();
        let count = setup.max_keycode - setup.min_keycode + 1;
        let mapping = desktop
            .c
            .get_keyboard_mapping(setup.min_keycode, count)?
            .reply()?;
        let per = usize::from(mapping.keysyms_per_keycode);
        let rows: Vec<&[u32]> = mapping.keysyms.chunks(per).collect();
        let code = |sym: u32| {
            rows.iter()
                .position(|syms| syms.contains(&sym))
                .map(|i| setup.min_keycode + i as u8)
        };
        let power = code(0x1008_ff2a).unwrap();
        if code(0x1008_ff21).is_none() {
            // Map XF86PowerDown on a free keycode.
            let free = rows
                .iter()
                .position(|syms| syms.iter().all(|s| *s == 0))
                .map(|i| setup.min_keycode + i as u8)
                .unwrap();
            let mut syms = vec![0u32; per];
            syms[0] = 0x1008_ff21;
            desktop
                .c
                .change_keyboard_mapping(1, free, mapping.keysyms_per_keycode, &syms)?
                .check()?;
        }
        desktop
            .c
            .grab_key(
                false,
                desktop.root,
                ModMask::from(0u16),
                power,
                GrabMode::ASYNC,
                GrabMode::ASYNC,
            )?
            .check()?;
        desktop.c.flush()?;
        let mut native = Native::connect_display(Some(&desktop.display))?;
        native.snapshot()?;
        assert_eq!(native.set_power(true, true)?, Reading::Raw);
        assert_eq!(native.snapshot()?.power_reading, Reading::Raw);
        desktop.press(&[], 0x1008_ff21)?;
        assert!(native.snapshot()?.power_key);
        desktop.press(&[], 0x1008_ff2a)?;
        assert!(native.snapshot()?.power_key);
        // Arinn grabs neither: the power manager's own grab of the other
        // key would succeed too.
        assert!(native.power_held_elsewhere()?);
        let other = x11rb::connect(Some(&desktop.display))?.0;
        let down = code(0x1008_ff21).or_else(|| {
            let map = desktop
                .c
                .get_keyboard_mapping(setup.min_keycode, count)
                .ok()?
                .reply()
                .ok()?;
            map.keysyms
                .chunks(per)
                .position(|syms| syms.contains(&0x1008_ff21))
                .map(|i| setup.min_keycode + i as u8)
        });
        other
            .grab_key(
                false,
                desktop.root,
                ModMask::ANY,
                down.unwrap(),
                GrabMode::ASYNC,
                GrabMode::ASYNC,
            )?
            .check()?;
        Ok(())
    }

    /// Review of #516: in the dedicated session Arinn never grabs the power
    /// key, so xfce4-power-manager starting later still gets its grab
    /// (it never grabs again), and Arinn keeps reading the key from raw
    /// events. `power_held_elsewhere` sees the manager's grab, and nothing
    /// before it.
    #[test]
    #[ignore = "requires Xvfb; spawns an isolated X server, never uses the real DISPLAY"]
    fn a_power_manager_starting_later_gets_its_grab_in_the_dedicated_session() -> Result<()> {
        let desktop = Desktop::new()?;
        let mut native = Native::connect_display(Some(&desktop.display))?;
        native.snapshot()?;
        assert_eq!(native.set_power(true, true)?, Reading::Raw);
        assert!(!native.power_held_elsewhere()?);
        let setup = desktop.c.setup();
        let mapping = desktop
            .c
            .get_keyboard_mapping(setup.min_keycode, setup.max_keycode - setup.min_keycode + 1)?
            .reply()?;
        let power = mapping
            .keysyms
            .chunks(usize::from(mapping.keysyms_per_keycode))
            .position(|syms| syms.contains(&0x1008_ff2a))
            .map(|i| setup.min_keycode + i as u8)
            .unwrap();
        // xfce4-power-manager grabs with any modifier, as xfpm-button.c does.
        let manager = x11rb::connect(Some(&desktop.display))?.0;
        manager
            .grab_key(
                false,
                desktop.root,
                ModMask::ANY,
                power,
                GrabMode::ASYNC,
                GrabMode::ASYNC,
            )?
            .check()?;
        manager.flush()?;
        assert!(native.power_held_elsewhere()?);
        desktop.press(&[], 0x1008_ff2a)?;
        assert!(native.snapshot()?.power_key);
        // An ordinary desktop's grab, by contrast, makes that manager's
        // grab fail: the case a restart repairs.
        drop(manager);
        native.set_power(true, false)?;
        assert_eq!(native.snapshot()?.power_reading, Reading::Grab);
        let late = x11rb::connect(Some(&desktop.display))?.0;
        assert!(
            late.grab_key(
                false,
                desktop.root,
                ModMask::ANY,
                power,
                GrabMode::ASYNC,
                GrabMode::ASYNC,
            )?
            .check()
            .is_err()
        );
        Ok(())
    }
    #[test]
    #[ignore = "requires Xvfb; spawns an isolated X server, never uses the real DISPLAY"]
    fn held_home_shortcut_keeps_home_page_across_regrab() -> Result<()> {
        // Another client holds the configured sequence: HomePage still opens
        // Home, at connection and after a mapping change regrabs (ADR-0094).
        let desktop = Desktop::new()?;
        let setup = desktop.c.setup();
        let mapping = desktop
            .c
            .get_keyboard_mapping(setup.min_keycode, setup.max_keycode - setup.min_keycode + 1)?
            .reply()?;
        let h = mapping
            .keysyms
            .chunks(usize::from(mapping.keysyms_per_keycode))
            .position(|syms| syms.contains(&u32::from(b'h')))
            .map(|i| setup.min_keycode + i as u8)
            .unwrap();
        let mods = desktop.c.get_modifier_mapping()?.reply()?;
        let per_mod = mods.keycodes.len() / 8;
        let alt_l = mapping
            .keysyms
            .chunks(usize::from(mapping.keysyms_per_keycode))
            .position(|syms| syms.contains(&0xffe9))
            .map(|i| setup.min_keycode + i as u8)
            .unwrap();
        let alt = mods
            .keycodes
            .chunks(per_mod)
            .position(|codes| codes.contains(&alt_l))
            .map(|modifier| 1u16 << modifier)
            .unwrap();
        desktop
            .c
            .grab_key(
                false,
                desktop.root,
                (u16::from(ModMask::CONTROL) | alt).into(),
                h,
                GrabMode::ASYNC,
                GrabMode::ASYNC,
            )?
            .check()?;
        desktop.c.flush()?;
        let home = crate::key_sequence::home("Ctrl+Alt+H").0;
        let mut native =
            Native::connect_display_with(Some(&desktop.display), home, Default::default())?;
        native.snapshot()?;
        desktop.press(&[], 0x1008_ff18)?;
        assert!(native.snapshot()?.home_key);
        // A mapping change ungrabs and grabs again; HomePage must survive it.
        let mapping = desktop.c.get_modifier_mapping()?.reply()?.keycodes;
        assert_eq!(
            desktop.c.set_modifier_mapping(&mapping)?.reply()?.status,
            x11rb::protocol::xproto::MappingStatus::SUCCESS
        );
        native.snapshot()?;
        desktop.press(&[], 0x1008_ff18)?;
        assert!(native.snapshot()?.home_key);
        Ok(())
    }
    #[test]
    #[ignore = "requires Xvfb; isolated modifier remapping regression"]
    fn home_follows_super_modifier_remapping() -> Result<()> {
        let desktop = Desktop::new()?;
        let mut native = Native::connect_display(Some(&desktop.display))?;
        let mut mapping = desktop.c.get_modifier_mapping()?.reply()?.keycodes;
        let count = mapping.len() / 8;
        for i in 0..count {
            mapping.swap(3 * count + i, 6 * count + i);
        }
        assert_eq!(
            desktop.c.set_modifier_mapping(&mapping)?.reply()?.status,
            x11rb::protocol::xproto::MappingStatus::SUCCESS
        );
        // MappingNotify rebuilds grabs without reconnecting the adapter.
        native.snapshot()?;
        desktop.home_key()?;
        assert!(native.snapshot()?.home_key);
        desktop.home_key_with(0xffe9)?; // Alt now occupies the old Mod4 slot.
        assert!(!native.snapshot()?.home_key);
        Ok(())
    }
    #[test]
    #[ignore = "requires Xvfb; spawns an isolated X server, never uses the real DISPLAY"]
    fn xfwm4_two_word_net_active_window() -> Result<()> {
        let desktop = Desktop::new()?;
        let shell = desktop.client("org.arinn.Shell", "")?;
        let video = desktop.client("org.example.Player.desktop", "")?;
        desktop.inventory(&[shell, video], video)?;
        let mut native = Native::connect_display(Some(&desktop.display))?;
        for words in [[video, 0], [video, 1_717_000_000], [0, video]] {
            desktop.set_net_active_window(&words)?;
            let snapshot = native.snapshot()?;
            assert_eq!(snapshot.active, Some(video));
            assert_eq!(snapshot.rows[0].flags & 1, 1);
        }
        Ok(())
    }
    #[test]
    #[ignore = "requires Xvfb; spawns an isolated X server, never uses the real DISPLAY"]
    fn xfwm4_two_zero_words_mean_no_active_window() -> Result<()> {
        // A fresh xfwm4 session with nothing focused publishes [0, 0] (#379);
        // the adapter must connect instead of retrying forever.
        let desktop = Desktop::new()?;
        let shell = desktop.client("org.arinn.Shell", "")?;
        let video = desktop.client("org.example.Player.desktop", "")?;
        desktop.inventory(&[shell, video], video)?;
        desktop.set_net_active_window(&[0, 0])?;
        let mut native = Native::connect_display(Some(&desktop.display))?;
        let snapshot = native.snapshot()?;
        assert_eq!(snapshot.active, None);
        assert_eq!(snapshot.rows[0].flags & 1, 0);
        desktop.set_net_active_window(&[0, video])?;
        assert_eq!(native.snapshot()?.active, Some(video));
        Ok(())
    }
    #[test]
    #[ignore = "requires Xvfb; spawns an isolated X server, never uses the real DISPLAY"]
    fn net_active_window_rejects_invalid_payloads() -> Result<()> {
        let desktop = Desktop::new()?;
        let shell = desktop.client("org.arinn.Shell", "")?;
        desktop.inventory(&[shell], shell)?;
        let mut native = Native::connect_display(Some(&desktop.display))?;
        desktop.set_net_active_window(&[0, shell, shell])?;
        let err = native.snapshot().unwrap_err().to_string();
        assert!(
            err.contains("_NET_ACTIVE_WINDOW") && err.contains("length 3"),
            "unexpected error: {err}"
        );
        Ok(())
    }
    #[test]
    #[ignore = "requires Xvfb; spawns an isolated X server, never uses the real DISPLAY"]
    fn isolated_inventory_control_and_manager_restart() -> Result<()> {
        let desktop = Desktop::new()?;
        let launch = Uuid::new_v4().to_string();
        let shell = desktop.client("org.arinn.Shell", "")?;
        let video = desktop.client("org.example.Player.desktop", &launch)?;
        desktop.inventory(&[shell, video], video)?;
        let mut native = Native::connect_display(Some(&desktop.display))?;
        let snapshot = native.snapshot()?;
        assert_eq!(snapshot.shell, Some(shell));
        assert_eq!(snapshot.rows.len(), 1);
        assert_eq!(snapshot.rows[0].desktop_id, "org.example.Player.desktop");
        assert_eq!(snapshot.rows[0].launch, launch);
        let gtk = desktop.client("org.gnome.Nautilus", "")?;
        desktop.inventory(&[shell, video, gtk], video)?;
        let metadata = native.snapshot()?;
        assert_eq!(
            metadata
                .rows
                .iter()
                .find(|r| r.xid == gtk)
                .unwrap()
                .desktop_id,
            "org.gnome.Nautilus.desktop"
        );
        desktop
            .c
            .delete_property(gtk, desktop.atoms._GTK_APPLICATION_ID)?
            .check()?;
        desktop
            .c
            .change_property8(
                PropMode::REPLACE,
                gtk,
                desktop.atoms._KDE_NET_WM_DESKTOP_FILE,
                desktop.atoms.UTF8_STRING,
                b"org.example.QtPlayer",
            )?
            .check()?;
        assert_eq!(
            native
                .snapshot()?
                .rows
                .iter()
                .find(|r| r.xid == gtk)
                .unwrap()
                .desktop_id,
            "org.example.QtPlayer.desktop"
        );
        desktop.inventory(&[shell, video], video)?;
        desktop.home_key()?;
        assert!(native.snapshot()?.home_key);
        let token = snapshot.rows[0].token.clone();
        assert!(!snapshot.compositor);
        assert!(native.present(video, shell, &launch).is_err());
        native.activate(video)?;
        native.close(video)?;
        let messages = desktop.messages()?;
        assert!(
            messages
                .iter()
                .any(|m| m.type_ == desktop.atoms._NET_ACTIVE_WINDOW
                    && m.window == video
                    && m.data.as_data32()[0] == 2)
        );
        assert!(
            messages
                .iter()
                .any(|m| m.type_ == desktop.atoms._NET_CLOSE_WINDOW && m.window == video)
        );
        desktop.compositing(true)?;
        assert!(native.snapshot()?.compositor);
        let saved = native.present(video, shell, &launch)?;
        assert!(saved.succeeded);
        native.release(video, &saved.saved, &launch)?;
        let messages = desktop.messages()?;
        assert!(messages.iter().any(|m| m.window == shell
            && m.type_ == desktop.atoms._NET_WM_DESKTOP
            && m.data.as_data32()[0] == u32::MAX));
        assert!(
            messages
                .iter()
                .any(|m| m.window == shell && m.type_ == desktop.atoms._NET_ACTIVE_WINDOW)
        );
        desktop.c.destroy_window(video)?.check()?;
        desktop.inventory(&[shell], shell)?;
        assert!(native.snapshot()?.rows.is_empty());
        let replacement = desktop.client("org.example.Player.desktop", &launch)?;
        desktop.inventory(&[shell, replacement], replacement)?;
        assert_ne!(native.snapshot()?.rows[0].token, token);
        desktop
            .c
            .delete_property(desktop.root, desktop.atoms._NET_SUPPORTING_WM_CHECK)?
            .check()?;
        assert!(native.snapshot().is_err());
        Ok(())
    }
    #[test]
    #[ignore = "requires Xvfb; spawns an isolated X server, never uses the real DISPLAY"]
    fn unmapped_root_child_shell_is_tracked_outside_client_list() -> Result<()> {
        let desktop = Desktop::new()?;
        let launch = Uuid::new_v4().to_string();
        let shell = desktop.client("org.arinn.Shell", "")?;
        let video = desktop.client("org.example.Player.desktop", &launch)?;
        desktop.inventory(&[video], video)?;
        let mut native = Native::connect_display(Some(&desktop.display))?;
        let snapshot = native.snapshot()?;
        assert_eq!(snapshot.shell, Some(shell));
        assert_eq!(snapshot.rows.len(), 1);
        assert_eq!(snapshot.rows[0].xid, video);
        assert!(snapshot.rows.iter().all(|row| row.xid != shell));
        // Unlisted: found for Home, but without a lifetime token.
        assert!(snapshot.shell_token.is_empty());
        Ok(())
    }
    #[test]
    #[ignore = "requires Xvfb; spawns an isolated X server, never uses the real DISPLAY"]
    fn missing_shell_leaves_home_target_unavailable() -> Result<()> {
        let desktop = Desktop::new()?;
        let launch = Uuid::new_v4().to_string();
        let video = desktop.client("org.example.Player.desktop", &launch)?;
        desktop.inventory(&[video], video)?;
        let mut native = Native::connect_display(Some(&desktop.display))?;
        let snapshot = native.snapshot()?;
        assert_eq!(snapshot.shell, None);
        assert_eq!(snapshot.rows.len(), 1);
        Ok(())
    }
    #[test]
    #[ignore = "requires Xvfb; spawns an isolated X server, never uses the real DISPLAY"]
    fn duplicate_shell_identity_fails_closed() -> Result<()> {
        let desktop = Desktop::new()?;
        let launch = Uuid::new_v4().to_string();
        let _shell_a = desktop.client("org.arinn.Shell", "")?;
        let _shell_b = desktop.client("org.arinn.Shell.desktop", "")?;
        let video = desktop.client("org.example.Player.desktop", &launch)?;
        desktop.inventory(&[video], video)?;
        let mut native = Native::connect_display(Some(&desktop.display))?;
        let err = native.snapshot().unwrap_err();
        assert!(
            err.to_string().contains("ambiguous shell identity"),
            "{err}"
        );
        Ok(())
    }
    #[test]
    #[ignore = "requires Xvfb; spawns an isolated X server, never uses the real DISPLAY"]
    fn root_children_over_budget_fail_closed() -> Result<()> {
        let desktop = Desktop::new()?;
        let mut native = Native::connect_display(Some(&desktop.display))?;
        desktop.inventory(&[], 0)?;
        let existing = desktop.c.query_tree(desktop.root)?.reply()?.children.len();
        for _ in existing..=4096 {
            let xid = desktop.c.generate_id()?;
            desktop.c.create_window(
                0,
                xid,
                desktop.root,
                0,
                0,
                1,
                1,
                0,
                WindowClass::INPUT_OUTPUT,
                0,
                &CreateWindowAux::new(),
            )?;
        }
        desktop.c.flush()?;
        let err = native.snapshot().unwrap_err();
        assert!(
            err.to_string()
                .contains("root window children exceed budget"),
            "{err}"
        );
        Ok(())
    }
    #[test]
    #[ignore = "requires Xvfb; spawns an isolated X server, never uses the real DISPLAY"]
    fn destroyed_hidden_shell_is_not_found_again() -> Result<()> {
        let desktop = Desktop::new()?;
        let shell = desktop.client("org.arinn.Shell", "")?;
        desktop.inventory(&[], 0)?;
        let mut native = Native::connect_display(Some(&desktop.display))?;
        assert_eq!(native.snapshot()?.shell, Some(shell));
        desktop.c.destroy_window(shell)?.check()?;
        let snapshot = native.snapshot()?;
        assert_eq!(snapshot.shell, None);
        Ok(())
    }
}
