<!-- SPDX-FileCopyrightText: 2026 Arinn contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Platform window contract

[ADR-0039](../adr/0039-platform-window-contract.md) defines the pre-stable
provider-facing contract required by the desktop provider, implementing the
existing window-management requirements of specification §§7.2, 10 and 12.

**Implementation status (2026-09-28):** the daemon exports this interface with
production authorization, immutable paging leases and acknowledged KWin controls.
The desktop provider consumes it through the SDK. See the
[implementation and verification record](desktop-provider.md) for isolated test
evidence and outstanding real-desktop qualification. No Phase 2 gate is closed.

## Endpoint and ownership

`org.arinn.PlatformWindows1` lives on the daemon's existing service and root path,
`org.arinn.Daemon1` at `/org/arinn/Daemon1`. See
[versioned XML](../plugin-api/platform-windows1.xml) and
[protocol types](../../crates/arinn-protocol/src/platform.rs). It is a platform
service, not a seventh plugin capability and not a provider endpoint. Public
application controls still route to the owning ApplicationProvider; that provider
uses this interface for the platform part of activation or graceful close. The
shell continues to use the daemon's application/Activity API.

The production server must authorize each sender against a live registered
provider declaring `application`, including its current registration generation.
Other callers receive `PermissionDenied`. The interface does not authorize a
provider to supply scripts, shell commands or compositor code. Installed plugins
remain trusted local software under the existing security policy; this contract
does not introduce a sandbox or change that policy.

The trusted platform layer owns window discovery, identity resolution and
compositor operations. The core routes typed requests and checks lifetimes; it
never scans desktop files or selects products. The internal compositor transport
is not this provider-facing interface. Any additional wire contract needed by
that transport must be reviewed separately before changing public protocol/XML;
this task does not silently extend the existing `Daemon1.ReportWindow` method.

## Window identity

Each window record has this exact field order:

| Field | D-Bus type | Meaning |
| --- | --- | --- |
| `token` | `s` | Nonempty opaque live-window identity, at most 256 UTF-8 bytes |
| `desktop_id` | `s` | Explicit resolved desktop-file ID, or empty if unproven |
| `output` | `s` | Opaque output identifier, or empty if unknown |
| `active` | `b` | Foreground external application window |
| `fullscreen` | `b` | Platform-reported fullscreen state |
| `can_activate` | `b` | Targeted activation is supported |
| `can_close` | `b` | Graceful window-close request is supported |

Nonempty `desktop_id` and `output` obey the same ID bounds. `desktop_id` is an
exact desktop-file ID including `.desktop`, never an executable, path or title.
Platform-specific conversion from compositor metadata must establish that exact
identity; ambiguous/unresolved metadata produces an empty field. No fuzzy title,
URL, product-name, process-name or resource-class matching is permitted in core.
The desktop provider may resolve the explicit ID against its own catalogue; an
unknown ID is not an invitation to guess a launch target.

Several windows can share one `desktop_id`; they always have distinct tokens.
Unknown applications remain distinct windows. No title, URL, artwork or process
command line is included. Shell, lock-screen and non-application compositor
surfaces are excluded. At most one exported window is active; zero is valid when
the shell or another excluded surface has focus. A missing output does not move
the window to a guessed output.

A non-nil UUID `generation` belongs to the platform session and is independent of
provider generations. It changes on daemon restart, compositor/adapter reconnect,
or loss of authoritative inventory. Window tokens are never reused, including
across platform generations. Removal invalidates a token permanently. A remapped
window gets a new token; providers must republish the resulting running instance
and explicit association. The old token never redirects to that new window.

## Coherent enumeration

`GetWindows(token)` returns one struct `(generation, revision, windows, next_token)`
with signature `(sta(sssbbbb)s)`. Start with an empty token. A nonempty next token
is opaque and bound to the caller, provider registration, platform generation and
one immutable revision. Subscribe to `WindowsChanged(generation, revision)` before
reading, buffer notifications and reconcile after installing a complete snapshot.
A late subscriber can observe any positive revision. An empty catalogue is valid;
an empty nonterminal page is invalid.

- Revision starts at 1 and increases only on inventory/field changes. An unchanged
  inventory need not emit a signal; overflow starts a fresh platform generation.
- Pages contain at most 200 windows and 256 KiB of actual encoded D-Bus body.
  IDs are unique across the complete snapshot and only one window may be active.
