# ADR-0094: Associate a browser's media session with its audible tab

- **Status:** Accepted
- **Date:** 2026-10-07
- **Deciders:** Owner (product behaviour, option A in [#359](https://github.com/LimeBeck/arinn/issues/359)); delegated technical choice for the contract
- **Amends / Supersedes:** —
- **Amended by / Superseded by:** —
- **Specification:** §5.1, §7.1, §7.3, §10; R-01, R-03

## Context

A video playing in a browser tab reaches Continue twice. The browser provider
publishes the tab, and the MPRIS provider publishes the browser's own media
session. Chromium-based browsers name it `org.mpris.MediaPlayer2.chromium.instance<pid>`.
#207 keeps independent sessions apart because no verified identity links
them, and titles must never be compared. The owner chose option A: while a
reliable link holds, Continue shows the tab card with the session's
playing/paused state, Return opens the exact tab and play/pause goes to the
session. Without a link both cards stay.

Two facts give a verifiable identity without titles or URLs:

- the process that owns the MPRIS bus name is the browser process
  (`GetConnectionUnixProcessID`);
- the Chrome native-messaging host is started by that browser process, so
  the browser provider can resolve the browser process of each connected
  browser from the host's connection.

A browser has many tabs but one media session per process. The extension
already reports each tab's `audible` flag ([ADR-0048](0048-browser-wire-contract.md)).
Providers are isolated and identifiers are opaque. The shell must not join
provider data on its own beyond verified daemon evidence
([ADR-0073](0073-return-window-association.md)).

## Decision

Arinn adds three optional interfaces. No existing signature changes.

1. **`org.arinn.MediaProcess1`**, exported by a media provider next to
   `MediaProvider1`: `GetProcessId(generation, id) -> u`. It returns the
   process ID of the player that owns the session, or 0 when unknown.
2. **`org.arinn.BrowserProcess1`**, exported by a browser provider next to
   `BrowserProvider1`: `GetProcessId(generation, window_id) -> u`. It returns
   the browser process ID of the window, or 0 when unknown.
3. **`org.arinn.MediaAssociation1`**, exported by the daemon at
   `/org/arinn/Daemon1`: `Get() -> (sta(ssssss))` and `Changed(epoch, revision)`.
   The snapshot carries the daemon epoch, a monotonic revision and up to 64
   links `(media provider, media generation, session, browser provider,
   browser generation, tab)`.

The daemon discovers the provider interfaces by introspection and joins them.
Process IDs stay between providers and the daemon. They are never published
to the shell, logged or sent to telemetry. A session links to a tab only when
all of these hold:

- the session has a non-zero process ID;
- exactly one browser provider has windows with that process ID;
- exactly one session of all media providers has that process ID;
- exactly one tab in those windows is audible.

An established link holds while both entries exist with the same generations
and process IDs, and no other tab of that browser becomes the single audible
one. A pause therefore keeps the link, and the next unambiguous audible tab
moves it. Ambiguity, a provider restart, a generation change, provider loss or
a daemon restart removes the link. A browser in another PID namespace (Flatpak,
Snap) gives non-matching IDs, so nothing is linked. The shell hides the media
card only for a link whose six fields match its current provider snapshots.
Everything else keeps both cards.

The [media association contract](../architecture/media-association.md) is
normative for producers and consumers.

## Consequences

The MPRIS provider and the Chrome provider each gain one small interface. The
daemon gains a join over existing snapshots with bounded per-session calls,
and the shell gains an asynchronous consumer. These are separate follow-ups
to this contract-only change, as with ADR-0073. Media keys keep working through
Plasma because the MPRIS session itself is untouched. Missing interfaces,
other browsers and sandboxed browsers keep today's two cards. The core still
knows capabilities, not products (R-01): the join uses only process identity
and the generic `audible` flag.

## Alternatives considered

- **Matching titles or URLs** between the session and the tab: forbidden by
  #207 and unreliable.
- **Parsing `instance<pid>` from the MPRIS bus name or session ID:** IDs are
  opaque, and the bus-name convention is browser-specific.
- **Publishing process IDs in `Media1`/`Browser1` and joining in the shell:**
  it leaks process identity to every client and duplicates evidence rules
  outside the daemon.
- **Requesting `activeTab` or tab-to-session data from the extension:** it
  needs more extension permissions than G-3.1 allows, and Chrome exposes no
  tab-to-MPRIS mapping.
- **Linking only while a tab is audible:** a pause would bring the duplicate
  card back immediately.

## Verification

- Protocol validation and roundtrip tests for the new types.
- Live-export tests that compare a fixture to the three XML files.
- Unit tests of the reference `media_association::join`: a link, an ambiguity
  (two audible tabs, two sessions with one process, two browser providers), a
  pause hold, a move to another tab, and a generation or process change. A
  private-bus fixture covers a daemon without the interface.
- Production producer, daemon and shell tests in the follow-up.
- Real Chrome/KDE evidence that the process IDs match needs explicit owner
  permission. Not run.
