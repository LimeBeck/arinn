# System control

Normative contract for [ADR-0108](../adr/0108-system-control.md)
([#464](https://github.com/LimeBeck/arinn/issues/464)). The daemon executes the
power actions and volume changes of spec §5.7 for the shell. This document
states what the producer does and how the shell applies the replies.

## Interface

| Interface | Owner and path | Members |
| --- | --- | --- |
| `org.arinn.SystemControl1` | Daemon, `/org/arinn/Daemon1`; [system-control1.xml](../plugin-api/system-control1.xml) | `GetPower() -> (sssa(sss))`, `Suspend()`, `Reboot()`, `PowerOff()`, `GetVolume() -> (su)`, `SetMuted(b muted) -> (su)`, `ToggleMuted() -> (su)`, `StepVolume(i steps) -> (su)` |

Types, validation and the step arithmetic are in
`arinn_protocol::system_control`. There is no signal: the shell reads when it
needs a value.

### Power state

`(suspend, reboot, power_off, inhibitors)`:

| Field | Producer values | Reader rule |
| --- | --- | --- |
| `suspend`, `reboot`, `power_off` | `available`, `inhibited`, `denied`, `unsupported`, `unknown` | Any other value is `unknown` |
| `inhibitors` | At most 16 `(what, who, why)`; `what` is `sleep` or `shutdown`; `who` ≤ 128 and `why` ≤ 256 bytes, single line | An entry whose `what` it does not know is ignored |

The producer derives each action from logind's `CanSuspend`, `CanReboot` and
`CanPowerOff` and from `ListInhibitors`:

| Condition | Value |
| --- | --- |
| `Can*` is `na` | `unsupported` |
| A `block` inhibitor of the action's lock exists (`sleep` for suspend, `shutdown` for reboot and power off) | `inhibited` |
| `Can*` is `yes` | `available` |
| `Can*` is `no` or `challenge` | `denied` (Arinn never asks for authorization) |
| No system bus, no logind, a failed or timed-out read, or any other answer | `unknown` |

`inhibited` is published only with at least one inhibitor of that lock. With
more than 16 inhibitors, the first of each lock is kept, then the rest in
order up to the bound. `delay` inhibitors and other locks (`idle`,
`handle-*`) are not listed. User and process IDs are dropped.

### Power actions

`Suspend`, `Reboot` and `PowerOff` take no arguments and return nothing. The
producer:

1. refuses a caller that owns a registered provider (`PermissionDenied`);
2. runs one power request at a time;
3. lists the inhibitors and refuses with `Conflict` when a `block` inhibitor of
   the action's lock exists, without asking logind. A failed list does not
   refuse; logind still applies its own rules;
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
  `pactl set-sink-volume -- @DEFAULT_SINK@ ±N%`, so the balance stays. Then it
  reads. Stepping never changes the mute flag.
- The changes return the state read after the change. A missing `pactl`, a
  server that is not running or no default sink is `Unavailable`; a `pactl`
  that does not finish within 2 s is `Timeout`. Changes are serialized in
  arrival order.
- `SystemStatus1` is unchanged. Its mute flag follows the change through the
  sound server's events ([system status](system-status.md)).

## Callers

Only the shell calls this interface. A call from a connection that owns a
registered provider is refused with `PermissionDenied` for every method,
including the reads. This keeps system actions out of the plugin API; it is
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
- `platform/system_control_tests.rs`: a fake logind on a private bus and a fake
  `pactl`.
- `crates/arinnd/src/runtime/system_control_tests.rs`: the export against the
  XML, typed errors and the provider refusal.
- `tests/integration/phase1.py`: the real export and both reads; it never calls
  a power action or changes the volume.
- Real suspend, reboot, power off and volume on the HTPC: owner operator tests
  (AC-19). Not run.
