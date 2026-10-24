// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Global Home and quick-menu key sequences shared by the platform adapters
//! (#286, #439).
//!
//! `config.toml` `home_shortcut` and `quick_menu_shortcut` use Qt's portable
//! text, for example `Meta+F12`. Every adapter also binds the remote's
//! HomePage key to Home. The
//! parser accepts a small, explicit set of keys so that a global shortcut
//! never takes Esc, Backspace or plain letters from other applications
//! (specification §6.2).

/// The documented default of `home_shortcut`.
pub const DEFAULT_HOME: &str = "Meta+F12";
/// The documented default of `quick_menu_shortcut` (ADR-0106, spec §6.2).
pub const DEFAULT_QUICK_MENU: &str = "Meta+F11";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Modifiers {
    pub meta: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    /// F1–F35.
    Function(u8),
    /// A–Z, upper case.
    Letter(u8),
    /// 0–9, as an ASCII digit.
    Digit(u8),
    HomePage,
    Favorites,
    Search,
    Menu,
    /// The keyboard Menu key of evdev `KEY_MENU` (Qt `Key_MenuKB`, X11
    /// `XF86MenuKB`); `KEY_COMPOSE` is Qt `Key_Menu`.
    MenuKb,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sequence {
    pub modifiers: Modifiers,
    pub key: Key,
}

const HOME_PAGE: Sequence = Sequence {
    modifiers: Modifiers {
        meta: false,
        ctrl: false,
        alt: false,
        shift: false,
    },
    key: Key::HomePage,
};

/// Parses one key sequence such as `Meta+F12`, `Ctrl+Alt+H` or `HomePage`.
/// Names are case-insensitive; `Super` is accepted for `Meta`.
pub fn parse(text: &str) -> Option<Sequence> {
    if text.len() > 64 {
        return None;
    }
    let mut parts: Vec<&str> = text.split('+').map(str::trim).collect();
    let name = parts.pop()?;
    let mut modifiers = Modifiers::default();
    for part in parts {
        let flag = match part.to_ascii_lowercase().as_str() {
            "meta" | "super" => &mut modifiers.meta,
            "ctrl" | "control" => &mut modifiers.ctrl,
            "alt" => &mut modifiers.alt,
            "shift" => &mut modifiers.shift,
            _ => return None,
        };
        if *flag {
            return None;
        }
        *flag = true;
    }
    let lower = name.to_ascii_lowercase();
    let key = match lower.as_str() {
        "homepage" => Key::HomePage,
        "favorites" => Key::Favorites,
        "search" => Key::Search,
        "menu" => Key::Menu,
        "menukb" => Key::MenuKb,
        _ if lower.len() == 1 && lower.as_bytes()[0].is_ascii_lowercase() => {
            Key::Letter(lower.as_bytes()[0].to_ascii_uppercase())
        }
        _ if lower.len() == 1 && lower.as_bytes()[0].is_ascii_digit() => {
            Key::Digit(lower.as_bytes()[0])
        }
        _ => {
            let number: u8 = lower.strip_prefix('f')?.parse().ok()?;
            if !(1..=35).contains(&number) || lower[1..].starts_with('0') {
                return None;
            }
            Key::Function(number)
        }
    };
    // A letter or digit without Meta, Ctrl or Alt types text in the focused
    // application; Arinn never takes such keys globally.
    if matches!(key, Key::Letter(_) | Key::Digit(_))
        && !(modifiers.meta || modifiers.ctrl || modifiers.alt)
    {
        return None;
    }
    Some(Sequence { modifiers, key })
}

/// The Home sequences an adapter binds: the configured one, or the default
/// when the value is empty or invalid, followed by HomePage. The flag reports
/// that the configured value was replaced by the default.
pub fn home(configured: &str) -> (Vec<Sequence>, bool) {
    let (primary, fallback) = match parse(configured) {
        Some(sequence) => (sequence, false),
        None => (parse(DEFAULT_HOME).expect("valid default"), true),
    };
    let mut sequences = vec![primary];
    if primary != HOME_PAGE {
        sequences.push(HOME_PAGE);
    }
    (sequences, fallback)
}

/// The remote's Menu key (owner decision of 2026-10-21, #439). Which code
/// the G60S PRO BT sends is not captured yet, so both are bound.
const REMOTE_MENU: [Sequence; 2] = [
    Sequence {
        modifiers: HOME_PAGE.modifiers,
        key: Key::Menu,
    },
    Sequence {
        modifiers: HOME_PAGE.modifiers,
        key: Key::MenuKb,
    },
];

/// The quick-menu keys an adapter binds (ADR-0106).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct QuickMenu {
    /// The configured key (or the default) first, then the remote's Menu
    /// and MenuKB keys; none of them is a Home key.
    pub keys: Vec<Sequence>,
    /// The configured key, when it is kept.
    configured: Option<Sequence>,
}
impl QuickMenu {
    /// The keys to bind for the current focus. Inside the shell Menu keeps
    /// its own meaning, so the remote's Menu keys are left out while the
    /// shell has focus. The configured key stays bound everywhere, also
    /// when it is Menu or MenuKB (ADR-0106).
    pub fn bound(&self, shell_focused: bool) -> Vec<Sequence> {
        self.keys
            .iter()
            .copied()
            .filter(|key| !shell_focused || !key.is_remote_menu() || Some(*key) == self.configured)
            .collect()
    }
}

