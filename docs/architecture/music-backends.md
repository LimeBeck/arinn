# Music backend implementation increment

Status: **in progress; not a usable music provider yet**. Task
[#707](https://github.com/LimeBeck/arinn/issues/707), parent
[#677](https://github.com/LimeBeck/arinn/issues/677). The owner authorized one
implementation MR for local music, Yandex and their common API/UI on 2026-11-01.
On 2026-11-02 the owner clarified that the design must be included in the same
MR. [#710](https://github.com/LimeBeck/arinn/pull/710) now carries the
[visual prototype](../design/prototypes/music/README.md), owner-selected layout
B, review captures and backend implementation together. Design PR #700 is
superseded; no separate design merge is required.
The [research](music-discovery.md) remains the source for feasibility limits.

## Current code boundary

| Package | Implemented in this increment | Still missing |
| --- | --- | --- |
| `arinn-music-common` | Provider-private metadata and bounded finite/radio queue with session/request/occurrence identity | Accepted wire projection and runtime command/confirmation integration |
| `arinn-plugin-music-local` | Audio catalogue backend, bounded ffprobe, metadata grouping, unavailable-root retention and secure descriptor reopening | Provider process/registration, persistent storage lifecycle, paged export, artwork extraction, settings and actual playback routing |
| `arinn-plugin-yandex-music` | Private authenticated HTTP adapter for track search, track metadata, playlist listing and radio session/batch/feedback; bounded response parsing | Account acquisition/storage, catalogue completion and liked content, stream resolution/playback, provider registration, UI and live qualification |

These are library crates, not installed executables. No new provider manifest is
shipped and no daemon capability is registered. `Track` contains no filesystem
path, stream address, token or HTTP header; provider-private catalogue records
may contain local paths and are not a shell/wire model. Internal types are not a
new Plugin API and do not stabilize an interface accidentally.

Shared protocol/SDK and runtime integration remain coordinated with the active
settings contract [#704](https://github.com/LimeBeck/arinn/pull/704) / #683. This
increment changes no files owned by that work. It does not close the design or
implementation tracker, change a phase status or claim streaming support.

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
- These primitives do not send playback/account feedback. The future runtime
  must distinguish selected entries from player-confirmed events and must not
  equate a seek-to-end with a natural EOF.

## Local catalogue

The scanner reads configured absolute roots and candidates with MP3, FLAC, Ogg,
Opus, M4A, AAC and WAV extensions. An extension alone is insufficient: ffprobe
must report audio. Attached cover artwork is accepted; moving video is not part
of this audio backend. Only generated WAV has been decoder-qualified in this
increment; the other candidates still need format fixtures and packaged tests.

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
do not delete references. Persistent save/load orchestration is not implemented.

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

## Yandex private adapter

The API adapter requires a caller-supplied authorized token in memory. It does
not acquire an account or provide a token-entry UI. No third-party OAuth client
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

This flow is a delivery requirement, **not implemented by the current token-only
adapter**. The upstream example establishes the interaction, not official API
support or permission to reuse a third-party application registration. Qualify
the underlying device-auth endpoints, client identity and credential lifecycle
before shipping it.

### Radio and qualification

The radio adapter uses session JSON requests with an explicit originating batch
for each feedback call. Fetching batch B must not relabel the finish/skip of a
track still playing from batch A. Track-seeded sessions normalize an
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

Before completion: integrate the accepted common contract, daemon and shell;
finish provider registration/settings/account/playback; add all format/artwork
and package fixtures; run the full required checks and obtain final review.
Live service and real-remote/HTPC qualification remain separate from synthetic
tests and require the applicable account setup/operator authorization.
