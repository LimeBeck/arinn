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
five-second daemon deadline. A command waits at most two seconds behind other
commands; a command that could not be forwarded in that time is refused with
Unavailable, so forwarding keeps at least three seconds. A timeout after
forwarding is an uncertain result, not permission to start another session:
the daemon treats that provider as active and reads its state again before it
admits another start. Refresh confirmed state before another transport action.

`GetState(generation)` returns revision, catalogue_revision, backend, account,
prompt, facets, actions, session, playback, position_us, current, history,
upcoming, continuous, refill and error. Enum values and limits are validated by
`State::validate`. An entry contains its occurrence and a metadata-only track.
A queue retains at most 32 historical and 64 upcoming occurrences. Repeated
finite-playlist tracks have distinct occurrence IDs. Artwork is an opaque asset
ID of at most 256 ASCII letters, digits and `-_.:`, never a shell-loadable
filesystem path or remote URL.

`GetPage(generation, query)` takes facet, parent, search and token. Facets are
tracks, albums, artists, playlists and selections. Pages contain generation,
revision, entries (at most 100) and next_token. A token binds the original query,
verified calling connection, generation and immutable catalogue snapshot. A
lease expires 30 seconds after it is issued; each continuation lease gets its
own 30 seconds. At most 32 leases and 16 MiB of distinct snapshot data and
cached pages are retained, the lease being served is never evicted, and at most
two initial materializations run concurrently. Retrying a continuation reads
its cached page; a different caller/query cannot reuse it.
Snapshots contain at most 100,000 unique entities. Search is at most 1024 bytes.

`Command(generation, command)` takes request_id (UUID), action, session, entity,
occurrence, position_us, replaces_provider and replaces_session. The last two
fields default to empty for non-replacing commands. Known actions are play,
pause, resume, stop, previous, next, seek, remove, like, dislike, radio, retry,
pin, connect, cancel, disconnect and scan; providers advertise only implemented
actions. Removal addresses an upcoming occurrence. Times are microseconds.
The last 32 command receipts are retained per endpoint, keyed by verified caller,
generation and UUID. Retrying the same payload returns its receipt; reusing the
UUID with another payload returns Conflict. A timeout is an uncertain outcome,
not a receipt: it is not recorded, and a retry with the same UUID runs again.
A provider handler is bounded to 10 seconds. Admission is bounded to 32 commands.

Starting play/radio while a music session is loading, playing or paused requires
matching replaces_provider/replaces_session from the confirmation screen. So do
next, previous, resume and retry on a provider other than the one that is
audible, since they can start audio there. A retry of a start that timed out,
with the same caller and request ID, goes to the provider, whose receipt holds
the real outcome. The
daemon confirms the previous provider stopped and cleared its current entry
before forwarding the new start. Failures leave the new source unstarted.
`GetActive` returns JSON null or {provider_id,generation,state}: a confirmed
loading, playing or paused session first, otherwise the last started provider's
session that still has a current entry. It is independent of the source
currently browsed. Owner loss invalidates cached sessions.

## Account and playback boundaries

Account states are local, disconnected, connecting, connected, expired and
unconfigured. A connecting prompt contains only verification_url (HTTPS),
user_code and expires_in. Providers keep device_code, client secrets and user
tokens private. Connect/Cancel/Disconnect run through the same verified command
route. Configured client identity and live-service eligibility are deployment
qualification, not an inference from a successful synthetic device-flow test.

Playback states are stopped, loading, playing, paused and ended. A load receipt
means loading; only a native file-loaded observation confirms readiness. A native
end-file error is not natural EOF. Transport polls must not consume EOF; it is
retained until the session poll acknowledges it. Seeking to the duration does
not manufacture natural completion. Resolver work is fenced by a private session
token; changing a queue stops previous audio before resolving its replacement.
