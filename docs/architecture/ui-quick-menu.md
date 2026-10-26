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
ADR-0106 says for the configured key. That key then never reaches the shell, so
it no longer toggles global navigation or opens the tile action sheet there; it
opens the quick menu instead. That is the user's explicit choice. Home wins a
key both want. On KDE, KGlobalAccel keeps each action's stored keys (the KWin
script registers the quick menu with Meta+F11), so the daemon first removes
Home's configured keys from the quick-menu action, then writes Home, then the
quick menu without Home's keys (`global_home::write_all`). It gives up every
Home key, HomePage included. The quick-menu lock is held from that release to
the quick-menu write, so a focus change during a second adapter connection
waits and cannot write the previous connection's keys back in between (#507).
A key that a third-party action holds stays a conflict for both.

**While arinnd is down.** With the daemon gone, the remote's Menu keys would
open nothing and still swallow the ordinary context-menu key in every
application. On KDE the stop helper (`arinnd --release-power-key`, run by
`ExecStopPost=` of arinnd.service and by `arinn-kwin-adapter stop`) removes
them from the quick-menu action; its other keys stay, and the next adapter
connection writes every key again (`global_home::release_menu_keys`, #507).
The X11 adapter keeps its grabs while it runs without the daemon; they end
with its process.

**Grab lag** (ADR-0106). On X11 the Menu grabs follow focus at the adapter's
next snapshot, up to 0.5 s after the shell gains or loses focus: the adapter
reads X events, grabbed key presses included, on its 0.5 s tick. Shortening
it means waking that loop on X events instead of the tick, a change to the
adapter's main loop that is left for a separate task. On KDE the keys follow
`ReportWindow` with one KGlobalAccel write, which runs after the report.

**Over an Activity or over the shell.** The sheet is over the shell only when
the shell was on screen and had focus when the request arrived; otherwise, and
over managed playback, it is over an Activity. Both adapters activate the shell
only after the request is sent: the KWin script raises it in the
`ReportQuickMenuRequest` reply callback, and the X11 adapter holds the raise
for a quick-menu press until the request is published (`raise_deferred`). A
shell that a `Status` read has just shown is therefore not active yet. No
timing window is involved: the quick-menu key right after Home is over the
shell.

**Behind the sheet: the live window.** Over an Activity the window is
transparent and the live window shows through (§5.7 allows "the live window or
its last frame"). The last frame would be used only where the live window
cannot show, but neither adapter can tell the shell that today, so both use the
live window:

- **X11 without a compositing manager:** the X11 adapter knows (it refuses
  managed presentation then), but `Daemon1` carries no such state, and the X11
  producer makes no frame without a compositor either (ADR-0087). The sheet
  then sits over whatever the X server leaves behind the transparent window.
- **An Activity that unmaps or closes its window** (a game on focus loss):
  the frame is invalidated with the window by contract (§12.3, "window
  disappearance/remap"), so there is none to show.
- **A minimized Activity:** it stays in both inventories, and neither adapter
  reports the minimized state to the shell.

Showing the frame whenever one exists was rejected: a playing video would look
frozen, the frame arrives after the sheet, and a windowed Activity would jump to
a centred fit. Using the frame where the live window cannot show needs an
adapter-reported visibility state, which is a contract change for a later task.

**Launch progress.** The "Opening …" progress of a web service and the
"Starting …" progress of a slow application launch
([ADR-0110](../adr/0110-application-launch-lifetime-and-cancellation.md)) never
cover the sheet: opening the sheet hides them as Home does (`hideForHome`,
§6.5), and the launch keeps running. An open stop confirmation of an application
launch closes with it and stops nothing. Only the launch dialogs (failure or
timeout) are drawn above the sheet (§6.1: dialog › power › sheet). Home on such
a dialog replaces the sheet with Home too.

**Automatic ambient** does not count while the sheet is over an Activity: the
Activity is in the foreground behind it (§5.7). Over the shell it counts as
usual.

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
| Now playing | `MediaCatalog` Continue entries: the playing session, else a paused one | play/pause to that session | no session, no daemon, or the session lacks the action its state needs (the toggle, Pause while playing, Play while paused): it says "Cannot be paused from Arinn", or "Cannot be resumed from Arinn" while paused |
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
power scope, the launch dialogs (web service, then application) and ambient in
one canvas above the sheet, in that order, so the launch dialogs stay above the
power scope as before.

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
- `media-catalog` `quickMenuBackBeforeTheFirstStatusReturnsToThePlayer` (#507):
  over managed playback the window is on screen before the Status refresh
  that follows the request, so Back may come first. Until Status reports the
  Home state, Back uses the target that `QuickMenuRequested` carried; once
  it does, Status decides, also when it names no target (then Home).
- `arinn-quick-menu-test` (`quick-menu`): Main.qml with the real `Backend` and
  a fake `Daemon1` with `SystemControl1` on its own bus connection: Back over an
  Activity calls `Back` without showing Home, Open Home and `HomeRequested`
  replace the sheet, Back over Home returns focus to its opener, the Sound
  row's level and its `GetVolume`/`SetMuted`/`StepVolume`/`ToggleMuted` calls,
  Power over the sheet and Back to the Power row over an Activity and over the
  shell, Sleep (one `Suspend`, its refusal, Back to the Sleep row), a late
  refusal after Home inside the scope and after the global Home key, the
  quick-menu key right after Home, a shown but inactive shell counting as an
  Activity, ambient waking alike for Menu and a request, automatic ambient
  waiting over an Activity, the launch progress giving way to the sheet and
  Home on the launch dialog, the same for "Starting …" of an application with
  its stop confirmation open and the late power answer held meanwhile, repeats
  and a foreign epoch, the degraded form without the daemon, the session
  lock, and the request's Return target reaching the shell
  (`quickMenuRequestCarriesItsTarget`, #507).
- `arinn-platform-x11` `quick_menu_raises_the_shell_only_after_its_request`
  (Xvfb).
- `key_sequence` `a_configured_menu_key_stays_bound_inside_the_shell`;
  `global_home` `home_wins_a_key_the_quick_menu_action_still_holds`,
  `swapped_home_and_quick_menu_keys_follow_the_configuration`,
  `a_third_party_key_is_still_a_conflict`,
  `home_wins_home_page_the_quick_menu_action_still_holds`,
  `a_focus_change_waits_for_the_whole_write` and
  `the_stop_helper_releases_only_the_menu_keys` (#507).
- Not tested: real KDE and X11 sessions with the remote (including the stop
  helper's KGlobalAccel call on a real KWin), and a visual comparison (no
  handoff yet).
