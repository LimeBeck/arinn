# ADR-0079: Shell key bindings from config.toml

<!-- SPDX-FileCopyrightText: 2026 Arinn contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

- **Status:** Accepted
- **Date:** 2026-10-03
- **Deciders:** project owner for the scope (2026-10-03, [#64](https://github.com/LimeBeck/arinn/issues/64): "minimal: bindings in config.toml, no Settings UI yet"); the mechanism is a delegated technical choice ([ADR-0037](0037-agent-autonomy-and-owner-gated-decisions.md))
- **Amends / Supersedes:** —
- **Amended by / Superseded by:** —
- **Specification:** [§6.2](../specifications/arinn-specification.md#62-keys), [§11](../specifications/arinn-specification.md#11-state-ownership-and-persistence)

## Context

Spec §6.2 requires semantic actions that are configurable per device, but every screen handles fixed Qt keys. A TV remote's voice button sends `KEY_VOICECOMMAND` (evdev 582). Qt has no key for it (`key() == 0`; it keeps the XKB keycode 590 as `nativeScanCode`), so it cannot open the menu. The owner asked to assign it in the configuration.

## Decision

- **Configuration.** `config.toml` gains `[input.bindings]`: semantic action → list of extra keys. The default keys keep working.
  - Actions: `up`, `down`, `left`, `right`, `select`, `back`, `home`, `menu`, `play_pause`.
  - A key is a Qt key name of more than one character (`Menu`, `F5`, `Media Play`) or `evdev:<code>` for a key Qt does not name.
  - At most 8 keys per action. A key bound to two actions is dropped from both. Invalid entries are dropped, and the daemon logs one warning without the payload.
  - Rules and normalisation live in `arinn_protocol::input`.
- **Transport.** The daemon keeps owning `config.toml` (§11). The internal shell method `org.arinn.Daemon1.GetInputBindings() → a(sas)` reads the file on every call and returns the accepted bindings in action order. No provider-facing contract changes.
- **Application.** The shell installs one application event filter (`InputBindings`). For key events sent to a window, a bound key is replaced by the action's canonical key:
  - `up`, `down`, `left`, `right` → the arrow keys;
  - `select` → Return; `back` → Escape; `home` → Home;
  - `menu` → Menu; `play_pause` → MediaTogglePlayPause.

  Press, release and auto-repeat are kept. Every existing screen handler therefore works unchanged.
- **Text keys are never rebound.** A key event that produces printable text, a single-character name, and a key with modifiers do not match. Text fields keep working whatever the configuration says.
- **Reloading.** The shell reads the bindings at start and whenever it becomes the active application, so an edited file applies the next time Home is shown.
- **Out of scope.** The global Home shortcut belongs to the platform layer and is separate ([#286](https://github.com/LimeBeck/arinn/issues/286)). A Settings → Input UI, per-device bindings and gamepads are later steps of #64.

## Consequences

- Any remote key that reaches the shell window can drive an action, including keys Qt cannot name.
- Bindings act only while the shell window has focus. Keys outside it still belong to the active application (§6.2).
- One more internal daemon method. `daemon1.xml` and the private-bus wire check include it.

## Alternatives considered

- **udev hwdb remapping.** Works today without Arinn changes, but it is system-wide, needs root and knowledge of the Qt key mapping (for example `KEY_MENU` arrives as `Key_MenuKB`, not `Key_Menu`). The owner preferred a config entry.
- **Daemon settings through `Settings1`.** A structured map does not fit the typed setting kinds, and the rows would appear as raw text in the Settings screen.
- **Per-screen handling of extra keys.** Rejected: every screen would need the same lookup, and a missed screen would ignore the binding.

## Verification

- `arinn_protocol::input` tests: key names, dropping invalid and shared keys, normalised order.
- `arinnd` `input_tests`: config.toml `[input.bindings]` → `GetInputBindings`, invalid entries dropped.
- `arinn-input-bindings-test`:
  - `evdev:582` → Menu and `F5` → Return for press and release;
  - text, letter and modified keys unchanged;
  - loading from a fake `Daemon1`.

  Without the substitution, the test fails.
- `tests/integration/phase1.py` compares `daemon1.xml` with the live export.
