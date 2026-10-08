# Browser media-session association

Normative contract for [ADR-0094](../adr/0094-browser-media-session-association.md)
([#359](https://github.com/LimeBeck/arinn/issues/359)). A browser's own media
session (for example `org.mpris.MediaPlayer2.chromium.instance<pid>`) and the tab
that plays it are one item in Continue. This document states what each side may
publish and when a link holds. Product behaviour is spec §5.1 (owner option A).

## Interfaces

| Interface | Owner and path | Members |
| --- | --- | --- |
| [`org.arinn.MediaProcess1`](../plugin-api/media-process1.xml) | Media provider, `/org/arinn/Provider1` | `GetProcessId(s generation, s id) -> u` |
| [`org.arinn.BrowserProcess1`](../plugin-api/browser-process1.xml) | Browser provider, `/org/arinn/Provider1` | `GetProcessId(s generation, s window_id) -> u` |
| [`org.arinn.MediaAssociation1`](../plugin-api/media-association1.xml) | Daemon, `/org/arinn/Daemon1` | `Get() -> (sta(ssssss))`, signal `Changed(s epoch, t revision)` |

All three are optional. Existing `MediaProvider1`, `Media1`, `BrowserProvider1`
and `Browser1` signatures do not change. Types and validation are in
`arinn_protocol::media_association`.

## Provider side

- `GetProcessId` answers only the registered daemon owner. Any other caller
  gets `PermissionDenied`: process IDs never reach the shell or other clients.
- Arguments are the current registration generation and one current local ID.
  A stale generation is `Conflict`, an unknown ID is `NotFound`, and malformed
  arguments are `InvalidArgument`.
- The result is the process ID of the process that owns the session's media
  endpoint (`MediaProcess1`), or of the browser process the window belongs to
  (`BrowserProcess1`). 0 means unknown. A provider never derives a process ID
  by parsing IDs, bus-name suffixes, titles or URLs.
- **MPRIS provider (reference producer):** `GetConnectionUnixProcessID` of the
  player's bus name, read when the name appears and dropped with it.
- **Chrome provider (reference producer):** the parent process of the
  native-messaging host connection (`GetConnectionUnixProcessID` of the host's
  bus name, then its parent from `/proc/<pid>/stat`). It is recorded per
  connection at attach time and applies to every window of that connection.
  A host that cannot resolve it reports 0.
- A provider publishes a new snapshot revision whenever the process behind a
  current local ID changes. The MPRIS provider removes a session when its bus
  name loses its owner; the Chrome provider gives a new connection new window
  IDs.
- Neither value is logged or sent to telemetry (spec §13).

## Daemon join

The daemon discovers the interfaces by introspecting each registered provider
of the matching capability. It reads process IDs only for current sessions and
windows, with bounded concurrency and the normal call timeout. It caches them
per (provider, generation, snapshot revision, local ID). Every new snapshot
revision of a provider re-reads its process IDs, so a session or window whose
process is replaced under the same local ID is never judged on stale evidence.

`GetProcessId` answers for the provider's state at call time, which may be
newer than the snapshot the daemon read. After its lookups, a pass therefore
compares each provider's generation and snapshot revision with the ones it
started from. If any changed, the pass makes no new link and moves none; it
only drops links whose session or tab is gone, and the next pass starts over
from the new snapshots. Every link is thus judged on process IDs and tabs from
one snapshot revision of each provider.

For each media session with a non-zero process ID `P`, a link to tab `T` is
**established** only when all of these hold:

1. exactly one media session across all media providers has process ID `P`;
2. exactly one browser provider has windows with process ID `P`;
3. exactly one tab of those windows has `audible = true`, and that tab is `T`.

Every window counts for rule 2, including a window without tabs. The daemon
keeps the process ID each link was established with; it is never published.

An established link **holds** while:

- the session and the tab still exist under the same provider generations;
- both process IDs are unchanged and equal to the one the link was
  established with;
- no other tab of those windows is the single audible tab.

A pause, which clears `audible`, therefore keeps the link. A different tab
becoming the only audible one moves the link to it. Two audible tabs keep an
established link but never create one.

A link is **removed** on any of these:

- the session or tab disappears;
- a provider generation changes or a provider is lost;
- a process ID changes or becomes 0;
- the conditions for a unique session or browser provider fail;
- the daemon restarts.

The daemon never picks the first of several candidates. `arinn_protocol::media_association::join`
is the reference implementation of these rules, and its unit tests are the contract fixtures.

The snapshot carries the daemon epoch and a revision that increases with every
content change (`checked_add`; on overflow the interface stops publishing
links). `Changed(epoch, revision)` invalidates readers; it never requests Home
presentation. At most 64 links are published, and a session or tab appears in
at most one link.

## Shell consumer

- The shell installs its `Changed` match **before** its first `Get`, so no change
  between them is missed. It re-reads on `Changed`, on a daemon owner change and
  on provider changes. Local focus never waits for it.
- A `Get` reply applies only when its epoch is the current daemon's epoch and its
  revision is not lower than the last applied one. A reply from before a newer
  `Changed` or reply is discarded, so an older in-flight read never overwrites a
  newer link. A new daemon owner or epoch resets the last applied revision and
  clears the links.
- A link applies only when all six fields match the shell's current, complete
  provider snapshots: media provider ID and generation with the session ID, and
  browser provider ID and generation with the tab ID. Anything stale or missing
  keeps both cards.
- For an applied link, Continue shows one card for the tab, with the session's
  playing or paused state:
  - Return or OK opens the exact tab;
  - play/pause on the card goes to the session;
  - the separate media card is hidden.
  
  When the tab is the current service, that card is the current-service card.
- Media keys keep working through the desktop because the MPRIS session is
  untouched.

## Limits

- A browser in another PID namespace (Flatpak, Snap) reports non-matching IDs,
  so nothing is linked. Other browsers without a provider interface are not
  linked either.
- `audible` can lag briefly behind playback. The hold rule keeps a link
  through a pause, and an ambiguous moment keeps today's two cards.
- That the native host's parent is the process owning the MPRIS name has to be
  confirmed on a real Chrome/KDE session. That check needs explicit owner
  permission and is not part of the contract tests.

## Follow-ups

The producers (`plugins/mpris`, `plugins/chrome`), the daemon join and export
(`crates/arinnd`), and the shell consumer (`apps/arinn-shell`) are implemented
separately after this contract. Each comes with private-bus tests for link,
ambiguity, pause hold, move, generation change and missing interfaces.
