# Music backend implementation increment

Status: **native local/Yandex integration in verification; MR remains draft**. Task
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
| `arinn-music-common` | Metadata, bounded finite/radio queue, private atomic state, owned audio-only player, natural playback events and continuous refill | Installed audio qualification |
| `arinn-plugin-music-local` | Persistent catalogue/playlists, registration, snapshot paging, root settings, background scan, finite playback, bounded artwork extraction and native playlist save/delete | Native package/audio qualification |
| `arinn-plugin-yandex-music` | Device-code controller, staged Secret Service lifecycle, catalogue/search/liked content, signed stream resolution, artwork and continuous radio runtime | Live deployment-client, account and authenticated audio qualification |

The proposed [music contract](../plugin-api/music.md) and
[ADR-0134](../adr/0134-native-music-catalogue-and-session.md) add protocol, SDK,
daemon and native shell integration in the owner-authorized unified MR. New
executables/manifests are included in Arch/Debian install rules. This is pending
package validation, not a claim of installed-product qualification.

`Track` has no path, stream URL, token or header. Providers alone resolve files
and streams. The shell browses one source while the daemon projects the active
session independently. Native layout B uses one icon row; the Home music card
opens the player and offers a separate pause/resume action. Search reuses the
shell's on-screen keyboard. New Russian strings have a separate music catalogue
to avoid the active #718 translation zone.

Local settings apply on provider restart through the existing settings contract
(#704/#683). Scan publishes an immutable view; secure playback reopening does
not wait for a scan lock. Shutdown cancels further scan work between files;
an in-flight bounded ffprobe may finish before exit. Unavailable/incomplete roots
produce a degraded state while preserving last-known metadata.

Yandex account connection, catalogue and playback share the native provider.
The owner supplied the public Arinn client ID
`4173e0c60dfa4066bd6ec2a504b8ae9a` on 2026-11-03. It is the default identity;
`ARINN_YANDEX_CLIENT_ID` overrides it and an explicit empty value disables
Connect. Registration alone does not establish Music API or device-flow access.
No third-party application identity or secret is bundled.

If this registration requires a client secret, provision it locally through
Secret Service as a unique unlocked `text/plain` item with attributes
`application=arinn-yandex-client` and `client-id=<public ID>`. Never paste the
secret or a user access token into chat, settings, environment files or the
repository. An absent optional application-secret item permits a public client;
locked or ambiguous matching items fail closed. Missing Secret Service does not
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
is a versioned, bounded (32 MiB) snapshot of roots, catalogue and at most 256
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

The provider process and connection UI are integrated. The deployment identity
still needs live qualification for device flow and Music API access. The upstream
example establishes the interaction, not official API support or permission to
reuse its registration. Expired access requires reconnect; automatic refresh is
not claimed. Outward state revisions remain monotonic through account changes.

Audio resolution uses authenticated download-info metadata, then a separate
anonymous client for allowlisted HTTPS download metadata, signed audio and
artwork. Credentials never accompany media-host requests. XML, URLs, image bytes
and cumulative catalogue metadata have explicit bounds; redirects are disabled.
Track URLs and authorization headers stay inside the provider/player boundary.

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

Before completion: qualify the proposed contract and daemon replacement/timeout
coordination, complete native/package fixtures, run the full required checks and
obtain current-head review. Catalogue details, playlist actions, artwork, signed
stream resolution and continuous-session feedback are implemented; their presence
does not establish live-service qualification.
Live service and real-remote/HTPC qualification remain separate from synthetic
tests and require the applicable account setup/operator authorization.
