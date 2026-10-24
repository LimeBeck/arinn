# ADR-0111: The remote's power key opens Arinn's power menu

<!-- SPDX-FileCopyrightText: 2026 Arinn contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

- **Status:** Accepted
- **Date:** 2026-10-24
- **Deciders:** project owner for the product behaviour (2026-10-21, [#481](https://github.com/LimeBeck/arinn/issues/481), specification §5.7 *Power button* merged in [#482](https://github.com/LimeBeck/arinn/pull/482): the ⏻ key opens Arinn's power menu while Arinn runs, Back goes to Home, no toggle, the key ends ambient, nothing happens on the lock screen, and the desktop's own power handling must not act on the key at the same time, arranged only while Arinn runs); XFCE (2026-10-24, [#481](https://github.com/LimeBeck/arinn/issues/481)): only in the dedicated Arinn XFCE session, xfce4-power-manager's power-button action is "do nothing" while Arinn runs and the user's value comes back when it stops; the ordinary XFCE desktop with opt-in autostart is not changed; the contract shape (2026-10-23, [#467](https://github.com/LimeBeck/arinn/issues/467#issuecomment-6360608652): a separate named request pair, no request kind); the mechanism is a delegated technical choice ([ADR-0037](0037-agent-autonomy-and-owner-gated-decisions.md))
- **Amends / Supersedes:** —
- **Amended by / Superseded by:** —
- **Specification:** [§5.7](../specifications/arinn-specification.md#57-system-ui-power-and-ambient), [§6.2](../specifications/arinn-specification.md#62-keys), [§6.3](../specifications/arinn-specification.md#63-home-and-back), [§12.1](../specifications/arinn-specification.md#121-global-home), [§13](../specifications/arinn-specification.md#13-security)

## Context

While Arinn runs, the remote's ⏻ key (evdev `KEY_POWER`, Qt `Key_PowerOff`, X11 `XF86PowerOff`) opens Arinn's power menu instead of the desktop's power dialog (§5.7 *Power button*). The platform layer owns the key as a global key, like Home. Over an Activity the shell comes up as for Home with the power menu open, and closing the menu leaves the shell on Home (§6.3). Pressing the key again keeps the menu open. During ambient the key ends ambient and opens the menu. While the session is locked it opens nothing. Every action in the menu keeps its confirmation and its inhibitor and permission reporting ([ADR-0108](0108-system-control.md)); the key itself never suspends or powers off.

The desktop's power handling must not act on the key at the same time, and Arinn may arrange that only while it runs, "through an inhibitor or a setting scoped to the running Arinn session", never by a permanent change of the user's desktop settings.

Three parties can react to `KEY_POWER`:

- **systemd-logind** reads the key from the input device itself and acts on it (`HandlePowerKey=`, `poweroff` by default) unless a process holds a `handle-power-key` block inhibitor.
- **Plasma PowerDevil** holds that inhibitor and reacts through two KGlobalAccel actions of the component `org_kde_powerdevil`: `PowerOff` (`Key_PowerOff`) and `PowerDown` (`Key_PowerDown`). On Wayland KWin turns a short press of `Key_PowerOff` into `Key_PowerOff` on release and a press held for one second into `Key_PowerDown`; on KDE/X11 KGlobalAccel sees `XF86PowerOff` directly. The action then runs the profile's *When power button pressed* choice (by default the logout screen). On the lock screen KWin lets `Key_PowerOff` through to global shortcuts.
- **xfce4-power-manager** grabs `XF86PowerOff` on the root window with `XGrabKey` and holds the logind inhibitor unless `logind-handle-power-key` is set.

The shell needs to know that a press is a power-key press and not Home. It must not infer that from `Status` or `Changed` ([ADR-0069](0069-explicit-home-presentation-requests.md)). [ADR-0106](0106-quick-menu-presentation.md) gave the quick-menu key its own request pair for the same reason and rejected a request kind; it records that the power key adds its own additive pair.

## Decision

**Contract.** `org.arinn.Daemon1` gains two members; every existing signature stays:

- `ReportPowerMenuRequest(target: s, output: s)` has the authentication and validation of `ReportHomeRequest` and `ReportQuickMenuRequest`: only the current platform adapter may call it, and both strings follow the ordinary-text bounds. It commits the same state as `ReportHomeRequest` (Home visible with that Return target and output), emits `Changed`, then emits one `PowerMenuRequested(epoch: s, target: s, output: s)`, also for identical repeats. After the signal it arms the Activity background capture exactly as `ReportHomeRequest` does ([ADR-0087](0087-bounded-activity-background-frames.md), §12.3): the power menu opens over Home, so the Activity's last frame belongs behind it. A rejected call changes no state, emits nothing and arms nothing.
- `PowerMenuRequested` is a transient presentation intent with the delivery rules of `HomeRequested`: it carries the daemon epoch and the request's opaque Return target and output, and it is never persisted, replayed to a late subscriber or synthesized from a state report. None of the three requests emits another's signal.

`Status` is unchanged. While the power menu is open the platform state is the Home state; whether the shell shows the power menu is shell-local state.

**Keys.** The power key is not configurable: §6.2 names no `config.toml` value for it, and `key_sequence::parse` does not accept the power key names, so no Home, quick-menu or other configured key can be a power key. The adapters bind every plausible code of the remote's ⏻, which is not captured yet: Qt `Key_PowerOff` and `Key_PowerDown` (KWin's long press) on KDE; X11 `XF86PowerOff` on X11/EWMH, where no long press is synthesized, and `XF86PowerDown` where the keymap has it.

**KDE.** The KWin script registers a third action, `ArinnPower` ("Arinn: Power menu"), without a default key. Its callback runs the Home path (unlocked-session check, remember the Return Activity and output, raise the shell) and reports through `ReportPowerMenuRequest`. When an adapter connection first polls `TakeCommand`, the daemon writes the keys after the Home and quick-menu keys (and after any later Arinn action), so an Arinn action that names a key first keeps it:

1. For each power key, the daemon asks KGlobalAccel which action holds it (`actionList`). An Arinn action keeps it, and so does any other third-party action: the key is left out and logged as `Conflict`, as for Home (ADR-0094).
2. A key held by PowerDevil's `PowerOff` or `PowerDown` is taken over for this KGlobalAccel run only: the daemon calls `setInactive` on that action. This marks the action not present, as KGlobalAccel does when an application quits. It writes nothing to `kglobalshortcutsrc`, and the user's keys for the action stay as they are.
3. The daemon writes the free and taken-over keys to `ArinnPower` with `setForeignShortcutKeys`, then reads them back. Any taken-over key that `ArinnPower` did not get (an older KGlobalAccel that refuses a key another action stores) gives PowerDevil's action back at once, so the desktop never loses the key to nothing.
4. While `ArinnPower` holds a key, the daemon holds a logind `handle-power-key` block inhibitor ("Arinn: the power key opens Arinn's power menu"), so logind never acts on the key either, also without PowerDevil. It is released when the daemon exits.

**Giving the key back.** PowerDevil's actions become present again through `setShortcutKeys` with `SetPresent` and autoloading, which restores the keys KGlobalAccel already stores and writes nothing. `ArinnPower`'s keys are cleared first, so the desktop is the only holder. `arinnd --release-power-key` does both. It runs as `ExecStopPost=` of `arinnd.service` (a clean stop, a crash, the start limit) and from `arinn-kwin-adapter stop`. A new adapter connection takes the key again. The restore is stateless: it names only PowerDevil's two actions, and making an action present that PowerDevil has not registered does nothing. Independently, KGlobalAccel forgets the inactive state when it restarts, so a new login always starts with the desktop's keys.

**X11/EWMH.** `arinn-platform-x11` grabs the power keys on the root window after the Home and quick-menu keys, also while the shell has focus. A power key that is also a Home or quick-menu key is left out, so they win. A press reports `ReportPowerMenuRequest` after the same unlocked check as Home and raises the shell as Home does. While the grab is held the adapter holds the same logind inhibitor; it is released when the grab is lost or the adapter exits. A key another client holds (`BadAccess`, for example xfce4-power-manager) is skipped and logged; it never stops the adapter, and the desktop keeps the key.

**XFCE, dedicated Arinn session only** (owner decision of 2026-10-24). When the adapter starts with `ARINN_SESSION_ENTRY=xfce` (the dedicated session of [ADR-0107](0107-dedicated-arinn-session.md)), it sets xfce4-power-manager's `/xfce4-power-manager/power-button-action` (xfconf channel `xfce4-power-manager`) to 0, "do nothing", through xfconf's D-Bus API. xfce4-power-manager keeps its X grab but no longer acts on the key, so the adapter reads that power key from XInput2 raw key presses (`XI_RawKeyPress` on the root window, XInput 2.1 or later; repeats ignored) instead of a grab, and holds the logind inhibitor as for a grab. The rules that keep the user's value:

- Before any change the user's value (a `uint` or `int`, or "not set") is written durably to `$XDG_STATE_HOME/arinn/xfce-power-button`. Without that copy nothing is changed; a value of another type, a locked property or a missing xfconf (neither running nor activatable) changes nothing either, and the key stays the desktop's.
- The copy is given back, and only then removed, on a clean stop of the adapter, from `ExecStopPost=` of `arinn-ewmh.service` (`arinn-platform-x11 --release-power-key`, which also covers a crash and the start limit), and at the next start of the adapter in any session. A start that finds a copy keeps it as the user's value and never reads Arinn's own "do nothing" as one.
- When the setting no longer holds "do nothing" at release time, the user changed it while Arinn ran; that value is kept and the copy dropped. A user value of "do nothing" needs no change and no copy.
- The ordinary XFCE desktop with opt-in autostart (no `ARINN_SESSION_ENTRY`) is never changed; there the desktop keeps the key and its dialog, and only a copy left by an earlier dedicated run is given back.

The desktop and Arinn never both act on one press.

**Shell.** The shell subscribes to `PowerMenuRequested` before it reads `Status` and handles a request only for the current daemon owner and epoch. It ignores the request while its session guard blocks it (locked, or the lock state unknown). It ends ambient without a consumed wake, then opens the power scope ([ADR-0108](0108-system-control.md)) over what it shows: over an Activity that is Home, as for the Home key. A quick menu over an Activity gives way to Home first, as for the Home key; a quick menu over the shell keeps the sheet under the power scope, as its own Power row does. An open power scope stays as it is, including a confirmation or a request in flight. Back and Cancel close the scope and leave the shell where the request put it.

## Consequences

- The power key reuses the Home presentation path: lock check, Return target, output and the Activity background. Only the request that names the intent is new.
- An old shell ignores `PowerMenuRequested`; with a new adapter the key then shows Home. An old daemon answers `UnknownMethod`, and the KWin script shows nothing. Adapter, daemon and shell are installed together.
- On KDE the desktop's power button does nothing on the lock screen while Arinn runs: Arinn opens nothing there (§5.7) and PowerDevil's action is inactive.
- A long press of the remote's ⏻ on KDE/Wayland opens the power menu too, instead of PowerDevil's long-press action.
- A PowerDevil restart or a tablet-mode change while Arinn runs registers PowerDevil's action again. KGlobalAccel then triggers the older of the two actions, normally PowerDevil's, until the adapter reconnects; Arinn and the desktop still never both act on one press.
- In the dedicated Arinn XFCE session the key opens Arinn's power menu; the user's xfconf value is out of effect only while Arinn runs there. A power loss leaves "do nothing" in xfconf until the adapter next starts (in any session with Arinn), which restores it; a login to the plain XFCE desktop in between still has "do nothing". In the ordinary XFCE desktop with xfce4-power-manager the desktop keeps its dialog. On an X11 desktop without a power manager Arinn owns the key and logind's default power-off is inhibited.
- A key Qt cannot name still cannot be a global shortcut; udev hwdb remapping remains the workaround (ADR-0094).

## Alternatives considered

- **A presentation kind on a generic request.** Rejected in ADR-0106 and on #467: the shell would dispatch on a string the daemon must still validate.
- **Infer the power menu from `Changed` or `Status`.** Forbidden by ADR-0069.
- **Move PowerDevil's key to `ArinnPower` with `setShortcutKeys` and restore it on exit.** Works on every KGlobalAccel version but writes the user's `kglobalshortcutsrc` twice per run, and a crash before the restore leaves the change in the user's configuration, also for the plain desktop session. `setInactive` reaches the same result without writing a setting.
- **Set PowerDevil's *When power button pressed* to *Do nothing* while Arinn runs.** Changes `powerdevilrc`, a user setting, and PowerDevil would still hold the key, so `ArinnPower` could not receive it.
- **Switch PowerDevil's KGlobalAccel component to another shortcut context.** Also session-only, but it disables every PowerDevil key (sleep, brightness), not only the power key.
- **Only a logind inhibitor.** Stops logind, not PowerDevil or xfce4-power-manager, which react to the key themselves.
- **Change xfce4-power-manager's `power-button-action` in every XFCE session Arinn runs in**, the opt-in autostart included. Rejected by the owner on 2026-10-24: the ordinary desktop keeps its dialog.
- **Set `logind-handle-power-key` instead.** The same persistence, and whether xfce4-power-manager then releases its X grab depends on its version.
- **Restore the value from the session wrapper** (`arinn-session`) as well, before a plain desktop login. Not done: the adapter's start and `ExecStopPost=` already cover every Arinn run, and the wrapper's desktop mode runs no Arinn code.
- **Listen without owning the key** (KGlobalAccel's `globalShortcutPressed` on PowerDevil's component, XInput2 raw events). The desktop would still act on the same press.

## Verification

- `runtime::window_tests::power_menu_requests_are_distinct_from_home_and_quick_menu_requests`: untrusted calls and an oversized target or output emit nothing; repeated requests each emit `Changed` and then `PowerMenuRequested` with the epoch, target and output and commit the Home state; no request emits another's signal; a late subscriber receives no history.
- `runtime::background::tests::power_menu_request_arms_the_activity_background`: the request arms the capture for its Return target and output, a rejected one arms nothing, and Home pressed afterwards on the same context keeps that attempt.
- `runtime::input_tests::build_info_reports_this_build_and_matches_the_xml` and `tests/integration/phase1.py` compare the live export with `daemon1.xml`.
- The KWin action, the KGlobalAccel takeover and release, the logind inhibitor, the X11 grab and the shell are not part of the contract pull request; they follow in [#481](https://github.com/LimeBeck/arinn/issues/481) with their tests (a fake KGlobalAccel and a fake logind on private buses, an Xvfb grab test, the KWin script tests and the production-shell tests).
- XFCE: `xfce_power` tests against a fake xfconf on a private bus (take and restore, a crash leaving the copy, an unset, int and "do nothing" value, a change the user made meanwhile, a locked property, a value of another type, an unreadable copy, no xfconf, the ordinary desktop) and the Xvfb test `held_power_key_is_read_from_raw_events_only_when_its_manager_does_nothing`, in the implementation pull request.
- Not tested: real KDE Plasma 6 and XFCE sessions with the remote (operator tests need the owner's permission).
