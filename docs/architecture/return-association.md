# Return-to-platform-window association

[ADR-0073](../adr/0073-return-window-association.md), contract task
[#212](https://github.com/LimeBeck/arinn/issues/212). Requirements: specification
§9 and §12; AC-02, AC-12, G-5.1 (partial), R-01, R-03, R-05, R-09, R-10.

**Status:** production daemon/KDE producer implemented in
[#217](https://github.com/LimeBeck/arinn/issues/217), following contract #212.
Both optional interfaces are exported at the daemon root. The asynchronous shell
consumer is implemented in [#219](https://github.com/LimeBeck/arinn/issues/219):
see [Home activity identity](ui-web-shortcuts.md#home-activity-identity-and-visual-treatment).
This enables browser-only deduplication when all identity evidence agrees; it
does not close an acceptance criterion/gate. Private-bus, offscreen and mocked
adapter tests are not real KDE qualification.

## Identity and ownership

There are four independent lifetimes, none interchangeable:

- `Status.epoch` (also the current `Status.generation`) is the daemon epoch.
- `Status.return_window` / `HomeRequested.target` is an opaque platform Return
  identity. The KDE adapter currently uses a KWin UUID. Clients never parse it.
- `WindowRef = (generation: s, token: s)` uses the authoritative PlatformWindows1
  inventory generation and opaque token. Tokens are never reused; disappearance
  invalidates them permanently, and remap mints a new token (ADR-0039).
- Browser window/tab IDs belong to a provider registration generation and unique
  owner. `BrowserWindow.window_token` is its already established platform link
  (ADR-0052), not the Return identity. That field has no encoded generation:
  clients never parse its spelling to manufacture one.

Only the platform resolves a Return identity to an exact live compositor object
and then to that same object's token in its committed inventory. No title, URL,
resource class, product name, last-focused-window or creation-order guess is
allowed. Zero or multiple candidate objects/tokens mean no association. A
provider cannot assert the Return mapping. This uses the existing trusted KWin
owner/session boundary; it changes neither the plugin trust policy nor control
permissions. Clients continue sending Back/Return through the existing daemon
API, never calling PlatformWindows1 controls directly.

## Public optional snapshot

`org.arinn.ReturnAssociation1` lives at the existing daemon service/root path
`org.arinn.Daemon1`, `/org/arinn/Daemon1`. It is read-only shell/CLI metadata,
not a provider capability. [XML](../plugin-api/return-association1.xml).

`Get() -> (epoch: s, revision: t, target: s, window: a(ss))` returns one struct.
The array encodes a typed optional `WindowRef`: length zero or one only. Empty
means unresolved, ambiguous, hidden Home, or unavailable platform; it never means
that the existing Return target should be removed. An empty target requires an
empty array. A nonempty target can validly have no association.

- Epoch is the current non-nil daemon UUID. Revision is positive and increases
  whenever the published target or optional mapping changes, including clears.
  Identical content need not emit a signal. Revision overflow requires a fresh
  daemon epoch before another value is published; do not wrap or reset in place.
- Target follows the existing 8 KiB ordinary-text bound, no NUL. A present window
  has a non-nil platform UUID and a nonempty token of at most 256 UTF-8 bytes.
  Maximum cardinality is one, with no paging or untyped metadata.
- `Changed(epoch: s, revision: t)` follows the atomic snapshot commit, including
  invalidation. It is a state notification, never a Home presentation request.
- `Get` still returns a coherent empty mapping during platform loss. The target
  mirrors the existing Home state; availability of this optional API must not
  change its Return/Back behaviour.

Consumer sequence: discover the optional interface for the current unique daemon
owner, subscribe before `Get`, and reconcile notifications against the reply's
epoch/revision. Discard a reply older than the latest notification or installed
revision. A signal with a different epoch invalidates cached state and requires
reconciliation against the current owner/Status. Drop all cached mappings,
subscriptions and pending reads on owner loss; pin calls to unique names. Missing
interface/method, failed read, malformed data or unsynchronized initial state
means no association and keeps Return. Retry reads through normal bounded
reconnection; never retry mutations or synthesize Home intent.

Before using a mapping, the shell must have current Home state with matching
epoch and exact target and Home visible. A Home/context change invalidates the
local match immediately, before asynchronous reload. The snapshot is evidence
for a window identity, not authority to hide Return on its own.

## Optional trusted adapter report

`org.arinn.KWinReturn1` is a separate optional interface on that daemon root.
[XML](../plugin-api/kwin-return1.xml). No existing KWinBridge1 or Daemon1 signature
changes. The adapter discovers support per daemon owner, pins calls to that
unique name, and uses the existing KWinBridge1 inventory session.

`Report(generation: s, inventory_revision: i, target: s, token: s)` takes four
scalar arguments plus callback, within KWin's callDBus limit. Revision is a
positive int32, exactly the last committed inventory revision; the public
association revision is an independent uint64. Empty token explicitly clears
the optional mapping. Nonempty token requires nonempty target. Text/ID bounds
are as above. The report contains no provider ID, executable, title or URL.

The producer must perform these checks together with publication:

1. Authenticate the current unique owner of `org.kde.KWin` **and** the owner of
   the live connected inventory session. Otherwise `PermissionDenied`. A new
   KWin owner must Connect/rebuild inventory; acquiring the name alone cannot
   reuse an older owner's generation.
2. Validate scalar bounds/types (`InvalidArgument`); require a live inventory
   and visible Home (`Unavailable` otherwise).
3. Require exact current inventory generation/revision and exact current opaque
   Return target (`Conflict` for a stale context). Never compare a daemon epoch
   or provider generation as the inventory generation.
4. For a nonempty token, require membership in that committed inventory
   (`NotFound` otherwise). The trusted adapter supplies the exact-object
   assertion; the daemon cannot derive a KWin UUID/token relation independently.
5. Atomically publish the optional reference and emit `Changed` if content
   changed. Rejected reports neither replace a good mapping nor emit a signal.

The adapter first commits inventory, reports existing Home state/intent through
its unchanged path, then reports the optional association from the saved Return
object. It rechecks target, inventory session and revision before sending; it
never waits for this metadata before normal Home display or focus. If inventory
changes while a report is in flight, discard the stale report and recompute from
current objects. Never replay a report blindly after a timeout/reconnect.
Old daemon or unsupported bridge: continue existing Home/Return, with no mapping.

## Invalidation and provider joins

The producer serializes Home state, inventory changes and association publication.
Every accepted `ReportHome`/`ReportHomeRequest` first invalidates the previous
association (even if its target string repeats); normal status/intent handling
continues. The adapter may then publish fresh evidence. Return dismissal, target
change, loss/removal of its window token, KWin owner/session loss, heartbeat
expiry, and platform generation or daemon owner/epoch change clear the mapping.
Ordinary inventory updates, including focus moving to the shell, preserve a
mapping while its exact generation/token remains live. Reappearance/remap never
redirects an old token; it requires a new explicit report with the new token.

For Home/service deduplication, the shell joins this mapping only with complete,
validated **current** provider snapshots mediated by the daemon:

- Each candidate retains provider ID, current registration generation and local
  browser window ID. Owner loss/re-registration, provider unavailability or an
  incomplete refresh invalidates that provider's cached candidate. Never carry a
  local-ID match into a new generation or choose a different provider by order.
- Producer follow-up must validate exported browser `window_token` links against
  its live inventory and current provider registration; revoked/nonexistent
  links must not be used as positive evidence. Never expose an old cached link as
  valid merely because its string equals a token. Tokens are globally nonreused,
  so a validated live token resolves to exactly one platform generation without
  changing BrowserWindow's existing signature.
- Across all eligible current browser snapshots, exactly one window may match
  the optional token. Duplicate claims, unresolved provider inventory, or any
  uncertain association retain Return. Do not pick the first candidate.
- A browser window need not be `focused`: an ADR-0052 link persists while both
  identities exist after Home takes focus. The new join does not recreate or
  change that provider's focus-agreement algorithm.
- Identify any displayed service/tab inside the uniquely matched window using
  that provider's current coherent snapshot. A missing/ambiguous display target
  keeps Return; this contract creates no tab-level identity or guessed fallback.
- Provider restart invalidates the **service match**, not the platform-owned
  Return mapping or Return action. Fresh registered evidence is required to join
  again. Independent media sessions never match via this browser-only evidence.
  A browser's own media session joins its tab only through
  [ADR-0096](media-association.md); other media/window association remains #189.

The consumer recomputes on each relevant state/provider change and clears stale
matches while reconciling. Local D-pad and focus changes never wait for these
reads. The contract supplies identity evidence; product presentation continues
to follow #207 and the accepted Home behaviour.

## Separately zoned production follow-ups

1. **Producer (daemon + platform):** `platform/kde/**`, relevant
   `crates/arinnd/**`, and private-bus/adapter tests. Export both optional
   interfaces, resolve exact objects, serialize report validation and Home/
   inventory invalidation, enforce owner/session and bounds, publish revisions,
   and sanitize provider window links for this use. Exercise Home before inventory
   is ready, reconnect, remap, two windows, duplicate claims and late callbacks.
   Keep the existing opaque Return and Home intent path working throughout.
2. **Consumer (shell):** `apps/arinn-shell/**`, `tests/ui/**`, and matching UI
   technical docs. Add an asynchronous owner-pinned optional model with subscribe-
   before-read/revision reconciliation; join current provider windows without
   focus guesses; retain Return until one safe display match exists. Verify Home
   over a mapped browser after focus loss, absent support, independent media,
   ambiguous providers, restarts, remap, and delayed/out-of-order replies. Preserve
   focus/Return access during all unresolved states. This completes the deferred
   identity-dependent portion of #207 only with its own evidence.
3. **Owner-run qualification:** explicitly authorized real KDE/Wayland Home,
   Return and remap checks after both implementations. Fixtures in this PR do not
   establish compositor behaviour, daily-use readiness or full acceptance.

The producer precedes enabling the consumer. These are requirements for future
issues, not claims or authorization to implement additional tasks in #212.


## Production implementation notes

The inventory mutex serializes association reports, inventory replacement and
Home commits. Home state still lives in the existing bridge; the lock order is
inventory then Home. Authorization is awaited before acquiring the inventory
lock; every accepted daemon Home report then enters that serialized commit path.
A private Home commit revision and inventory generation fence are captured before
that lookup and rechecked under the lock. Every accepted Home report advances
the private revision, even if its public association content is identical. A
stale continuation returns `Conflict` without touching Home or association state;
a session reset invalidates the captured generation as well. This fence is not
a wire revision or a new contract field.
Association notifications are emitted under the same inventory lock after commit,
so their revisions remain ordered. Inventory reset keeps the opaque Home target
and daemon epoch while invalidating the optional mapping. The owner monitor and
five-second heartbeat expiry clear it; `Get` also checks current owner/liveness.
A transient owner lookup failure fails the read with `Unavailable` without
publishing an identity loss. The last verified link stays private until a later
successful read verifies the owner/liveness again. Confirmed absence, replacement
or heartbeat expiry still clears the link; lookup uncertainty never exposes it.
Revision exhaustion fails this optional API closed until daemon restart supplies
a fresh epoch; it never wraps or resets the revision within one epoch.

The adapter discovers `KWinReturn1` per unique daemon owner and pins reports to
that owner. It requires the current Home acknowledgement, committed inventory,
and exactly one live object identical to the saved Return object. A duplicate
identity, pending inventory, stale callback or absent interface supplies no
positive evidence. Remap obtains a fresh token and needs a fresh report. Home
raising and Return continue without waiting for optional metadata acknowledgements.
The adapter records a report as accepted only after its acknowledgement. If KWin
drops an error/timeout callback, the watchdog may retry at most once every five
seconds for the same context, after recomputing the current object, inventory and
Home evidence. Old attempt acknowledgements cannot suppress a newer retry;
accepted unchanged evidence is not replayed.

`Browser1.GetTabs` copies its page and registration under the registry lock,
performs bounded bus/liveness lookups without that lock, then rechecks the current
registration and link evidence before returning. Concurrent owner/generation
replacement rejects the pending read; concurrent link revocation is revalidated.
It validates registration/owner, refresh health and live platform
links before returning cached evidence. A revoked, nonexistent or unavailable
link fails the read with `Unavailable`; a page from an obsolete registration
fails with `Conflict`. Leased pages keep their original contents and revision:
previous links must still agree with the current registered browser snapshot.
The daemon does not rewrite immutable pages or invent provider revisions.
Unlinked snapshots, including normal empty offline browser snapshots, remain
readable without platform evidence.
A failed read keeps Return in the shell consumer, and may temporarily make the
browser snapshot unavailable until its provider publishes fresh links. Multiple
valid claims are not resolved by order; the consumer must still require one
unambiguous candidate across complete current provider snapshots.

The producer tests cover real daemon exports on a private bus, XML equality,
report authentication, hidden-Home precedence, rejected-report preservation,
Home invalidation, focus-only inventory changes, removal/session/owner loss,
heartbeat expiry and browser-link validation. Mocked JavaScript tests cover
exact objects, two windows, duplicate identities, remap, delayed Home/inventory/
discovery callbacks and old-daemon compatibility. These tests do not prove
compositor behavior or shell deduplication.
