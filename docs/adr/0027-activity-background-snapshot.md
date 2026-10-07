# ADR-0027: Compositor snapshot of the Activity for a separate Qt shell

- **Status:** Accepted — first implementation path; the prototype is not a finished Home integration
- **Date:** 2026-09-27
- **Deciders:** delegated technical choice, based on the Phase 0 runtime experiment
- **Amends / Supersedes:** amends [ADR-0009](0009-kde-wayland-platform.md), [ADR-0019](0019-design-handoff-contract.md)
- **Amended by / Superseded by:** [ADR-0087](0087-bounded-activity-background-frames.md)
- **Specification:** [§12.3](../specifications/arinn-specification.md)

## Context

The design handoff allows a live compositor texture **or a last-frame snapshot** of the real external window. Near/far require scale, blur and dimming. The shell stays a separate Qt process; moving the UI into the compositor is not required.

## Decision

The first production path is the last frame of the selected Activity: a trusted KWin platform component takes a `WindowThumbnail` of the specific window identity and exports a bounded frame; the daemon tells the shell it is ready. The shell reads only the prepared frame and renders near/far locally. The same `ActivityBackground.qml` works with a compositor source item and with an exported image.

The snapshot is bound to generation + Activity + Home request. Return, Activity change, lock/sleep, provider or daemon loss, or cancellation invalidates it. A stale reply cannot replace the current background. The frame lives only in a private runtime directory and is deleted after use; screenshot paths are never accepted from providers. Repeated Home does not capture again. The UI never waits for the image before local focus.

These lifecycle rules are requirements for the production implementation. Currently capture runs **only from an operator test**; Home does not capture automatically yet.

## Consequences

Continuous live texture sharing is deferred to a separate performance/lifecycle review. The full visual contract (transitions, app tint, TV readability) is checked when the UI is implemented.

## Alternatives considered

KWin ScreenShot2 with a desktop permission returned `NoAuthorized`; the cause was not found and permission checks were not disabled. Moving the whole UI into KWin was not needed.

## Verification

The [runtime experiment](../architecture/background-experiment.md) confirmed a live source from a real Chrome window, near/far shaders and reading the exported frame in a separate Qt process. PNG export takes about 0.3 s; frame size limits, asynchronous delivery and measurements are required before enabling it in Home.
