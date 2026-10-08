<!-- SPDX-FileCopyrightText: 2026 Arinn contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Trusted KWin inventory transport

[ADR-0040](../adr/0040-kwin-inventory-transport.md), prerequisite for desktop-provider
implementation in issue #20. This document and the [XML](../plugin-api/kwin-bridge1.xml)
define an internal platform transport, not a plugin capability. Production export
and deterministic adapter tests are implemented. Actual KWin JavaScript binding
verification and real-desktop qualification remain pending.

The [KWin callDBus implementation](https://github.com/KDE/kwin/blob/master/src/scripting/scripting.cpp)
accepts at most nine JavaScript arguments after the method name, including the
callback, and converts values through QJSValue::toVariant. PutWindow therefore
uses seven inputs plus the callback. Counts, revisions and flags are signed int32
(D-Bus `i`); negative counts/indices and non-positive revisions are rejected.

## Identity and lifetime

`org.arinn.KWinBridge1` is exported by the daemon at `/org/arinn/Daemon1`. Every
method authenticates the sender as the current unique owner of the explicitly
selected platform: `org.kde.KWin` by default, or `org.arinn.X11` for optional
EWMH mode ([ADR-0078](../adr/0078-x11-ewmh-adapter.md)). The existing trusted-platform
boundary accepts only one adapter owner at a time. Only the selected name owner can submit inventories
or acknowledge commands. No script, expression, title, URL or executable is sent.

`Connect(instance)` takes a non-nil UUID identifying one loaded adapter instance.
The daemon returns its newly allocated non-nil platform generation. Repeating Connect
for the same live owner/instance is idempotent. A new instance, KWin owner loss,
daemon restart or heartbeat expiry invalidates inventory, leases, staging and
pending commands before a new generation can be used. Old generations return
Conflict. There is one active adapter. After invalidation, even the same instance
gets a fresh generation. The adapter creates fresh opaque tokens for all windows
in each generation; removed tokens are never reused. Identity and association
follow the [provider-facing contract](platform-windows.md#window-identity).

## Atomic inventory

The adapter captures a complete immutable inventory locally, then calls
BeginInventory, PutWindow for every index, and CommitInventory sequentially,
awaiting each reply. The daemon never publishes partially received state.

- BeginInventory(generation, revision, count): positive int32 revision, at most
  100,000 windows. One staged inventory, expiring 3 seconds after Begin; no renewal.
  A revision must exceed the last committed revision. A new Begin replaces only
  the staging buffer, never the last complete inventory. Count zero is valid.
- PutWindow submits generation, revision, zero-based index, token, desktop ID, output and int32 flags
  (active=1, fullscreen=2, can_activate=4, can_close=8; other bits forbidden). Indices must be unique and below count; malformed input rejects
  and discards staging. Apply Window validation before storing. Bound encoded
  staged data to 16 MiB and every incoming body to 256 KiB. Count is validated
  before allocation. No allocation based solely on untrusted index values.
- CommitInventory requires every index exactly once, valid unique window tokens,
  at most one active window and all aggregate limits. Failure or staging timeout
  discards the partial inventory, leaving the complete inventory visible. Unknown,
  expired or mismatched generation/revision gives Conflict. Invalid content gives
  InvalidArgument. Successful commit atomically updates fields, revision and
  WindowsChanged. Empty inventory is different from unavailable platform.
- Revisions use int32 on this internal transport to match QJSValue integer conversion without JavaScript integer
  precision loss; they widen exactly to the public uint64 revision. At int32
  exhaustion the adapter reconnects with a new instance and generation. Unchanged
  inventories need no new revision. Failed revisions may be skipped.

Read-only metadata such as desktopFileName is resolved to an exact installed
desktop-file ID by the platform layer. Ambiguity produces an empty association;
there is no resource-class/title/process-name fallback. A managed player window always has an empty desktop ID. Only normal application
windows are included, excluding shell and lock surfaces.

## Control acknowledgements

NextControl(generation) waits at most 1 second. An empty string array is idle;
otherwise it returns exactly [request UUID, action, opaque window token], with
action `activate` or `close`. This fixed record is not a command line. The daemon
keeps each delivered request pending until acknowledgement or its original
5-second execution deadline. No automatic redispatch follows a missing reply.

The adapter looks up the exact current token, checks supported actions, performs
the requested compositor operation, then calls CompleteControl with generation,
request UUID and an empty error on dispatch success or one normalized ErrorCode.
No raw exception text is accepted. An unknown, expired, completed or old-generation
request returns Conflict and cannot complete another operation. Success means
accepted dispatch, not focus acquisition or process exit. Graceful close never
falls back to force kill. Timeout is an unknown outcome and requires reconciliation.

NextControl refreshes the adapter heartbeat, including idle polls. At most one
poll is pending per instance; extra polls return Conflict. Five seconds without
a poll invalidates the platform. Staging progress is not a substitute heartbeat.
The production queue shares per-provider operation admission limits and is
bounded; queued commands carry the provider's registration lifetime. Lost providers,
platform generations or daemon ownership fail pending commands with Unavailable;
late acknowledgements cannot revive them. Existing Home/Back transport is unchanged.

## SDK registration context

ApplicationService::registrations returns a watch receiver of optional
RegistrationContext. Subscribe before starting run/run_on. None means unavailable;
a context carries the actual registered connection, daemon unique owner/epoch and
provider generation. It is published only after registration and initial snapshot
installation, invalidated on owner loss or runtime task exit/cancellation, and
replaced on registration. Only one runner may own a service at a time.

A desktop provider constructs WindowClient from that context's connection, never
from a separate connection. On *every* watch change it discards old clients and
window state, even if a transient None was coalesced. Snapshot/control tasks must
be cancelled on context replacement and old results ignored. The context grants
no authorization by itself; production daemon authorization remains authoritative.
run_on accepts an explicit connection for isolated tests and embedding; run remains
the compatible session-bus wrapper. Launch environment variables are unchanged.

## Verification

Protocol tests cover bounded headers and control/error validation. Private-bus
fixtures compare live introspection to XML and exercise staging, authentication,
partial commits and acknowledgement records. SDK tests verify actual connection
identity, owner-loss invalidation, re-registration and cancellation. The production follow-up now implements the daemon backend and KWin adapter;
[desktop-provider verification](desktop-provider.md#verification-status) covers
production staging, leases, authorization, queue limits and acknowledgements on
private buses, plus deterministic adapter behavior. Actual KWin JavaScript
marshalling and real-session behavior remain operator qualification work.
No Phase 2 gate or live desktop criterion is closed here.

In EWMH mode, owning `org.arinn.X11` is the identity check; the daemon does not
attest executable, unit or cgroup identity. Same-user processes can claim the name
when it is unowned. See [X11 runtime assumptions](x11-ewmh.md#runtime-and-identity).
