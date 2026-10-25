// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The global on-screen keyboard (ADR-0105): the shell-facing
//! `org.arinn.ExternalKeyboard1` and the adapter-facing
//! `org.arinn.KeyboardBridge1`. Both are internal daemon-root interfaces,
//! never plugin capabilities. Typed text crosses them as a bounded value;
//! no record is a command line or an expression.
use crate::{platform, validation::ErrorCode};

/// Shell-facing interface: panel signals and typed keys.
pub const SHELL_INTERFACE: &str = "org.arinn.ExternalKeyboard1";
/// Adapter-facing interface: the key request and the deliverer's lane.
pub const BRIDGE_INTERFACE: &str = "org.arinn.KeyboardBridge1";
/// The bus name the KDE input method owns (ADR-0105).
pub const INPUT_METHOD_SERVICE: &str = "org.arinn.InputMethod";
/// The largest text one `Type` call delivers, in UTF-8 bytes.
pub const MAX_TEXT_BYTES: usize = 256;
/// Records that may wait for the deliverer before `Type` and `Press` fail.
pub const MAX_QUEUED: usize = 64;

/// Panel keys a deliverer captures for the shell.
pub const INPUT_KEYS: [&str; 6] = ["up", "down", "left", "right", "select", "back"];
/// Editing keys the shell asks a deliverer to send.
pub const PRESS_KEYS: [&str; 4] = ["backspace", "return", "left", "right"];
/// Why a deliverer ended a session.
pub const CLOSE_REASONS: [&str; 3] = ["focus", "locked", "unavailable"];

/// A session or generation: a canonical non-nil UUID.
pub fn session(value: &str) -> Result<(), ErrorCode> {
    platform::generation(value)?;
    // One spelling per ID: lowercase and hyphenated, as `Uuid` writes it.
    // Uppercase, braced, URN or unhyphenated forms of the same UUID would
    // otherwise be distinct strings for one session (ADR-0105).
    match uuid::Uuid::parse_str(value) {
        Ok(uuid) if uuid.to_string() == value => Ok(()),
        _ => Err(ErrorCode::InvalidArgument),
    }
}

/// Text for one delivery: 1–256 UTF-8 bytes and no control characters
/// (Unicode category Cc, `char::is_control`), so tab, newline and carriage
/// return are never text; Enter is the `return` key. Other separators and
/// format characters (U+2028, U+2029, Cf) are not rejected.
pub fn text(value: &str) -> Result<(), ErrorCode> {
    if value.is_empty() || value.len() > MAX_TEXT_BYTES || value.chars().any(char::is_control) {
        return Err(ErrorCode::InvalidArgument);
    }
    Ok(())
}

pub fn input_key(value: &str) -> Result<(), ErrorCode> {
    INPUT_KEYS
        .contains(&value)
        .then_some(())
        .ok_or(ErrorCode::InvalidArgument)
}

pub fn press_key(value: &str) -> Result<(), ErrorCode> {
    PRESS_KEYS
        .contains(&value)
        .then_some(())
        .ok_or(ErrorCode::InvalidArgument)
}

pub fn close_reason(value: &str) -> Result<(), ErrorCode> {
    CLOSE_REASONS
        .contains(&value)
        .then_some(())
        .ok_or(ErrorCode::InvalidArgument)
}

/// One record of `KeyboardBridge1.Next`, decoded and validated.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Record {
    Idle,
    Open(String),
    Type(String, String),
    Press(String, String),
    Close(String),
}

impl Record {
    /// The wire form: a fixed string array.
    pub fn fields(&self) -> Vec<String> {
        match self {
            Self::Idle => Vec::new(),
            Self::Open(s) => vec!["open".into(), s.clone()],
            Self::Type(s, text) => vec!["type".into(), s.clone(), text.clone()],
            Self::Press(s, key) => vec!["press".into(), s.clone(), key.clone()],
            Self::Close(s) => vec!["close".into(), s.clone()],
        }
    }
    /// Decodes a record; anything else is `InvalidArgument`.
    pub fn parse(fields: &[String]) -> Result<Self, ErrorCode> {
        let record = match fields {
            [] => Self::Idle,
            [kind, s] if kind == "open" => Self::Open(s.clone()),
            [kind, s] if kind == "close" => Self::Close(s.clone()),
            [kind, s, text] if kind == "type" => {
                self::text(text)?;
                Self::Type(s.clone(), text.clone())
            }
            [kind, s, key] if kind == "press" => {
                press_key(key)?;
                Self::Press(s.clone(), key.clone())
            }
            _ => return Err(ErrorCode::InvalidArgument),
        };
        if let Self::Open(s) | Self::Close(s) | Self::Type(s, _) | Self::Press(s, _) = &record {
            session(s)?;
        }
        Ok(record)
    }
}

