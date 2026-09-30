# ADR-0069: Explicit Home presentation requests

- **Status:** Accepted
- **Date:** 2026-09-30
- **Deciders:** Delegated technical choice within owner-assigned contract task #45
- **Amends / Supersedes:** —
- **Amended by / Superseded by:** —
- **Specification:** §6.3, §8.6, §12.1, §17

## Context

The existing Home/Back table requires repeated Home to close transient UI and
focus Return without toggling or confirming an action (AC-18). `Status` and
`Changed` describe state, while `ReportHome` also restores context after daemon
reconnect, window disappearance and remap. Interpreting those as keypresses
would steal focus. The shell must receive explicit intent through the daemon
(R-03, R-05, G-1.4), with no provider IPC in local navigation.

## Decision

Add `ReportHomeRequest(target, output)` and `HomeRequested(epoch, target, output)`
to the versioned `org.arinn.Daemon1` interface. Existing signatures stay intact.
The method uses the same KWin-owner authentication and bounded text validation
as `ReportHome`. It commits visible Home state, emits `Changed`, then emits one
presentation request even if the state was already identical. Invalid or
unauthorized input neither changes state nor emits intent.

Only an explicit global shortcut or dequeued `Home` command uses this method,
after the existing unlocked-session check. The adapter pins that call to the
current daemon unique owner and drops delayed lock-check/reply callbacks from
an obsolete owner. Ordinary reports, reconnect, remap and missing Return targets
continue using `ReportHome`; they never emit `HomeRequested`.

The signal is a transient intent, not a snapshot revision or persisted command.
It carries the daemon epoch and the request's opaque Return target/output, so a
consumer need not wait for a status read to perform local Home navigation.
Subscribers bind to the unique owner, validate the epoch, and drop queued work
on owner loss. Signals are not replayed to late subscribers or on reconnect.
A missing signal on an older daemon never licenses inferring intent from
`Changed`. See the [wire semantics](../plugin-api/README.md#explicit-home-presentation).

## Consequences

A future shell consumer can route each request into its existing local Home
handler without a provider round trip. Status refreshes still update visibility
and Return availability independently. Old shells ignore the new signal; old
adapters do not produce it. Deploy the matching adapter/daemon together; there
is no retry or fallback that could turn state recovery into a user action.

This increment exports the contract and its production origin only. Shell
consumption and its QML focus regression follow separately after merge. It does
not complete AC-18, change product behaviour or claim a real-KDE result.

## Alternatives considered

- Treat every `Changed` or `ReportHome(true, ...)` as Home: recovery and ordinary
  updates would steal focus.
- Change the existing `ReportHome` signature: unnecessary incompatibility when
  an additive, explicit method distinguishes request origin.
- Emit on `Daemon1.Home` admission: the platform may reject showing Home while
  locked, and the global shortcut does not pass through that queue entry point.
- Store/replay an intent counter: reconnect could repeat navigation from an old
  user action; state recovery and current presentation intent stay separate.

## Verification

Private-bus tests of the actual daemon in `platform/kde/window_tests.rs` cover
repeated requests, unchanged state, ordinary refreshes, no replay, text bounds
and current KWin-owner authentication. JS adapter regressions distinguish global
and queued Home from reconnect/remap/disappearance and obsolete callbacks.
The Phase 1 private-bus harness compares live introspection with `daemon1.xml`.
Real-desktop tests require separate owner permission; shell focus tests and full
AC-18 acceptance remain outstanding.
