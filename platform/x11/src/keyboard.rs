// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The X11/EWMH deliverer of the global on-screen keyboard (ADR-0105).
//!
//! A keyboard worker with its own X connection:
//!
//! - grabs the keyboard keys (`keyboard_shortcut`, then XF86Launch1, the
//!   remapped microphone) on the root window, like Home;
//! - on open, binds the target to `_NET_ACTIVE_WINDOW` (a listed normal
//!   client that is not the shell) and passively grabs the panel keys, so
//!   focus stays with the application and every other key still reaches it;
//! - types text with XTest through scratch keycodes mapped to each
//!   character's keysym, so neither the layout nor Shift matters;
//! - ends the session when the target loses focus or disappears.
//!
//! The lock state is checked by the caller (`main.rs`) before a key request
//! and before every delivery. Nothing typed is ever logged.
use crate::{
    key_sequence::Sequence,
    native::{self, Atoms, KeyMap},
};
use anyhow::{Result, ensure};
use std::collections::HashSet;
use x11rb::{
    CURRENT_TIME,
    connection::{Connection as _, RequestConnection as _},
    protocol::{Event, xproto::*, xtest::ConnectionExt as _},
    rust_connection::RustConnection,
};

/// Panel keys: keysym and the input name the shell receives.
const PANEL_KEYS: [(u32, &str); 8] = [
    (0xff52, "up"),
    (0xff54, "down"),
    (0xff51, "left"),
    (0xff53, "right"),
    (0xff0d, "select"),    // Return
    (0xff8d, "select"),    // KP_Enter
    (0xff1b, "back"),      // Escape
    (0x1008_ff26, "back"), // XF86Back
];

/// What the worker saw.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Seen {
    /// The keyboard key was pressed.
    Key,
    /// A panel key of the open session.
    Input(&'static str),
}

struct Open {
    target: Window,
    grabs: Vec<(u16, Keycode)>,
    /// Scratch keycodes and the keysym each one carries now.
    scratch: Vec<(Keycode, u32)>,
    next: usize,
}

pub struct Keyboard {
    c: RustConnection,
    root: Window,
    atoms: Atoms,
    keys: Vec<Sequence>,
    /// The keyboard key's grab combinations, to tell them from panel keys.
    key_grabs: HashSet<(u16, Keycode)>,
    open: Option<Open>,
}

/// The keysym of a character: Latin-1 directly, everything else as a
/// Unicode keysym (`0x01000000 + code point`).
pub fn keysym(ch: char) -> u32 {
    let cp = u32::from(ch);
    if (0x20..=0x7e).contains(&cp) || (0xa0..=0xff).contains(&cp) {
        cp
    } else {
        0x0100_0000 + cp
    }
}

/// The keysym of an editing key the shell asks for.
pub fn press_keysym(key: &str) -> Option<u32> {
    Some(match key {
        "backspace" => 0xff08,
        "return" => 0xff0d,
        "left" => 0xff51,
        "right" => 0xff53,
        _ => return None,
    })
}

impl Keyboard {
    pub fn connect(display: Option<&str>, keys: Vec<Sequence>) -> Result<Self> {
        let (c, screen) = x11rb::connect(display)?;
        let root = c.setup().roots[screen].root;
        let atoms = Atoms::new(&c)?.reply()?;
        c.prefetch_extension_information(x11rb::protocol::xtest::X11_EXTENSION_NAME)?;
        ensure!(
            c.extension_information(x11rb::protocol::xtest::X11_EXTENSION_NAME)?
                .is_some(),
            "XTest required"
        );
        let mut keyboard = Self {
            c,
            root,
            atoms,
            keys,
            key_grabs: HashSet::new(),
            open: None,
        };
        keyboard.grab_keys()?;
        Ok(keyboard)
    }

    fn grab_keys(&mut self) -> Result<()> {
        for (modifiers, key) in self.key_grabs.drain() {
            self.c
                .ungrab_key(key, self.root, modifiers.into())?
                .check()?;
        }
        let map = KeyMap::read(&self.c)?;
        for sequence in &self.keys {
            let combinations = map.combinations(sequence);
            if combinations.is_empty() {
                continue;
            }
            if native::grab_combinations(&self.c, self.root, &combinations)? {
                self.key_grabs.extend(combinations);
            } else {
                eprintln!("arinn-platform-x11: a keyboard key is held by another client; skipped");
            }
        }
        if self.key_grabs.is_empty() {
            eprintln!("arinn-platform-x11: no keyboard key could be grabbed");
        }
        self.c.flush()?;
        Ok(())
    }