/// The quick-menu keys: the configured sequence, or the default when the
/// value is empty or invalid, followed by the remote's Menu and MenuKB keys.
/// A sequence that is also one of `home` is left out, so Home wins. The flag
/// reports that the configured value was replaced by the default.
pub fn quick_menu(configured: &str, home: &[Sequence]) -> (QuickMenu, bool) {
    let (sequence, fallback) = match parse(configured) {
        Some(sequence) => (sequence, false),
        None => (parse(DEFAULT_QUICK_MENU).expect("valid default"), true),
    };
    let mut keys = Vec::new();
    for candidate in std::iter::once(sequence).chain(REMOTE_MENU) {
        if !home.contains(&candidate) && !keys.contains(&candidate) {
            keys.push(candidate);
        }
    }
    let configured = (!home.contains(&sequence)).then_some(sequence);
    (QuickMenu { keys, configured }, fallback)
}

impl Sequence {
    /// The remote's Menu key, plain Menu or MenuKB. Inside the shell Menu
    /// keeps its own meaning, so adapters bind it only while another window
    /// has focus.
    pub fn is_remote_menu(self) -> bool {
        REMOTE_MENU.contains(&self)
    }
    /// Qt's combined key code (`QKeyCombination::toCombined()`), the form
    /// KGlobalAccel stores.
    pub fn qt(self) -> i32 {
        let key: u32 = match self.key {
            Key::Function(n) => 0x0100_0030 + u32::from(n) - 1,
            Key::Letter(c) | Key::Digit(c) => u32::from(c),
            Key::HomePage => 0x0100_0090,
            Key::Favorites => 0x0100_0091,
            Key::Search => 0x0100_0092,
            Key::Menu => 0x0100_0055,
            Key::MenuKb => 0x0100_00dc,
        };
        let m = self.modifiers;
        let mut modifiers = 0;
        for (held, bit) in [
            (m.shift, 0x0200_0000),
            (m.ctrl, 0x0400_0000),
            (m.alt, 0x0800_0000),
            (m.meta, 0x1000_0000),
        ] {
            if held {
                modifiers |= bit;
            }
        }
        (key | modifiers) as i32
    }
    /// The X11 keysym of the key, without modifiers.
    pub fn keysym(self) -> u32 {
        match self.key {
            Key::Function(n) => 0xffbe + u32::from(n) - 1,
            Key::Letter(c) => u32::from(c.to_ascii_lowercase()),
            Key::Digit(c) => u32::from(c),
            Key::HomePage => 0x1008_ff18,
            Key::Favorites => 0x1008_ff30,
            Key::Search => 0x1008_ff1b,
            Key::Menu => 0xff67,
            Key::MenuKb => 0x1008_ff65,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_portable_text() {
        let meta_f12 = parse("Meta+F12").unwrap();
        assert!(meta_f12.modifiers.meta);
        assert_eq!(meta_f12.key, Key::Function(12));
        assert_eq!(meta_f12.qt(), 0x1100_003b);
        assert_eq!(meta_f12.keysym(), 0xffc9);
        assert_eq!(parse(" super + f12 "), Some(meta_f12));
        let h = parse("Ctrl+Alt+H").unwrap();
        assert_eq!(h.qt(), 0x0C00_0048);
        assert_eq!(h.keysym(), u32::from(b'h'));
        assert_eq!(parse("HomePage").unwrap().qt(), 0x0100_0090);
        assert_eq!(parse("homepage").unwrap().keysym(), 0x1008_ff18);
        assert_eq!(parse("F35").unwrap().keysym(), 0xffe0);
        assert_eq!(parse("Meta+1").unwrap().qt(), 0x1000_0031);
    }

    #[test]
    fn rejects_text_keys_and_unknown_names() {
        for text in [
            "",
            "H",
            "Shift+H",
            "5",
            "Escape",
            "Esc",
            "Backspace",
            "Meta+Esc",
            "Meta+",
            "+F12",
            "Hyper+F12",
            "Meta+Meta+F12",
            "F0",
            "F36",
            "F012",
            "Meta+F12+F11",
            "Meta+Space",
        ] {
            assert_eq!(parse(text), None, "{text}");
        }
    }

    #[test]
    fn home_adds_home_page_and_falls_back() {
        let (keys, fallback) = home("Ctrl+Alt+H");
        assert!(!fallback);
        assert_eq!(
            keys.iter().map(|k| k.qt()).collect::<Vec<_>>(),
            [0x0C00_0048, 0x0100_0090]
        );
        let (keys, fallback) = home("not a key");
        assert!(fallback);
        assert_eq!(keys, home(DEFAULT_HOME).0);
        assert_eq!(keys.len(), 2);
        let (keys, fallback) = home("HomePage");
        assert!(!fallback);
        assert_eq!(keys, [HOME_PAGE]);
        assert!(home("").1);
    }

    #[test]
    fn quick_menu_falls_back_and_yields_to_home() {
        let qt = |keys: &[Sequence]| keys.iter().map(|k| k.qt()).collect::<Vec<_>>();
        let home_keys = home(DEFAULT_HOME).0;
        let (menu, fallback) = quick_menu("Ctrl+Alt+Q", &home_keys);
        let keys = menu.keys;
        assert!(!fallback);
        // The configured key, then the remote's Menu as Qt Menu and MenuKB.
        assert_eq!(qt(&keys), [0x0C00_0051, 0x0100_0055, 0x0100_00dc]);
        assert_eq!(
            keys.iter().map(|k| k.is_remote_menu()).collect::<Vec<_>>(),
            [false, true, true]
        );
        assert_eq!(keys[1].keysym(), 0xff67);
        assert_eq!(keys[2].keysym(), 0x1008_ff65);
        let (menu, fallback) = quick_menu("Backspace", &home_keys);
        let keys = menu.keys;
        assert!(fallback);
        assert_eq!(keys[0], parse("Meta+F11").unwrap());
        assert_eq!(keys[0].qt(), 0x1100_003a);
        assert_eq!(keys[0].keysym(), 0xffc8);
        assert!(quick_menu("", &home_keys).1);
        // A key Home already uses stays Home's, whatever its spelling.
        let remote = [0x0100_0055, 0x0100_00dc];
        assert_eq!(qt(&quick_menu("super+f12", &home_keys).0.keys), remote);
        assert_eq!(qt(&quick_menu("HomePage", &home_keys).0.keys), remote);
        assert_eq!(
            qt(&quick_menu("Ctrl+Alt+Q", &home("Menu").0).0.keys),
            [0x0C00_0051, 0x0100_00dc]
        );
        // A configured Menu is not bound twice.
        assert_eq!(qt(&quick_menu("menu", &home_keys).0.keys), remote);
        assert_eq!(parse("MenuKB").unwrap().qt(), 0x0100_00dc);
        assert!(!parse("Meta+Menu").unwrap().is_remote_menu());
    }

    /// The remote's Menu keys are unbound while the shell has focus; the
    /// configured key stays bound everywhere, also when it is a Menu key
    /// (ADR-0106, review of #502).
    #[test]
    fn a_configured_menu_key_stays_bound_inside_the_shell() {
        let qt = |keys: &[Sequence]| keys.iter().map(|k| k.qt()).collect::<Vec<_>>();
        let home_keys = home(DEFAULT_HOME).0;
        let menu = quick_menu("Ctrl+Alt+Q", &home_keys).0;
        assert_eq!(
            qt(&menu.bound(false)),
            [0x0C00_0051, 0x0100_0055, 0x0100_00dc]
        );
        assert_eq!(qt(&menu.bound(true)), [0x0C00_0051]);
        for (configured, kept, other) in [
            ("Menu", 0x0100_0055, 0x0100_00dc),
            ("MenuKB", 0x0100_00dc, 0x0100_0055),
        ] {
            let menu = quick_menu(configured, &home_keys).0;
            assert_eq!(qt(&menu.bound(false)), [kept, other]);
            assert_eq!(qt(&menu.bound(true)), [kept], "{configured}");
        }
        // A configured key that is one of Home's is not kept at all.
        let menu = quick_menu("Menu", &home("Menu").0).0;
        assert_eq!(qt(&menu.bound(true)), Vec::<i32>::new());
        assert_eq!(qt(&menu.bound(false)), [0x0100_00dc]);
    }
}
