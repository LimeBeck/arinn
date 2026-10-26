<!-- SPDX-FileCopyrightText: 2026 Arinn contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Power menu and volume HUD

Shell side of [specification §5.7](../specifications/arinn-specification.md#57-system-ui-power-and-ambient)
(P-04, OQ-04) for [#148](https://github.com/LimeBeck/arinn/issues/148), and its
**Switch to desktop** entry for [#438](https://github.com/LimeBeck/arinn/issues/438)
([ADR-0107](../adr/0107-dedicated-arinn-session.md)). The
platform side is `SystemControl1` ([ADR-0108](../adr/0108-system-control.md),
[system control](system-control.md)). Implementing this does not meet AC-19:
real suspend, wake and the lock screen are owner operator tests.

## Pieces

| File | Role |
| --- | --- |
| `apps/arinn-shell/system_control.{h,cpp}` | `SystemControl` QML type. Calls `SystemControl1` asynchronously and exposes availability, inhibitors, whether the session can switch to the desktop, the request in flight, the accepted request, the refusal and the volume reply. The UI thread never waits for D-Bus |
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
`GetPower` and `GetSessionSwitch`.

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
| Restart, Shut down | Open the confirmation with Cancel focused. Its body lists the applications that will close and, in the error colour, the row's blocker or permission note, so confirming never hides it. Confirm calls `Reboot` or `PowerOff`; the pill is busy until the answer. See *Unsupported after opening* below. |
| Switch to desktop | See *Switch to desktop* below. The same confirmation, titled "Switch to desktop?", with one more line saying that the next login starts the desktop until Return to Arinn. Confirm (the "Switch" pill) calls `SwitchToDesktop`. |
| Accepted | A waiting pane: "Going to sleep…", "Restarting…", "Shutting down…" or "Switching to the desktop…" with "Waiting for the system". This is not success, and no timer produces it or anything after it. Back returns to the screen beneath. |
| Refused | An error pane titled for the action, with the reason for `inhibited` (the holders' *who (why)*), `denied`, `unsupported`, `unavailable`, `timeout` or `failed`. Back clears it and returns to the step that asked. |
| Ambient | Starts the shell's ambient screen over the menu (see *Ambient* below). Wake returns to this row. |
| Back, Home | Esc, Backspace and Back close the confirmation onto its opener and then the menu, one step per press: a held key's repeats are swallowed. Home closes the menu without any action and shows Home (§6.3). While a request is in flight (at most 20 s, the shell's power timeout), Back is ignored and Cancel (Restart, Shut down, Switch to desktop) is disabled until the system answers, so a sent request never looks cancelled. Home still leaves to Home and does not cancel it (§6.3, owner decision on 2026-10-23); see *Answer after Home* below. Each opening starts on the first row. |

**One request at a time.** The model sends one power request at a time
and ignores a second; Switch to desktop counts as one, as it shares the
daemon's power lock. While a request is in flight the other power rows
stay focusable but are held: they are dimmed, say "Available after the
system answers", and open no confirmation. The Ambient row is held as
well, so the answer is never shown behind the ambient screen.

**Unsupported after opening.** The menu opens before `GetPower` answers, so
a Restart or Shut down confirmation can be open while its action is still
`unknown`. If the read then reports it `unsupported`
([#497](https://github.com/LimeBeck/arinn/issues/497)):

- the confirmation stays open and its action pill is disabled, like the row,
  so the arrows, Tab, Return and a click cannot send the request;
- focus stays on Cancel, or moves there from the action pill;
- the confirmation's note says "Not supported on this computer" in the error
  colour, the row's own text;
- Cancel and Back close it onto the menu, where the row is disabled.

A request already sent goes on, and the pill stays busy until its answer.
Switch to desktop instead closes its confirmation when the entry goes (see
*Switch to desktop* below), and Sleep has no confirmation.

**Answer after Home.** When Home leaves the menu while a request is in
flight, the overlay remembers it. On success nothing more is shown: the
system suspends, restarts or powers off. A refusal or timeout that arrives
later reopens the power scope on the refusal pane, over whatever screen is
shown.

- Back on that pane closes it, and focus returns to the item the user was
  on. If that item is gone, focus goes to the screen's preferred item. Home
  on the pane goes to Home.
- While a launch overlay (a web service's or an application's) or the
  ambient screen is on screen (all are drawn above the power overlay), the
  refusal waits. It appears once they are gone, so it never takes focus
  behind them.
- If an external application is in front when the answer arrives, the pane
  appears when the user returns to Arinn. The shell does not raise itself
  over the application for it.
- The pane is used instead of the feedback strip. The strip holds one line
  for 3 s and is hidden over managed playback. The pane can show the
  holders and stays until the user dismisses it.
- Opening the menu again before the answer shows the request as it was
  left: the busy Sleep row, or the busy Restart, Shut down or Switch to
  desktop confirmation with Cancel disabled and Back ignored. The answer
  then appears in the menu as usual.

**Switch to desktop** (spec §5.7, [system control](system-control.md#switch-to-desktop)).

- The row exists only while `GetSessionSwitch` says `available`, which
  means Arinn runs as the dedicated Arinn session. `unsupported`, `unknown`,
  an error, no daemon and a daemon without the method hide it; a hidden row
  is skipped by the arrows and Tab. It sits after Shut down.
- If the value changes while the menu is open, the row appears or goes. Focus
  on it, or its open confirmation, moves to the first available row before
  anything is sent. A request already sent goes on.
- The confirmation is the one of Restart and Shut down: Cancel focused, the
  same live closing list. The daemon does not ask again.
- No inhibitor holds the switch, so the row shows no blocker note and a
  refusal names no holder. A refusal is titled "Could not switch to the
  desktop" and says that Arinn keeps running. The texts per code: `unavailable` (the
  desktop did not accept the logout or did not answer within the helper's
  5 s), `timeout`, `failed` (`Internal`), `unsupported`, `inhibited`
  (`Conflict`: another power request runs) and `denied`. After `unsupported`
  the model reads `GetSessionSwitch` again, so the row goes.
- The reopened menu, Home and the late refusal behave as for Restart.
- An accepted request means the desktop accepted the logout. The waiting
  pane stays until the session ends or the user presses Back. A logout the
  desktop accepts and an application cancels later is not detected; that is
  the open point of ADR-0107 (*Decision* 3) and is not settled here.

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

## Power key

The remote's ⏻ opens this menu too (spec §5.7 *Power button*,
[ADR-0111](../adr/0111-power-key.md), [#481](https://github.com/LimeBeck/arinn/issues/481)).
The platform owns the key globally and reports `Daemon1.ReportPowerMenuRequest`,
which commits the Home state and emits `PowerMenuRequested`. `Backend` passes the
request on for the current daemon epoch only, and `Main.openPowerFromKey()` acts
on it:

| Where | Result |
| --- | --- |
| Over an external Activity | The platform shows the shell as for Home; the power scope opens over Home, with the Activity's last frame behind it. Back and Cancel leave Home on screen with Return to the Activity; the Activity is not activated |
| Inside the shell | The scope opens over the current screen; Back returns focus to where it was |
| Quick menu over an Activity | The sheet gives way to Home, as for the Home key, and the scope opens over Home |
| Quick menu over the shell | The scope opens over the sheet, as from its Power row; Back returns to the sheet |
| A launch in progress | As for the quick menu: "Opening…" and "Starting…" give way to the scope without stopping the launch, a stop confirmation closes, and a failure or timeout dialog is drawn above the scope |
| Scope already open | Nothing changes: no toggle, the step and focus stay (a confirmation or a request in flight included) |
| Ambient | Ambient ends without a consumed wake, and the scope opens |
| Locked or unknown lock state | Nothing; the shell stays hidden |

The key itself never requests a power action; every action keeps its
confirmation and refusal rules above. Older KDE Plasma (before 6.7) and the
ordinary XFCE desktop keep the desktop's own dialog (owner decisions of
2026-10-25 and 2026-10-24); the dedicated Arinn XFCE session and other X11
desktops give the key to Arinn.

**The computer's own power button.** It sends the same `KEY_POWER`, so while
Arinn holds the key (a live shell, see [system control](system-control.md#power-key))
it also opens this menu, and on the lock screen it does nothing at all: Arinn
opens nothing there (§5.7), and the desktop's handling and logind's are set
aside. Only the firmware's long press (a hard power-off after about four
seconds) still works there. When the shell is gone, the desktop handles the
button again, on the lock screen too. The platform side is in
[system control](system-control.md#power-key).

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
  while ambient is already on, while a web service is launching or an
  application's starting screen is on screen (ADR-0110; a launch hidden by
  Home may run for hours and does not count), or while
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
  Esc/Backspace/Back, the explicit refusal, the Ambient row, geometry), and
  Switch to desktop: hidden unless available, its place, its confirmation
  with Cancel first and the live list, Back and Home, and the panel with five
  rows.
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
  rows, a Restart or Shut down confirmation whose action becomes
  unsupported after it opened (pill disabled, focus on Cancel, the note,
  Return sends nothing), Home, the volume keys and HUD, and the HUD geometry. For Switch to
  desktop: the row following `sessionSwitch` (focus and an open
  confirmation leave a row that goes), the request in flight (busy, Cancel
  disabled, Back ignored, the waiting pane), the power rows and the switch
  holding each other, the reopened menu, Home with a late refusal, and every
  refusal text.
- `apps/arinn-shell/system_control_test.cpp`: the model against a fake
  `SystemControl1` on a private bus: reader rules, no daemon, no acceptance
  without a reply, typed refusals and the inhibitor re-read, volume
  replies, the `GetSessionSwitch` reader rules, and `SwitchToDesktop` under
  the one-request rule in both directions with its typed refusals.
- `apps/arinn-shell/quick_menu_test.cpp` (`switchToDesktopFromTheSheet`):
  the production `Main.qml`; the quick menu's Power row opens the power scope
  over the sheet, which offers the entry only when the fake daemon says
  `available`, confirms with Cancel first, shows the refusal and walks back
  to the sheet's Power row.
- `apps/arinn-shell/quick_menu_test.cpp` (`powerKey*`): the production
  `Main.qml` and `Backend` against `PowerMenuRequested` from the fake daemon:
  over an Activity the menu opens over Home and Back leaves Home without a
  Return, a repeated key keeps the menu and its focus, another epoch is
  ignored, inside the shell Back returns to the opener, both quick-menu
  cases, a pending web or application launch (`powerKeyOverAPendingLaunch`),
  ambient ends and the menu opens, and nothing happens while locked.
- Real suspend, wake, the lock screen and volume on the HTPC are owner
  operator tests (AC-19). Not run.
- Switching to the desktop and back on the HTPC is the owner operator test
  of G-5.8. Not run.