    /// Drains X events: keyboard key presses and the open session's panel
    /// keys. A mapping change grabs again.
    pub fn poll(&mut self) -> Result<Vec<Seen>> {
        let mut seen = Vec::new();
        while let Some(event) = self.c.poll_for_event()? {
            match event {
                Event::KeyPress(e) => {
                    let state = u16::from(e.state) & 0xff;
                    if self.key_grabs.contains(&(state, e.detail)) {
                        seen.push(Seen::Key);
                    } else if let Some(open) = &self.open
                        && open.grabs.contains(&(state, e.detail))
                        && let Some(name) = self.panel_key(e.detail)
                    {
                        seen.push(Seen::Input(name));
                    }
                }
                // Our own scratch mappings also notify while a session is
                // open; a real change grabs again once it is closed.
                Event::MappingNotify(e) if e.request != Mapping::POINTER && self.open.is_none() => {
                    self.grab_keys()?;
                }
                _ => (),
            }
        }
        Ok(seen)
    }

    fn panel_key(&self, code: Keycode) -> Option<&'static str> {
        let map = KeyMap::read(&self.c).ok()?;
        PANEL_KEYS
            .iter()
            .find(|(sym, _)| map.codes(*sym).contains(&code))
            .map(|(_, name)| *name)
    }

    /// The active window, if it is a listed normal client other than the
    /// shell.
    fn application(&self) -> Result<Option<Window>> {
        let Some(active) =
            native::active_window(&self.c, self.root, self.atoms._NET_ACTIVE_WINDOW)?
        else {
            return Ok(None);
        };
        let clients = self
            .c
            .get_property(
                false,
                self.root,
                self.atoms._NET_CLIENT_LIST,
                AtomEnum::WINDOW,
                0,
                4096,
            )?
            .reply()?;
        if !clients
            .value32()
            .is_some_and(|mut ids| ids.any(|id| id == active))
        {
            return Ok(None);
        }
        if native::is_shell(&native::identity(&self.c, &self.atoms, active)) {
            return Ok(None);
        }
        let types = self
            .c
            .get_property(
                false,
                active,
                self.atoms._NET_WM_WINDOW_TYPE,
                AtomEnum::ATOM,
                0,
                32,
            )?
            .reply()?;
        let types: Vec<u32> = types.value32().map(Iterator::collect).unwrap_or_default();
        if !types.is_empty() && !types.contains(&self.atoms._NET_WM_WINDOW_TYPE_NORMAL) {
            return Ok(None);
        }
        Ok(Some(active))
    }

    /// Binds the target and grabs the panel keys. `Err` is a session the
    /// worker cannot serve (`unavailable`).
    pub fn open(&mut self) -> Result<()> {
        self.close()?;
        let Some(target) = self.application()? else {
            anyhow::bail!("no application window has focus");
        };
        let map = KeyMap::read(&self.c)?;
        let mut grabs = Vec::new();
        let mut arrows = false;
        let mut back = false;
        for (sym, name) in PANEL_KEYS {
            let combinations: Vec<(u16, Keycode)> = map
                .codes(sym)
                .into_iter()
                .flat_map(|code| {
                    (0u16..256)
                        .filter(|subset| subset & !map.locks == 0)
                        .map(move |subset| (subset, code))
                })
                .collect();
            if combinations.is_empty() {
                continue;
            }
            if native::grab_combinations(&self.c, self.root, &combinations)? {
                grabs.extend(combinations);
                arrows |= ["up", "down", "left", "right"].contains(&name);
                back |= name == "back";
            }
        }
        let scratch: Vec<(Keycode, u32)> = map
            .free()
            .into_iter()
            .rev()
            .take(8)
            .map(|c| (c, 0))
            .collect();
        let open = Open {
            target,
            grabs,
            scratch,
            next: 0,
        };
        let usable = arrows && back && !open.scratch.is_empty();
        self.open = Some(open);
        if !usable {
            self.close()?;
            anyhow::bail!("panel keys or scratch keycodes unavailable");
        }
        self.c.flush()?;
        Ok(())
    }

    /// Releases the panel grabs and the scratch mappings.
    pub fn close(&mut self) -> Result<()> {
        let Some(open) = self.open.take() else {
            return Ok(());
        };
        for (modifiers, key) in &open.grabs {
            self.c.ungrab_key(*key, self.root, (*modifiers).into())?;
        }
        let per_key = KeyMap::read(&self.c)?.per_key();
        for (code, sym) in &open.scratch {
            if *sym != 0 {
                self.c
                    .change_keyboard_mapping(1, *code, per_key as u8, &vec![0; per_key])?;
            }
        }
        self.c.flush()?;
        Ok(())
    }

    /// Whether the open session's target still has focus. False ends it.
    pub fn target_focused(&self) -> Result<bool> {
        let Some(open) = &self.open else {
            return Ok(false);
        };
        Ok(
            native::active_window(&self.c, self.root, self.atoms._NET_ACTIVE_WINDOW)?
                == Some(open.target)
                && self.c.get_window_attributes(open.target)?.reply().is_ok(),
        )
    }

    /// Types `text` into the target. Fails without typing anything when the
    /// target no longer has focus.
    pub fn type_text(&mut self, text: &str) -> Result<()> {
        ensure!(self.target_focused()?, "target lost focus");
        for ch in text.chars() {
            self.type_keysym(keysym(ch))?;
        }
        Ok(())
    }

    /// Sends one editing key to the target.
    pub fn press(&mut self, key: &str) -> Result<()> {
        ensure!(self.target_focused()?, "target lost focus");
        let sym = press_keysym(key).ok_or_else(|| anyhow::anyhow!("unknown key"))?;
        self.type_keysym(sym)
    }

    /// One press and release of `sym` through a scratch keycode. A keycode
    /// that already carries `sym` is reused; otherwise the next one in
    /// rotation is remapped, so a client still reading the previous mapping
    /// never sees its character change under it.
    fn type_keysym(&mut self, sym: u32) -> Result<()> {
        let per_key = KeyMap::read(&self.c)?.per_key();
        let open = self
            .open
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("no session"))?;
        let code = match open.scratch.iter().find(|(_, s)| *s == sym) {
            Some((code, _)) => *code,
            None => {
                let slot = open.next % open.scratch.len();
                open.next = open.next.wrapping_add(1);
                let code = open.scratch[slot].0;
                self.c
                    .change_keyboard_mapping(1, code, per_key as u8, &vec![sym; per_key])?
                    .check()?;
                open.scratch[slot].1 = sym;
                code
            }
        };
        // KEY_PRESS = 2, KEY_RELEASE = 3.
        self.c
            .xtest_fake_input(2, code, CURRENT_TIME, self.root, 0, 0, 0)?;
        self.c
            .xtest_fake_input(3, code, CURRENT_TIME, self.root, 0, 0, 0)?;
        self.c.get_input_focus()?.reply()?;
        Ok(())
    }
}

