# ADR-0133: Less Sentry noise and grouping that survives edits

<!-- SPDX-FileCopyrightText: 2026 Arinn contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

- **Status:** Accepted
- **Date:** 2026-11-02
- **Deciders:** Owner, for the goal: less noise and more useful diagnostics
  (2026-11-02, Claude session, [#714](https://github.com/LimeBeck/arinn/issues/714)).
  The rules below are a delegated technical choice
  ([ADR-0037](0037-agent-autonomy-and-owner-gated-decisions.md)): they send
  less and add no field, value category or data source.
- **Amends / Supersedes:** amends [ADR-0093](0093-structured-sentry-diagnostics.md)
  (daemon events, shell fingerprint, known noise and shell breadcrumbs)
- **Amended by / Superseded by:** —
- **Specification:** §13, §14

## Context

A triage of the `limebeckdev / arinn` Sentry project on 2026-11-02 found 18
unresolved issues. Only four were real defects:

- **Expected outcomes as issues.** Superseded seek thumbnails and media
  releases the platform cancelled were the most frequent live issues. Both end
  with `Cancelled`, which ADR-0093 reports as a warning event.
- **One defect, six issues.** The shell fingerprint holds `qt.location` with
  its line. The same "Focus fell to the window root" warning moved from
  `Main.qml:261` to `:515` and `:529` across builds and became a new issue each
  time.
- **A Qt-internal warning.** `Failed to register with host portal`
  (`qt.qpa.services`) comes from Qt registering the application with a portal
  that has no registry. Arinn cannot act on it.
- **A breadcrumb trail of duplicates.** The recorder is an application-wide
  event filter. Qt hands one key press, focus change or window activation to
  every item along its delivery path, so each became several identical
  breadcrumbs. One event held 69 `window.activate` crumbs from the same
  millisecond. With the 100-crumb limit, the steps before the failure were gone.

## Decision

ADR-0093 stays in force with these changes.

**Daemon events.** A failed operation whose error is `Cancelled` produces no
event. It still counts in `arinn.daemon.operations` and `arinn.daemon.errors`,
keeps its transaction with status `cancelled` and its `operation.complete`
breadcrumb. Every other error code still produces one event with the ADR-0093
title, fingerprint and level.

**Shell fingerprint.** A Qt event is grouped by
`["arinn-shell", <type>, <file>, <template>]`, where `<file>` is the
`qt.location` without its line (`Main.qml`), or `unknown.location`. The
`qt.location` tag keeps the line. Both privacy boundaries derive the file from
the already checked location with a crash-safe helper; nothing new is read.

**Known noise.** `Failed to register with host portal` joins the known-noise
list: counted in `arinn.shell.messages.benign`, never an event.

**Shell breadcrumbs.** The recorder records an input, focus, visibility or
activation event only when its receiver is a window, so one user action is one
breadcrumb. A key press that is an autorepeat is not recorded.

## Consequences

Sentry issues are fewer and last across releases. The shell trail shows the
user's actions before a failure instead of repeated activations.

- Two different warnings with the same template in one file now share an issue.
  The `qt.location` tag still shows each line, so Sentry's tag breakdown
  separates them.
- A storm of cancellations is no longer visible as issues; it remains visible
  in `arinn.daemon.errors` with `error = Cancelled`.
- Item-level focus changes no longer leave breadcrumbs. They carried no item
  identity, so nothing usable is lost; window focus is still recorded.

## Alternatives considered

- **Ignore the issues in Sentry only.** Rejected: ignored issues still count
  against quota and a new build re-creates line-split issues.
- **Drop `Cancelled` and `Unsupported` events.** Rejected for `Unsupported`: an
  unsupported command the shell offered is a defect worth seeing.
- **Keep the line and rely on Sentry's merge.** Rejected: every edit above the
  warning would need a manual merge.
- **Collapse repeated breadcrumbs into one with a count.** Rejected: the
  duplicates come from the recorder itself, so filtering at the source is
  simpler and also fixes the key and focus crumbs.

## Verification

- `cargo test --locked -p arinnd telemetry --lib`: a `Cancelled` operation sends
  its transaction and error metric but no event; `Unsupported` still sends a
  warning event.
- `sentry-configuration` CTest: the portal message is benign; the transport
  groups `Main.qml:515` and `Main.qml:529` under `Main.qml` and keeps each line
  in the tag; the file helper rejects unsafe locations and small buffers.
- `sentry-isolated` CTest: the shell event's fingerprint holds `HomeView.qml`,
  and activation events sent to a non-window object leave no breadcrumb.

The window-receiver path and the autorepeat skip are not exercised by these
tests (the fixture has no window); they need a check on a running shell.
