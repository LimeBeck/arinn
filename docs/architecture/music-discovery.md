# Music discovery and feasibility

Research date: 2026-11-01. Parent: [#677](https://github.com/LimeBeck/arinn/issues/677).
First increment: [#694](https://github.com/LimeBeck/arinn/issues/694).
Status: research and proposed requirements for owner review. No provider has
passed live qualification. This document is not a wire contract or an accepted
product specification. [Roadmap](../../ROADMAP.md#music),
[design brief](../design/music-design-brief.md).

## Existing baseline

The [specification](../specifications/arinn-specification.md) requires external
providers and daemon routing (R-01–R-10). The
[current media architecture](media-mvp.md) offers MPRIS session control and a
film/series library with external mpv playback. The current library parser
(`plugins/library/src/parse.rs`) scans video extensions; the protocol library
kinds (`crates/arinn-protocol/src/library.rs`) describe films, episodes and
series, with an unknown-kind fallback. Neither implements a music catalogue,
album grouping, music authentication or a queue contract. Adding audio
extensions alone would apply video resume/completion semantics to songs.

The owner requested a common native interface for five candidates. Website
shortcuts and controlling an already running MPRIS session do not meet that goal.
Preparation is authorized now; this work does not declare the Plugin API stable
or change the Phase 6 implementation gate.

## Evidence and capability matrix

These are documentation findings, not tested integrations. An API being
documented does not establish that Arinn has app access, permission to distribute
the integration, or a working playback backend. Recheck mutable service rules
before implementing each provider. Sources were read on the research date.

| Candidate | Catalogue/search | Playback path | Account/access | Readiness and next proof |
| --- | --- | --- | --- | --- |
| Yandex Music | An upstream unofficial client documents search and catalogue operations [Y1] | Stream resolution and entitlement need an independent spike | Token lifecycle, subscription, region and distribution permission remain unverified | Blocked pending an approved integration route; the existence of a client is not official service authorization |
| Spotify | Official catalogue API exists, but development-mode endpoint/playlist restrictions affect coverage [S1, S2] | Player API controls an active device; Web Playback SDK creates a browser device, not a raw mpv stream [S3, S4] | Premium, app registration, allowlisted development users and a distribution path [S1, S3] | Blocked for the proposed multi-service product pending policy clarification/permission [S5], then access and playback qualification |
| SoundCloud | Official search, tracks and playlists [C1] | Custom-player streams for eligible tracks; preview/blocked content must stay distinguishable [C1] | Registered app; Artist Pro required for registration [C2]. OAuth/PKCE still requires a client secret [C1] | Conditional candidate: prove app access, a safe desktop credential model, attribution and playback of eligible content |
| YouTube Music | An unofficial project emulates the web client for library/search; it is not endorsed by Google [G1] | No approved native audio playback route established by this research | Authentication, region, subscription and distribution route unverified | Blocked: YouTube API policy restricts audio separation and hidden/background playback [G2]; neither Data API metadata nor unofficial catalogue access proves playback support |
| Local library | Proposed scan of configured roots with tags and filename fallback | Existing external-player integration is a candidate for reuse, subject to queue/audio-session work | No service account; filesystem and codec availability still matter | Best first technical slice; not implemented or qualified. Test scan, metadata, missing mounts and audio playback using synthetic fixtures |

### Spotify findings

[S1](https://developer.spotify.com/documentation/web-api/concepts/quota-modes)
documents Premium for the development app owner and up to five allowlisted users;
extended access has organizational eligibility and review requirements. A local
test app is not evidence that Arinn can ship to arbitrary users.
[S2](https://developer.spotify.com/documentation/web-api/references/changes/february-2026)
records development-mode changes, including restricted playlist contents and
removed endpoints. Do not design guaranteed catalogue parity from old tutorials.
[S3](https://developer.spotify.com/documentation/web-api/reference/start-a-users-playback)
requires Premium and targets an active device.
[S4](https://developer.spotify.com/documentation/web-playback-sdk) is a JavaScript
browser player. Its Linux browser support does not prove Qt/QML compatibility.
[S5](https://developer.spotify.com/policy), section III.5, restricts products
integrated with another service's streams or content; III.11 restricts replacing
the core Spotify experience without permission. This directly affects the
requested product, even with a source selector. Recommendation: keep Spotify
tracked but blocked until an authorized route is established; do not assume
separate processes or source-scoped queues resolve the policy issue.

### SoundCloud findings

[C1](https://developers.soundcloud.com/docs/api/guide) documents OAuth 2.1/PKCE,
single-use refresh tokens and confidential clients requiring a secret. It also
describes streaming access levels and source/creator attribution with links.
Recommendation: spike eligible content only after app access is available;
serialize refresh and test expiry, cancellation and disconnect. A public desktop
binary cannot conceal a shared app secret. A credential broker or per-install
credentials would have different deployment/security consequences and needs an
explicit design decision; neither is introduced here.
[C2](https://developers.soundcloud.com/docs/api/register-app) currently requires
Artist Pro to register an app. This is an app-owner requirement, not a claim
that every listener requires that subscription. Private, restricted and preview
tracks require distinct UI states rather than optimistic Play buttons.

### Yandex Music and YouTube Music findings

[Y1](https://github.com/MarshalX/yandex-music-api) and its
[upstream documentation](https://ym.marshal.dev/) describe an
unofficial client. This research did not establish an official public native
integration contract or supported credential provisioning route. Absence of
verified documentation is an unknown, not proof that no agreement is possible.
Keep the candidate open; request service guidance or review an explicitly
proposed unofficial route before selecting dependencies or token workflows.

[G1](https://ytmusicapi.readthedocs.io/en/stable/) describes browser-request
emulation and catalogue/library operations. It does not establish an authorized
audio backend. [G2](https://developers.google.com/youtube/terms/developer-policies)
sections III.F.7–9 restrict separating audio and hidden/background players.
An ordinary visible YouTube embed is not automatically a YouTube Music library
integration and would change the requested audio/Home experience. Retain this
candidate as blocked pending a permitted route and owner review of any product
alternative; do not silently substitute a web shortcut or audio extractor.

## Proposed common requirements

The following IDs are local proposal references, not new MVP AC identifiers.

| ID | Proposal | Evidence required before acceptance |
| --- | --- | --- |
| MU-01 | One common shell layout, explicit source identity, provider-qualified entity IDs; no title-based merging | Same-title entities from different sources stay distinct |
| MU-02 | Browse artists, albums, playlists and tracks; expose supported facets only | Paged snapshots, absent fields/artwork and unsupported facets |
| MU-03 | Search within the selected source initially; no promised federated search | Cancellable requests; late replies never overwrite another source/query |
| MU-04 | Playback starts only after an explicit command and reports confirmed state | Denial, no device, unsupported format, timeout and actual start |
| MU-05 | Proposed queue belongs to one source/session; only supported mutations appear | External queue changes, duplicate commands and provider restart; no mixed-source playback promise |
| MU-06 | Play/pause, previous/next, seek, repeat/shuffle follow reported capabilities | Unknown duration, non-seekable stream and unsupported command |
| MU-07 | Browsing and Home preserve the active music session where the permitted backend supports it | Source switching does not implicitly start or stop playback; blocked background playback is explicit |
| MU-08 | Connection is cancellable; expired/revoked access explains recovery; disconnect removes local credentials/cache as required | No tokens in QML models, URLs in logs, analytics or diagnostic payloads |
| MU-09 | Service failure is isolated and never presented as an empty library | Distinguish loading, empty, unsupported, account-required, unavailable, quota-limited and error |
| MU-10 | Remote-only access and predictable Back/Home focus restoration | Screen/sheet focus, keyboard return, paging and list changes at 1080p/4K |
| MU-11 | Browse provider-owned personal selections and stations, including My Wave and personal mixes where supported | Distinguish finite playlists from continuous stations; no local recommendation engine or assumed feature parity |
| MU-12 | Start radio from a provider-defined station or track seed; pin supported stations | Seed/session identity remains opaque to core/QML; unsupported sources expose no successful start action |
| MU-13 | A continuous queue replenishes before exhaustion and labels the loaded window rather than a fictional total | Bounded upcoming/history windows, duplicate suppression, append versus replace, concurrent refill, cancellation and stale-session replies |
| MU-14 | Like/dislike and playback feedback follow each provider's capabilities | Confirmed account feedback, clear like/dislike distinction, skip versus natural end, no fabricated success or assumed identical provider semantics |
| MU-15 | Station loading, exhausted-buffer and refill-failure states preserve current playback and offer recovery | Retry does not restart the station or cut off the current track; stopping/replacing a station rejects late batches |

Capability flags, pagination, account states, assets, command confirmation and
queue revision ownership need a separate contract-change task. Reuse existing
generation/revision and asynchronous-operation principles; this report does not
allocate a new capability name or extend an existing interface.

### CloudMus functionality reference

Owner-requested comparison on 2026-11-01: inspected
[CloudMus at d8f5c6d](https://github.com/cloudmus/cloudmus/tree/d8f5c6dad9818a1f3bfe5935437f97ddf138818e),
not a live account session. Its
[protocol](https://github.com/cloudmus/cloudmus/blob/d8f5c6dad9818a1f3bfe5935437f97ddf138818e/docs/protocol.md)
distinguishes ordinary playlists, liked tracks and radio stations; a radio starts
with an initial batch and receives more tracks later. Its backend-defined seed
and station identifiers are a useful reference for keeping service names out of
core routing.

The [Yandex radio implementation](https://github.com/cloudmus/cloudmus/blob/d8f5c6dad9818a1f3bfe5935437f97ddf138818e/backends/yandex-music/cloudmus_backend_yandex/radio.py)
reports start/end/skip feedback, tracks its session/batch and replenishes upcoming
tracks. A skip can replace unplayed recommendations; completion normally appends
when the buffer is low. It also filters recent/played entries and rejects a
response superseded by another replacement. The
[YouTube radio implementation](https://github.com/cloudmus/cloudmus/blob/d8f5c6dad9818a1f3bfe5935437f97ddf138818e/backends/youtube-music/cloudmus_backend_ytmusic/radio.py)
uses watch-playlist batches and listening-history feedback rather than the same
Yandex mechanism. A common interface must preserve those differences.

For Arinn this expands design scope beyond album selection: personal selections
and pinned stations on source landing, track-seeded radio, capability-dependent
feedback and continuous queue states. Personal playlists are not automatically
endless. The future contract task must settle queue ownership and generation,
station lifecycle, continuation, append/replace semantics, buffering limits,
feedback acknowledgement and safe retry. Names here describe requirements, not
approved D-Bus method names. A third-party implementation demonstrates a route
worth investigating; it does not resolve Arinn's service permission/access
blockers or authorize copying its credentials, branding or implementation.

## Proposed local-library requirements

- User-selected roots only; scan recursively with bounded concurrency and probe
  time/output limits. Do not follow directory symlinks outside a configured root.
- Candidate formats: MP3, FLAC, Ogg Vorbis/Opus, M4A/AAC and WAV. Detect audio
  streams rather than trust extensions; accept a format only after packaged
  decoder tests. No claim that every file in these containers is playable.
- Read title, artist, album artist, album, disc/track numbers and duration;
  treat missing values as unknown. Group compilations by album artist, preserve
  disc order, and do not merge albums solely because their titles match.
- Prefer embedded tags; use the filename for a missing title and explicit
  unknown-artist/album presentation. Bound embedded artwork decoding and use
  provider-owned asset references. No online enrichment in the first slice.
- Keep stable provider IDs over unchanged rescans; preserve duplicate files as
  separate items. Rename/move identity and folder grouping need explicit tests
  and a decision before claiming those operations preserve identity.
- An absent mount is unavailable, not an empty directory. Keep catalogue
  references and queue entries; a rescan must not destructively purge them.
  Corrupt/unreadable files fail individually without aborting the whole scan.
- Do not apply the film 90-second watched rule or automatically resume a song
  mid-track. Proposed default: explicit Play starts at the beginning; pause/resume
  affects the live session. Restart persistence remains a review decision.
- Proposed first slice is read-only catalogue browsing and playback. Playlist
  file import, editing, ratings, lyrics, downloads and metadata writes require
  separate scope decisions; this does not remove them from future consideration.

[ffprobe documentation](https://ffmpeg.org/ffprobe.html) supports inspecting
streams/metadata and machine-readable output. It does not prove this proposed
scanner or the deployed codecs work. No real user folders have been scanned.

## Decisions and next gates

| Question | Recommendation | Alternative / consequence |
| --- | --- | --- |
| First qualified source | Local library, then SoundCloud if credentials and streaming access are established | Another source needs equivalent evidence first; no candidate silently disappears |
| Common UX scope | Source-scoped search and queue using one layout | Federated search/mixed queue adds rights, routing and interruption questions and needs separate approval |
| Spotify multi-service restriction | Keep blocked and seek an authorized integration route | Defer it explicitly; source separation alone is not a permission grant |
| SoundCloud app secret | Resolve registration and credential deployment before coding auth | A hosted broker introduces a new service/security posture; per-install setup burdens users |
| Unofficial Yandex/YouTube paths | Keep blocked pending a reviewed supported route or explicit investigation scope | Owner may defer or request a dedicated feasibility spike; no production dependency chosen here |
| Entry point and song persistence | Review the accompanying design brief | Do not add navigation items or reuse film resume defaults before review |

Next: owner reviews this brief, then a visual design task produces editable
screens and interaction states. Only after design acceptance, feasibility and
the Phase 6 gate may separate contract/runtime/provider tasks proceed.

## Verification record

Completed: repository baseline inspection and dated primary-document/upstream
research above. Not run: service API calls with user accounts, app registration,
credential acquisition, billing/subscription actions, audio playback, desktop
operator tests or provider certification. Documentation CI validates links and
structure, not service permission or live integration. Parent #677 stays open;
no provider support or end-to-end acceptance criterion is marked complete.
