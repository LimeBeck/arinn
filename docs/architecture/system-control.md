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
| `SwitchToDesktop()` | Runs the helper as `arinn-session-switch desktop` with the entry in its environment, no other arguments and no shell, bounded by 10 s | A reply means the desktop accepted the logout request; the session then ends |

The helper records that the user's next login of the Arinn entry starts the
plain desktop, then asks the desktop to log out through its own session-bus
API. Errors:

| Condition | Arinn error |
| --- | --- |
| Not in the dedicated session, or the helper is not an installed executable file | `Unsupported` |
| The helper exits 1: no supported desktop, or the desktop refused the logout (the helper restored the previous choice) | `Unavailable` |
| The helper does not finish within 10 s (it is killed) | `Timeout` |
| Any other exit or spawn failure | `Internal` |

`SwitchToDesktop` counts as a power request: while one runs, another power request or switch is refused with `Conflict`, and vice versa. The
shell asks for confirmation first, with the live list of applications that
will close and Cancel as the default focus, exactly as for Restart and Shut
down; the daemon does not ask again. The helper's output is discarded; the log
line has the operation and error code only.

A logout the desktop accepts and an application cancels later (an unsaved-work
prompt) is not detected: `SwitchToDesktop` has already replied, the session
continues with `desktop` recorded, and the next login starts the plain desktop.
This is the open point of [ADR-0107](../adr/0107-dedicated-arinn-session.md)
(*Decision* 3). This contract does not settle it; the power-menu follow-up
does, and either direction there keeps these members.

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
- After `Conflict` the shell reads `GetPower` again and shows the holders'
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
  exit codes, the timeout and the provider refusal.
- Real suspend, reboot, power off and volume on the HTPC: owner operator tests
  (AC-19). Not run.
- Switching to the desktop and back on the HTPC: owner operator test (G-5.8).
  Not run.
