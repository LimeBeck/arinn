# ADR-0041: Daemon-independent session lifecycle guard

<!-- SPDX-FileCopyrightText: 2026 Arinn contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

- **Status:** Accepted
- **Date:** 2026-09-28
- **Deciders:** project owner, explicit approval in the task conversation; recorded in [issue #26](https://github.com/LimeBeck/arinn/issues/26)
- **Amends / Supersedes:** amends [ADR-0002](0002-component-boundaries.md) and [ADR-0020](0020-dbus-contract-baseline.md) (scope of the shell/daemon integration boundary)
- **Amended by / Superseded by:** —
- **Specification:** [§5.7, §8.2, §8.5–8.6](../specifications/arinn-specification.md)

## Context

The [Phase 1 shell review](../architecture/phase-1-shell-review.md) found that
G-1.4's literal requirement for all shell data to come through Daemon1 conflicts
with the documented independent ScreenSaver/login1 lifecycle observer.
The shell must hide on lock or before sleep regardless of daemon state and must
remain hidden while lock state is unknown. A stalled daemon must not prevent
that reaction.

## Decision

Keep a daemon-independent, asynchronous, fail-closed session lifecycle guard.
It observes lock/sleep events directly through the platform layer, hides the
shell on lock or before sleep, and rechecks lock state after wake before allowing
the shell to show. Unknown lock state keeps the shell hidden. The guard does not
replace the system lock screen or implement power commands.

All application/provider integration data and commands continue to pass through
`org.arinn.Daemon1` and its daemon-owned capability/operation endpoints. G-1.4
checks that boundary and removal of executable Spike interfaces. The lifecycle
guard is a narrow exception for observing session state; it does not permit
shell-to-provider, MPRIS, KWin window-control or third-party application calls.

Platform-specific lifecycle transport belongs in the platform layer (R-05).
The current shell-local `SessionGuard` transport will be isolated there in
[task #29](https://github.com/LimeBeck/arinn/issues/29), preserving its behaviour
and independent execution. No new provider capability or public wire contract
is introduced by this decision.

The shell-facing guard exposes platform-neutral visibility state. ScreenSaver,
login1 and other native service details remain adapter implementation details.
A future platform supplies its own lifecycle observer with the same independent,
fail-closed semantics; daemon independence does not require Linux-specific APIs
in shared shell or QML code.

## Consequences

A daemon failure cannot be the sole reason lifecycle events stop hiding the
shell. Application integration retains one daemon boundary. Tests must exercise
lifecycle events with the daemon absent or unresponsive, stale lock replies,
service-owner loss and wake revalidation. The platform extraction remains work
to do; this decision does not claim R-05 implementation is complete.

## Alternatives considered

Route all lifecycle observations through the daemon. Rejected by the owner in
favour of retaining the existing independent guard: this would require a new
failure-detection design to preserve hiding during daemon stalls or loss.

## Verification

The source review and prior [Phase 1 CI](../architecture/phase-1-progress.md#github-ci-qualification)
support the clarified application boundary. Existing private-bus Qt tests cover
lock/sleep observation. Task #29 must qualify the extracted platform adapter and
daemon-independent failure cases. Real lock/suspend checks remain AC-19 and
require separate operator authorization. Neither Phase 1 nor Phase 5 is marked
Done, and approval is not new runtime test evidence.
