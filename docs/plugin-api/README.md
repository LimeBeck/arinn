# Plugin API

API major 1 is implemented for the production daemon and the `application`
capability, but **has not been released as a stable Plugin API**. Requirements:
[specification §9](../specifications/arinn-specification.md),
[ADR-0020](../adr/0020-dbus-contract-baseline.md),
[ADR-0035](../adr/0035-phase1-application-wire-contract.md).

## Wire contracts

| Interface | Endpoint and contract |
| --- | --- |
| `org.arinn.Favorites1` | Daemon root; [favorites1.xml](favorites1.xml), [favorites contract](../architecture/favorites.md). Daemon export implemented; shell pin/unpin remains tracked in #68 |
| `org.arinn.Daemon1` | `org.arinn.Daemon1`, `/org/arinn/Daemon1`; [daemon1.xml](daemon1.xml) |
| `org.arinn.Application1` | Daemon-owned `/org/arinn/Daemon1/providers/p_<uuid_hex>`; [application1.xml](application1.xml) |
| `org.arinn.Provider1` | Provider unique bus owner, `/org/arinn/Provider1`; [provider1.xml](provider1.xml) |
| `org.arinn.Operation1` | Daemon-owned operation path returned by a command; [operation1.xml](operation1.xml) |
| `org.arinn.Browser1` | Daemon-owned `/org/arinn/Daemon1/providers/p_<uuid_hex>` of a `browser` provider; [browser1.xml](browser1.xml) |
| `org.arinn.BrowserProvider1` | Provider unique bus owner, `/org/arinn/Provider1`; [browser-provider1.xml](browser-provider1.xml) |
| `org.arinn.WebShortcutIcons1` | Optional daemon-root site icons; [web-shortcut-icons1.xml](web-shortcut-icons1.xml). Contract only, production implementation pending |
| `org.arinn.WebIconProvider1` | Optional browser-provider site discovery; [web-icon-provider1.xml](web-icon-provider1.xml). Contract only, production implementation pending |
| `org.arinn.Media1` | Daemon-owned `/org/arinn/Daemon1/providers/p_<uuid_hex>` of a `media` provider; [media1.xml](media1.xml). Exported by the daemon ([ADR-0058](../adr/0058-media-routing.md)) |
| `org.arinn.MediaProvider1` | Provider unique bus owner, `/org/arinn/Provider1`; [media-provider1.xml](media-provider1.xml) |
| `org.arinn.MediaOpen1` | Optional provider interface; [media-open1.xml](media-open1.xml) |
| `org.arinn.MediaTracks1` | Optional provider interface; [media-tracks1.xml](media-tracks1.xml) |
| `org.arinn.MediaTimeline1` | Optional provider interface; [media-timeline1.xml](media-timeline1.xml) |
| `org.arinn.MediaProcess1` | Optional media-provider interface, daemon callers only; [media-process1.xml](media-process1.xml) ([ADR-0096](../adr/0096-browser-media-session-association.md)) |
| `org.arinn.BrowserProcess1` | Optional browser-provider interface, daemon callers only; [browser-process1.xml](browser-process1.xml) ([ADR-0096](../adr/0096-browser-media-session-association.md)) |
| `org.arinn.MediaAssociation1` | Optional daemon-root media-session ↔ tab links; [media-association1.xml](media-association1.xml) ([ADR-0096](../adr/0096-browser-media-session-association.md)). Exported by the daemon |
| `org.arinn.SystemStatus1` | Optional daemon-root mute and network state; [system-status1.xml](system-status1.xml) ([ADR-0100](../adr/0100-system-status.md), [contract](../architecture/system-status.md)). Exported by the daemon; Home shows the mute and Wi-Fi icons from it ([#387](https://github.com/LimeBeck/arinn/issues/387), [#407](https://github.com/LimeBeck/arinn/pull/407)) |
| `org.arinn.SystemControl1` | Optional daemon-root power actions, the dedicated session's "Switch to desktop" ([#438](https://github.com/LimeBeck/arinn/issues/438)) and volume for the shell; [system-control1.xml](system-control1.xml) ([ADR-0108](../adr/0108-system-control.md), [contract](../architecture/system-control.md)). Exported by the daemon, which refuses provider connections. The shell's power menu and volume HUD follow in [#148](https://github.com/LimeBeck/arinn/issues/148) |
| `org.arinn.Library1` | Daemon-owned provider path of a `library` provider; [library1.xml](library1.xml). Exported by the daemon ([ADR-0060](../adr/0060-library-routing.md)) |
| `org.arinn.LibraryProvider1` | Provider unique bus owner, `/org/arinn/Provider1`; [library-provider1.xml](library-provider1.xml) |
| `org.arinn.LibraryResume1` | Optional provider interface; [library-resume1.xml](library-resume1.xml) |
| `org.arinn.LibraryImport1` | Optional provider interface; [library-import1.xml](library-import1.xml) |
| `org.arinn.LibraryRoots1` | Optional provider interface; [library-roots1.xml](library-roots1.xml) ([ADR-0071](../adr/0071-settings-editor-contract.md)). The daemon serves it to the shell as `Library1.GetRoots` |
| `org.arinn.DownloadProvider1` | Provider unique bus owner, `/org/arinn/Provider1`; [download-provider1.xml](download-provider1.xml) ([ADR-0067](../adr/0067-download-wire-contract.md)) |
| `org.arinn.Downloads1` | Daemon-owned provider path of a `download` provider; [downloads1.xml](downloads1.xml) ([ADR-0067](../adr/0067-download-wire-contract.md)). Daemon export is not implemented yet ([#144](https://github.com/LimeBeck/arinn/issues/144)) |
| `org.arinn.Browse1` | Daemon, `/org/arinn/Daemon1`; [browse1.xml](browse1.xml) ([ADR-0066](../adr/0066-address-and-search-contract.md)) |
| `org.arinn.ProviderSettings1` | Optional interface of any provider; [provider-settings1.xml](provider-settings1.xml) ([ADR-0065](../adr/0065-provider-settings-contract.md)) |
| `org.arinn.Settings1` | Daemon, `/org/arinn/Daemon1`; [settings1.xml](settings1.xml). Exported by the daemon ([ADR-0065](../adr/0065-provider-settings-contract.md)) |
| `org.arinn.Folders1` | Daemon, `/org/arinn/Daemon1`; [folders1.xml](folders1.xml) ([ADR-0071](../adr/0071-settings-editor-contract.md)). Exported by the daemon |
| `org.arinn.AssetProvider1` | Optional interface of `media` and `library` providers; [asset-provider1.xml](asset-provider1.xml) ([ADR-0062](../adr/0062-asset-contract.md)) |
| `org.arinn.Assets1` | Daemon-owned provider path of a provider that lists `AssetProvider1`; [assets1.xml](assets1.xml). Exported by the daemon ([ADR-0062](../adr/0062-asset-contract.md)) |

The private-bus suite compares the implemented daemon exports to live
introspection. Provider XML for browser, media and library is compared by the
SDK tests. Struct field order is defined in `arinn-protocol`; methods returning
one struct use a one-element tuple to preserve the D-Bus body signature for QtDBus.
The old [phase0.xml](phase0.xml) is historical evidence, not an active interface.

### Browser contract

`browser` providers export `org.arinn.BrowserProvider1`
([browser-provider1.xml](browser-provider1.xml)); the daemon exposes their tabs to
the shell through `org.arinn.Browser1` ([browser1.xml](browser1.xml)). Types are in
`arinn-protocol::browser`; decisions are in [ADR-0048](../adr/0048-browser-wire-contract.md).

- A snapshot `(generation, revision, [(id, window_token, focused)],
  [(id, window_id, title, url, active, audible)], next_token)` pages like the
  application snapshot. Every tab belongs to a listed window; a window has at
  most one active tab; at most one window is focused. `window_token` is a
  validated PlatformWindows1 token or empty. Tab URLs are single-line, at most
  16 KiB and may use any scheme the browser shows. Tab records carry no favicon in
  version 1; saved-shortcut site icons use the separate optional API below.
- Provider methods: `GetTabs`, `BackendState`, `OpenUrl(generation, url)`
  (`http`/`https` only), `ActivateTab(generation, id)`, `CloseTab(generation, id)`
  and the `StateChanged(generation, revision)` signal; a backend-state change
  alone publishes a new revision. Only the daemon may call them.
- Daemon methods: `GetTabs`, and `ActivateTab`/`CloseTab` with a request ID,
  returning `Operation1` objects. The shell has no generic open-URL method; it
  opens web shortcuts through `WebShortcuts1.Open`.
- Tab titles and URLs never appear in logs or diagnostics.

The SDK serves the provider side with `runtime::BrowserService` and
`run_browser`/`run_browser_on`; its private-bus test compares the live export with
the XML.

### Media and library contract

`media` providers export `org.arinn.MediaProvider1`
([media-provider1.xml](media-provider1.xml)); `library` providers export
`org.arinn.LibraryProvider1` ([library-provider1.xml](library-provider1.xml)).
Types are in `arinn-protocol::media` and `arinn-protocol::library`. Decisions
are in [ADR-0054](../adr/0054-media-and-library-wire-contract.md). `Media1` and
`Library1` are specified for the daemon path. Both are exported
([ADR-0058](../adr/0058-media-routing.md), [ADR-0060](../adr/0060-library-routing.md)).

- A session is `(id, title, subtitle, artwork, state, duration_us, position_us,
  anchor_us, actions, has_content, content)`. `artwork` is an asset id or empty.
  `anchor_us` is `CLOCK_MONOTONIC` microseconds. Known states are `playing`,
  `paused`, `stopped` and `ended` (a confirmed end of file, never inferred from
  the position; [ADR-0063](../adr/0063-ended-session-state.md)). Only `playing` advances.
  Unknown duration is −1. Position and seek are microseconds.
- `Features` lists `MediaOpen1`, `MediaTracks1` and `MediaTimeline1` when those
  interfaces are exported. An MPRIS provider returns an empty list. A missing
  flag is `Unsupported` on the daemon methods, never a successful no-op.
- `MediaOpen1.Open` takes a `file:///` URI. The shell never sends it. Library
  `Launch` returns that URI to the daemon only.
- A library page is collections plus items. Year, season and episode use a
  has-value flag. `available` false keeps a missing file in the catalogue.
  `parent_id` links an episode to its series.
- `LibraryRoots1` is optional: `GetRoots` reports each configured root and
  whether it can be read now, and answers while the backend is unavailable.
- `LibraryResume1` and `LibraryImport1` are optional. `SetProgress` and
  `ImportCompleted` are daemon-to-provider methods. Resume revisions do not
  bump the catalogue revision.
- Asset ids (session `artwork`, thumbnails, item `poster` and `fanart`) are
  fetched with the optional `AssetProvider1.GetAsset(generation, id) → (mime,
  bytes)`, which the provider lists as `AssetProvider1` in `Features`. The
  daemon serves the shell through `Assets1` on the provider's daemon path.
  Assets are JPEG, PNG or WebP of at most 8 MiB whose bytes match the declared
  type. There are no paths, URLs or logs ([ADR-0062](../adr/0062-asset-contract.md)).

The SDK serves the provider side with `runtime::MediaService` and
`runtime::LibraryService` (`run_media` / `run_library`). Private-bus tests
compare the live provider export with the XML. File URIs and titles are not
logged.

### Download contract

`download` providers export `org.arinn.DownloadProvider1`
([download-provider1.xml](download-provider1.xml)); the daemon serves their jobs to
the shell through `org.arinn.Downloads1` ([downloads1.xml](downloads1.xml)). Types
are in `arinn-protocol::download`, the SDK service is `DownloadService`; decisions
are in [ADR-0025](../adr/0025-downloads-mvp.md) and
[ADR-0067](../adr/0067-download-wire-contract.md).

- A job is `(id, title, state, completed_bytes, has_total, total_bytes,
  rate_bytes_per_second, eta_seconds, has_progress, progress, actions,
  error_code, error_message, outputs)`. States are `queued`, `downloading`,
  `paused`, `verifying`, `completed` and `error`; actions are `pause`, `resume`,
  `start-now` and `retry`. Without a total or a progress value, the number is 0
  and the flag is false. `eta_seconds` is −1 when unknown. `error_code` is an
  `Error1` name exactly when the state is `error`.
- An output is `(id, title, file_uri)`. The id is the stable output identity for
  the library's completion ledger. `file_uri` is a local `file:///` URI the
  provider mapped, or empty when no mapping covers it.
- Provider methods: `GetJobs`, `GetJob`, `BackendState`, `Features` (empty in
  version 1), `Pause`, `Resume`, `StartNow`, `Retry` and the
  `StateChanged(generation, revision)` signal. Only the daemon may call them. A
  command the job does not list is `Unsupported`; with the backend
  `unavailable`, cached rows stay readable and commands are `Unavailable`.
- Daemon methods: `GetDownloads`, `GetDownload`, and the four commands with a
  request ID, returning `Operation1` objects. A download is the job with
  outputs reduced to `(id, title)`, plus `import_state` (`none`, `pending`,
  `imported`, `unavailable`, `unsupported`) and `library_refs`, which are set
  only when imported. Changes arrive as `Daemon1.StateChanged` with the
  capability `download`.
- There is no method to add, delete or move data.

### Platform window contract (production implementation pending)

`org.arinn.PlatformWindows1` is specified for the daemon service/root path by
[platform-windows1.xml](platform-windows1.xml) and
[ADR-0039](../adr/0039-platform-window-contract.md). It defines paged live windows,
platform generations and targeted activation/graceful-close operations for
registered application providers. Exact field meanings, authorization, lifetime
and failure rules are in the [platform contract](../architecture/platform-windows.md).
The SDK exports `platform::WindowClient`; its private-bus fixture checks XML and
client behaviour. The production daemon does **not** export this interface yet.
This is not a seventh capability or a shell-to-compositor API.

### Web shortcut contract (daemon implementation pending)

`org.arinn.WebShortcuts1` is specified for the daemon service/root path by
[web-shortcuts1.xml](web-shortcuts1.xml) and typed in `arinn-protocol::web`
([ADR-0043](../adr/0043-web-shortcuts.md)). Web shortcuts are Arinn-owned state,
not a capability; the shell manages them from the TV.

- `List` returns the complete ordered list `(revision, [(id, title, url, icon)])`.
  At most 64 shortcuts; titles are single-line, non-empty and at most 256 bytes;
  URLs are absolute `https`, or `http` for a local-network host judged without
  DNS (private, loopback, link-local and unique-local IP literals, single-label
  names, `.local`, `.lan`, `.home.arpa`, `.internal`;
  [ADR-0045](../adr/0045-local-http-web-shortcuts.md)), without user information,
  whitespace or control characters and at most 2048 bytes; `icon` is empty or an opaque packaged icon
  reference. The largest valid list fits one message, so there is no paging.
- `Add(title, url, icon, request_id)` returns the new ID and is deduplicated by
  caller and request ID. `Update(id, revision, …)` and `Remove(id, revision)`
  take the list revision the caller last read and fail with `Conflict` when it
  is stale, `NotFound` for an unknown ID and `InvalidArgument` for invalid input.
- `Open(id, request_id)` returns an `org.arinn.Operation1` object. The daemon
  opens the URL through the default BrowserProvider; with none available the
  operation fails with `Unavailable`.
- `Changed(revision)` follows every committed change; subscribe before `List`.

Shortcut URLs are user configuration: they never appear in logs, diagnostics or
`arinnctl diagnostics` output. The protocol test checks the XML against a live
fixture export on a private bus.

The production daemon exports it ([task #56](https://github.com/LimeBeck/arinn/issues/56)).
Shortcuts and the list revision are stored in `state.sqlite3` (schema 3), so a
stale revision stays stale across daemon restarts. On start the daemon seeds the
packaged default set from the first `arinn/web-shortcuts.toml` in the XDG data
home and data directories, once per state database; invalid entries are skipped
and a removed default is never re-added. A full list rejects `Add` with
`InvalidArgument`. `Open` opens the URL in the first ready `browser` provider with a connected
browser and brings its focused window forward ([#83](https://github.com/LimeBeck/arinn/issues/83));
with no connected browser the operation fails with `Unavailable`. Launching a
closed browser first is not implemented yet.

### Production application and registry semantics

`Providers(token)` returns an immutable registry page with a snapshot ID. Start
with an empty token and follow `next_token`; tokens are caller-bound and expire
in 30 seconds. Each provider record identifies its capability endpoint, generation,
revision, runtime/backend status, source manifest and safe diagnostic timestamps.
Clients derive available capabilities from ready provider records.

Each application record is `(id, title, can_launch, group, icon)`
([ADR-0059](../adr/0059-application-icon-references.md)). `group` is chosen
by the provider: `online`, `games`, `media` or `other` (spec §5.5). It is a short
lowercase token on the wire; consumers display an unknown value as `other`, so
providers can add groups without breaking older shells.

`icon` is empty or a local reference of at most 1024 bytes: a theme icon name
(ASCII letters, digits, `.`, `_` and `-`, with at least one letter or digit) or
an absolute file path (`/` but not `//`, no empty, `.` or `..` segment). A URL,
a `file:` URI or a relative path is `InvalidArgument`. Empty, invalid, or a
reference the consumer cannot load as a local image is the existing neutral
placeholder. The consumer does not fetch an icon and does not call the provider
for one. The reference is not image bytes and is not logged. Desktop `Icon`
discovery and shell loading are later tasks; in-tree providers send an empty
reference until then. `Application1` is pre-stable, so this signature change is
the same class as the group field, not a minor addition to a released API.

`GetApplications(token)` combines installed applications and running instances
under one immutable generation/revision. Pages have at most 200 total entries and
256 KiB of encoded body. The complete catalogue is bounded to 100,000 entries and
16 MiB; at most 32 concurrent leases are retained by each store. Consumers install
the signal match before the first read and publish only a complete, validated
snapshot. `Conflict` on a token requires a new read from the beginning. A late
subscriber can first observe any positive revision, not necessarily revision 1.

A public application command takes generation, opaque local ID and request UUID,
then returns an operation object. Subscribe to `Changed` before `Get`; completion
can precede the admission reply. Deduplication uses caller unique name and request
UUID; changed content gives `Conflict`. Read/registration budget is 3 seconds,
controls 5 seconds, launch dispatch 30 seconds. Commands are never retried automatically.
The private provider command reply completes the daemon's operation; providers do
not create a second public operation tree.

**Launch lifetime and cancellation**
([ADR-0110](../adr/0110-application-launch-lifetime-and-cancellation.md)). A
provider that observes windows answers `Launch` when a new window of the
application appears, and fails as soon as the launch can no longer show one: when
the launch's processes end with no window of the application. A window that
already existed counts at that moment, because the launch was handed to a running
instance (spec §7.2). The desktop provider fails with `Unavailable` in that case.
The reply may come long after 30 seconds: the daemon keeps the public operation
`running` and applies no overall deadline to an application launch. Only dispatch
(starting the system launcher) is bounded by the 30-second budget, inside the
provider. A provider that cannot observe windows answers at dispatch, as before.
Provider loss or a new generation still ends the operation with `Unavailable`.

An application launch reports `cancellable = true` until it ends; every other
operation reports `false`. `cancellable` is a daemon property: it does not mean
that the provider implements `CancelLaunch`. `Operation1.Cancel` is accepted only
from the caller that started the operation (`PermissionDenied` otherwise) and only
for a cancellable operation (`Unsupported` otherwise). One `Cancel` stops at most
that launch:

- The daemon forwards `CancelLaunch(generation, id)` only while the operation is
  the only running launch of that provider, generation and ID, from any caller.
  Otherwise `Cancel` answers `Conflict`, a second `Cancel` of the same launch in
  flight included, and nothing is stopped.
- A new `Launch` of that ID waits to be dispatched until an in-flight
  `CancelLaunch` has been answered, so a cancel never reaches a launch started
  after it.
- A launch counts as running until the provider has answered its
  `Provider1.Launch`, whatever the operation's state. The daemon's own launch of
  the browser application for a web open
  ([ADR-0055](../adr/0055-launching-a-closed-browser.md)) counts too.

When the provider confirms, the operation ends as `cancelled` with error
`Cancelled`. When the provider refuses (`Unsupported`, `NotFound`, `Conflict`),
`Cancel` returns that error and the operation keeps running. A provider without
the method (`org.freedesktop.DBus.Error.UnknownMethod`) counts as `Unsupported`.
A provider that does not answer within 15 seconds makes `Cancel` return
`Timeout`; the operation keeps running, and the launch's processes may still be
starting. A provider reply of `Cancelled` also ends the operation as `cancelled`.
`Cancel` on an application launch that has already ended returns success and
changes nothing; clients read the final state with `Get`.

`CancelLaunch` stops every unfinished launch of that application ID, with the
processes those launches started, and their `Launch` replies fail with
`Cancelled`. The daemon's rule above keeps that set to the one launch being
cancelled. It is authorized like the other provider commands and checks that the
generation is the provider's current one, but not the catalogue, so a launch
whose entry disappeared stays cancellable. The handler receives only the ID, so a
launch that the provider still tracks from its previous registration (an earlier
generation, whose operation the daemon already ended with `Unavailable`) is
stopped as well. The SDK's default handler answers `Unsupported`.
Cancellation stops processes; it never rolls back other side effects.

`Status`, `HomeItems`, `Home` and `Back` support the minimal shell/platform bridge.
`HomeItems` is a bounded projection of activatable running instances (up to five),
not a complete catalogue. `Status.generation` contains the daemon epoch;
provider generations and revisions come from provider/application pages.

### Build identity

[ADR-0097](../adr/0097-build-identity.md) adds `BuildInfo() -> (version: s,
build_id: s)` to [Daemon1](daemon1.xml), preserving every existing signature.
`version` is the workspace version; `build_id` is `<version>+g<12 hex>`, with
`-dirty` for a modified checkout, or the bare version when the commit is
unknown (`arinn_protocol::build::validate`). Two ids name the same build when
they are equal without `-dirty` (`same_build`). A daemon older than this method
answers `UnknownMethod`, which a client treats as another build.
`scripts/build_id.py` is the only source of the id for cargo and CMake builds.

### Explicit Home presentation

[ADR-0069](../adr/0069-explicit-home-presentation-requests.md) adds two members to
[Daemon1](daemon1.xml), preserving every existing signature:

- `ReportHomeRequest(target: s, output: s)` is restricted to the current
  `org.kde.KWin` unique owner. Both strings follow the existing ordinary-text
  validation (at most 8 KiB, no NUL); either may be empty. It commits Home visible
  with that Return target/output, emits `Changed`, then emits one
  `HomeRequested(epoch: s, target: s, output: s)`, including for identical repeats.
  Unauthorized callers receive `PermissionDenied`; invalid text receives
  `InvalidArgument`. Neither failure publishes state or presentation intent.
- `HomeRequested` carries the daemon epoch from `Status.epoch` and the request's
  opaque target/output. It requests the existing §6.3 Home action locally: close
  transient UI, navigate Home, focus Return when available, never toggle or
  confirm an action. It is neither provider state nor an operation result.

`Home()` still queues a platform command; its reply only acknowledges admission.
The platform reports explicit intent after its unlocked-session check, just as
for the global shortcut. `ReportHome(visible, target, output)` and `Changed`
remain state-only: reconnect, disappearance, remap, dismissal and ordinary
provider updates never synthesize `HomeRequested`.

A consumer subscribes against the current daemon unique owner before reading
status, validates the epoch and payload, and discards queued events on owner
loss. The signal's context avoids a status round trip in local navigation;
subsequent state reports independently update Return availability. Each received
request is handled, including identical repeats. This is a transient signal,
with no persistence, retry, historical replay or catch-up after reconnect.
Do not infer missed intent from `Status` or `Changed` on an older daemon.

The matching adapter/daemon must be installed together. Old shells ignore the
signal and keep their existing status path; old adapters cannot report explicit
intent. Production shell consumption and QML focus regression are a follow-up
after #45 merges. This contract alone does not complete AC-18.

### Quick-menu presentation

[ADR-0106](../adr/0106-quick-menu-presentation.md) adds two members to
[Daemon1](daemon1.xml), preserving every existing signature:

- `ReportQuickMenuRequest(target: s, output: s)` has the authentication and
  validation of `ReportHomeRequest`. It commits the same visible Home state with
  that Return target/output, emits `Changed`, then emits one
  `QuickMenuRequested(epoch: s, target: s, output: s)`, including for identical
  repeats. A rejected call publishes neither state nor intent.
- `QuickMenuRequested` asks the shell to open the quick menu (spec §5.7) over
  what it shows. It is a transient intent with the delivery rules of
  `HomeRequested` above: bound to the current unique owner and epoch, never
  persisted, replayed or inferred from `Status` or `Changed`.

`ReportHomeRequest` never emits `QuickMenuRequested`, and
`ReportQuickMenuRequest` never emits `HomeRequested`. `Status.home_visible` is
true while either is shown; which one the shell shows is its own state. Both
requests arm the [Activity background](../architecture/activity-background.md)
capture after their signal, so Home opened from the quick menu shows the
Activity's last frame.

### Optional Return/window association

[ADR-0073](../adr/0073-return-window-association.md) defines optional
[ReturnAssociation1](return-association1.xml) and trusted
[KWinReturn1](kwin-return1.xml) at the daemon root. The production daemon/KDE
producer is implemented in [#217](https://github.com/LimeBeck/arinn/issues/217);
asynchronous shell consumption is implemented in [#219](https://github.com/LimeBeck/arinn/issues/219).
Real KDE qualification remains pending.
The [association contract](../architecture/return-association.md) specifies typed
optional values, ownership, lifetime invalidation and conservative provider joins. Existing Status,
HomeRequested, BrowserWindow, PlatformWindows1 and KWinBridge1 signatures remain
unchanged. Missing association always preserves opaque Return. The consumer
implements browser-only deduplication; media association remains separately scoped.

### Optional browser media-session association

[ADR-0096](../adr/0096-browser-media-session-association.md) defines
[MediaProcess1](media-process1.xml) on a media provider,
[BrowserProcess1](browser-process1.xml) on a browser provider and
[MediaAssociation1](media-association1.xml) at the daemon root. The daemon
links a browser's media session to its one audible tab by process identity;
process IDs never leave the daemon. The [media association contract](../architecture/media-association.md)
gives the join, hold and invalidation rules. The MPRIS and Chrome providers,
the daemon and the shell implement it ([#359](https://github.com/LimeBeck/arinn/issues/359));
real Chrome/KDE qualification remains pending. Missing interfaces keep today's
separate media and tab cards.

## SDK and fixtures

`arinn-plugin-sdk::runtime` exports `ApplicationService`, `ApplicationHandler` and
`run`. The daemon supplies provider/plugin IDs, version, token file and private
provider data/state directories in its launch environment. SDK methods authenticate
the daemon's unique owner. Owner changes trigger re-registration and a fresh snapshot;
`replace` publishes validated content atomically and emits a revision notification.

`arinn-plugin-demo --fixture <name>` supports `populated`, `empty`, `delayed`,
`invalid-id`, `crash`, `reconnect`, `revision-race`, `unavailable` and `large`.
The [production contract harness](../../tests/integration/phase1.py) runs those
fixtures through manifest reloads on a private bus. See the
[implementation record](../architecture/phase-1-progress.md) for evidence and limits.

Known capabilities are `application`, `browser`, `media`, `library`, `action` and
`download`. The daemon routes `application`, `browser`, `media`
([ADR-0058](../adr/0058-media-routing.md)) and `library`
([ADR-0060](../adr/0060-library-routing.md)). `action` and `download` stay incompatible
until their XML exists. [MVP extensions](mvp-extensions.md) describe the media
flags and the download model; download XML is still outstanding.

## Licensing

Protocol types, SDK and interface XML use `MIT OR Apache-2.0`; providers may use any
license ([ADR-0029](../adr/0029-licensing.md)).

## Internal platform transport

[KWinBridge1](kwin-bridge1.xml) and [its lifecycle contract](../architecture/kwin-transport.md)
define trusted adapter inventory/control transport. This is not a provider capability.
SDK registration contexts expose the registered connection for authenticated platform
access. Production daemon/KDE implementation remains in issue #20.

Managed playback window association: [contract](../architecture/managed-playback.md),
[MediaWindow1](media-window1.xml), [MediaWindowProvider1](media-window-provider1.xml)
and [KWinMedia1](kwin-media1.xml). These optional bindings do not yet imply production support.

### Optional saved-shortcut site icons

[ADR-0075](../adr/0075-web-shortcut-site-icons.md) defines
[WebShortcutIcons1](web-shortcut-icons1.xml) on the daemon root and
[WebIconProvider1](web-icon-provider1.xml) on a browser provider. These optional
interfaces leave existing shortcut/tab records unchanged. See the
[site-icon contract](../architecture/web-shortcut-icons.md) for caller/lifetime
checks, bounded public-HTTPS discovery, local cache and raster-image limits.
Contract-only preparation: production fetching, routing and rendering remain
dependent work. Missing support retains the monogram; installed-app icons remain
under ADR-0059.

### Optional Activity background transport

[Background1](background1.xml) and [BackgroundCapture1](background-capture1.xml)
are optional daemon-root metadata/frame and selected-platform publisher interfaces,
not provider capabilities. [ADR-0087](../adr/0087-bounded-activity-background-frames.md)
and the [background contract](../architecture/activity-background.md) define
identity, immutable descriptor bounds, authorization and invalidation. Contract
only: production capture, export and shell binding remain #149 dependent work.
