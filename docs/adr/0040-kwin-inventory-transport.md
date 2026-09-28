# ADR-0040: Trusted KWin inventory transport and SDK registration context

<!-- SPDX-FileCopyrightText: 2026 Arinn contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

- **Status:** Accepted
- **Date:** 2026-09-28
- **Deciders:** Delegated technical choice for prerequisite task #24 within owner-approved desktop-provider work #20
- **Amends / Supersedes:** —
- **Amended by / Superseded by:** —
- **Specification:** §8.5, §9, §12, §17

## Context

ADR-0039 fixes the provider-facing platform interface. Its production backend
still needs authoritative multi-window inventory and acknowledged controls from
the trusted compositor. The existing active-window report and Home/Back polling
cannot express either. The SDK hides the connection whose unique owner registered
the provider, although platform access must use that owner.

## Decision

Define a separate internal org.arinn.KWinBridge1 transport with bounded staged
inventory commits, adapter generation/heartbeat and explicit control acknowledgements.
Use scalar method inputs and a fixed string-array control record for the JavaScript
adapter, while public provider pages remain typed WindowPage structures. Freeze
[XML](../plugin-api/kwin-bridge1.xml) and [semantics](../architecture/kwin-transport.md)
before production implementation in #20. No provider can act as the adapter.

Expose an optional registration context through an SDK watch channel, including
the actual connection, daemon owner/epoch and provider generation. Invalidate it
on owner loss and runtime cancellation. Keep run(service) compatible; add run_on
for explicit connections. Consumers cancel stale work on every context change.

## Consequences

Partial inventories never replace a complete snapshot. Dispatch acknowledgements
separate admitted work from successful compositor requests. The platform backend
must enforce heartbeat, aggregate staging and operation limits; declarations and
fixtures do not provide production enforcement. New adapter sessions require fresh
generations and tokens. Provider consumers must manage registration-dependent tasks.
The trusted-plugin security posture and existing KWin-owner authentication do not
change. Real desktop operator tests still require explicit owner authorization.

## Alternatives considered

- Inferring inventory from ReportWindow cannot enumerate inactive windows.
- Encoding commands in opaque window tokens breaks identity and validation.
- Nested JavaScript variant arrays complicate exact D-Bus struct signatures;
  scalar submissions keep the input wire types explicit and independently bounded.
- A separate unregistered SDK connection fails the required caller authentication.

## Verification

G-1.5: XML/live fixture comparison and typed calls. G-2.1–G-2.3 prerequisites:
bounded staging fixtures and registration lifetime tests. Production compositor
transport, actual JavaScript marshalling and real application isolation remain
in #20; this ADR closes no phase or operator acceptance gate.
