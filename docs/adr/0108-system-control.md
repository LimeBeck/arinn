# ADR-0108: Power and volume actions through the daemon

- **Status:** Accepted
- **Date:** 2026-10-21
- **Deciders:** Delegated technical choice ([#464](https://github.com/LimeBeck/arinn/issues/464)); the product behaviour is spec §5.7 and P-04 as decided by the owner
- **Amends / Supersedes:** —
- **Amended by / Superseded by:** —
- **Specification:** §5.7, §8.2, §8.5, §8.6, §13; P-04; AC-19; R-01, R-05

## Context

Spec §5.7 gives the shell a power menu and, while the shell is visible, the
media and volume keys with Arinn's own volume HUD (OQ-04, decided on
2026-09-28). Sleep calls the platform's system suspend and respects
inhibitors and permissions (P-04). A refusal is shown, and the UI never
reports a successful sleep from a timer. Restart and Shut down never hide
inhibitor or permission errors. §8.5 assigns volume, network and power to the
platform layer, and the shell never talks to system services itself (§8.2,
R-05).

Today nothing executes these actions. `SystemStatus1`
([ADR-0100](0100-system-status.md)) is read-only and carries no volume level.
The power overlay ([ui-power.md](../architecture/ui-power.md)) is a fixture
with nothing to call ([#148](https://github.com/LimeBeck/arinn/issues/148)).

The session lifecycle guard ([ADR-0041](0041-independent-session-lifecycle-guard.md))
observes lock and sleep and never executes power commands. That stays as it is.

## Decision

The daemon exports **`org.arinn.SystemControl1`** at `/org/arinn/Daemon1`
([system-control1.xml](../plugin-api/system-control1.xml)). Types and
validation are in `arinn_protocol::system_control`; the normative rules are in
the [system control contract](../architecture/system-control.md).

**Power.**

- `GetPower() -> (sssa(sss))`: `(suspend, reboot, power_off, inhibitors)`.
  Each action is `available`, `inhibited`, `denied`, `unsupported` or
  `unknown`. `inhibitors` lists the `block` inhibitors of the `sleep` and
  `shutdown` locks as `(what, who, why)`, at most 16, with single-line texts of
  at most 128 and 256 bytes. It is read on every call: logind announces no
  inhibitor changes.
- `Suspend()`, `Reboot()`, `PowerOff()` call logind's `Suspend`, `Reboot` and
  `PowerOff` on the system bus with `interactive = false`. A reply means only
  that logind accepted the request. The shell learns that the machine sleeps
  from the session lifecycle guard, never from this reply or from a timer.
- Arinn **respects every block inhibitor**. Before it asks logind, the daemon
  lists the inhibitors and refuses an action that a `block` inhibitor of its
  lock holds, even when the user could override it. It inspects every record
  and bounds only the list it publishes. When the list cannot be read, the
  action is refused with `Unavailable` and `GetPower` reports every action
  `unknown`: Arinn fails closed rather than risk overriding an inhibitor.
  `delay` inhibitors only delay and do not refuse.
- Arinn never asks for authorization. A `challenge` answer is `denied`, and an
  action that needs authorization is refused.
- Refusals use the existing `org.arinn.Error1` names:

  | Error | Meaning |
  | --- | --- |
  | `Conflict` | A block inhibitor holds the action, or another power operation is in progress. `GetPower` names the holder |
  | `PermissionDenied` | logind or polkit refused it for this user, or the caller is a provider |
  | `Unsupported` | The system cannot do it (`na`, sleep not supported) |
  | `Unavailable` | No system bus, no logind, or an inhibitor list that cannot be read |
  | `Timeout` | logind did not answer within 10 s |
  | `Internal` | Any other failure |

  logind's own error names map as listed in the contract. A permission refusal
  while a block inhibitor exists is reported as `Conflict`, because overriding
  the inhibitor is what was refused. One power request runs at a time; a
  request that arrives while another runs is refused with `Conflict` and is
  never queued.

**Volume.**

- `GetVolume() -> (su)`, `SetMuted(b muted) -> (su)`, `ToggleMuted() -> (su)`
  and `StepVolume(i steps) -> (su)` act on the default sink through the
  PulseAudio protocol (`pactl`, served by `pipewire-pulse` on Plasma 6), the
  source `SystemStatus1` already reads.
- The reply `(audio, level)` is the state after the change: `audio` is
  `muted`, `unmuted` or `unknown` as in ADR-0100, and `level` is the loudest
  channel in percent, at most 150, 0 when `audio` is `unknown`. The shell's
  volume HUD shows it.
- `StepVolume` takes 1–20 steps in either direction, 5 points each. It moves
  every channel by the same amount, so the balance stays. Stepping up stops
  at 100 % and never raises a level that is already above 100 %. Stepping
  never changes the mute flag.
- Volume changes apply in the order they arrive. A missing `pactl`, server or
  default sink is `Unavailable`.
- `SystemStatus1` does not change. Its mute flag follows through the sound
  server's own change events.

**Callers.** The interface serves the shell. A call from a bus
connection that owns a registered provider is refused with
`PermissionDenied`, so plugins never execute system actions (§13). This is
not a sandbox: plugins are trusted local software (§13), and any process of
the user can reach logind and the sound server with the same rights. It keeps
the plugin API free of system actions, and no SDK binding exposes them.

**Platform layer.** The logind and `pactl` code is in
`platform/system_control.rs` (R-05). The daemon export
(`crates/arinnd/src/runtime/system_control.rs`) checks the caller, serializes
and logs outcomes. Both services are freedesktop services, so every platform
backend uses them, as ADR-0100 does.

**Privacy.** Logs carry the operation and the error code only. Inhibitor texts,
device names and levels are never logged (§13). Inhibitor user and process IDs
never leave the platform layer.

## Consequences

- The shell can wire the power menu, Sleep availability and the volume HUD
  (#148) without any system dependency of its own. The quick menu (#439) can
  reuse the same volume methods.
- The shell consumer (power menu, volume HUD) follows in
  [#148](https://github.com/LimeBeck/arinn/issues/148).
- The daemon depends at runtime on logind and a PulseAudio-protocol server when
  they exist. Neither is needed to start; without them reads are `unknown` and
  actions are `Unavailable`.
- A later action, such as "Switch to desktop" (#438), is an additive method
  here or its own interface; neither changes these signatures.
- Real suspend, reboot and volume behaviour still need the owner's operator
  tests on the HTPC (AC-19). This contract does not verify them.

## Alternatives considered

- **The shell calls logind and the sound server directly:** it breaks §8.2 and
  R-05 and puts platform code in the shell. The session lifecycle guard is a
  narrow observation exception (ADR-0041) and must not execute power commands.
- **Interactive authorization:** a polkit dialog over a TV, answered with a
  remote, is not the product. A refusal with its reason is.
- **Let logind decide about inhibitors alone:** logind lets a user with the
  `*-ignore-inhibit` permission override a block inhibitor without a prompt,
  and depending on its version it can skip block inhibitors that the
  caller's own user holds. Arinn would then
  silently break "respects inhibitors" (P-04).
- **Interface-specific error names** (`…SystemControl1.Error.Inhibited`): the
  `org.arinn.Error1` names already cover each case, the shell already maps
  them, and a separate domain adds names without adding information.
- **Volume level in `SystemStatus1`:** it would change ADR-0100's signature or
  need a new version, and the HUD needs the state right after its own change,
  which a change signal cannot promise.
- **Absolute volume (`N%`) for steps:** it resets the balance between
  channels. Relative changes keep it.
- **A cached power state with change signals:** logind has no inhibitor signal,
  so a cache would be stale or polled. The shell reads when the menu opens.

## Verification

- `crates/arinn-protocol/src/system_control.rs`: signatures, roundtrip,
  producer validation, reader fallbacks and step arithmetic.
- `crates/arinn-protocol/tests/system_control.rs`: a fixture daemon on a
  private bus. It covers a daemon without the interface, compares the live
  export with the XML, and covers typed refusals and volume replies on the
  wire.
- `platform/system_control_tests.rs`: a fake logind on a private bus (`Can*`
  answers, block and delay inhibitors, one block behind hundreds of
  irrelevant records, an unreadable list that refuses, a bounded inhibitor
  list, refusals, `interactive = false`, no logind, no bus) and a fake `pactl`
  (mute, toggle, steps with balance, the 100 % stop, a level above 100 %, a
  change whose read-back fails, no server, no tool, the C locale).
- `crates/arinnd/src/runtime/system_control_tests.rs`: the daemon export
  against the XML, typed errors on the wire, a second power request refused
  while the first runs, and the refusal of a provider connection.
- `tests/integration/phase1.py`: the real export against the XML and both
  reads. It never calls a power action or changes the volume.
- Real suspend, reboot, power off and volume on the HTPC are owner operator
  tests (AC-19). Not run.
