# System control

Normative contract for [ADR-0108](../adr/0108-system-control.md)
([#464](https://github.com/LimeBeck/arinn/issues/464)). The daemon executes the
power actions and volume changes of spec §5.7 for the shell. This document
states what the producer does and how the shell applies the replies.

## Interface

| Interface | Owner and path | Members |
| --- | --- | --- |
| `org.arinn.SystemControl1` | Daemon, `/org/arinn/Daemon1`; [system-control1.xml](../plugin-api/system-control1.xml) | `GetPower() -> (sssa(sss))`, `Suspend()`, `Reboot()`, `PowerOff()`, `GetVolume() -> (su)`, `SetMuted(b muted) -> (su)`, `ToggleMuted() -> (su)`, `StepVolume(i steps) -> (su)`, `GetSessionSwitch() -> s`, `SwitchToDesktop()` |

Types, validation and the step arithmetic are in
`arinn_protocol::system_control`. There is no signal: the shell reads when it
needs a value.

### Power state

`(suspend, reboot, power_off, inhibitors)`:

| Field | Producer values | Reader rule |
| --- | --- | --- |
| `suspend`, `reboot`, `power_off` | `available`, `inhibited`, `denied`, `unsupported`, `unknown` | Any other value is `unknown` |
| `inhibitors` | At most 16 `(what, who, why)`; `what` is `sleep` or `shutdown`; `who` ≤ 128 and `why` ≤ 256 bytes, single line (no control characters, no U+2028 or U+2029) | An entry whose `what` it does not know is ignored |

The producer derives each action from logind's `CanSuspend`, `CanReboot` and
`CanPowerOff` and from `ListInhibitors`:

| Condition | Value |
| --- | --- |
| `Can*` is `na` | `unsupported` |
| A `block` inhibitor of the action's lock exists (`sleep` for suspend, `shutdown` for reboot and power off) | `inhibited` |
| `Can*` is `yes` | `available` |
| `Can*` is `no` or `challenge` | `denied` (Arinn never asks for authorization) |
| No system bus, no logind, an inhibitor list that cannot be read (then every action), a failed or timed-out `Can*` read, or any other answer | `unknown` |

`inhibited` is published only with at least one inhibitor of that lock. Every
`ListInhibitors` record is inspected; only the published list is bounded. With
more than 16 inhibitors, the first of each lock is kept, then the rest in
logind's order up to the bound. `delay` inhibitors and other locks (`idle`,
`handle-*`) are not listed. User and process IDs are dropped.

### Power actions

`Suspend`, `Reboot` and `PowerOff` take no arguments and return nothing. The
producer:

1. refuses a caller that owns a registered provider (`PermissionDenied`);
2. runs one power request at a time and refuses a request that arrives while
   another runs with `Conflict`, without queueing it;
3. lists the inhibitors, inspecting every record, and refuses with `Conflict`
   when a `block` inhibitor of the action's lock exists, without asking
   logind. A list that cannot be read or times out refuses the action with
   `Unavailable` (fail closed);
4. calls logind's method with `interactive = false`, bounded by 10 s
   (`Timeout`).

A reply means logind accepted the request, not that the machine slept or
restarted. logind's refusals map to `org.arinn.Error1` names:

| logind error | Arinn error |
| --- | --- |
| `org.freedesktop.login1.BlockedByInhibitorLock`, `…login1.OperationInProgress` | `Conflict` |
| `…DBus.Error.InteractiveAuthorizationRequired`, `…DBus.Error.AccessDenied`, `…DBus.Error.AuthFailed` | `Conflict` when a block inhibitor of the lock exists now, otherwise `PermissionDenied` |
| `…login1.SleepVerbNotSupported`, `…DBus.Error.NotSupported`, `…DBus.Error.UnknownMethod` | `Unsupported` |
| `…DBus.Error.ServiceUnknown`, `…NameHasNoOwner`, `…UnknownObject`, `…Disconnected`, no system bus | `Unavailable` |
| `…DBus.Error.NoReply`, `…DBus.Error.Timeout` | `Timeout` |
| anything else | `Internal` |

### Volume

`(audio, level)`:

| Field | Producer values | Reader rule |
| --- | --- | --- |
| `audio` | `muted`, `unmuted`, `unknown` (as in ADR-0100) | Any other value is `unknown` |
| `level` | The loudest channel of the default sink in percent, 0–150; 0 while `audio` is `unknown` | Above 150 reads as 150; meaningless while `audio` is `unknown` |

- `GetVolume` reads `pactl get-sink-mute` and `pactl get-sink-volume` for
  `@DEFAULT_SINK@` with `LC_ALL=C`. Either read failing gives `unknown` with
  level 0. A read never fails the call.
- `SetMuted(muted)` and `ToggleMuted()` run `pactl set-sink-mute`, then read.
- `StepVolume(steps)` takes 1 to 20 steps either way (`InvalidArgument`
  otherwise), 5 points each. It reads the level, computes the target (never
  below 0, never above 100 when stepping up, and an existing level above 100
  is kept when stepping up) and moves every channel by the difference with
  `pactl set-sink-volume @DEFAULT_SINK@ ±N%`, so the balance stays. Then it
  reads. Stepping never changes the mute flag.
- The changes return the state read after the change. A missing `pactl`, a
  server that is not running or no default sink is `Unavailable`; a `pactl`
  that does not finish within 2 s is `Timeout`. Changes are serialized in
  arrival order.
- `SystemStatus1` is unchanged. Its mute flag follows the change through the
  sound server's events ([system status](system-status.md)).

### Switch to desktop

Spec §5.7, [#438](https://github.com/LimeBeck/arinn/issues/438). The
mechanism is the dedicated Arinn login session and its unprivileged switch
helper; see the [session lifecycle](session-lifecycle.md).

| Member | Producer | Reader rule |
| --- | --- | --- |
| `GetSessionSwitch() -> s` | `available` when the daemon runs in the dedicated Arinn session (its environment has `ARINN_SESSION_ENTRY=plasma` or `xfce`, exported by the session wrapper) and `/usr/lib/arinn/arinn-session-switch` is an executable file; `unsupported` otherwise | Any other value is `unknown`; the shell offers the action only for `available` |
| `SwitchToDesktop()` | Runs the helper as `arinn-session-switch desktop` with the entry in its environment, no other arguments and no shell, in its own process group, bounded by 10 s | A reply means the desktop accepted the logout request; the session then ends |

The daemon reads `ARINN_SESSION_ENTRY` once, when it starts. A daemon that
started before the wrapper exported it (for example through the opt-in
`arinn-session.target` of [ADR-0042](../adr/0042-opt-in-session-autostart.md),
which ADR-0107 says not to combine with the Arinn entry) reports
`unsupported` for that whole login: the switch is not offered, and nothing
else changes.

The helper records that the user's next login of the Arinn entry starts the
plain desktop, then asks the desktop to log out through its own session-bus
API. It bounds that call to 5 s itself (`busctl --timeout=5`), so a desktop
that does not answer is handled like a refusal and the previous choice is
restored before the daemon's limit. The daemon's 10 s limit is a backstop,
well below the shell's 20 s wait for a power reply. Errors:

| Condition | Arinn error |
| --- | --- |
| Not in the dedicated session, or the helper is not an installed executable file | `Unsupported` |
| The helper exits 1: no supported desktop, or the desktop refused the logout or did not answer within 5 s (the helper restored the previous choice) | `Unavailable` |
| The helper does not finish within 10 s | `Timeout` |
| The helper exits 3: the choice could not be recorded (no logout was requested), or 2 (usage), a signal, a wait or spawn failure | `Internal` |

On `Timeout` the daemon kills the helper's whole process group, including a
logout call still waiting, and reaps it before it replies. No logout call
is then still waiting on Arinn's side; a request the desktop already queued
may still be processed late. The power lock is released only after that.
The same holds for the helper's own 5 s bound: a desktop that freezes for
about that long may still log out after the helper restored the previous
choice and the shell showed the error. That is the fail-safe direction,
because the next login starts Arinn again. The helper may have been killed after it recorded `desktop` and
before it could restore the previous choice, so `Timeout` may leave `desktop`
recorded. That needs a helper stalled for 10 s outside its bounded logout
call (for example, on a hung state file system), and its effect is the
outcome the user asked for at the next login. The shell reports the error and
the session continues.

`SwitchToDesktop` counts as a power request: while one runs, another power request or switch is refused with `Conflict`, and vice versa. The
shell asks for confirmation first, with the live list of applications that
will close and Cancel as the default focus, exactly as for Restart and Shut
down; the daemon does not ask again. The helper's output is discarded; the log
line has the operation and error code only.

A logout the desktop accepts and an application cancels later (an unsaved-work
prompt) is not detected: `SwitchToDesktop` has already replied, the session
continues with `desktop` recorded, and the next login starts the plain desktop.
This is the open point of [ADR-0107](../adr/0107-dedicated-arinn-session.md)
(*Decision* 3). Neither this contract nor the shell's power-menu entry
([power menu](ui-power.md)) settles it; it stays open on
[#438](https://github.com/LimeBeck/arinn/issues/438), and either direction
keeps these members.

## Power key

The remote's ⏻ opens the power menu instead of the desktop's power dialog
while Arinn runs (spec §5.7 *Power button*, [ADR-0111](../adr/0111-power-key.md)).
The key never calls this interface by itself; the shell opens the menu and
every action goes through the methods above.

- **Only with a live shell.** Arinn holds the key only while the shell owns
  `org.arinn.Shell1` and the key is really read: on KDE while the adapter
  polls (a `TakeCommand` within 45 s), on X11 while the grab or raw reading
  works. Close Arinn, a shell crash loop at its start limit, a stopped KWin
  script or a failing reader give everything back at once; a returning shell
  takes it again.
- **KDE, Plasma 6.7 or later** (owner decision of 2026-10-25). The KWin action
  `ArinnPower` reports `ReportPowerMenuRequest` after the unlocked check. The
  daemon (`platform/kde/power_key.rs`) takes the logind inhibitor first, then
  gives `ArinnPower` `Key_PowerOff` and `Key_PowerDown` (KWin's long press),
  written after every configured key, and reads them back. Only when
  KGlobalAccel stored them (it stores one key for several actions from
  kglobalacceld 6.6.90, Plasma 6.7) does it mark PowerDevil's `PowerOff` and
  `PowerDown` KGlobalAccel actions not present for this KGlobalAccel run
  (`setInactive`; nothing about them is written to `kglobalshortcutsrc`).
  Before Plasma 6.7 KGlobalAccel refuses the keys: PowerDevil's actions are
  never touched, `ArinnPower` stays empty, the inhibitor is dropped, and the
  desktop's dialog stays (logged as `power-key` `Unsupported`). A key another
  action holds is left out. Giving the key back clears `ArinnPower`, makes
  PowerDevil's actions present again with the keys KGlobalAccel stores, and
  only then drops the inhibitor; the daemon does it when the shell or the
  adapter is lost, and `arinnd --release-power-key` does it as `ExecStopPost=`
  of `arinnd.service` and from `arinn-kwin-adapter stop`. When PowerDevil's
  bus name `org.kde.Solid.PowerManagement` gets a new owner (a restart), the
  daemon takes the key again at once and after 5 and 30 s, since PowerDevil
  registers its actions present again after it owns the name. Without the
  inhibitor nothing is taken, and the next attempt follows after 30 s. A
  failed `setInactive` or a write KGlobalAccel does not answer rolls the take
  back: `ArinnPower` is cleared, PowerDevil's actions are present, and the
  inhibitor goes once `ArinnPower` is known to be empty; the next attempt
  follows after 30 s. A release KGlobalAccel does not answer keeps the
  inhibitor and is tried again at the next check.
- **X11/EWMH** (`platform/x11/src/power_owner.rs`). The ordinary XFCE desktop
  keeps the key and its dialog. The adapter recognises it on the session bus,
  not from `XDG_CURRENT_DESKTOP`, which does not reach `arinn-ewmh.service` on
  every distribution: while `org.xfce.SessionManager` or xfce4-power-manager's
  `org.xfce.PowerManager` has an owner, only the dedicated Arinn XFCE session
  (`ARINN_SESSION_ENTRY=xfce`, set by its session wrapper) takes the key. Any
  other X11/EWMH desktop takes it. Both names are checked with the shell's on
  every loop (500 ms), so an XFCE name that appears later gives the key back.
  With a live shell the adapter takes the logind inhibitor, then grabs
  `XF86PowerOff` (and `XF86PowerDown` where mapped). A key another client
  holds stays that client's.
- **XFCE, dedicated Arinn session only** (owner decision of 2026-10-24). With
  `ARINN_SESSION_ENTRY=xfce` the adapter reads the key xfce4-power-manager
  grabs from XInput2 raw events, and only once that works sets
  xfce4-power-manager's power-button action to "do nothing" through xfconf
  (`platform/x11/src/xfce_power.rs`). The user's value (or "not set", when the
  user's own channel file does not name it) is kept in
  `$XDG_STATE_HOME/arinn/xfce-power-button` before the change and restored
  when the shell is lost, on a clean stop, by `arinn-platform-x11
  --release-power-key` (`ExecStopPost=` of `arinn-ewmh.service`), at the next
  start in any X11/EWMH session, and at every XFCE login by
  `/etc/xdg/autostart/org.arinn.PowerButtonRestore.desktop`
  (`--restore-power-button`, a no-op while the adapter owns `org.arinn.X11`).
  A take first gives a stale copy back; a value the user set meanwhile is kept.
  With the copy, the adapter also writes a per-user XDG autostart entry,
  `$XDG_CONFIG_HOME/autostart/arinn-power-button-restore.desktop`
  (`OnlyShowIn=XFCE;`), and the `sh` script next to it that holds the value.
  It uses only `sh` and `xfconf-query`, so the restore survives removing the
  package after an unclean end: at the next XFCE login, once the adapter
  binary that wrote it (its own path, not only `/usr/bin`) is gone, it gives
  the value back (unless the user chose another one since) and removes itself
  and the copy. While that binary exists it does nothing, a value it cannot
  read is left for the next login, and every release removes it. Without the
  entry nothing is changed. The entry goes to the user manager's
  `$XDG_CONFIG_HOME`; an `XDG_CONFIG_HOME` set only in the login environment
  puts it where XFCE does not look, and then only the system entry restores.
- **logind.** While Arinn holds the key (the daemon on KDE, the adapter on
  X11) it holds a `handle-power-key` block inhibitor (`platform/power_inhibitor.rs`),
  taken before the key, so logind's own `HandlePowerKey=` never runs next to
  the menu. The lock ends with the key or the process. `GetPower` never lists
  `handle-power-key` inhibitors, so this one never blocks Sleep, Restart or
  Shut down.

Tests: `platform/kde/power_key.rs` (a fake KGlobalAccel with shared keys and a
fake logind on a private bus: takeover without a write, KGlobalAccel before
Plasma 6.7, a third-party key, no PowerDevil, no KGlobalAccel, no inhibitor
and its retry, a reconnect without a key, every key held by other actions,
a failed `setInactive` or write and an unanswered release with their
retries, the release with the inhibitor dropped last, a PowerDevil restart,
and the key following a live shell and adapter),
`platform/power_inhibitor.rs`, `power_owner` (a fake reader, xfconf and logind:
a shell exit, a crash loop, a reader that never works or stops, no inhibitor, a
grab, the ordinary XFCE desktop, another desktop until xfce4-power-manager
appears, a reading that moves from a grab to raw events, XFCE recognised by
its bus names), `xfce_power` (a fake xfconf: stale copies, a
newer user choice, a system default, the login restore, and the per-user entry
run with a fake `xfconf-query` after Arinn is removed), `an_ended_session_gives_the_power_key_back`,
`tests/package/test_xfce_session.py` (the opt-in bridge starts only with the
XFCE session manager's name owned and never sets the marker), the Xvfb tests
`power_key_is_reported_apart_from_home_and_the_quick_menu`,
`held_power_key_is_read_from_raw_events_only_when_its_manager_does_nothing` and
`power_key_opens_the_power_menu_only_when_unlocked`, `tests/ci/test_power_button_restore.py`,
and `tests/integration/kwin_adapter.test.cjs`. Real KDE and XFCE sessions with
the remote are owner operator tests (G-5.5); not run. The power-key checklist
for them:

- **One press, one action** on Plasma 6.7 or later: a short and a long press
  of ⏻ open only Arinn's power menu, never PowerDevil's logout screen as
  well, also right after Arinn starts and after PowerDevil restarts; with
  the shell closed, only PowerDevil acts.
- After a PowerDevil settings reload (for example a changed *When power
  button pressed*), note whether PowerDevil keeps the key until the adapter
  reconnects; it must never act together with Arinn.
- Plasma before 6.7: the desktop's dialog only, and `kglobalshortcutsrc`
  unchanged apart from Arinn's own entries.
- `kglobalshortcutsrc` after a logout during an Arinn session, and the next
  login with and without Arinn.
- XFCE: the dedicated session (Arinn's menu only), the ordinary desktop (its
  own dialog only), and a restore after a power cut.

## Callers

Only the shell calls this interface. A call from a connection that owns a
registered provider is refused with `PermissionDenied` for every method,
including the reads and the session switch. This keeps system actions out of the plugin API; it is
not a sandbox (spec §13: plugins are trusted local software, and any process
of the user can reach logind and the sound server itself).

## Shell rules

- Calls are asynchronous; the UI thread never waits for them (§8.2).
- The power menu reads `GetPower` when it opens. `unsupported` actions are
  disabled. `inhibited` and `denied` stay focusable and explain why; the
  request still goes to the daemon, which refuses it with the typed error.
  `unknown` behaves as `available`.
- The power menu also reads `GetSessionSwitch` when it opens and shows
  **Switch to desktop** only for `available`. A refused switch is an error
  pane and the session continues; no inhibitor is named for it, and after
  `Unsupported` the shell reads `GetSessionSwitch` again
  ([power menu](ui-power.md)).
- After `Conflict` on a power action the shell reads `GetPower` again and shows the holders'
  `who` and `why`. Other errors show their own text. An accepted request
  never shows success by itself: the session lifecycle guard hides the shell
  before sleep, and the shell treats a wake as described in the
  [session lifecycle](session-lifecycle.md).
- The volume HUD shows the `(audio, level)` of the reply to the change the
  user made. An error shows nothing new and never a guessed level.

## Privacy

Logs carry `system-<operation>` and the error code only (spec §13). Inhibitor
texts, device names and levels are never logged or sent as telemetry. The
shell shows inhibitor texts on screen only.

## Verification

- `crates/arinn-protocol/src/system_control.rs` unit tests.
- `crates/arinn-protocol/tests/system_control.rs`: a fixture daemon on a private
  bus; a daemon without the interface, the live export against the XML, typed
  refusals, volume replies and the session switch on the wire.
- `platform/system_control_tests.rs`: the logind and `pactl` mappings, a fake
  logind on a private bus (`Can*` answers, block and delay inhibitors behind
  hundreds of irrelevant records, an unreadable list that refuses, the bounded list, refusals,
  `interactive = false`, no logind, no bus) and a fake `pactl` (mute, toggle,
  steps with balance, the 100 % stop, a level above 100 %, a change whose
  read-back fails, no server, no tool, the C locale).
- `crates/arinnd/src/runtime/system_control_tests.rs`: the daemon export
  against the XML, typed errors on the wire, a second power request refused
  while the first runs, and the refusal of a provider connection for every
  method, before its arguments are checked.
- `tests/integration/phase1.py`: the real daemon's export against the XML and
  both reads; it never calls a power action or changes the volume.
- `platform/system_control_tests.rs` and the daemon tests also run a fake
  `arinn-session-switch`: unsupported outside the session, the fixed call,
  exit codes, the timeout (a child the helper started is dead afterwards),
  the provider refusal, and the shared power lock in both directions.
- `tests/package/test_session_entry.py`: the helper's bounded logout call, its
  restore when the desktop does not answer, and exit 3 for an unrecorded
  choice.
- Real suspend, reboot, power off and volume on the HTPC: owner operator tests
  (AC-19). Not run.
- Switching to the desktop and back on the HTPC: owner operator test (G-5.8).
  Not run.
