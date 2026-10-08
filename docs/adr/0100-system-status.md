# ADR-0100: Read-only system status for Home's status area

- **Status:** Accepted
- **Date:** 2026-10-08
- **Deciders:** Owner (scope of [#387](https://github.com/LimeBeck/arinn/issues/387): mute and Wi-Fi from platform state, unknown shows nothing); delegated technical choice for the contract and the sources
- **Amends / Supersedes:** —
- **Amended by / Superseded by:** —
- **Specification:** §5.1, §8.2, §8.5, §13; R-01, R-05

## Context

Handoff 01 puts two icons between the download chip and the clock in Home's
status area: a mute icon, shown only while the sound is muted, and a Wi-Fi
icon. Spec §8.5 assigns volume and network to the platform layer. The shell
must not talk to a sound server or NetworkManager itself (§8.2, R-05), and its
UI thread never makes blocking D-Bus calls. No daemon interface exposed either
state ([#387](https://github.com/LimeBeck/arinn/issues/387)).

Either state can be unavailable: a session without NetworkManager, a sound
server that is not running yet, or a platform without a producer. Showing a
guess on a TV is worse than showing nothing.

## Decision

The daemon exports **`org.arinn.SystemStatus1`** at `/org/arinn/Daemon1`:

- `Get() -> (stsssy)`: one snapshot `(epoch, revision, audio, network, link, signal)`;
- `Changed(s epoch, t revision)`: the snapshot changed.

The fields:

| Field | Values | Meaning |
| --- | --- | --- |
| `epoch` | UUID | The daemon epoch, as in `MediaAssociation1` |
| `revision` | ≥ 1 | Increases with every content change |
| `audio` | `muted`, `unmuted`, `unknown` | The default audio output's mute state |
| `network` | `connected`, `disconnected`, `unknown` | Whether an active network connection exists |
| `link` | `wifi`, `wired`, `other`, `unknown` | The kind of the primary connection; `unknown` unless `connected` |
| `signal` | 0–4 | Coarse Wi-Fi signal; 0 is unknown or not Wi-Fi |

Each enum is a string on the wire. A reader treats any value it does not know
as `unknown`, and treats a `link` or `signal` that contradicts `network` or
`link` the same way. A later minor change can then add a value without a new
interface version. A producer publishes only the values above
(`system_status::SystemStatus::validate`).

The snapshot carries no SSID, access point, device or connection name, address
or volume level. Nothing in it identifies a network (§13).

**Sources.** The daemon reads both states through a platform-layer module
under `platform/`, never from core routing (R-01):

- **Network:** NetworkManager on the system bus. Its `State` maps
  `CONNECTED_LOCAL`, `CONNECTED_SITE` and `CONNECTED_GLOBAL` to `connected`,
  `ASLEEP`, `DISCONNECTED`, `DISCONNECTING` and `CONNECTING` to
  `disconnected`, and anything else to `unknown`. `connected` therefore
  means a link exists, not that the internet is reachable; a limited state
  would be a later additive value. `PrimaryConnectionType`
  `802-11-wireless` gives `wifi`, `802-3-ethernet` gives `wired`, any other
  non-empty type gives `other`. The signal comes from the access point
  `Strength` of the primary Wi-Fi device: 1–24 % gives 1, 25–49 % gives 2,
  50–74 % gives 3 and 75–100 % gives 4; 0 % or no access point gives 0
  (unknown). Changes arrive through
  `PropertiesChanged`.
- **Audio:** the mute flag of the default sink, read through the
  PulseAudio protocol (`pipewire-pulse` on Plasma 6). The change feed is the
  server's subscription.
- A missing service, a failed read or a source that stops answering makes its
  fields `unknown` until the next good read. A failure never repeats the last
  known value.

Both sources are freedesktop services, not KDE-specific. The producer
therefore runs on every supported platform backend. A session without them
publishes `unknown`. That is the "not available" result #387 asks for, and
other platforms need no separate stub.

**Readers.** The [system status contract](../architecture/system-status.md)
is normative. The shell installs its `Changed` match before the first `Get`,
fences replies by epoch and revision, and shows nothing for `unknown`. Home's
presentation follows handoff 01 and #387: the mute icon appears only while
`audio` is `muted`, and nothing is shown for `unknown`.

## Consequences

- Home gets mute and Wi-Fi state with no new dependency in the shell.
- The daemon depends at runtime on NetworkManager and a PulseAudio-protocol
  server when they exist. Neither is required to start.
- The contract is in this change. The daemon producer and the shell consumer
  follow as separate changes, as with ADR-0096.
- Settings > Sound and Network can later read the same snapshot. No volume
  level or network list is promised.

## Alternatives considered

- **The shell reads NetworkManager and PipeWire itself:** this breaks §8.2 and
  R-05, and duplicates platform code outside the platform layer.
- **A boolean `muted` and a boolean `online`:** these cannot express
  "unknown", and #387 requires unknown to show nothing.
- **A dictionary `a{sv}`:** it extends freely, but it is untyped for QtDBus
  readers and hides the field set. String enums with "unknown for anything
  new" give the same additive path with a fixed signature.
- **Properties with `PropertiesChanged` instead of `Get`/`Changed`:** several
  fields would update separately, so a reader could see a mixed state. The
  snapshot-and-revision pattern matches `MediaAssociation1` and the shell's
  existing fencing code.
- **A KDE-only producer with a stub elsewhere:** the sources are not KDE
  services. A stub would hide state that EWMH sessions can report too.
- **Native PipeWire (`libpipewire`) for the mute flag:** a heavier C
  dependency, and the PulseAudio protocol is served by both PipeWire and
  PulseAudio.

## Verification

- Protocol unit tests: signature `(stsssy)`, roundtrip, validation of
  producer snapshots, and reader fallback to `unknown`.
- A private-bus fixture compares the live export with
  [system-status1.xml](../plugin-api/system-status1.xml), covers a daemon
  without the interface and covers a `Changed` followed by a re-read.
- Producer tests against fake NetworkManager and sound-server sources, the
  daemon export in the private-bus suite and shell tests come in the
  follow-up changes.
- Real KDE evidence of mute and Wi-Fi changes needs explicit owner permission.
  Not run.
