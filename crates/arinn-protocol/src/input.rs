// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Pre-stable key bindings for the shell (ADR-0079, spec §6.2).
//!
//! The daemon reads `[input.bindings]` from `config.toml` and the shell asks
//! for them with `org.arinn.Daemon1.GetInputBindings`. A binding maps one
//! semantic action to extra keys; the default keys keep working. A key is a
//! Qt key name such as `Menu` or `F5`, or `evdev:<code>` for a key Qt has no
//! name for (a remote's voice button is `evdev:582`). Bindings apply only
//! inside the shell window; the global Home shortcut is separate.
use crate::validation::ErrorCode;

/// Semantic actions a key can be bound to, in the order the shell documents
/// them.
pub const ACTIONS: &[&str] = &[
    "up",
    "down",
    "left",
    "right",
    "select",
    "back",
    "home",
    "menu",
    "play_pause",
];

/// Most keys bound to one action.
pub const MAX_KEYS_PER_ACTION: usize = 8;
/// Longest key name, in bytes.
pub const MAX_KEY_BYTES: usize = 32;
/// Highest Linux evdev key code (`KEY_MAX`).
pub const MAX_EVDEV_CODE: u32 = 0x2ff;

/// A key name: `evdev:<1..=KEY_MAX>`, or a Qt key name of ASCII letters,
/// digits, spaces and `_`, at most [`MAX_KEY_BYTES`]. A single character is
/// rejected: keys that type text are never rebound, so text fields keep
/// working.
pub fn key(value: &str) -> bool {
    if let Some(code) = value.strip_prefix("evdev:") {
        return !code.starts_with('0')
            && code
                .parse::<u32>()
                .is_ok_and(|code| (1..=MAX_EVDEV_CODE).contains(&code));
    }
    value.len() > 1
        && value.len() <= MAX_KEY_BYTES
        && value.trim() == value
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == ' ' || c == '_')
}

/// The key a name refers to: Qt key names are case-insensitive.
fn identity(key: &str) -> String {
    key.to_ascii_lowercase()
}

/// One action and its extra keys, as `GetInputBindings` returns them.
pub type Binding = (String, Vec<String>);

/// Valid bindings only: known actions, 1..=[`MAX_KEYS_PER_ACTION`] valid
/// keys without duplicates, and no key bound to two actions. Returns the
/// accepted bindings in [`ACTIONS`] order and whether anything was dropped.
pub fn accept(bindings: impl IntoIterator<Item = (String, Vec<String>)>) -> (Vec<Binding>, bool) {
    let mut dropped = false;
    let mut by_action: Vec<Binding> = Vec::new();
    for (action, keys) in bindings {
        if !ACTIONS.contains(&action.as_str())
            || keys.is_empty()
            || keys.len() > MAX_KEYS_PER_ACTION
            || !keys.iter().all(|k| key(k))
            || by_action.iter().any(|(a, _)| *a == action)
        {
            dropped = true;
            continue;
        }
        let mut unique: Vec<String> = Vec::new();
        for k in keys {
            if !unique.iter().any(|u| identity(u) == identity(&k)) {
                unique.push(k);
            }
        }
        by_action.push((action, unique));
    }
    // A key bound to two actions is ambiguous, whatever its spelling (Qt key
    // names are case-insensitive): drop it from both.
    let mut counts = std::collections::HashMap::new();
    for (_, keys) in &by_action {
        for k in keys {
            *counts.entry(identity(k)).or_insert(0) += 1;
        }
    }
    let mut accepted: Vec<Binding> = by_action
        .into_iter()
        .filter_map(|(action, keys)| {
            let kept: Vec<String> = keys
                .into_iter()
                .filter(|k| counts[&identity(k)] == 1)
                .collect();
            if kept.is_empty() {
                None
            } else {
                Some((action, kept))
            }
        })
        .collect();
    if counts.values().any(|n| *n > 1) {
        dropped = true;
    }
    accepted.sort_by_key(|(action, _)| ACTIONS.iter().position(|a| a == action));
    (accepted, dropped)
}

/// Rejects a list that `accept` would not return as is.
pub fn validate(bindings: &[Binding]) -> Result<(), ErrorCode> {
    let (accepted, dropped) = accept(bindings.iter().cloned());
    (!dropped && accepted == bindings)
        .then_some(())
        .ok_or(ErrorCode::InvalidArgument)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn b(action: &str, keys: &[&str]) -> (String, Vec<String>) {
        (action.into(), keys.iter().map(|k| k.to_string()).collect())
    }

    #[test]
    fn key_names() {
        for good in [
            "Menu",
            "F5",
            "Media Play",
            "evdev:582",
            "evdev:767",
            "Volume_Up",
        ] {
            assert!(key(good), "{good}");
        }
        for bad in [
            "",
            "a",
            "1",
            "evdev:0",
            "evdev:768",
            "evdev:-1",
            "evdev:01",
            "evdev:",
            " Menu",
            "Ctrl+M",
            "Menu\n",
            &"x".repeat(MAX_KEY_BYTES + 1),
        ] {
            assert!(!key(bad), "{bad:?}");
        }
    }

    #[test]
    fn invalid_entries_and_shared_keys_are_dropped() {
        let (accepted, dropped) = accept(vec![
            b("menu", &["evdev:582", "Menu", "evdev:582"]),
            b("select", &["F5"]),
            b("jump", &["F6"]),
            b("back", &["a"]),
            b("up", &["F5"]),
        ]);
        assert!(dropped);
        // Order follows ACTIONS; F5 was bound twice and is dropped from both.
        assert_eq!(accepted, vec![b("menu", &["evdev:582", "Menu"])]);

        // Spellings of one Qt key count as the same key.
        let (accepted, dropped) = accept(vec![
            b("up", &["F5"]),
            b("down", &["f5"]),
            b("menu", &["F7", "f7"]),
        ]);
        assert!(dropped);
        assert_eq!(accepted, vec![b("menu", &["F7"])]);
    }

    #[test]
    fn validate_accepts_only_normalized_lists() {
        let ok = vec![b("up", &["F2"]), b("menu", &["evdev:582"])];
        assert_eq!(validate(&ok), Ok(()));
        assert_eq!(
            validate(&[b("menu", &["evdev:582"]), b("up", &["F2"])]),
            Err(ErrorCode::InvalidArgument)
        );
        assert_eq!(
            validate(&[b("menu", &["x"])]),
            Err(ErrorCode::InvalidArgument)
        );
        let many: Vec<String> = (1..=MAX_KEYS_PER_ACTION + 1)
            .map(|i| format!("F{i}"))
            .collect();
        assert_eq!(
            validate(&[("menu".into(), many)]),
            Err(ErrorCode::InvalidArgument)
        );
    }
}
