# Native music protocol (pre-stable)

Status: proposed in [ADR-0134](../adr/0134-native-music-catalogue-and-session.md),
implementation task #707, unified MR #710. No Plugin API stability claim.

## Routing and methods

A provider advertises `music`. Its normal registered path exposes the
[daemon endpoint](music1.xml); its unique bus owner exports the
[provider endpoint](music-provider1.xml) at `/org/arinn/Provider1`.
The root `/org/arinn/Daemon1` exports [MusicController1](music-controller1.xml).
The shell never calls the provider process or a third-party service directly.

Methods return UTF-8 JSON strings bounded to 256 KiB. Shared Rust types live in
`arinn_protocol::music`; unknown fields are rejected. Errors use `ErrorCode`.
State queries have a three-second provider deadline; pages and commands have a
five-second daemon deadline. A timeout is an uncertain result, not permission to
start another session. The daemon retains the exact pending command identity and
settles it through the provider receipt before forwarding another transport;
owner/generation loss invalidates that pending identity. Refresh confirmed state
before another transport action.

`GetState(generation)` returns revision, catalogue_revision, backend, account,
prompt, facets, actions, session, playback, position_us, current, history,
upcoming, continuous, station, refill, catalogue_loading, feedback, pinned and
error. `feedback` is empty, liked or disliked, based on confirmed provider actions;
`pinned` describes the current station. Enum values and limits are validated by
`State::validate`. An entry contains its occurrence and a metadata-only track.
A queue retains at most 32 historical and 64 upcoming occurrences. Repeated
finite-playlist tracks have distinct occurrence IDs. Artwork is an opaque asset
ID, never a shell-loadable filesystem path or remote URL.

`GetPage(generation, query)` takes facet, parent, search and token. Facets are
tracks, albums, artists, playlists and selections. Pages contain generation,
revision, entries (at most 100) and next_token. A token binds the original query,
verified calling connection, generation and immutable catalogue snapshot. Leases
expire after 30 seconds, at most 32 leases and 16 MiB of distinct snapshot data
are retained, and at most two initial materializations run concurrently. Retrying
a continuation reads its cached page; a different caller/query cannot reuse it.
Snapshots contain at most 100,000 unique entities. Search is at most 1024 bytes.

An entity may carry `added_at`: when it joined the user's library, in Unix
seconds, at most 253402300799. It is optional and omitted when the provider does
not know it. An album or artist takes its newest track's date. The shell orders
ordinary entries on shared shelves by it, newest first, after the provider's
personal `selections` ([ADR-0142](../adr/0142-music-in-hearth-light.md) MH-9).
Local music reports the time a file was first found; a changed file keeps it,
and a catalogue stored before the field existed uses the file's modification
time once. Other providers fill it when their service reports a date (#806).

A station may carry `pinnable` (#811, [ADR-0142](../adr/0142-music-in-hearth-light.md)
MH-6). It promises that the station's `id` stays valid across provider restarts
and generations, is never reused for another station, and that `radio` with that
`id` as `entity` starts this station. Only a station may carry it; it is optional
and omitted when false. The shell may store a pinnable station and start it
later; a stored ID the provider no longer offers fails with NotFound and is never
silently replaced. Yandex stations derive their IDs from the service's station
keys (`station:wave` for `user:onyourwave`, `station:track:<id>`). Local music
offers one station, `station:library` ("All my music, shuffled"), in its
`selections` facet.

`Command(generation, command)` takes request_id (UUID), action, session, entity,
occurrence, position_us, replaces_provider, replaces_session and text. The last two
fields default to empty for non-replacing commands. Known actions are play,
pause, resume, stop, previous, next, seek, remove, like, dislike, radio, retry,
pin, connect, authorize, cancel, disconnect, scan, save_playlist and delete_playlist; providers advertise only implemented
actions. Removal addresses an upcoming occurrence. Times are microseconds.
`text` defaults to empty, is at most 1024 bytes without control characters and
must contain a nonblank playlist name for save_playlist. Saving a group uses
entity; saving the finite remaining queue uses session and an empty entity.
Deleting a playlist removes its metadata, never audio files.
The last 32 command receipts are retained per endpoint, keyed by verified caller,
generation and UUID. Retrying the same payload returns its receipt; reusing the
UUID with another payload returns Conflict. Admission is bounded to 32 commands.
The daemon retains at most 32 endpoint/generation ledgers, each with 32 receipts
across callers (at most 1024 bounded receipts total); owner/generation loss removes
that endpoint's ledger. A full ledger admission limit returns Unavailable before
forwarding a new endpoint command.

Starting play/radio while a music session is loading, playing or paused requires
matching replaces_provider/replaces_session from the confirmation screen. The
daemon confirms the previous provider stopped and cleared its current entry
before forwarding the new start. Failures leave the new source unstarted.
`GetActive` returns JSON null or {provider_id,generation,state}; it is independent
of the source currently browsed. Owner loss invalidates cached sessions.

## Account and playback boundaries

Account states are local, disconnected, connecting, connected, expired,
unconfigured, restricted and unavailable. `restricted` means authenticated service
access was denied; `unavailable` represents a transient account/API failure.
Neither is a connected empty catalogue. A connecting prompt contains only
verification_url (HTTPS), user_code and expires_in. Providers keep device_code,
client secrets and user tokens private. Connect/Authorize/Cancel/Disconnect use
the same verified command route. Optional `authorize` takes no session/entity
and opens the prompt through [BrowserAuth1](browser-authorization.md); the shell
never launches third-party processes. Providers must verify their actual music
API before reporting connected. OAuth success alone is insufficient. Client
identity and live-service eligibility remain deployment qualification.

Playback states are stopped, loading, playing, paused and ended. A load receipt
means loading; only a native file-loaded observation confirms readiness. A native
end-file error is not natural EOF. Transport polls must not consume EOF; it is
retained until the session poll acknowledges it. Seeking to the duration does
not manufacture natural completion. Resolver work is fenced by a private session
token; changing a queue stops previous audio before resolving its replacement.

## Optional artwork extension

`MusicProvider1.Features` advertises `AssetProvider1` when supported.
The daemon exposes artwork through its existing Assets1 route, validating provider
owner/generation before and after each request. The shell receives bounded image
bytes through its native asset cache; opaque cover IDs never become remote URLs.
