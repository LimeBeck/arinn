// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use anyhow::{Result, bail, ensure};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;
use x11rb::{
    CURRENT_TIME,
    connection::Connection,
    protocol::{Event, xproto::*},
    rust_connection::RustConnection,
};

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
    pub home_key: bool,
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
}
impl Native {
    pub fn connect() -> Result<Self> {
        ensure!(
            std::env::var("XDG_SESSION_TYPE").as_deref() == Ok("x11"),
            "X11 session required"
        );
        Self::connect_display(None)
    }
    pub(crate) fn connect_display(display: Option<&str>) -> Result<Self> {
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
    fn bytes(&self, window: Window, property: Atom, ty: Atom) -> Result<Vec<u8>> {
        let r = self
            .c
            .get_property(false, window, property, ty, 0, 128)?
            .reply()?;
        if r.type_ == 0 {
            return Ok(Vec::new());
        }
        ensure!(
            r.type_ == ty && r.format == 8 && r.bytes_after == 0,
            "invalid metadata"
        );
        Ok(r.value)
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
    fn grab_home(&self) -> Result<()> {
        self.c.ungrab_key(0, self.root, ModMask::ANY)?.check()?;
        let setup = self.c.setup();
        let count = setup.max_keycode - setup.min_keycode + 1;
        let mapping = self
            .c
            .get_keyboard_mapping(setup.min_keycode, count)?
            .reply()?;
        let per_key = usize::from(mapping.keysyms_per_keycode);
        ensure!(per_key > 0, "invalid keyboard mapping");
        let mut f12 = Vec::new();
        let mut locks = u16::from(ModMask::LOCK);
        let mut supers = HashSet::new();
        let mut metas = HashSet::new();
        let mods = self.c.get_modifier_mapping()?.reply()?;
        let per_mod = mods.keycodes.len() / 8;
        for (index, syms) in mapping.keysyms.chunks(per_key).enumerate() {
            let key = setup.min_keycode + index as u8;
            if syms.contains(&0xffc9) {
                f12.push(key);
            }
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
            }
        }
        // Qt's Meta transport normally uses Super; fall back to native Meta
        // only when no mapped Super key exists. Do not treat Alt aliases as Meta.
        let meta_masks = if supers.is_empty() { metas } else { supers };
        ensure!(!meta_masks.is_empty(), "Meta modifier unavailable");
        ensure!(!f12.is_empty(), "F12 key unavailable");
        for meta in meta_masks {
            for key in &f12 {
                for subset in 0u16..256 {
                    if subset & !locks == 0 {
                        self.c
                            .grab_key(
                                false,
                                self.root,
                                (meta | subset).into(),
                                *key,
                                GrabMode::ASYNC,
                                GrabMode::ASYNC,
                            )?
                            .check()?;
                    }
                }
            }
        }
        self.c.flush()?;
        Ok(())
    }
    pub fn snapshot(&mut self) -> Result<Snapshot> {
        // Establish an X-server barrier before draining lifecycle events.
        self.c.get_input_focus()?.reply()?;
        let mut home_key = false;
        while let Some(event) = self.c.poll_for_event()? {
            match event {
                Event::DestroyNotify(e) => {
                    self.tokens.forget(e.window);
                    self.watched.remove(&e.window);
                }
                Event::KeyPress(_) => home_key = true,
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
        let live = clients.iter().copied().collect();
        self.tokens.retain(&live);
        self.watched.retain(|id| live.contains(id));
        let active = self
            .words(
                self.root,
                self.atoms._NET_ACTIVE_WINDOW,
                AtomEnum::WINDOW.into(),
                1,
            )?
            .first()
            .copied()
            .filter(|id| *id != 0);
        let mut shell = None;
        let mut shell_token = String::new();
        let mut rows = Vec::new();
        for xid in clients {
            let Ok(attr) = self.c.get_window_attributes(xid)?.reply() else {
                continue;
            };
            if attr.override_redirect || attr.class != WindowClass::INPUT_OUTPUT {
                continue;
            }
            if !self.watched.contains(&xid) {
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
                    continue;
                }
                self.watched.insert(xid);
            }
            let identity = [
                self.atoms._NET_WM_DESKTOP_FILE,
                self.atoms._KDE_NET_WM_DESKTOP_FILE,
                self.atoms._GTK_APPLICATION_ID,
            ]
            .into_iter()
            .map(|atom| {
                desktop_id(
                    &self
                        .bytes(xid, atom, self.atoms.UTF8_STRING)
                        .unwrap_or_default(),
                    atom != self.atoms._NET_WM_DESKTOP_FILE,
                )
            })
            .find(|id| !id.is_empty())
            .unwrap_or_default();
            if ["org.arinn.Shell", "org.arinn.Shell.desktop"].contains(&identity.as_str()) {
                ensure!(shell.is_none(), "ambiguous shell identity");
                shell = Some(xid);
                shell_token = self.tokens.get(xid);
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
        Ok(Snapshot {
            rows,
            shell,
            shell_token,
            active,
            home_key,
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
            self.c
                .change_property32(
                    PropMode::REPLACE,
                    self.root,
                    self.atoms._NET_ACTIVE_WINDOW,
                    AtomEnum::WINDOW,
                    &[active],
                )?
                .check()?;
            self.c.flush()?;
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
            let meta = key(modifier_symbol);
            let f12 = key(0xffc9);
            for (kind, code) in [(2, meta), (2, f12), (3, f12), (3, meta)] {
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
}
