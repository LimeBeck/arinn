# ADR-0039: Platform window snapshot and control contract

<!-- SPDX-FileCopyrightText: 2026 Arinn contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

- **Status:** Accepted
- **Date:** 2026-09-28
- **Deciders:** Delegated technical choice within owner-approved contract task #22
- **Amends / Supersedes:** —
- **Amended by / Superseded by:** —
- **Specification:** §9, §10, §12, §17

## Context

The desktop provider needs to enumerate, activate and gracefully close windows
through PlatformService (R-05, G-2.1–G-2.3). The existing bridge only reports the
active/return window and cannot express those operations. The provider must not
embed compositor commands in opaque window tokens or bypass the platform layer.
Contract changes precede production implementation under ADR-0036.

## Decision

Add a daemon-owned `org.arinn.PlatformWindows1` interface on the existing root
object, with coherent paged window snapshots, `WindowsChanged`, and targeted
`Activate`/`Close` commands using the existing operation objects. Freeze its types,
XML and SDK client before implementing the production backend in a separate task.
The exact contract and status are in
[Platform window contract](../architecture/platform-windows.md).

A platform-session generation is independent of provider registration generations.
Tokens identify one live window, are never reused and never redirect after remap.
The platform may report an explicitly resolved desktop-file ID; unresolved
association stays empty. Window titles and command lines are absent. Identity
resolution and all compositor details stay in the platform layer, not the core
or shell. This adds no plugin capability and does not change product behaviour.

Registered application providers use the interface under existing sender and
registration validation. Commands follow existing timeout, deduplication and
error semantics. Graceful close success reports dispatch, never a fabricated
process-exit guarantee. The SDK is pinned to the daemon's unique owner and returns
only validated complete snapshots.

## Consequences

The desktop provider can implement against a concrete, testable contract without
altering the application API. The production server must implement independent
platform lifetime tracking, paging and command acknowledgements. This contract
and its mock do not make that production service available. The internal
compositor transport is a separate boundary and needs a separate contract review
if additional wire interfaces are introduced. The SDK does not automatically
retry commands or schedule snapshot reloads for consumers.

## Alternatives considered

- Direct provider calls to KWin put platform mechanisms inside providers and
  violate R-05.
- Reusing the active-window report cannot enumerate inactive/multiple windows or
  safely target close.
- Encoding actions into tokens loses typed validation and conflates identity with
  instructions.
- Returning plain success from public control calls loses the existing operation
  lifecycle, deduplication and failure reporting.

## Verification

Protocol unit tests and SDK private-bus tests compare live fixture introspection
with XML and exercise generation, paging and command handling. Required repository
checks and CI cover the new bindings. Production server/compositor behaviour,
launch scope isolation and real-desktop qualification remain follow-up work in
[#20](https://github.com/LimeBeck/arinn/issues/20), not evidence supplied by this ADR.
No Phase 2 acceptance criterion or gate is marked passed.