impl Drop for Keyboard {
    fn drop(&mut self) {
        let _ = self.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native::fixture::Desktop;
    use x11rb::wrapper::ConnectionExt as _;

    #[test]
    fn characters_map_to_their_keysyms() {
        assert_eq!(keysym('a'), 0x61);
        assert_eq!(keysym('/'), 0x2f);
        assert_eq!(keysym('é'), 0xe9);
        assert_eq!(keysym('Я'), 0x0100_042f);
        assert_eq!(keysym('😀'), 0x0101_f600);
        assert_eq!(press_keysym("backspace"), Some(0xff08));
        assert_eq!(press_keysym("tab"), None);
    }

    /// A focused test window that records what it receives, resolving each
    /// key through the mapping current when it arrives, as a client does.
    struct Target {
        c: RustConnection,
        xid: Window,
    }
    impl Target {
        fn new(desktop: &Desktop, identity: &str) -> Result<Self> {
            let (c, screen) = x11rb::connect(Some(&desktop.display))?;
            let root = c.setup().roots[screen].root;
            let xid = c.generate_id()?;
            c.create_window(
                0,
                xid,
                root,
                0,
                0,
                100,
                100,
                0,
                WindowClass::INPUT_OUTPUT,
                0,
                &CreateWindowAux::new().event_mask(EventMask::KEY_PRESS),
            )?
            .check()?;
            c.change_property8(
                PropMode::REPLACE,
                xid,
                desktop.atoms._GTK_APPLICATION_ID,
                desktop.atoms.UTF8_STRING,
                identity.as_bytes(),
            )?
            .check()?;
            c.flush()?;
            // The fake window manager redirects maps; it maps the window.
            desktop.c.map_window(xid)?.check()?;
            c.set_input_focus(InputFocus::POINTER_ROOT, xid, CURRENT_TIME)?
                .check()?;
            c.flush()?;
            Ok(Self { c, xid })
        }
        fn keysyms(&self) -> Result<Vec<u32>> {
            self.c.get_input_focus()?.reply()?;
            let mut out = Vec::new();
            while let Some(event) = self.c.poll_for_event()? {
                if let Event::KeyPress(e) = event {
                    let mapping = self.c.get_keyboard_mapping(e.detail, 1)?.reply()?;
                    let sym = mapping.keysyms.first().copied().unwrap_or(0);
                    // Modifier keys of a held combination are not text.
                    if !(0xffe1..=0xffee).contains(&sym) {
                        out.push(sym);
                    }
                }
            }
            Ok(out)
        }
    }

    fn settle() {
        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    #[test]
    #[ignore = "requires Xvfb; spawns an isolated X server, never uses the real DISPLAY"]
    fn the_keyboard_key_opens_and_xtest_types_into_the_focused_window() -> Result<()> {
        let desktop = Desktop::new()?;
        let app = Target::new(&desktop, "org.example.Browser")?;
        desktop.inventory(&[app.xid], app.xid)?;
        let home = crate::key_sequence::home("Meta+F12").0;
        let keys = crate::key_sequence::keyboard("Ctrl+Alt+K", &home).0;
        let mut keyboard = Keyboard::connect(Some(&desktop.display), keys)?;
        desktop.press(&[0xffe3, 0xffe9], u32::from(b'k'))?; // Control_L, Alt_L
        settle();
        assert_eq!(keyboard.poll()?, [Seen::Key]);
        // The application does not see the keyboard key.
        assert_eq!(app.keysyms()?, Vec::<u32>::new());
        keyboard.open()?;
        // ASCII and Cyrillic with an English keymap, and an editing key.
        keyboard.type_text("aZ/Яё")?;
        keyboard.press("backspace")?;
        settle();
        assert_eq!(
            app.keysyms()?,
            [0x61, 0x5a, 0x2f, 0x0100_042f, 0x0100_0451, 0xff08]
        );
        // Panel keys go to the worker, not to the application; other keys,
        // a hardware Backspace included, still reach it.
        desktop.press(&[], 0xff52)?; // Up
        desktop.press(&[], 0xff0d)?; // Return
        desktop.press(&[], 0xff1b)?; // Escape
        desktop.press(&[], 0xff08)?; // BackSpace
        desktop.press(&[], u32::from(b'q'))?;
        settle();
        assert_eq!(
            keyboard.poll()?,
            [
                Seen::Input("up"),
                Seen::Input("select"),
                Seen::Input("back")
            ]
        );
        assert_eq!(app.keysyms()?, [0xff08, u32::from(b'q')]);
        // The keyboard key still requests while open (the daemon toggles).
        desktop.press(&[0xffe3, 0xffe9], u32::from(b'k'))?;
        settle();
        assert_eq!(keyboard.poll()?, [Seen::Key]);
        keyboard.close()?;
        // After close the panel keys belong to the application again, and
        // no scratch keycode keeps a mapping.
        desktop.press(&[], 0xff52)?;
        settle();
        assert_eq!(keyboard.poll()?, Vec::<Seen>::new());
        assert_eq!(app.keysyms()?, [0xff52]);
        let map = KeyMap::read(&keyboard.c)?;
        assert!(map.codes(0x0100_042f).is_empty());
        Ok(())
    }

    #[test]
    #[ignore = "requires Xvfb; spawns an isolated X server, never uses the real DISPLAY"]
    fn focus_loss_stops_delivery_and_the_shell_is_never_a_target() -> Result<()> {
        let desktop = Desktop::new()?;
        let app = Target::new(&desktop, "org.example.Browser")?;
        let other = Target::new(&desktop, "org.example.Editor")?;
        let shell = Target::new(&desktop, "org.arinn.Shell")?;
        let keys = vec![crate::key_sequence::MICROPHONE];
        let mut keyboard = Keyboard::connect(Some(&desktop.display), keys)?;
        // The shell is never a target; nor is an unlisted window.
        desktop.inventory(&[app.xid, other.xid, shell.xid], shell.xid)?;
        assert!(keyboard.open().is_err());
        desktop.inventory(&[app.xid], other.xid)?;
        assert!(keyboard.open().is_err());
        desktop.inventory(&[app.xid, other.xid, shell.xid], app.xid)?;
        keyboard.open()?;
        assert!(keyboard.target_focused()?);
        // Another window becomes active: nothing more is delivered.
        desktop.inventory(&[app.xid, other.xid, shell.xid], other.xid)?;
        assert!(!keyboard.target_focused()?);
        assert!(keyboard.type_text("x").is_err());
        assert!(keyboard.press("return").is_err());
        settle();
        assert_eq!(app.keysyms()?, Vec::<u32>::new());
        assert_eq!(other.keysyms()?, Vec::<u32>::new());
        // A destroyed target ends the session too.
        desktop.inventory(&[app.xid, other.xid, shell.xid], app.xid)?;
        assert!(keyboard.target_focused()?);
        app.c.destroy_window(app.xid)?.check()?;
        assert!(!keyboard.target_focused()?);
        keyboard.close()?;
        let _ = desktop.c.flush();
        Ok(())
    }
}
