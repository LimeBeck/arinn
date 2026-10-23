<!-- SPDX-FileCopyrightText: 2026 Arinn contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Quick menu

Presentation of the quick menu of [specification §5.7](../specifications/arinn-specification.md#57-system-ui-power-and-ambient)
([#439](https://github.com/LimeBeck/arinn/issues/439), [ADR-0106](../adr/0106-quick-menu-presentation.md)).
The shell behaviour, Back and the platform path are in
[Activity and interaction](activity-and-interaction.md#quick-menu).

The sheet opens for `quick_menu_shortcut` (default Meta+F11) and, outside
the shell, for the remote's Menu key. The platform binds Menu only while
another window has focus. Inside the shell Menu keeps its meaning (navigation,
tile action sheet), and inside the sheet it does nothing.

`apps/arinn-shell/ui/QuickMenu.qml` is the *sheet* focus scope (§6.1). Main.qml
routes focus to it while it is open, so no screen beneath can take focus. It
only emits intents; Main.qml connects them to `MediaCatalog`, `Backend`,
`SystemControl` (`SystemControl1`, [system control](system-control.md)) and the
power scope ([power menu](ui-power.md)).

## Layout

There is no design handoff for the sheet yet; G-5.1 needs one. Until then the
sheet follows the tile action sheet (handoff `cl-1`) and the power menu rows:

- a right-hand sheet 720 wide on the 1920×1080 canvas, `#1d1e21`, over a scrim
  (`rgba(0, 0, 0, 0.55)` over the shell, `0.35` over a live Activity);
- the clock and the title "Quick menu" (52 px, weight 700) at the top;
- rows 624 × 96, radius 30, a 64 px round icon plate, a 32 px label and a
  26 px description; the focused row is a light plate with ink text;
- a result line and the OK / Back hints at the bottom.

## Rows

| Row | Source | OK | Unavailable when |
| --- | --- | --- | --- |
| Now playing | `MediaCatalog` Continue entries: the playing session, else a paused one | play/pause to that session | no session, no play/pause, no daemon |
| Sound | `SystemControl1` level and mute (read when the sheet opens, and after each change); `SystemStatus1` mute state until then | toggle mute; Left steps down, Right turns the sound on and steps up | no daemon |
| Network | `SystemStatus1`: Wi-Fi with its level, cable, other, not connected, unknown | — (not focusable) | — |
| Sleep | power scope | power menu with the suspend request sent | no daemon |
| Power | power scope | power menu | no daemon |
| Open Home | — | fullscreen Home, focus on Return | never |
| Close Arinn | — | closes the shell, as in global navigation | never |

An unavailable row stays visible, disabled, and says why; focus skips it. A
row that becomes unavailable while focused passes focus to the next available
row, or the previous one. Up and Down move without wrapping; Left and Right
never leave the sheet. Esc, Backspace and Back emit Back; Home emits Open Home;
Menu does nothing inside the sheet. A click outside the sheet is Back.

The Sound row shows a level only from a platform reply, never a guessed one. A
failed change says "Volume control is not available" in the sheet, because the
volume HUD belongs to the screens hidden over an Activity. The volume keys keep
working while the sheet is open, since the shell is on screen (OQ-04).

Sleep and Power close the sheet and open the shell's power scope, which keeps
its confirmations, its one-request-at-a-time rule, the busy state of a request
left in flight and its refusals (spec §5.7 *Power*). Over an Activity the shell
first comes up as for Home, as the power key does (§5.7 *Power button*): the
Activity background shows behind Home, and Back from the power menu leaves Home
with that Activity as Return. Over the shell the power menu opens over the
current screen, and Back returns there. Sleep sends `Suspend` at once; nothing
reports a sleep the platform did not accept.

The quick-menu key also ends ambient, as the power key does.

## Verification

- `tests/ui/tst_quick_menu.qml` (`ui-components`): row order and focus, the
  intents, Back/Home/Menu keys, outside click, the Sound row with and without
  platform control and its level text, Now playing without play/pause or a session, the degraded
  form and the focus hand-off, the sheet geometry and the scrim.
- `media-catalog` `quickMenuOverManagedPlaybackReturnsToThePlayer`: over managed
  playback, Back returns to the live player at once and Home never shows.
- `arinn-quick-menu-test` (`quick-menu`): Main.qml with the real `Backend` and
  a fake `Daemon1` with `SystemControl1`: Back over an Activity calls `Back`
  without showing Home, Open Home and `HomeRequested` replace the sheet, Back
  over Home returns focus to its opener, the Sound row's level and its
  `GetVolume`/`SetMuted`/`StepVolume`/`ToggleMuted` calls, Sleep and Power
  opening the power scope (one `Suspend`, its refusal shown, Back to Home with
  no Return), repeats and a foreign epoch, the degraded form without the
  daemon, and the session lock.
- Not tested: real KDE and X11 sessions with the remote, and a visual
  comparison (no handoff yet).
