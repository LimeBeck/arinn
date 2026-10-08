# ADR-0094: Configurable global Home keys

<!-- SPDX-FileCopyrightText: 2026 Arinn contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

- **Status:** Accepted
- **Date:** 2026-10-07
- **Deciders:** project owner for the default keys (2026-10-07, [#286](https://github.com/LimeBeck/arinn/issues/286): "`home_shortcut` plus HomePage"); the mechanism is a delegated technical choice ([ADR-0037](0037-agent-autonomy-and-owner-gated-decisions.md))
- **Amends / Supersedes:** —
- **Amended by / Superseded by:** —
- **Specification:** [§6.2](../specifications/arinn-specification.md#62-keys), [§12.1](../specifications/arinn-specification.md#121-global-home)

## Context

Spec §6.2 calls the global Home shortcut configurable, and `config.toml` has `home_shortcut` (default `Meta+F12`). The daemon only validated the field. The KWin script registered `ArinnPhase0Home` with a fixed `Meta+F12`, and the X11/EWMH adapter grabbed a fixed `Meta+F12`, so editing the field did nothing.

KWin's `registerShortcut` hands one default sequence to KGlobalAccel, which keeps the stored keys on every later registration. Passing the configured value as that default would therefore only work on a fresh installation.

A TV remote has no F-keys. Its Home button usually sends `KEY_HOMEPAGE` (Qt `Key_HomePage`, X11 `XF86HomePage`), so Home could not be reached from the remote at all.

## Decision

- **Keys.** The global Home keys are `home_shortcut` followed by HomePage. When the value is empty or invalid, the default `Meta+F12` is used and the adapter logs one warning without the value.
- **Syntax.** `home_shortcut` uses Qt's portable key-sequence text, case-insensitive. The modifiers are `Meta` (or `Super`), `Ctrl`, `Alt` and `Shift`, and the keys are `F1`–`F35`, `HomePage`, `Favorites`, `Search`, `Menu`, `A`–`Z` and `0`–`9`. A letter or digit needs `Meta`, `Ctrl` or `Alt`. Esc, Backspace and keys that type text are never accepted, because Arinn must not take them from other applications (§6.2). The parser is shared by the adapters: `platform/key_sequence.rs`.
- **KDE.** When a KWin adapter connection polls `Daemon1.TakeCommand` for the first time, the daemon's KDE platform code (`platform/kde/global_home.rs`) writes the keys to the action `kwin/ArinnPhase0Home` with `org.kde.KGlobalAccel.setForeignShortcutKeys`. It then reads them back with `shortcutKeys` and logs a `Conflict` warning when KGlobalAccel dropped a key that another global shortcut holds. A daemon or KWin restart writes them again. `config.toml` is the source of truth: a change made in KDE's Shortcuts settings is replaced the next time the adapter connects.
- **X11/EWMH.** `arinn-platform-x11` reads `home_shortcut` read-only from `$XDG_CONFIG_HOME/arinn/config.toml` when it connects and grabs every sequence, including lock-modifier variants. A sequence whose key or modifier is not in the keymap, or that another client already grabs, is skipped (the conflict is logged); the adapter fails only when no sequence can be grabbed, at connection or when a mapping change grabs again. The daemon keeps sole write ownership of the file (§11).
- **Applying a change.** An edited `home_shortcut` applies when the daemon (KDE) or the X11 adapter restarts.
- **No contract change.** Daemon1, KWinBridge1 and the KWin script are unchanged; the action name and its first default stay as they are.

## Consequences

- The remote's Home button opens Home without udev hwdb remapping or KDE settings.
- A key Qt does not name (for example `KEY_VOICECOMMAND`) still cannot be a global shortcut; hwdb remapping to a named key remains the workaround.
- Users who changed the shortcut in KDE's settings must move that choice to `config.toml`.
- The daemon depends on the KGlobalAccel D-Bus API on KDE. If it is missing, Home keeps the keys KWin registered and the daemon logs `Unavailable`.

## Alternatives considered

- **Pass `home_shortcut` to the KWin script as its default sequence.** Rejected: KGlobalAccel ignores a new default once the action is stored, and the script has no access to `config.toml` without a new Daemon1 or KWinBridge1 member.
- **Remove the field and document KDE's Shortcuts settings.** Rejected by the owner, and it would not cover the X11 adapter.
- **A second KWin action for HomePage.** Works for the default but not for a configured key, and splits one product action in two in KDE's settings.
- **Deliver the value to the adapters over D-Bus.** Needs a contract change; reading the file read-only in the X11 adapter keeps this change out of the contract queue.

## Verification

- `key_sequence` tests: accepted names and codes (Qt combined codes and X11 keysyms), rejected text keys, default fallback and HomePage deduplication.
- `global_home` tests on a private bus with a fake KGlobalAccel: keys written, invalid values replaced by the default, conflicts and a missing service reported.
- `runtime::window_tests::adapter_connection_applies_configured_home_shortcut`: `config.toml` `home_shortcut = "Ctrl+Alt+H"` reaches KGlobalAccel as `Ctrl+Alt+H` and HomePage on the adapter's first poll, and only once per connection. It fails without the fix.
- `arinn-platform-x11` tests: the configured value comes from `config.toml`; under Xvfb, `Ctrl+Alt+H` and `XF86HomePage` open Home and `Meta+F12` no longer does; when another client holds `Ctrl+Alt+H`, HomePage still opens Home at connection and after a mapping change (fails without the per-sequence handling).
- Not tested: real KDE Plasma 6 and real X11 sessions with a physical remote (operator tests need the owner's permission).
