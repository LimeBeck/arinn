<!-- SPDX-FileCopyrightText: 2026 Arinn contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Quick menu

Presentation of the quick menu of [specification §5.7](../specifications/arinn-specification.md#57-system-ui-power-and-ambient)
([#439](https://github.com/LimeBeck/arinn/issues/439), [ADR-0106](../adr/0106-quick-menu-presentation.md)).
The shell behaviour, Back and the platform path are in
[Activity and interaction](activity-and-interaction.md#quick-menu).

The sheet opens for `quick_menu_shortcut` (default Meta+F11) and, outside
the shell, for the remote's Menu key. The platform binds the remote's Menu keys
only while another window has focus. Inside the shell Menu keeps its meaning
(navigation, tile action sheet), and inside the sheet it does nothing. A
`quick_menu_shortcut` that is itself Menu or MenuKB stays bound everywhere, as
ADR-0106 says for the configured key.

**Over an Activity or over the shell.** The sheet is over the shell only when
the shell was on screen and had focus when the request arrived; otherwise, and
over managed playback, it is over an Activity. Both platforms activate the
shell only after the request, so a shell that a `Status` read has just shown
is not active yet. No timing window is involved: the quick-menu key right
after Home is over the shell.

**Behind the sheet.** Over an Activity the window is transparent and the live
window shows through. When the Activity background has a frame for this Home
context (`ActivityBackgroundSource`, ADR-0087; the quick-menu request arms the
capture), the sheet shows that last frame instead, full-screen and unblurred, so
it never sits on an empty or black screen: X11 without a compositor, or a game
that unmaps on focus loss. The frame stays until the platform hides the shell
after Back. Over managed playback the player stays live.

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

The Sound row shows a level only from a platform reply, never a guessed one.
The volume HUD and the power scope are drawn above the sheet, also over an
Activity, so a change shows the HUD and a failed change says so there. The
volume keys keep working while the sheet is open, since the shell is on screen
(OQ-04).

**Sleep and Power** open the shell's power scope over the sheet (§6.1: dialog ›
power › sheet). The Activity stays behind it. The scope keeps its confirmations,
its one-request-at-a-time rule, the busy state of a request in flight and its
refusals (spec §5.7 *Power*, [power menu](ui-power.md)). Back closes the scope
and focus returns to the Sleep or Power row (§6.3); Back on the sheet then
returns to the Activity. Sleep sends `Suspend` at once, without confirmation;
nothing reports a sleep the platform did not accept. Home inside the scope, or
the global Home key, while a request is in flight replaces the sheet with Home;
the request goes on, and a late refusal reopens the scope over Home. A repeated
quick-menu key under the scope leaves the scope on top. Main.qml draws the
power scope, the launch dialog and ambient in one canvas above the sheet, in
that order, so the launch dialog stays above the power scope as before.

**Ambient.** During ambient the quick-menu key only wakes, and that first event
triggers nothing (§5.7 *Ambient*): Meta+F11 arrives as a request, the remote's
Menu as a key, because the shell has focus; both end ambient without opening
the sheet. The ⏻ exception of §5.7 does not apply to it.

## Verification

- `tests/ui/tst_quick_menu.qml` (`ui-components`): row order and focus, the
  intents, Back/Home/Menu keys, outside click, the Sound row with and without
  platform control and its level text, Now playing without play/pause or a session, the degraded
  form and the focus hand-off, the sheet geometry and the scrim.
- `media-catalog` `quickMenuOverManagedPlaybackReturnsToThePlayer`: over managed
  playback, Back returns to the live player at once and Home never shows.
- `arinn-quick-menu-test` (`quick-menu`): Main.qml with the real `Backend` and
  a fake `Daemon1` with `SystemControl1` on its own bus connection: Back over an
  Activity calls `Back` without showing Home, Open Home and `HomeRequested`
  replace the sheet, Back over Home returns focus to its opener, the Sound
  row's level and its `GetVolume`/`SetMuted`/`StepVolume`/`ToggleMuted` calls,
  Power over the sheet and Back to the Power row over an Activity and over the
  shell, Sleep (one `Suspend`, its refusal, Back to the Sleep row), a late
  refusal after Home left the sheet, the quick-menu key right after Home,
  ambient waking alike for Menu and a request, the Activity frame behind the
  sheet, repeats and a foreign epoch, the degraded form without the daemon, and
  the session lock.
- `key_sequence` `a_configured_menu_key_stays_bound_inside_the_shell`.
- Not tested: real KDE and X11 sessions with the remote, and a visual
  comparison (no handoff yet).
