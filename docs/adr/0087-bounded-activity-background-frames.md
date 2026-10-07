# ADR-0087: Bounded Activity background frames through immutable descriptors

- **Status:** Accepted
- **Date:** 2026-10-06
- **Deciders:** delegated technical choice for owner-directed #149, contract task #348
- **Amends / Supersedes:** amends ADR-0027 (cross-process frame handoff)
- **Amended by / Superseded by:** —
- **Specification:** [§12.3, §6.5, §13](../specifications/arinn-specification.md)

## Context

The operator experiment validates WindowThumbnail but its synchronous PNG export
costs about 0.3 s and mixed-DPI output can exceed the intended dimensions.
Production capture needs a bounded asynchronous handoff, explicit identity and
release, without accepting provider screenshot paths or weakening compositor
permissions. #149 also needs daemon-mediated readiness before shell integration.

## Decision

Add optional Background1 metadata/reader and BackgroundCapture1 selected-platform
publisher interfaces, separately from provider capabilities. Bind each frame to
daemon epoch, one capture request, exact Return target/output and authoritative
platform window generation/token. Publish tightly packed bounded RGBA pixels
through a sealed memfd; the selected trusted platform owner publishes and only
the current shell owner reads pixels. Retain no screenshot files. An in-process
platform worker uses the existing selected-platform bus connection; no new helper
authority is added. This replaces ADR-0027's runtime-file handoff while retaining
its capture source, near/far visual treatment, no-wait behavior and invalidation
requirements. This is a technical transport choice within the existing trust and
privacy boundaries, not a new provider permission or sandbox policy.

The [contract](../architecture/activity-background.md) specifies 1920×1080 / 8 MiB
admission, one pending worker/frame, an unrenewed 1500 ms deadline, authenticated
publication, consumer checks and resource release. Repeated Home never recaptures
within the same context; failure keeps ordinary Home. Implementation remains
separate after the contract merges.

## Consequences

No PNG encoding or filesystem path handoff is needed on the compositor's critical
path. Raw pixels have predictable memory bounds and FD sealing prevents later
mutation. Linux-specific descriptor validation remains inside the platform/runtime
implementation. Capture, consumer rendering and real-device budgets still need
implementation and qualification; this ADR closes no acceptance gate.

## Alternatives considered

Continuing synchronous PNG writes preserves the experiment's latency and path
lifecycle work. Existing provider asset routing has different authority/lifetimes
and must not turn providers into screenshot publishers. Live texture sharing
remains deferred under ADR-0027; this change does not introduce it.

## Verification

Protocol tests cover signatures, snapshot round trips, lifetimes, cardinality,
state tokens and pixel overflow/bounds. Implementation requires sealed-FD and
private-bus lifecycle tests. The target-device report required by G-5.2 remains
owner-run; accepting this technical choice does not claim memory/GPU/frame-time
or real KDE/XFCE capture acceptance.