- Snapshot storage/client accumulation is bounded to 100,000 windows and 16 MiB
  of encoded pages. At most 32 live paging leases are retained by the server;
  capacity exhaustion returns `Unavailable` without evicting a valid lease.
- Leases expire 30 seconds after the first read, are not renewed by subsequent
  pages, and preserve their original revision during ordinary updates. Removal
  of a window can therefore appear in the next snapshot while its old command
  token already returns `NotFound`.
- Expired/unknown paging tokens or invalidated platform generations return
  `Conflict`; a token belonging to another caller returns `PermissionDenied`.
  Disconnect or provider re-registration drops its leases. An unavailable
  platform returns `Unavailable`, not a misleading empty catalogue.

After `Conflict`, discard the partial snapshot and explicitly start a new read.
Do not combine revisions, generations or duplicate tokens. The SDK bounds an
entire snapshot attempt to 3 seconds (stricter than a separate budget per page).
It returns no partial result and performs no automatic retry. Callers reconcile
signals and schedule their own new snapshot attempts.

## Controls and operation lifecycle

`Activate(generation, token, request_id)` and `Close(generation, token, request_id)`
return a daemon-owned `org.arinn.Operation1` path after admission. They use the
existing operation type and rules of the
[implementation baseline](implementation-baseline.md#commands-and-errors).
Generation and request ID are UUIDs; the token is an opaque ID, never a command.
There is no launch, force kill or process-wide termination method here.

After authorizing the sender and validating the argument bounds, check existing
request-UUID deduplication: the same caller/request/content returns the existing
operation even if its window has since disappeared; different content returns
`Conflict`. For a new request, an unavailable backend returns `Unavailable`, an
old platform generation returns `Conflict`, an unknown token in the current
generation returns `NotFound`, and an unsupported action returns `Unsupported`.
Reject errors known at admission immediately; errors discovered during dispatch
complete the operation as failed. Do not dispatch a stale or unsupported request.

Controls have a 3-second admission/read budget and a 5-second execution budget.
The daemon enforces the existing 32-concurrent-operations budget per provider
across platform commands and other work, so this interface cannot bypass it.
Caller/request deduplication lasts 5 minutes after completion within one daemon
epoch, including both method and all arguments in the content identity. Do not
automatically retry mutations after a timeout or owner change. Reconcile inventory
because a timed-out side effect can already have happened.

`Close` success means the platform accepted/dispatched the graceful close request,
not that the window or application exited: a save dialog may keep it open.
`Activate` success means dispatch to the specified window, not proof it gained
focus; use later inventory to observe foreground state. A supported request may
still fail with `PermissionDenied`, `Unavailable` or another normalized error.
No success timer substitutes for a platform result. Both operations are currently
non-cancellable and have no entity result refs. They do not kill a provider or its
application scope and do not change window placement or display configuration.

Provider disconnect/re-registration, platform-generation change or daemon loss
fails pending operations with `Unavailable`; late backend replies are ignored.
Platform disappearance also invalidates tokens and snapshots. Clients rebuild
subscriptions and reload after recovery. The SDK pins calls and operation reads
to one unique daemon owner, checks ownership before/after snapshot/control calls,
and watches owner changes while waiting for operations. A new client is required
after daemon ownership or provider registration changes; an old client never
silently sends a command to the replacement daemon.

## Verification and remaining work

`cargo test --locked -p arinn-protocol -p arinn-plugin-sdk` checks wire types,
validation, typed client reads, malformed/mixed/cyclic pages, signal subscription,
XML against live fixture introspection, immediate and delayed completion, failure
propagation, disappearance/remap, stale generations, owner replacement and bounded
waits. Each integration test creates its own `dbus-daemon --session` and destroys
it on exit, without connecting to the user's session bus or starting KWin.
The fixture models selected server behaviours; it is not reusable production code
and does not prove production authentication, lease/queue enforcement or cgroup
isolation. `dbus-daemon` and Python 3 are required, as in the repository test setup.

The production backend and adapter tests now cover authentication, atomic staging,
caller-bound leases, deduplication, shared queue limits, dispatch acknowledgements,
provider lifetime and late replies. The private-bus desktop suite exercises the
actual daemon/provider processes and GIO with a test manager. See
[desktop-provider verification](desktop-provider.md#verification-status).
G-2.1/G-2.3 real-desktop evidence and G-2.2 actual cgroup qualification remain pending.
The internal compositor transport is [documented separately](kwin-transport.md).