/// The deliverer's and the platform adapter's view of the bridge.
#[zbus::proxy(
    interface = "org.arinn.KeyboardBridge1",
    default_service = "org.arinn.Daemon1",
    default_path = "/org/arinn/Daemon1"
)]
pub trait KeyboardBridge {
    fn request(&self) -> zbus::Result<()>;
    fn connect(&self, instance: &str) -> zbus::Result<String>;
    fn next(&self, generation: &str) -> zbus::Result<Vec<String>>;
    fn opened(&self, generation: &str, session: &str) -> zbus::Result<()>;
    fn input(&self, generation: &str, session: &str, key: &str) -> zbus::Result<()>;
    fn closed(&self, generation: &str, session: &str, reason: &str) -> zbus::Result<()>;
}

/// The shell's view; the Qt shell uses QtDBus with the same members.
#[zbus::proxy(
    interface = "org.arinn.ExternalKeyboard1",
    default_service = "org.arinn.Daemon1",
    default_path = "/org/arinn/Daemon1"
)]
pub trait ExternalKeyboard {
    #[zbus(name = "Type")]
    fn type_text(&self, session: &str, text: &str) -> zbus::Result<()>;
    fn press(&self, session: &str, key: &str) -> zbus::Result<()>;
    fn close(&self, session: &str) -> zbus::Result<()>;
    #[zbus(signal)]
    fn opened(&self, session: &str) -> zbus::Result<()>;
    #[zbus(signal)]
    fn input(&self, session: &str, key: &str) -> zbus::Result<()>;
    #[zbus(signal)]
    fn closed(&self, session: &str) -> zbus::Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    const S: &str = "10000000-0000-4000-8000-000000000001";

    #[test]
    fn text_is_bounded_and_printable() {
        for ok in ["a", "Я", "https://", "😀", &"x".repeat(256), " "] {
            assert_eq!(text(ok), Ok(()), "{ok:?}");
        }
        for bad in ["", "\n", "a\tb", "\u{7f}", "\u{85}", "\0", &"x".repeat(257)] {
            assert_eq!(text(bad), Err(ErrorCode::InvalidArgument), "{bad:?}");
        }
        // 128 two-byte characters fit, 129 do not.
        assert!(text(&"я".repeat(128)).is_ok());
        assert!(text(&"я".repeat(129)).is_err());
    }

    #[test]
    fn keys_and_reasons_are_closed_sets() {
        for key in INPUT_KEYS {
            assert!(input_key(key).is_ok());
        }
        for key in PRESS_KEYS {
            assert!(press_key(key).is_ok());
        }
        for reason in CLOSE_REASONS {
            assert!(close_reason(reason).is_ok());
        }
        for bad in ["", "Up", "home", "menu", "space", "escape", "a"] {
            assert!(input_key(bad).is_err(), "{bad}");
        }
        for bad in ["", "Backspace", "up", "delete", "tab", "a"] {
            assert!(press_key(bad).is_err(), "{bad}");
        }
        assert!(close_reason("timeout").is_err());
        assert!(session(S).is_ok());
        assert!(session("00000000-0000-0000-0000-000000000000").is_err());
        assert!(session("not-a-uuid").is_err());
        // Only the canonical spelling (review of #512): lowercase and
        // hyphenated, never uppercase, braced, URN or simple.
        for other in [
            "10000000-0000-4000-8000-00000000000A",
            "{10000000-0000-4000-8000-000000000001}",
            "urn:uuid:10000000-0000-4000-8000-000000000001",
            "10000000000040008000000000000001",
        ] {
            assert_eq!(session(other), Err(ErrorCode::InvalidArgument), "{other}");
        }
        assert!(session("10000000-0000-4000-8000-00000000000a").is_ok());
    }

    #[test]
    fn records_round_trip_and_reject_anything_else() {
        let records = [
            Record::Idle,
            Record::Open(S.into()),
            Record::Type(S.into(), "Привет".into()),
            Record::Press(S.into(), "backspace".into()),
            Record::Close(S.into()),
        ];
        for record in records {
            assert_eq!(Record::parse(&record.fields()), Ok(record));
        }
        let fields = |items: &[&str]| items.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        for bad in [
            fields(&["open"]),
            fields(&["open", "bad"]),
            fields(&["open", S, "x"]),
            fields(&["type", S]),
            fields(&["type", S, ""]),
            fields(&["type", S, "a\nb"]),
            fields(&["press", S, "tab"]),
            fields(&["exec", S]),
            fields(&["close", S, "focus"]),
        ] {
            assert_eq!(
                Record::parse(&bad),
                Err(ErrorCode::InvalidArgument),
                "{bad:?}"
            );
        }
    }
}
