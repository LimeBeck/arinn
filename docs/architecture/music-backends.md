# Music backend implementation increment

Status: **local/Yandex native integration merged in [#710](https://github.com/LimeBeck/arinn/pull/710); browser sign-in and Music API verification follow-up [#771](https://github.com/LimeBeck/arinn/issues/771)**.
Parent tasks [#707](https://github.com/LimeBeck/arinn/issues/707) and
[#677](https://github.com/LimeBeck/arinn/issues/677) retain qualification work.
The owner requested one implementation MR for local/Yandex and the selected
single-row layout B; #710 consolidated the earlier design PR #700.
The [research](music-discovery.md) remains the source for feasibility limits.

## Current code boundary

| Package | Implemented in this increment | Still missing |
| --- | --- | --- |
| `arinn-music-common` | Metadata, bounded finite/radio queue, private atomic state, owned audio-only player, natural playback events and continuous refill | Installed audio qualification |
| `arinn-plugin-music-local` | Persistent catalogue/playlists, registration, snapshot paging, root settings, background scan, finite playback, the library station (`station:library`, a shuffled continuous queue the provider tops up from the published catalogue; `radio` with a track or group plays the whole group first), bounded artwork extraction and native playlist save/delete | Native package/audio qualification |
| `arinn-plugin-yandex-music` | Device-code controller, staged Secret Service lifecycle, catalogue/search/liked content, signed stream resolution, artwork and continuous radio runtime | Live deployment-client, account and authenticated audio qualification |

The proposed [music contract](../plugin-api/music.md) and
[ADR-0134](../adr/0134-native-music-catalogue-and-session.md) add protocol, SDK,
daemon and native shell integration in the owner-authorized unified MR. New
executables/manifests are included in Arch/Debian install rules. The prior MR includes package validation; the #771 follow-up requires its own
checks and does not claim live account/audio qualification.

`Track` has no path, stream URL, token or header. Providers alone resolve files
and streams. The shell browses one source while the daemon projects the active
session independently. Native layout B uses one icon row; the Home music card
opens the player and offers a separate pause/resume action. Search reuses the
shell's on-screen keyboard. New Russian strings have a separate music catalogue
to avoid the active #718 translation zone.

The active session reaches the rest of the shell through `MusicCatalog`, not
MPRIS, which the native providers do not publish (#756 M1–M8, #794):

- Automatic Ambient may start while music plays: the owner decided on
  2026-11-06 that music does not hold it off (#756 M4 dropped). Video and
  other media sessions still do.
- `nowPlaying` is the Quick menu's Now playing row when no managed or MPRIS
  session plays; its OK pauses or resumes through `togglePlayback()`, as the
  Home card's action does.
- Home's Continue row keeps a playing or paused music card ahead of saved
  positions, so it is never pushed past the four further cards; a stopped
  session keeps its place (spec §5.1).
- `playbackProblem` turns the session's error code into a reason while the
  session stays stopped; providers report a failure as `stopped` and a natural
  end as `ended`. A provider may keep its last error after it skipped to the
  next track (#810), so the reason clears once the session plays, is paused,
  loads or ends. `notice` is the one status line under the music screens: a
  failed request, a denied account, a degraded source or a stopped session,
  never a bare code. The player keeps its state as the heading and shows the
  reason under the controls with Retry, which plays the failed track again; a
  station shows *Play next* instead while it has a loaded next track, because
  its `retry` only reloads the queue.
- `MusicView` fills the logical canvas itself, so its background, the confirm
  dialogs (the close flow's scrim and centred 900 × r48 plate) and the
  bottom-anchored keyboard panel are drawn on screen.

Focus and code rules of the music screens (#795, #756 M13–M16, M22–M27):

- Play/Pause is the player's first focus. A command in flight keeps it and its
  focus, and a press meanwhile is ignored. While it cannot act (a track
  loading), also when that happens under focus, the nearest working control
  holds focus and hands it back once Play/Pause works, unless the user moved
  on.
- The player's control row is its last row: Up from any control reaches the
  seek bar, and Down leaves the row only for a failure's Retry.
- Back returns from the queue to *Queue*, from a detail to the album, artist
  or playlist it was opened from (loading up to ten further pages to find
  it), and from a player opened on Home's card to Home, also after a queue
  replacement reopened that player. Another list, a source change or a
  failed page (`MusicCatalog.pageFailed`, not a failed command) ends that
  search, and it takes focus only while focus is still
  in the list. An empty last page ends it too.
- Scrims, dialog plates and radii are theme tokens (`Theme.scrim`,
  `dialogSurface`, `dialogRadius`, the shape-role radii). Icons are
  `LineIcon` glyphs. An album, artist or playlist title is Playfair Display
  (ADR-0137). Track times show hours from an hour on. A facet the shell has
  no label for is left out. Button widths follow their labels.
- `audible` (playing or loading) keeps interface sounds quiet, so none lands
  on the first note of a loading track. The Home card names the station.

Local settings apply on provider restart through the existing settings contract
(#704/#683). Scan publishes an immutable view; secure playback reopening does
not wait for a scan lock. Shutdown cancels further scan work between files;
an in-flight bounded ffprobe may finish before exit. Unavailable/incomplete roots
produce a degraded state while preserving last-known metadata.

Yandex account connection, catalogue and playback share the native provider.
The default registration now matches the pinned SDK used by CloudMus, following
the owner's explicit compatibility choice on 2026-11-04. Its embedded application
ID/secret are third-party compatibility data, not credentials owned by Arinn and
not user tokens. Only those public registration constants are included, with
pinned source attribution in `plugins/yandex-music/src/compatibility.rs`; no SDK
implementation or Python dependency is vendored. This replaces the earlier own
Arinn registration default, which completed OAuth but returned Music API 403 on
the owner's TV. Matching CloudMus's registration does not establish official API
support, service permission, subscription or regional eligibility.

`ARINN_YANDEX_CLIENT_ID` overrides the default; an explicit empty value disables
Connect. An override does not receive the compatibility registration's secret.
Stored tokens carry their issuing client ID; unknown/old-client tokens require
fresh sign-in rather than silently restoring under another registration.

Connect obtains the short code and exposes **Open sign-in in browser**. The
provider opens its verification page through the generic
[temporary authorization route](../plugin-api/browser-authorization.md). The
user enters/confirms the displayed code; polling receives the token automatically.
Phone confirmation remains possible. No redirect URI, backend server or
Authorization Code/PKCE flow is claimed for this third-party registration.
Temporary windows and their child tabs are excluded from Arinn tab snapshots and
recent-address history. Regular browser-profile history, cookies and site data
remain governed by the browser/user; Arinn neither reads nor clears them. This
is a temporary leased window, not an incognito session. Arinn retains cleanup
ownership on cancellation, expiry, disconnect or a failed launch, and returns
Home only after the dispatched launch and window cleanup settle.

Before publishing `connected`, both new and restored credentials must pass
`account/status` and the authenticated liked-tracks endpoint. An empty valid
library succeeds; HTTP 403 becomes `restricted`/PermissionDenied with an explicit
Music access explanation, rather than an empty connected library. Temporary API
failures become `unavailable`/degraded without deleting valid OAuth credentials.

If this registration requires a client secret, provision it locally through
Secret Service as a unique unlocked `text/plain` item with attributes
`application=arinn-yandex-client` and `client-id=<public ID>`. Never paste the
secret or a user access token into chat, settings, environment files or the
repository. An absent optional application-secret item permits a public client;
locked or ambiguous matching items fail closed. Invalid client configuration or
approval state is projected as an unavailable source without discarding the
journal or aborting provider registration. Missing Secret Service does not
prevent the provider starting, but account credential persistence still requires
it. Approval records and a stable nonsecret device ID use provider-private state;
user tokens use Secret Service only.

No live identity or account has been qualified in this environment. Its enforced
network policy currently excludes Yandex OAuth, Music API and media hosts.
Synthetic tests do not establish API eligibility, subscription or regional access.
This work does not change the phase status or close #707 before qualification.

## Queue invariants

- Queue entries have unique occurrence IDs even for repeated finite-playlist
  tracks. Removing an upcoming occurrence does not remove all copies of a song.
- A request binds its queue instance, session and request sequence. Stop, source
  replacement or feedback replacement invalidates outstanding work. An append
  request cannot silently replace the upcoming tail.
- The loaded end never loops automatically. A failed refill leaves the current
  entry and loaded tail unchanged; a new request can retry. An initially empty
  station can receive its first batch without inventing a current track.
- Replacement preserves the current occurrence and bounded prior history. At
  most 32 historical and 64 upcoming entries are retained, with a 256-track
  recent window. Recent/current/upcoming duplicates are filtered for radio;
  a batch containing no acceptable entries remains an explicit empty result.
- Playback emits events only after native readiness, successful navigation or
  natural completion. Radio feedback retains the originating batch even after
  later batches arrive. Seeking to the duration does not manufacture completion.
- Known token expiry, revoked leases and account changes cancel private work;
  late responses cannot populate a newer account or session. Failed refills
  preserve the loaded tail and expose retry without looping it.

## Local catalogue

The scanner reads configured absolute roots and candidates with MP3, FLAC, Ogg,
Opus, M4A, AAC and WAV extensions. An extension alone is insufficient: ffprobe
must report audio. Attached cover artwork is accepted; moving video is not part
of this audio backend. Synthetic format tests generate MP3, FLAC, Ogg/Vorbis, Opus, M4A/AAC, raw AAC
and WAV using packaged FFmpeg before probing them. These fixtures do not replace
package-install, artwork or real-library qualification; actual results are recorded
in the implementation MR.

Format and stream tags supply title, artist, album artist, album, disc/track
numbers and duration. Unknown metadata stays unknown; a missing title falls
back to the filename. Album identity includes root, album directory and tags;
recognized CD/Disc/Disk-number folders share their album parent. Identical titles
in different album directories do not merge. Move/rename policy within a root
is path-based: it leaves an unavailable old reference and creates a new entry.
Renaming the configured root itself while keeping its ID preserves track IDs.

Scans skip symlinks and cap traversal count/depth. Errors are isolated per file
or subtree. A missing or replaced root preserves last-known records, marks them
unavailable and prevents playback opening. A different device/inode at the root
does not silently become an empty authoritative rescan. Configured roots need
their first successful scan while mounted; initial-mount expectations and an
explicit replacement-root confirmation belong to the later root-settings work.
Incomplete scans conservatively mark unverified old records unavailable; they
do not delete references. The `Library` backend now orchestrates private save/load, root configuration,
rescans and ordered finite-playlist CRUD. Repeated track references are preserved;
unknown references are rejected, and unavailable tracks stay referenced. A
restored catalogue is unavailable for playback until a rescan revalidates roots
and file descriptors. Unchanged metadata can still reuse its cached probe.

Cached metadata avoids repeating ffprobe for unchanged files, but reopening is
still checked. Each path component is opened relative to an anchored directory
descriptor with no-follow semantics, including at playback resolution time. The
public backend returns an open file, not a path that can be swapped for a link
after validation. Root identity is checked again at that boundary.

ffprobe reads the inherited regular-file descriptor through `/proc/self/fd/0`.
It is invoked without a shell; stdout has byte/time limits and stderr is not
retained. Protocol/demuxer restrictions reject playlist/subresource probing and
disable MOV external data references. Timeout/error cleanup kills only the
owned process group and reaps its leader; exit is observed without reaping first
so cleanup cannot race PID reuse. No real desktop processes are controlled.

### Private local state

`Library::open` holds an exclusive advisory writer lock for its lifetime. State
is a versioned, bounded (96 MiB) compact snapshot of roots, catalogue and at most 256
playlists with at most 10,000 references each. It uses an owned private directory
(mode 0700), regular no-follow files (0600), anchored descriptor-relative opens,
exclusive temporary files, atomic rename and file/directory synchronization.
Unsafe, corrupt or unsupported snapshots fail closed and are never silently
replaced with an empty library. A failed pre-rename commit leaves live state
unchanged; a post-rename directory-sync error keeps memory aligned with the
committed file and reports uncertain durability. The library revision covers
successful root, catalogue and playlist changes, including startup invalidation.
Traversal and retained-record limits apply across configured roots and rescans.

The common state-file helper is provider-private implementation, not a D-Bus
contract. It is also used by the Yandex nonsecret approval checkpoint below.
Provider registration integrates state-directory selection, settings export and
background scan scheduling. Playlist metadata writes publish immutable views;
only catalogue references are removed when deleting a playlist.

## Yandex private adapter

The API adapter accepts a provider-owned authorized token in memory. The new
`DeviceAuth` controller obtains one through device-code confirmation and creates
the API client without returning tokens to its caller. No third-party OAuth client
registration or secret is bundled. The fixed HTTPS service origin, disabled
redirects, sensitive authorization header, response size limit and redacted
error enum keep service URLs/credentials out of diagnostics and shell models.

### Required connection flow

The owner selected device-code sign-in, matching CloudMus, on 2026-11-01.
CloudMus calls `Client.device_auth` and publishes a verification URL, user code
and expiry before receiving the access token automatically; see its
[Yandex auth backend](https://github.com/cloudmus/cloudmus/blob/d8f5c6dad9818a1f3bfe5935437f97ddf138818e/backends/yandex-music/cloudmus_backend_yandex/auth.py)
and [connection card](https://github.com/cloudmus/cloudmus/blob/d8f5c6dad9818a1f3bfe5935437f97ddf138818e/fronts/qt/src/Ui/AuthCard.cpp).

The production connection must:

- Start from Connect and show the provider's verification address, user code,
  expiry and waiting state. The user confirms access on Yandex's page from a
  phone or browser; Arinn receives the result automatically.
- Allow cancellation and retry after expiry or failure. Cancellation,
  disconnect or a newer attempt must invalidate late authorization results;
  they must not store credentials or reconnect the account.
- Keep device credentials, access tokens and polling inside the provider. Only
  the bounded verification prompt and account state reach the generic UI.
  Never collect the account password or require manual access-token copying as
  the normal connection path.
- Store credentials through the agreed secure provider storage lifecycle,
  restore the connection across restarts and remove local credentials on
  disconnect. Expired or revoked access must offer a new sign-in.
- Cover pending, success, denial, expiry, cancellation, retry, polling limits,
  restart and late-result races with synthetic tests before live qualification.

The device-code response may name the exact public short address
`https://ya.ru/device`. The provider normalizes only that literal alias to the
existing canonical `https://oauth.yandex.ru/device` prompt. Other hosts, paths or
decorated short addresses remain rejected, and OAuth requests still use the
fixed origin with redirects disabled. This compatibility fix does not qualify
account approval or authenticated Music API access ([#766](https://github.com/LimeBeck/arinn/issues/766)).

The provider-private flow is implemented with `POST /device/code` and
`POST /token` at the fixed HTTPS OAuth origin. It uses the upstream Yandex
[device-auth request semantics](https://github.com/MarshalX/yandex-music-api/blob/main/yandex_music/_client/device_auth.py)
(`grant_type=device_code`, `code`, client identity). Bounded responses, disabled
redirects, an allowlisted verification origin and redacted errors keep device
credentials and response details out of UI/logging. Polling respects the server
interval and slowdown, with cancellation and a monotonic expiry deadline.
Attempts bind their controller and generation; newer attempts reject older
results, including during slow storage. Persistence/cleanup tasks remain owned
when a caller drops its completion future. The private API client becomes
available only after the caller acknowledges completion; dropping that lease
revokes in-memory access synchronously before deferred cleanup.

`SecretService` stores tokens only in `org.freedesktop.secrets`. It stages each
attempt under a fresh grant nonce; existing items are updated in their actual
collection. A separate private atomic checkpoint contains only the committed
nonce, never token or account data. Only a matching approved grant may restore
after restart. Cancellation/disconnect revoke that approval before fallible
remote cleanup; a timed-out or dropped remote write can finish later but cannot
activate the abandoned grant. Stale nonce-specific cleanup preserves a newer
grant. Keyring calls have deadlines, cleanup is bounded, known sessions close
on error/drop, and locked/ambiguous storage fails without opening prompts or
falling back to plaintext credentials.

The provider process and connection UI are integrated. The compatibility identity
still needs live qualification for device flow, Music API access and audio. The
owner authorized compatibility registration reuse; this is not evidence of
third-party service approval or official API support. Expired access requires reconnect; automatic refresh is
not claimed. Outward state revisions remain monotonic through account changes.

Audio resolution uses authenticated download-info metadata, then a separate
anonymous client for allowlisted HTTPS download metadata, signed audio and
artwork. MP3 choices may omit the optional `transport` field; when present it must
be the string `raw`. Gain-enabled choices remain excluded, and the highest
bitrate among supported choices is selected. Credentials never accompany
media-host requests. XML, URLs, image bytes and cumulative catalogue metadata
have explicit bounds; redirects are disabled.
Track URLs and authorization headers stay inside the provider/player boundary.

### Radio and qualification

The radio adapter uses session JSON requests with an explicit originating batch
for each feedback call. Fetching batch B must not relabel the finish/skip of a
track still playing from batch A. Batch IDs are bounded nonempty ASCII strings
of letters, digits, underscore, hyphen and dot (at most 512 bytes); they stay
private JSON data and playback-origin keys. Radio session IDs retain the stricter
URL-path validator. Startup preparation, including session creation and initial
feedback, has one 30-second deadline with epoch cancellation; each HTTP request
also retains its 15-second limit. The epoch is checked again before playback
commit. The daemon's shorter transport deadline and existing pending-receipt
reconciliation remain unchanged; this is not a 30-second UI response guarantee.
Track-seeded sessions normalize an
album-qualified track ID before making a station seed. Service recommendations
remain provider-owned; this is not an Arinn recommendation engine.

This route is based on the upstream evidence cited in the research and is not
an assertion of official API support. Access, subscription, region, credential
provisioning and authenticated audio playback still require qualification.
Tests use a synthetic loopback HTTP server only, with deliberately fake tokens.
No real service account has been accessed and no service audio has been played.

## Verification and remaining integration

Focused commands, with the configured session Rust toolchain:

Install FFmpeg (including `ffprobe`) before running the backend integration
tests. Both CI build images include it, and package test dependencies declare
it. The package smoke images do not include it.

```sh
cargo fmt --all --check
cargo test --locked -p arinn-music-common -p arinn-plugin-music-local -p arinn-plugin-yandex-music
cargo clippy --locked -p arinn-music-common -p arinn-plugin-music-local -p arinn-plugin-yandex-music --all-targets -- -D warnings
```

The focused suite covers queue generation/refill/retry/trim races, duplicate
occurrences, empty-station startup, metadata, missing/renamed roots, link swaps,
real generated WAV probing, timeout/output limits and a disguised playlist.
HTTP tests cover request payloads, originating-batch feedback, authentication,
rate limits and oversized responses. The implementation PR records actual local
commands, toolchain wrapper failures and reruns; these command examples alone
are not evidence that the complete project check list passed.

Before completion: qualify the proposed contract and daemon replacement/timeout
coordination, complete native/package fixtures, run the full required checks and
obtain current-head review. Catalogue details, playlist actions, artwork, signed
stream resolution and continuous-session feedback are implemented; their presence
does not establish live-service qualification.
Live service and real-remote/HTPC qualification remain separate from synthetic
tests and require the applicable account setup/operator authorization.

### TV playback compatibility corrections (#787, #788)

Signed download-info timestamps accept bounded ASCII hexadecimal text and are
preserved verbatim in the CDN URL; they are not converted to decimal. Host,
path, duplicate-field and response bounds remain enforced. The music catalogue
places Refresh and Disconnect in separate remote-accessible positions. The
player heading follows loading, paused, stopped and failed states rather than
claiming playback solely because a track has been selected. These corrections
do not establish end-to-end live playback qualification.

Artwork requests have a four-second reply budget but no longer own network fetch
lifetimes. The provider keeps at most four bounded background fetches, a 16 MiB /
16-entry memory cache and a 30-second fetch deadline. The account epoch fences
both cached and in-flight results; reconnect clears the cache. Completed failed
fetches are evicted so the next visible retry starts fresh work; successful images
keep their cache lifetime. The music artwork
slot retries a transient image failure up to six times while visible. It never
fetches a provider URL directly or loops indefinitely on a missing cover.

The music player begins preparing upcoming queue occurrences immediately after
the current file loads. The Yandex provider exposes `prefetch_tracks` through the
existing Settings1 API: an integer from 0 to 5, default 5, applied to upcoming
preparations without interrupting the current track. The value is persisted in
`config.toml` and supplied at provider startup, with no rebuild required:

```toml
[providers.yandex-music.settings]
prefetch_tracks = 5
```

0 disables preparation. At most five tasks/buffers are retained and downloads
are admitted in explicit queue order, one at a time. Each admitted fetch has a
30-second deadline. Stop, replacement and incompatible queue changes cancel
both queued work and a consumed preparation waiting to play. Yandex keeps up to
32 MiB per prepared track in a memfd, without writing audio to disk. Including
the current RAM-backed track, at most 192 MiB of audio is retained. Completed RAM
files do not expire while in this queue; generic signed stream sources older
than 45 seconds are resolved again rather than reused. Oversized or failed
preparation falls back to ordinary resolution when that occurrence is selected.
Preparation never advances the queue, starts sound or sends played feedback.
Natural EOF remains the only automatic advancement trigger. This removes network
resolution/download waits at prepared transitions; it does not claim sample-
accurate gapless decoding across two player processes.

The Music root (#800, [ADR-0142](../adr/0142-music-in-hearth-light.md) MH-1,
MH-5, MH-9, MH-10; the [music Hearth light brief](../design/briefs/music-hearth-light/README.md)):

- `MusicRoom` follows `MusicCatalog.sources` and reads every source on its own:
  its `GetState`, then the first page of `selections`, `playlists`, `albums`,
  `artists` and `tracks` one after another, again when the catalogue
  revision changes. A new revision keeps each facet's entries until its new
  first page replaces them, so no card leaves and returns. Only first pages
  chain; further pages are the *All* list's (`loading` covers first pages and
  sources whose state has not answered yet), from `selections` and the
  shelf's own facet. A continuation older than its 30 s lease, or one that
  fails, reads again from the first page and pages forward to where the list
  was, keeping the shown entries until it has caught up. A first page that
  fails is read again on a later state poll, at most every 3 s. The room polls every 2 s while
  the Music section is shown (`active`) and announces `sources` only when a
  row changes. Browsing never calls `MusicCatalog.choose`; only opening a card
  does, for that card's source: a station starts with `radio`, anything else
  with `play`, by that source's own capabilities. A detail, the player and the
  queue stay `MusicCatalog`'s.
- Shelves, in the brief's order: *Stations* (`station` entries of
  `selections`), *For you* (playlists), *Albums*, *Artists*, *Recent tracks*.
  On each, the source's personal entries from `selections` come first, sources
  in their registry order; then the ordinary entries, those with `added_at`
  newest first, then the undated ones one source after another in turn
  (`MusicRoom::order`). A personal entry the source also lists as ordinary
  shows once. A shelf no source supplies is absent. Shelves show at most 24
  entries (*Recent tracks* 12) of the pages read so far, so a date order covers
  each source's first page; the *All* list pages further.
- Entries carry `provider`, `providerName`, `local` and a `key` of provider and
  entity id, so the same id from two sources stays two cards (no merging).
  The source mark draws `screen` for a source whose account is `local` and
  `public` otherwise; no provider has its own icon yet, so a service's card
  also names the service after the mark.
- Late entries never move focus: a shelf holding focus keeps the cards it
  shows in place and appends new ones (`MusicRoom.stable`), and shelves are
  keyed by id, so a shelf is not recreated when another changes. A card or
  shelf that leaves hands focus to its neighbour.
- A source whose state cannot be read leaves the shelves, and the *Sources*
  sheet says why. The sheet shows each source's state and its `connect`,
  `authorize`, `cancel`, `scan` (*Refresh*) and `disconnect` pills by
  capability, sent through `MusicRoom.sourceCommand`; *Disconnect* asks first
  with Cancel focused. A command the source refuses shows a note under that
  source until the next command, the account state changes or the source
  restarts. The catalogue screen no longer shows the source switch,
  *Refresh*, *Disconnect* or the account panel while a room is present.
- The room is never shown empty. Once every source has answered and nothing
  loads, a room without shelves says why, from the source rows: every source
  unreachable gives the reason and *Sources*; a source that needs sign-in
  gives *Connect a source*; otherwise *No music yet* (for this computer, add
  folders with music in Settings) and *Sources*. The shell's status area hides while the
  *Sources* sheet or a confirm dialog covers the screen.
- The *All* list filters by source with tabs, for sources that can browse,
  that start at *All*; it pages on while focus is near the end, also when a
  page adds no card of its kind and orders by
  the shelf order (labelled *By date* only once entries carry dates) or *A–Z*.
- *Find* searches every browsable source (#801, MH-10): `MusicRoom.search`
  reads the `artists`, `albums` and `tracks` facets with the query from each
  source, one page at a time per source, and the result shelves take each
  source's results in their own order, sources in turn, unmerged. Sources
  still answering are named under the field; late results never move focus.
  A page the source refuses (its two materialisation slots busy with the
  room's and the catalogue's reads) is read again after the source's other
  pages, up to three attempts.
  A source that joins, leaves or restarts during a search is searched again
  alone, and the others' results stay; a result shelf that leaves with focus
  hands it to the shelf in its place, or to the field. A query beyond the
  protocol's 1024 bytes finds nothing rather than keeping older results.
  Back closes the keyboard first, then returns to the room.
- An album, playlist or artist opened from the room or a result is the
  detail (brief mh-02): the blurred cover with a left scrim, the cover, an
  eyebrow that names the source, the title in Playfair Display (104, 80 on
  three lines), creator and length, and *Listen* and *Radio from …* by
  capability; there is no Shuffle (no protocol action). Rows carry their
  number and duration, the playing mark on the row of the session that plays,
  and the artist only where it differs from the album's. An artist detail
  lists its tracks; the protocol pages no releases for an artist, so that
  shelf is absent. Opening never plays; Back returns to the room or the
  results with focus on the entry it was opened from. The pills move down
  under a title on three lines, and the tracks start below them and end above
  the status line; the playlist actions join the pills' line for the D-pad
  (Right from the last pill, Down to the tracks), or take their own line under
  the pills when the line would pass the safe area (Down from the pills, Up
  from the first track). A detail opened from a
  detail's row never takes the Find text as a filter.
- Back from Find returns to the *Find* button. A Find query typed just
  before a result opens is still searched; a catalogue filter typed in
  another mode is dropped.
- A resume that fails keeps its target and the shown entries, and its retry
  resumes, reading its tokens afresh; a retry started from a source's chain of
  first pages goes on with that chain, so every failed facet is read again. A
  provider that repeats a token ends the resume with what it read, never with
  fewer entries than are shown. Back from a detail, the player or the *All* list returns to the room
  with focus on the card it was opened from.
