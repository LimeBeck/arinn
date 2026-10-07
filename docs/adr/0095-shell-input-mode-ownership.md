# ADR-0095: Shell input mode ownership

<!-- SPDX-FileCopyrightText: 2026 Arinn contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

- **Status:** Accepted
- **Date:** 2026-10-07
- **Deciders:** project owner for the behaviour (2026-10-04, [#308](https://github.com/LimeBeck/arinn/issues/308)); the mechanism is a delegated technical choice ([ADR-0037](0037-agent-autonomy-and-owner-gated-decisions.md))
- **Amends / Supersedes:** —
- **Amended by / Superseded by:** —
- **Specification:** [§6.1](../specifications/arinn-specification.md#61-focus), AC-02, AC-14, AC-18

## Context

Spec §6.1 defines D-pad mode (cursor hidden) and pointer mode (hover moves focus). Home, Library, Downloads and Session each kept their own `pointerMode` and switched it from their own hover handler and their own arrow-key handler. That had three gaps:

- An air mouse resting in the hand moves slightly. Its motion right after a remote key switched the screen back to pointer mode, and hover took focus from the control the remote had selected.
- Only arrow keys reached the screens' handlers, and only when no control consumed them first. Enter, Back or typing did not hide the cursor.
- A screen shown under a resting pointer, or content scrolling under it, produced hover updates. The Library handler counted any update as movement.

The owner asked (#308) to hide the cursor on key presses, ignore the pointer for two seconds, and show it again only when the mouse moves.

## Decision

1. **One input mode.** `Arinn.Ui.InputMode` (`ui/InputMode.qml`) is a QML singleton that holds the shell's mode: `pointerActive`, `keyCooldownMs` (2000) and `cooldownActive`. Screens bind `pointerMode` to `pointerActive` and report input to it; they no longer own the mode.
2. **Keys.** `keyPressed()` enters D-pad mode and restarts the cooldown. During the cooldown, `pointerMoved()` changes nothing, so hover cannot take focus. When the cooldown ends, the mode stays D-pad until the next genuine movement; time alone never reveals the cursor or focuses a hovered item.
3. **Genuine movement.** `pointerMoved(scenePosition)` counts only a position that differs from the last reported one. Hover re-evaluated under a resting pointer (scrolling, animations, model refreshes, a screen shown under it) reports the same point. `forgetPointer()` makes the next position a reference only.
4. **Every key, every window.** `InputModeSource` (`input_mode.{h,cpp}`, QML type `Arinn.Shell.InputModeSource`, instantiated by `Main.qml`) is an application event filter. For events sent to a shell window it reports:
   - every key press except modifier and lock keys alone, including keys a control consumes and typed text;
   - pointer motion whose global position changed;
   - window enter, show, hide, activation and deactivation as an arrival, which calls `forgetPointer()`.

   It never consumes an event, so every key keeps its action and text fields keep working.
5. **Cursor.** `InputModeSource` hides the cursor with an application override cursor (`Qt::BlankCursor`) while `pointerActive` is false. The override applies only to Arinn's windows; other applications' cursors are untouched.
6. **Unchanged.** Hover suppression for 500 ms after a scroll stays (`hoverFocusEnabled`). Focus ownership stays with ADR-0076: hidden screens still cannot take focus. Settings, Open services and the power menu never entered pointer mode and keep that; the cursor rule applies to them through the filter. The ambient screen wakes on the first pointer movement away from where the pointer rested when it opened (#307 and AC-19 still own the screensaver itself).

## Consequences

- A key press anywhere in the shell hides the cursor, and a resting air mouse cannot move focus for two seconds.
- New screens with pointer interaction bind to `InputMode` and report their hover position; they need no own mode logic.
- Tests reset the singleton (`reset()`, `forgetPointer()`) in their fixtures because the mode is shared across screens.
- The first pointer sample after the shell window appears never counts as movement; a real movement produces more samples.

## Alternatives considered

- **Per-screen cooldowns.** Rejected: the same copy in four screens, and screens without hover logic would still show the cursor after keys.
- **A QML-only key hook.** Rejected: QML key handlers do not see keys a control has consumed, so Enter or typing would not hide the cursor.
- **Hiding the system cursor globally (compositor or `unclutter`-style tools).** Rejected: it would affect other applications, which spec §6.2 leaves alone.

## Verification

- `arinn-input-mode-test`: every non-modifier key press in a window counts once; only a changed position is movement; show and enter reset the reference; the override cursor follows the mode and is restored on destruction; events are never consumed.
- `tests/ui/tst_input_mode.qml`: a key keeps its action and enters D-pad mode; motion during the cooldown keeps keyboard focus; repeated keys extend the cooldown; after it the cursor stays hidden until movement, which restores hover focus; a resting pointer under scrolling, a model refresh or a newly shown screen reveals nothing; screens share one mode and a hidden screen takes no focus.
- Existing Home, Downloads, Library and Session pointer tests pass with the shared mode; tests that move the pointer right after a key wait for the cooldown.
- `tst_power.qml` `test_consumed_ambient_wake`: the resting pointer anchors the ambient wake at once (this case was flaky on `master`, 3 of 15 isolated runs).
- Not tested: a real KDE Wayland or X11 session with a physical remote and air mouse (operator test; needs the owner's permission).
