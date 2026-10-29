# ADR-0110: Application launch lifetime and cancellation

- **Status:** Accepted
- **Date:** 2026-10-21
- **Deciders:** delegated technical choice ([ADR-0037](0037-agent-autonomy-and-owner-gated-decisions.md)) implementing the owner decision of 2026-10-10 on [#436](https://github.com/LimeBeck/arinn/issues/436)
- **Amends / Supersedes:** amends [ADR-0035](0035-phase1-application-wire-contract.md)
- **Amended by / Superseded by:** —
- **Specification:** [§6.5](../specifications/arinn-specification.md#65-performance-principles), [§7.2](../specifications/arinn-specification.md#72-applications-steam-and-es-de), [§9.5](../specifications/arinn-specification.md#95-operations)

## Context

The desktop provider answers `Provider1.Launch` when `gio launch` exits, so a launch reports success before any window exists. It also reports success when the application exits with no window ([#436](https://github.com/LimeBeck/arinn/issues/436), recorded on 2026-10-21). Games often need more than the 30 s launch timeout before their first window. The owner decided on 2026-10-10 that such a launch stays pending while its application scope runs. The user sees "Starting <app>…" and can cancel after a confirmation, which stops that launch's scope. If the scope ends with no window, the launch fails at once.

[ADR-0035](0035-phase1-application-wire-contract.md) makes the private provider reply complete the daemon-owned operation and declares every operation `cancellable = false`. `Operation1` already has a `Cancel` method and a `cancellable` field. The daemon has no way to make a provider stop a launch it is still waiting for: an in-flight D-Bus call cannot be withdrawn.

## Decision

1. **Launch reply.** A provider that observes windows answers `Launch` when a new window of the application appears. It fails as soon as the launch can no longer show one: when the launch's processes end with no window of the application, where a window that already existed counts because the launch was handed to a running instance (§7.2). A provider that cannot observe windows answers at dispatch. The daemon applies no overall deadline to an application launch: the public operation stays `running` while the provider waits. The 30 s budget bounds dispatch inside the provider. Provider loss and a new generation still end the operation with `Unavailable` (ADR-0035).
2. **`Provider1.CancelLaunch(generation s, id s)`**, a new private method. It stops every unfinished launch of `id` and the processes those launches started; their `Launch` replies fail with `Cancelled`. It answers `NotFound` when no launch of `id` is unfinished and `Unsupported` when the provider cannot cancel. It is authorized like the other provider commands and checks the generation, but not the catalogue. Launches are cancelled per application ID rather than per call, so `Launch` keeps its signature; the daemon's rule in 3 makes that set the one launch being cancelled.
3. **`Operation1`.** An application launch reports `cancellable = true` until it ends; other operations stay `false`. `Cancel` is accepted only from the operation's caller (`PermissionDenied`) and only for a cancellable operation (`Unsupported`). The daemon forwards it as `CancelLaunch` only while the operation is the only running launch of that provider, generation and ID, from any caller; otherwise `Cancel` answers `Conflict` and stops nothing, as does a second `Cancel` while one is in flight. A new `Launch` of that ID is not dispatched until an in-flight `CancelLaunch` has been answered, so a cancel never reaches a launch started after it. A launch counts as running until the provider has answered its `Provider1.Launch`, the daemon's own browser launch for a web open (ADR-0055) included. Together these give §6.5's "stops only the processes of that launch" without a launch handle on the wire. A provider without the method (`UnknownMethod`) counts as `Unsupported`; a provider that does not answer `CancelLaunch` within 15 s makes `Cancel` return `Timeout`, and the operation keeps running. `cancellable` is the daemon's property and does not promise that the provider can cancel. The provider's confirmation, or a `Cancelled` launch reply, ends the operation as `cancelled` with error `Cancelled`. A provider refusal is returned by `Cancel`, and the operation keeps running. `Cancel` on an application launch that has already ended succeeds without changing it.
4. **SDK.** `ApplicationHandler::cancel_launch(id)` has a default implementation that answers `Unsupported`. Existing providers compile unchanged and refuse cancellation.

The "starting" state needs no new field. The operation is `running` and `cancellable`, and the shell decides when to show "Starting <app>…" (§6.5).

## Consequences

- The change is generic: any application provider, not one product, may answer late or cancel (R-01, R-10).
- Out-of-tree providers built on the SDK keep working. Their launches are not cancellable until they implement `cancel_launch`.
- A provider that never answers `Launch` keeps the operation running until the provider goes away or the user cancels. The provider must bound its own dispatch and end the wait when the launch can no longer produce a window.
- Two clients that launch the same application at the same time (the shell and `arinnctl`, or a restarted shell) cannot cancel either launch until one of them ends: `Cancel` answers `Conflict`. The shell starts at most one launch per application, so it meets this only with another client.
- A provider that hangs in dispatch, or never answers `CancelLaunch`, keeps the operation `running` until the provider is lost: `Cancel` returns `Timeout`. An unanswered `CancelLaunch` also holds new launches of that ID back until the provider answers or is lost. Before this decision the daemon's 30 s launch timeout ended such an operation; now the provider's own dispatch budget has to.
- A launch whose processes keep running with no window (a tray-only or background application) stays `running` indefinitely, as §6.5 asks. It holds one of the provider's 32 active-operation slots; 32 such launches make further `Application1` commands of that provider `Unavailable` until one ends or the provider restarts. This is accepted for now: no shell flow starts launches without a user action, and Cancel frees the slot.
- `CancelLaunch` carries no launch identity, so a launch the provider still tracks from its previous registration (an earlier generation, already ended by the daemon) is stopped by a later cancel of the same ID.
- `Cancel` stops processes. It still rolls back no other side effect (§9.5).

## Alternatives considered

- **A `Launch` signature with a launch handle.** Rejected: it breaks every provider for a case the shell does not produce, namely two concurrent launches of one application.
- **A daemon-side timeout raised to minutes.** Rejected: the owner decided that the scope, not a timer, decides the outcome (§6.5: no fake success or failure after a timer).
- **A launch handle on `CancelLaunch` (for example the D-Bus serial of the daemon's `Launch` call), passed to an SDK hook.** Rejected for now: it is a larger contract and SDK change for the same guarantee that the daemon rule in 3 gives, and only a case with two clients needs it. It stays possible as an additive change.
- **The daemon refuses a second concurrent launch of one application.** Rejected: it changes `Launch` for every client to protect `Cancel`, which most launches never use.
- **A new progress field or state on `Operation1`.** Not needed: `running` plus `cancellable` carries the state, and a new field would change the `Get` signature for every client.
- **The daemon stops the launch unit itself.** Rejected: the unit belongs to the provider, and the core must not call the session manager for an application (R-02, ADR-0021).

## Verification

- SDK private-bus test: `CancelLaunch` reaches the handler only from the daemon and only for the current generation. The default handler answers `Unsupported`. Live introspection matches [`provider1.xml`](../plugin-api/provider1.xml).
- Daemon tests ([#436](https://github.com/LimeBeck/arinn/issues/436)): with two running launches of one ID, `Cancel` answers `Conflict` and both keep running; a `Launch` admitted while a `CancelLaunch` is in flight is dispatched only after it.
- `tests/integration/phase1.py` compares the demo provider's live `Provider1` with the XML.
- The implementation in [#436](https://github.com/LimeBeck/arinn/issues/436) adds private-bus tests. A window that appears after more than 30 s completes the launch. A scope that exits with no window fails it at once. Cancel stops only that launch's scope.
