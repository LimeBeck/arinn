# System status

Normative contract for [ADR-0100](../adr/0100-system-status.md)
([#387](https://github.com/LimeBeck/arinn/issues/387)). The daemon publishes the
mute state of the default audio output and network connectivity. Home's status
area shows them (handoff 01). This document states what the producer may
publish and how readers apply it.

## Interface

| Interface | Owner and path | Members |
| --- | --- | --- |
| `org.arinn.SystemStatus1` | Daemon, `/org/arinn/Daemon1`; [system-status1.xml](../plugin-api/system-status1.xml) | `Get() -> (stsssy)`, `Changed(s epoch, t revision)` |

Types and validation are in `arinn_protocol::system_status`. The snapshot is
`(epoch, revision, audio, network, link, signal)`:

| Field | Producer values | Reader rule |
| --- | --- | --- |
| `epoch` | The daemon epoch (UUID) | A reply from another epoch is discarded |
| `revision` | ≥ 1, increases with every content change | A reply below the last applied revision is discarded |
| `audio` | `muted`, `unmuted`, `unknown` | Any other value is `unknown` |
| `network` | `connected`, `disconnected`, `unknown` | Any other value is `unknown` |
| `link` | `wifi`, `wired`, `other`; `unknown` unless `network` is `connected` | Any other value, or any value while not `connected`, is `unknown` |
| `signal` | 1–4 for a Wi-Fi link when known, otherwise 0 | Anything above 4, or not on Wi-Fi, is 0 |

`SystemStatus::validate` is the producer check. `audio()`, `network()`,
`link()` and `signal()` are the reader rules. A platform without a source
publishes every field `unknown` with signal 0. That is a valid snapshot, not an
error.

The snapshot never carries an SSID, access point, interface, device or
connection name, address, or volume level. Logs and telemetry follow spec §13:
at most the enum values.

## Producer

- The sources live in the platform layer, `platform/status_sources.rs`. The
  daemon export (`crates/arinnd/src/runtime/system_status.rs`) knows only
  their updates (R-01). Both sources run on every platform backend.
- **Network:** a system bus connection follows NetworkManager's
  `PropertiesChanged` from any of its objects, including the access point's
  `Strength`. A burst of signals becomes one read after 200 ms, and the state
  is also re-read every 30 s in case a signal was missed.
  A lost bus connection reports `unknown` and reconnects after 10 s.
- **Audio:** `pactl subscribe` follows the sound server. A sink or server
  event re-reads `pactl get-sink-mute @DEFAULT_SINK@`, and the flag is also
  re-read every 30 s. `pactl` runs with `LC_ALL=C` because it localizes
  yes/no; any other answer is `unknown`. A missing `pactl` or a subscription
  that ends reports `unknown` and retries after 10 s. Without `pactl`
  (`libpulse`) or NetworkManager the fields stay `unknown`.
- It replaces the snapshot as a whole and bumps the revision only when the
  content changed (`checked_add`; on overflow it publishes every field
  `unknown` and stops updating). Then it emits `Changed(epoch, revision)`.
- A source that is missing, fails a read or stops delivering changes makes its
  own fields `unknown` until the next good read. The other source's fields are
  unaffected. The producer never republishes an older value after a failure.
- The source mappings (NetworkManager `State`, `PrimaryConnectionType` and
  access point `Strength`, and the default sink's mute flag) are in ADR-0100.

## Shell consumer

- The shell installs its `Changed` match **before** its first `Get`, so no
  change between them is missed. It re-reads on `Changed` and on a daemon
  owner change. Reads are asynchronous; the UI thread never waits for them.
- A `Get` reply applies only when its epoch is the current daemon's epoch and
  its revision is not lower than the last applied one. A new daemon owner or
  epoch resets the last applied revision and clears the state to `unknown`.
- A daemon without the interface, a failed or timed-out read, a reply with
  another signature, an epoch that is not a UUID or is the nil UUID, or
  revision 0 leaves the
  state `unknown`. Stale icons never stay up after a failure.
- Readers never reject a reply with `SystemStatus::validate`, which is the
  producer check. A value they do not know, or an inconsistent `link` or
  `signal`, falls back to `unknown` or 0 field by field, so a newer daemon
  never blanks the fields an older shell understands.
- `unknown` shows nothing. The mute icon appears only while `audio` is `muted`.
  The network icon follows `network`, `link` and `signal`, within handoff 01.
- `Changed` never requests Home presentation and never moves focus.

## Verification

- `crates/arinn-protocol/src/system_status.rs` unit tests: signature, roundtrip,
  producer validation and reader fallbacks.
- `crates/arinn-protocol/tests/system_status.rs`: a fixture daemon on a private
  bus. It covers a daemon without the interface, compares the live export with
  the XML, and covers an all-unknown snapshot and a `Changed` followed by a
  re-read.
- `platform/status_sources_tests.rs`: the NetworkManager and `pactl`
  mappings, a fake NetworkManager on a private bus (Wi-Fi level, wired,
  disconnected, two devices, absent, lost bus) and a fake `pactl` (sink and
  server events, ignored events, locale, ended subscription, missing tool).
- `crates/arinnd/src/runtime/system_status_tests.rs`: the daemon export with
  both fakes. It compares the methods with the XML, and covers `Changed` on a
  new revision, unchanged updates and revision exhaustion.
- `tests/integration/phase1.py`: the real daemon's export against the XML and
  a valid `Get` on the CI host.
