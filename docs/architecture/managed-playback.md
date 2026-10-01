# Managed playback window association

[ADR-0074](../adr/0074-managed-playback-window.md) implements existing specification
§6.4 and ADR-0046. The contract is implemented by the mpv producer and daemon routing; adapter stacking and shell integration are delivered separately in #141.

## Identity

A provider with `MediaWindow1` exports `MediaWindowProvider1.GetLaunchId` on its
registered connection and provider path. Generation and session checks are the
same as MediaProvider1. An unknown session is NotFound, an obsolete registration
is Conflict, and a stopped session has an empty launch ID. A live managed player
process receives a random non-nil canonical lowercase hyphenated launch UUID before it starts. Its exact Wayland
application ID is `org.arinn.Playback.<UUID>`; this is a protocol-owned identity,
not a product name, title, resource-class guess or desktop-entry lookup. For native XDG toplevels KWin's
[handleAppIdChanged](https://github.com/KDE/kwin/blob/master/src/xdgshellwindow.cpp)
sets resourceClass from the exact protocol app ID. The adapter reads that property
only to extract the complete protocol-owned prefix plus canonical UUID, never to
recognize a product. mpv supports setting this with
[--wayland-app-id](https://mpv.io/manual/stable/#options-wayland-app-id).
The provider reads that same identity from its own player on reconnection. It never
asserts an arbitrary platform token. Unmanaged players return an empty identity.
Several files in one player can share a launch ID; sessions remain distinct.

The trusted adapter exports observations through `KWinMedia1.Report` at the
root daemon path. Authenticate the current KWin unique owner, live inventory
generation and exact committed revision. The reported token must be in that
inventory. The adapter resolves only the exact application ID of that object;
empty launch ID explicitly clears its association. Every new inventory revision
invalidates observations until they are reported against that revision. At most
one report per live token is retained. Resolution waits for a report, including
an empty one, for every token in the committed inventory. This completion barrier
prevents a partially reported duplicate from resolving. Duplicates of a launch UUID are ambiguous
and never resolve. Observations are bounded by the inventory limits. No launch
identity, window title, URL, file path or command line is logged.

The daemon exports optional `MediaWindow1` on each media provider endpoint.
`GetWindow(generation, id)` checks the current registered provider and complete
session snapshot, negotiates `MediaWindow1`, then asks the pinned provider owner
for the launch ID. After the asynchronous call, recheck provider, session and
platform lifetimes. Match against one complete current platform inventory and
exactly one current observation. Return zero or one `(platform generation, token)`
references. Empty means unresolved; no guessing from focus. Absent feature is
Unsupported. Unavailable backend is Unavailable. Reads have a 3-second total
budget. Subscribe to Changed before reading; it invalidates reads on platform,
provider or association changes and carries the provider registration generation.

Provider disappearance/re-registration, daemon/adapter owner changes, heartbeat
expiry, window removal and remap invalidate pending work. Field-only inventory
changes preserve an admitted control while its exact token, launch identity and
owners remain current; the control's own fullscreen/focus changes must not fence
out its acknowledgement. New association reads still require a complete report
barrier for the new revision.
A remap needs a new token and new exact observation. Provider restart may recover
the same player launch identity through its own private IPC, then republish it in
the new registration. Old generations never authorize the recovered player.
Existing Media1/Session and PlatformWindows1 signatures remain unchanged.

## Presentation and teardown

`Present(generation, id, request_id)` and `Release(...)` return Operation1 paths.
Request UUID deduplication, per-provider 32-operation limits, owner fencing and
5-second command timeout follow the existing operation contract. The fingerprint
includes method, provider, registration and session. Admission checks the current
registration and session; Present requires playing, paused or ended and a resolved
association. Recheck runtime readiness, backend availability and negotiated
MediaWindow1 support under the final provider lock before returning an
association or admitting Present. Store that exact presentation reference and its original flags.
Release is idempotent teardown of the presentation stored for that registration
and session, even after Stop clears the provider launch ID or the window vanishes.
No stored presentation means successful teardown with no compositor mutation.
Release never looks up a replacement window or changes another session. A per-registration/session exit fence rejects an older Present still awaiting provider identity when Release completes first. Exit records are bounded to 100,000 and cleared on provider loss. Known errors
are rejected before dispatch. Missing support never reports success.

Commands use optional KWinMedia1.NextControl(generation), separate from the
unchanged KWinBridge1 queue. It waits at most one second; at most one poll may be
pending. Association-only notifications re-arm that poll within its original
deadline rather than returning idle on every inventory or session update.
Empty is idle; otherwise the record is [request UUID, action, token],
where action is `present` or `release`. Present names an exact live window.
Release names the remembered token, or empty when only shell teardown remains.
CompleteControl(generation, request_id, error) acknowledges dispatch using the
existing normalized error set. Unknown/completed/obsolete requests are Conflict.
A failed dispatched Present keeps its tentative exact scope for one bounded internal
Release, because compositor flags may have changed before the error. An adapter
with no stored presentation treats that cleanup as a no-op; successful cleanup
clears remembrance. Each dispatched Present has one automatic cleanup attempt,
bound to that exact presentation request. Failed or expired Release does not
reopen the attempt on later invalidations; an explicit Release can still retry
using the remembered scope. A replacement Present starts a new attempt, and old
internal cleanup cannot tear it down even for the same session. Provider loss
preserves already-admitted Release cleanup. Provider and session loss wake the
media lane after invalidation. Skipping a stale Release still leaves the next
Present behind the current complete-report barrier.
Commands expire after five seconds and are never redelivered. The total queue
limit is 1024 and shares per-provider operation admission limits. Client admission reserves one slot for owner/session-loss teardown. Polling does not
replace the existing inventory heartbeat. Old adapters without this optional
interface never receive these commands and presentation is Unsupported.
Daemon interface introspection alone does not prove adapter support: the first
successful authenticated NextControl in the current inventory generation is the
support handshake. Clear that support on every owner/instance/generation reset
and heartbeat expiry. Present is Unsupported until that handshake completes. Present makes video fullscreen on its
current output, keeps the transparent shell above it and focuses the shell.
Release restores the shell's ordinary Home presentation without closing a player.
Stopping playback remains Media1.Stop, so leave sends Stop and Release separately.
The shell preserves its Home/Detail origin locally and returns there after leave.
The adapter records and restores any temporary window flags it changes.

Home switches to the opaque shell while preserving the managed playback target;
Return restores the transparent input layer above that exact player. Focus changes
caused by those operations never select a different session. If an unrelated
application takes focus, release input ownership rather than stealing it back.
Window loss, daemon loss, adapter reconnect or session lock releases the layer.
The independent session guard continues to hide the shell on lock/unknown state.
Late replies never raise a surface after cancellation or lifetime invalidation.

## Delivery and verification

#189 supplies typed bindings, validation, XML, the optional SDK
MediaExtensions.window handler and private-bus contract fixtures. SDK authorization,
stale registration/session checks and handler-result validation apply to launch
identity reads; MPRIS/base services export no window extension.
#221 implements provider/daemon association and routing: mpv launch identity, authoritative complete-inventory matching, authenticated optional KWin control lane, request deduplication and remembered session teardown.
#141 implements adapter stacking and shell integration after that producer lands.
Private-bus tests cover authentication, stale revision/generation, wrong session,
ambiguity, remap, restart and absent-interface compatibility. Actual KDE input,
video transparency, Home/Return and performance remain G-4.1 owner qualification.
