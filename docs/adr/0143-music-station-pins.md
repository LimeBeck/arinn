# ADR-0143: Music station pins in the daemon's favorites

- **Status:** Accepted
- **Date:** 2026-11-06
- **Deciders:** delegated technical choice ([ADR-0037](0037-agent-autonomy-and-owner-gated-decisions.md)), implementing the owner's ADR-0142 MH-6 and the music brief's *Presence › Favourites*
- **Amends / Supersedes:** —
- **Amended by / Superseded by:** —
- **Specification:** [§5.1, §11](../specifications/arinn-specification.md)

## Context

Stations can be pinned to the Favourites shelf on Home ([ADR-0142](0142-music-in-hearth-light.md) MH-6). The brief requires Arinn to keep the pins itself, with the provider ID, the station ID, the title and the artwork ID, in the order pinned. It must never infer pins from `selections`, and it must keep showing a pin, marked unavailable, while its provider is away. #811 gave pinnable stations a stable ID.

The daemon owns favorites and their order in `state.sqlite3` (§11, [ADR-0084](0084-favorites-contract.md)). ADR-0084 rejects shell-local copies. `Favorites1` stores identities only, so web shortcuts keep their presentation in `WebShortcuts1`.

## Decision

- **A pinned station is a `Favorites1` target:** `EntityRef { provider_id: <music provider>, capability: "music-station", local_id: <station id> }`. It takes its place in the one Favourites order.
- **`org.arinn.MusicPins1`** at the daemon root keeps what Home shows for a pinned station: `MusicPin (provider_id, station_id, title, artwork)`. It shares the favorites' revision, serializer and `Favorites1.Changed` signal.
  - `List() → (revision, pins)` returns the pins in favorites order.
  - `Pin(revision, pin) → revision`, in one transaction, stores the presentation and appends the target. For a station already pinned it refreshes the title and artwork in place; an identical pin changes nothing.
  - `Unpin(revision, provider_id, station_id) → revision` removes both.
  - A stale revision gives `Conflict`, as `Favorites1.Set` does.
- **`Favorites1.Set` keeps both lists consistent.** It still reorders and removes freely. It adds a `music-station` target only if `MusicPins1` stored it (`NotFound` otherwise). A target it removes drops its pin record in the same transaction.
- **The provider's own `pin` action** stays the shell's concern, as does starting the station. The daemon never calls a music provider for a pin.

## Consequences

- Pins survive restarts and provider removal, like the other favorites. They join reorder and removal through the existing interface.
- The shell needs one more read, `MusicPins1.List`, when `Favorites1.Changed` arrives (#804).
- SQLite schema 9 adds the `music_pins` table.
- A pin's title is a snapshot from the moment it was pinned. A later `Pin` refreshes it, for example when the shell sees the station again.

## Alternatives considered

- **Shell-local storage:** rejected by ADR-0084. It duplicates daemon state and fails the CLI and concurrent writers.
- **The provider's own pin list** (Yandex `pin`): not every source has one; the brief requires Arinn's own store; and a pin must show while its provider is away.
- **Titles inside `Favorites1`:** it deliberately carries identities only, and web shortcuts set the precedent of a separate presentation interface.
- **Two calls** (`MusicPins1.Add`, then `Favorites1.Set`): this leaves a half-pinned state between them. One transaction avoids it.

## Verification

- `arinn-protocol`: the `music-station` target and the `MusicPin` validation and signatures.
- `arinnd`:
  - storage tests for the order, the CAS, the refresh in place, unpinning, the `Set` consistency, survival across a reopen and the schema 9 migration;
  - the private-bus test `music_pins_share_the_favorites_revision_and_signal`.
- The shell binding and the TV check belong to #804.
