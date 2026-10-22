<!-- SPDX-FileCopyrightText: 2026 Arinn contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Power menu and volume HUD

Shell side of [specification §5.7](../specifications/arinn-specification.md#57-system-ui-power-and-ambient)
(P-04, OQ-04) for [#148](https://github.com/LimeBeck/arinn/issues/148). The
platform side is `SystemControl1` ([ADR-0108](../adr/0108-system-control.md),
[system control](system-control.md)). Implementing this does not meet AC-19:
real suspend, wake and the lock screen are owner operator tests.

## Pieces

| File | Role |
| --- | --- |
| `apps/arinn-shell/system_control.{h,cpp}` | `SystemControl` QML type. Calls `SystemControl1` asynchronously and exposes availability, inhibitors, the request in flight, the accepted request, the refusal and the volume reply. The UI thread never waits for D-Bus |
| `apps/arinn-shell/ui/screens/power/PowerMenu.qml` | The power scope: menu, confirmation, refusal and accepted panes, and the ambient screen. Presentation only |
| `apps/arinn-shell/ui/screens/power/PowerOverlay.qml` | Maps a `SystemControl` (or a test double) to `PowerMenu`, owns the volume keys and the HUD |
| `apps/arinn-shell/ui/VolumeHud.qml` | The handoff's volume HUD |
| `apps/arinn-shell/ui/screens/power/AmbientScreen.qml` | The ambient screen (handoff 20) and its consumed wake |
| `apps/arinn-shell/ui/screens/power/AmbientIdle.qml` | The idle timer and the rules that inhibit automatic ambient |
| `apps/arinn-shell/Main.qml` | Opens the overlay from global navigation › Power and gives it the power focus scope |

## Power menu

Global navigation › **Power** opens the menu over the current screen. The menu
is the *power* focus scope of §6.1: `Main.focusedScreen` is `power` while it is
open, so every screen beneath is inert, and closing it returns focus to the
opener (the navigation entry). Opening it clears any old answer and reads
`GetPower`.

The look follows handoff 15: a full-screen scrim at `rgba(0, 0, 0, 0.55)`, a
centered glass panel 800 wide with radius 48, a 52/700 title, rows 112 tall
with radius 30, a 64 px icon plate, a 32 px label and a 26 px description.
Restart and Shut down replace the rows with two 96 px pills; Cancel has the
default focus and the action pill is destructive.

| Row state (`GetPower`) | Row |
| --- | --- |
| `available`, `unknown` | Normal description |
| `inhibited` | "Blocked by *who (why)*", from the block inhibitors of the row's lock; still selectable |
| `denied` | "Not allowed for this user"; still selectable |
| `unsupported` | "Not supported on this computer"; disabled and skipped by the arrows and Tab |

Inhibited and denied rows stay selectable so the platform decides and the user
sees its refusal; the menu never guesses the outcome.

| Action | Behaviour |
| --- | --- |
| Sleep | Calls `Suspend`. While the request is in flight the row is busy and says "Waiting for the system…". |
| Restart, Shut down | Open the confirmation with Cancel focused. Its body lists the applications that will close and, in the error colour, the row's blocker or permission note, so confirming never hides it. Confirm calls `Reboot` or `PowerOff`; the pill is busy until the answer. |
| Accepted | A waiting pane: "Going to sleep…", "Restarting…" or "Shutting down…" with "Waiting for the system". This is not success, and no timer produces it or anything after it. Back returns to the screen beneath. |
| Refused | An error pane titled for the action, with the reason for `inhibited` (the holders' *who (why)*), `denied`, `unsupported`, `unavailable`, `timeout` or `failed`. Back clears it and returns to the step that asked. |
| Ambient | Starts the shell's ambient screen over the menu (see *Ambient* below). Wake returns to this row. |
| Back, Home | Esc, Backspace and Back close the confirmation onto its opener and then the menu, one step per press: a held key's repeats are swallowed. Home closes the menu without any action and shows Home (§6.3). Each opening starts on the first row. |

**Closing list.** The confirmation names each running application from the
live registry (`Backend.items`, the running entries of the application
providers) once. While the registry cannot be read (no daemon), it says that
the list is not available instead of "No running applications".

**Sleep and wake.** The shell does not decide that the computer slept. The
session lifecycle guard (ADR-0041) hides the shell before sleep and on lock.
When it does so after an accepted request, the menu closes; after wake the
shell shows the screen and Activity context it kept, and the daemon reconnects
providers as before. A suspend that never happens leaves the waiting pane until
the user presses Back.

## Volume keys and HUD

While the shell window is on screen and active and the session is not locked,
Volume Up, Volume Down and Mute are window shortcuts: they work over every
screen, including managed playback. Volume Up first turns the sound back on
(`SetMuted(false)`) and then raises the level by one step (5 %); Volume Down
lowers it by one step; Mute toggles. Volume Up unmuting is the owner's answer
of 2026-10-21 on #477. The steps go through `SystemControl1`. Outside the shell Plasma keeps them
(OQ-04).

The HUD follows the handoff (*Toast and volume HUD*): a top-center glass pill
560×88, a 40 px icon (volume or muted), an 8 px white bar and the value at
30/600. It hides 1.8 s after the last change and never takes focus. It shows
only a level the daemon returned for the change; a failed change shows no
level: the HUD itself says "Volume control is not available", so the failure
shows over managed playback too. A failed unmute does not go on to change the
level. The bar is dimmed while muted. Volume calls run one at a time in the
order of the keys, so a quick second key never overtakes a Volume Up that is
still unmuting; at most eight pending calls are kept and further presses are
dropped.

## Ambient

Spec §5.7 and [#307](https://github.com/LimeBeck/arinn/issues/307), P-04:
ambient is a screensaver, not sleep.

- **Start.** The power menu's Ambient row, or `AmbientIdle` after ten minutes
  without a key or genuine pointer movement in the shell (the
  `InputModeSource` events of ADR-0095). The timeout is the Settings ›
  Appearance › *Ambient after* setting (5, 10, 15, 30 or 60 minutes, 10 by
  default; [ADR-0112](../adr/0112-shell-preferences-as-core-settings.md)):
  `Main.qml` binds `AmbientIdle.timeout` to
  `SettingsCatalog.ambientTimeoutMinutes`, so a change applies at once.
- **No automatic start** while the shell is hidden or its window inactive
  (an external Activity is in the foreground), while the session is locked,
  while ambient is already on, while a web service is launching, or while
  media plays: an MPRIS session in `playing`, managed playback or its input
  layer, or an audible browser tab. Losing any of these conditions stops the
  count; regaining them starts a full timeout.
- **Look.** Handoff 20: `bg-deep` (#050506), the clock at 160/500 and the
  date at 32 in dim greys, as one block that moves to a random place every
  8 s with a 4 s ease-in-out move. The optional paused-media line of the
  handoff is not shown.
- **Wake.** Ambient is the topmost route (`focusedScreen` is `ambient`) and
  takes every key and pointer event. The event that opened it is not a wake.
  After that, a key press and its release, a click, the wheel or a pointer
  movement away from where the pointer rested wakes it, and that event is
  consumed. The item that had focus before gets it back, or the screen's
  preferred item when it is gone. The volume keys are not Arinn's while
  ambient is on, so they wake it too.
- **Never more.** Ambient never pauses playback, closes applications or
  requests suspend; it only draws. A lock or sleep ends it without a wake
  event, and the session lifecycle guard keeps the shell hidden until it is
  safe to show.

## Verification

```sh
QT_QPA_PLATFORM=offscreen QT_QUICK_BACKEND=software \
  build/apps/arinn-shell/arinn-ui-test -input tests/ui
dbus-run-session -- build/apps/arinn-shell/arinn-system-control-test
```

- `tests/ui/tst_power.qml`: the menu itself (focus scope, Cancel default,
  Esc/Backspace/Back, the explicit refusal, the Ambient row, geometry).
- `tests/ui/tst_ambient.qml`: the ambient screen (handoff look and drift,
  consumed first key, opening key not a wake, pointer anchor, consumed click,
  stop without wake) and the idle timer on a shortened timeout (default ten
  minutes, reset by activity, each inhibiting condition, hidden shell, what
  counts as playing).
- `platform/session/sessionlifecycle_test.cpp` (`powerMenuAcrossSleep`,
  `ambientAfterIdle`): the production `Main.qml` with fake login1 and
  screensaver services: the explicit Ambient row and wake back to it, idle
  ambient on Home with the wake key consumed and focus restored, and a lock
  that ends ambient.
- `tests/ui/tst_power_overlay.qml`: the overlay over a `SystemControl` double:
  reading on open, a request that waits for the platform, the accepted pane,
  closing on the lifecycle guard, refusal texts and inhibitor names, the
  confirmation's blocker note and live closing list, disabled unsupported
  rows, Home, the volume keys and HUD, and the HUD geometry.
- `apps/arinn-shell/system_control_test.cpp`: the model against a fake
  `SystemControl1` on a private bus: reader rules, no daemon, no acceptance
  without a reply, typed refusals and the inhibitor re-read, and volume
  replies.
- Real suspend, wake, the lock screen and volume on the HTPC are owner
  operator tests (AC-19). Not run.
