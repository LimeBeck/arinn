<!-- SPDX-FileCopyrightText: 2026 Arinn contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Phase 1 shell boundary review

Reviewed on 2026-09-28 for [task #25](https://github.com/LimeBeck/arinn/issues/25).
Baseline: `d20e9f15c5da013ca82973491e93388394e28d50` on `master`.
This is source-review evidence for G-1.4 and AC-12, not Phase 1 completion,
full UI acceptance or an operator test.

## Owner resolution after review

On 2026-09-28 the owner approved [ADR-0041](../adr/0041-independent-session-lifecycle-guard.md): G-1.4 covers
application/provider integration, with independent fail-closed session observation.
The inspected call paths below support the clarified gate. The original conflict
below records the reviewed baseline, not an outstanding decision. R-05 platform
transport extraction is implemented by [#29](https://github.com/LimeBeck/arinn/issues/29):
[SessionLifecycle](../../platform/session/sessionlifecycle.h) exposes neutral
visibility state and [the platform adapter](../../platform/session/dbussessionlifecycle.cpp)
contains the native subscriptions and asynchronous queries.
The stale lifecycle interface name reported below is corrected in the living
runbook. The original review below remains historical evidence.

## Results at the original baseline

| Requirement | Review result |
| --- | --- |
| AC-12 | Pass for the reviewed production shell: no product-specific routing or capability conditions found. |
| G-1.4, production wire migration | Application data and commands use the daemon service and its versioned endpoints. No obsolete Spike interface names were found by the executable-source guard. |
| G-1.4, all shell data through Daemon1 | Unresolved: the documented direct lifecycle guard contradicts the literal gate. [Owner decision #26](https://github.com/LimeBeck/arinn/issues/26) is required before marking this gate passed. |

## Inspected boundaries

- [Backend](../../apps/arinn-shell/backend.cpp): `request` fixes the service,
  root path and interface to `org.arinn.Daemon1`. `refresh` reads `Status` and
  `HomeItems`; subscriptions and owner watching use the same service.
  `activate` sends `Application1.Activate` to a daemon-provided object path
  while retaining the daemon service as destination. Operation polling uses
  `Operation1.Get` on that same service. `back` calls `Daemon1.Back`.
  All those calls use `asyncCall` and pending-call watchers.
- Identity and routing remain separate: the generated UI UUID identifies a row;
  provider path, generation and local ID are retained as opaque fields for the
  command. There is no provider-name or application-name dispatch branch.
- [Production QML](../../apps/arinn-shell/Main.qml): binds generic availability,
  return target, title and message properties, then invokes `back` or `activate`.
  Focus and the minimal navigation state are local. The identifier `demoButton`
  names a control; it does not compare provider IDs or route to the demo product.
  The first-item presentation is a minimal shell limitation, not full Home UI.
- [Build targets](../../apps/arinn-shell/CMakeLists.txt): production embeds
  `Main.qml`. The fixture gallery is a separate executable linked to Qt Quick;
  it is not the production backend. Its presentation/navigation qualification
  remains in the [component record](ui-components.md).
- [Daemon endpoints](../../crates/arinnd/src/runtime.rs): `Daemon::home_items`
  projects activatable running entries from ready providers; `ApplicationEndpoint`
  routes methods through its registered provider ID and the generic command
  path. The shell does not open a provider connection or launch a process.
- [Qt tests](../../apps/arinn-shell/backend_test.cpp): the `demo.activity`
  comparison belongs to the explicit demo fixture test, not production routing.
  Existing tests exercise local focus during delayed activation and lifecycle
  events on private buses. Their earlier execution evidence is linked in the
  [Phase 1 record](phase-1-progress.md#github-ci-qualification); those tests were
  not rerun for this documentation-only review.

## Original conflict (resolved by ADR-0041)

At the reviewed revision, [ROADMAP.md line 135](../../ROADMAP.md) requires all
shell data to come through Daemon1. The
[implementation baseline, line 12](implementation-baseline.md) describes the
daemon as the shell's only integration API. However,
[session lifecycle, line 3](session-lifecycle.md) explicitly requires the shell
to observe ScreenSaver and login1 directly, independently of daemon state.
[SessionGuard lines 13–19 and 39–43](https://github.com/LimeBeck/arinn/blob/d20e9f15c5da013ca82973491e93388394e28d50/apps/arinn-shell/sessionguard.cpp)
implement those subscriptions and `GetActive`; the shell constructs this class
and uses its `blocked` property to hide the window.

The [specification §5.7](../specifications/arinn-specification.md#57-system-ui-power-and-ambient)
requires hiding independently of daemon state and treating unknown lock state
as locked. R-05 also requires platform-specific code to be isolated. The direct
lifecycle observer is therefore not evidence of product-specific application
routing, but the current wording cannot support an unconditional G-1.4 pass.

Recommendation in #26: explicitly scope the gate to application/provider
integration, preserve the independent fail-closed guard, and isolate lifecycle
transport behind the platform layer in a separately claimed technical task.
The alternative is to route lifecycle state through the daemon with a reviewed
failure design. Neither option is selected here; runtime, specification and
acceptance wording remain unchanged pending the owner's decision.

The lifecycle runbook also still names `org.arinn.SpikeSession1.blocked` at
line 15, whereas the executable exports `org.arinn.Session1`. This is stale
living documentation, not a remaining executable Spike interface; correct it
with the lifecycle boundary follow-up.

## Verification and limits

Run for this review: architecture guards, documentation consistency, ADR register,
release consistency, `reuse --no-multiprocessing lint` and whitespace checks.
All passed. Default REUSE multiprocessing was blocked by sandbox socket
restrictions; the serial run passed. The architecture
guard is a narrow literal/regex check; the inspected call destinations and QML
bindings above supply the separate source review.

Rust/Qt builds, unit tests, private-bus integration, KWin JavaScript tests,
dependency licensing and clean Arch package tests were not rerun locally: this
change only records review evidence and does not modify executable code or
contracts. Earlier CI results are historical evidence, not a new test run.
Real-desktop operator tests were not run; no authorization was requested.
No performance, full product navigation, lock-screen or suspend acceptance is
claimed. G-1.6 and Phase 1 status are unchanged.

## Platform extraction qualification (#29)

The shell links the platform session library and retains the same QML visibility
binding and exported `org.arinn.Session1.blocked` property. No application routing,
public daemon/provider API or lifecycle behaviour changes. The platform observer
has no dependency on the shell Backend or daemon; only its test executable links
the production Backend and QML to verify actual window visibility during daemon
failure. The extracted Qt test is registered in CTest on its own private bus.

Coverage includes lock/unlock, sleep/wake, unlock during sleep, absent or unknown
ScreenSaver state, owner loss/recovery, and stale GetActive replies following
lock, owner loss and wake. The production window hides while Daemon1 is absent
or accepts requests without answering, and wake waits for fresh lock state.
Real-desktop operator tests remain unrun; AC-19 and phase completion are unchanged.

Local verification on 2026-09-28 passed: CMake/Ninja, CTest (UI and lifecycle),
Phase 1 private-bus integration, Rust formatting/Clippy/tests/example and binary
builds, KWin JavaScript tests, documentation/architecture/ADR/release checks,
REUSE and dependency licenses. Private-bus tests ran outside the socket-restricted
sandbox. Rust tests/builds used debug information and incremental compilation
disabled after the initial build exhausted temporary storage. An initial UI
navigation test failure under the sandbox did not recur in the final full CTest
run. Clean Arch package qualification is delegated to PR CI.
