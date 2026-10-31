# Plugin runtime: discovery, supervision and the Chrome bridge

Part of [ADR-0021](../adr/0021-plugin-runtime-ownership.md), 2026-09-27. Status: production application runtime implemented; local private-bus verification uses a test manager. Real systemd/KDE restart, reload and Home/Back checks also passed; see the [implementation record](phase-1-progress.md) for scope and remaining checks.

## Manifest and discovery

A manifest is TOML with the required fields `id`, `name`, `version`, `api_version = 1`, `executable` and `capabilities`, and optional `args` (array of strings). After installation `executable` is an absolute path to a regular executable file. Neither `executable` nor `args` is a shell expression. Capabilities: `application`, `browser`, `media`, `library`, `action` and `download` ([ADR-0025](../adr/0025-downloads-mvp.md)); their API major version is 1. An unknown capability is never registered silently: the manifest gets `Unsupported` in diagnostics.

Discovery reads `*.toml` directly in `$XDG_DATA_HOME/arinn/plugins`, then in `arinn/plugins` of each `$XDG_DATA_DIRS` entry in order. XDG defaults apply as specified. A user manifest overrides a system manifest with the same ID; an invalid user manifest disables the provider with an explicit error and no surprising fallback. Duplicate IDs at the same level are `Conflict`; no file is picked at random. The source path is shown in diagnostics.

The registry is re-read at startup and on `arinnctl providers reload`; file watching and timed hot reload are not required for the MVP. Removing a manifest and reloading stops the provider unit and removes its capabilities; user applications it launched keep running. Changing `executable`, the API version or capabilities requires a provider restart; changing display metadata updates the registry without a restart. Favorites stay, marked unavailable.

### Discovery implementation status (2026-09-28)

The discovery library and reload planner are implemented in `crates/arinnd/src/registry.rs`;
`arinnd --check-plugins` exposes an offline check. The live daemon applies it at startup and on an asynchronous `providers reload` operation.

Discovery reads only direct `.toml` children, ignores relative XDG environment values,
and uses deterministic ordering. Reads are bounded to 256 KiB per manifest and 1,024
manifests per priority level. Non-regular manifest files are rejected without blocking
on FIFOs. Arguments are passed through as data in the model; they are never evaluated.
The manifest model allows at most 256 arguments, each within the ordinary-text limit.

If an invalid manifest still has a valid ID, it shadows lower-priority entries for
that ID and is shown as an error. If its ID cannot be recovered (including malformed
TOML or an unreadable file/directory), discovery retains already resolved levels and
valid entries in the current level, but suppresses lower-priority discovery: it cannot
prove that a system manifest is not the broken override. Repair the diagnostic and
re-run the check to restore lower-priority discovery. Duplicate IDs at one level
remain conflicts when the ID is readable, even if other required fields are invalid.

Changing argv also requires a restart. Name/version-only changes update metadata.
The planner reports start, stop, restart or metadata actions; the runtime applies them
through the systemd adapter. Diagnostic JSON contains source paths and safe
error codes, but omits executable paths and argv values from the manifest.

## Provider data-directory identity

OQ-06 was resolved by the project owner on 2026-09-28: use the full lowercase
SHA-256 digest of the exact UTF-8 `provider_id` for provider-owned data and state
folders. This preserves their location independently of `state.sqlite3`.
See [ADR-0034](../adr/0034-provider-data-directory-identity.md) and the
[storage contract](implementation-baseline.md#storage). Directory provisioning
and registry-reset recovery tests are implemented in Phase 1.

## Units and ownership

**Daemon:** a regular `arinnd.service` bound to the graphical session, without user lingering. The shell is started by the session autostart launcher after Plasma login and connects to the same daemon. Autostart must not create a second shell or daemon; packaging tests graphical session shutdown.

The package ships these as systemd user units: `arinn-session.target` (opt-in, `WantedBy=graphical-session.target`) pulls in `arinnd.service` and `arinn-shell.service` (both `Type=dbus` on their bus names) and `arinn-kwin.service`, which loads the packaged KWin script for the session, plus the optional Activity background producer script ([background experiment](background-experiment.md#kde-producer)); a producer that KWin refuses never fails the unit. All are `PartOf=` the graphical session and the target. A shell that finds `org.arinn.Shell1` already owned logs the owner's PID and exits with status 2, which the unit does not restart (`RestartPreventExitStatus=2`), so a leftover instance cannot exhaust the start limit. Nothing is enabled by the package; a user runs `systemctl --user enable arinn-session.target` ([ADR-0042](../adr/0042-opt-in-session-autostart.md)).

**Provider:** when the session has a systemd user manager, a **transient user service** created by the daemon through the systemd D-Bus API. The unit name is derived from the registry UUID; `ExecStart` is the validated binary plus argv, never a shell string. Only that supervisor restarts the process; the daemon controls handshake and availability. When no user manager is available ([ADR-0089](../adr/0089-daemon-provider-supervisor-without-a-systemd-user-manager.md)), the daemon enables its own supervisor once at startup and applies the same exec, environment and restart budget to the manifest process only. A provider never runs under both runtimes in one daemon instance. Chrome, the Native Messaging host and desktop-launched applications stay outside provider supervision.

Defaults: `Type=exec`, `Restart=on-failure`, `RestartSec=2s`, `StartLimitIntervalSec=60s`, `StartLimitBurst=5`, `TimeoutStopSec=5s`. After the limit is exhausted the provider is failed, the error is in diagnostics and a manual restart is needed; there is no infinite loop. Only an explicit request clears the failed/start-limit state. A handshake timeout stops the hung plugin and counts against the same restart budget.

Provider runtime states: discovered → starting → ready; on crash restarting or failed; on explicit stop stopped, without auto-restart; an incompatible manifest or API gives incompatible and no working capability. Backend state is separate: available, unavailable or degraded. A closed Chrome or no MPRIS player is not a plugin crash.

Running external applications are not in the plugin service cgroup: a desktop application is launched by the session application launcher in its own application unit or scope. An integration test must confirm that restarting or removing the desktop plugin does not kill Chrome or Steam. The launcher parses `.desktop` files, `DBusActivatable` and field codes according to the [Desktop Entry Specification](https://specifications.freedesktop.org/desktop-entry/latest-single/).

systemd documents the transient service settings ([transient settings](https://systemd.io/TRANSIENT-SETTINGS/)); the defaults above are Arinn's choice. Restart semantics are checked against the installed `systemd.service(5)` and `systemd.unit(5)` during implementation.

## Provider environment and settings

A provider unit starts with these variables:
- `ARINN_PROVIDER_ID`, `ARINN_PLUGIN_ID` and `ARINN_PLUGIN_VERSION`;
- `ARINN_TOKEN_FILE` (the registration token);
- `ARINN_PROVIDER_DATA_DIR` and `ARINN_PROVIDER_STATE_DIR` (ADR-0034);
- `ARINN_PROVIDER_SETTINGS`: the provider's settings from
  `[providers.<id>.settings]` in `config.toml`, as compact JSON, at most 64 KiB.
  It is `{}` without settings.

The settings table is opaque to the daemon. A value without a JSON form, such
as a TOML datetime, makes the configuration invalid. Settings are read at each
start, so a change reaches the provider at its next start or restart
([ADR-0064](../adr/0064-provider-settings-in-config.md)). A transient unit keeps
the environment it was created with, so `Daemon1.Restart` stops the unit, resets a
failed state, waits until systemd has unloaded it, and creates it again
instead of restarting it in place
([#234](https://github.com/LimeBeck/arinn/issues/234)). Stopping a unit always
waits briefly for that unload, because systemd refuses a new transient unit
under a name that is still loaded. A daemon restart or `providers reload`
attaches to a surviving unit when its main process still runs the manifest
executable: the same device and inode, and `/proc/<MainPID>/exe` is not marked
deleted. A replaced file (the old inode is deleted after a package upgrade)
makes the daemon discard that unit and create it again, so the provider runs
the installed binary and the current settings. The replacement is logged with
the provider id and without paths
([#243](https://github.com/LimeBeck/arinn/issues/243)). A surviving unit whose
binary still matches keeps the environment it was created with until it
restarts. A typed schema and editing from the TV are
[#145](https://github.com/LimeBeck/arinn/issues/145).

## Handshake and recovery

1. The registry validates the manifest and creates a unit with the `provider_id` and a one-time registration token in runtime credentials or a file (mode 0600, never logged).
2. The plugin connects to the session bus and exports its provider object.
3. The plugin calls `Register(plugin_id, provider_id, api_major, api_minor, version, capabilities, object_path, token)`. The daemon takes the sender from the D-Bus envelope and checks the UID, the launch token and that the PID belongs to the expected unit/cgroup. The token is not a sandbox.
4. The daemon accepts only capabilities listed in the manifest and returns `daemon_epoch`, `generation` and negotiated limits. The provider becomes available only after successful registration and its first valid snapshot.
5. `NameOwnerChanged` invalidates the provider. After a daemon restart, providers notice the owner change, receive a new registration challenge for the existing unit and register again with a new generation. The registry never starts a duplicate unit.

The SDK filters its ownership signal subscription to `org.arinn.Daemon1` at the
bus. Unrelated connection/name churn must not fill its signal queue while it waits
for registration, including re-registration after daemon replacement. The SDK
registration-context regression test generates unrelated ownership changes before
the replacement daemon returns its registration reply (G-1.2, G-1.3).

Token replay and a second active owner of one provider give `PermissionDenied` or `Conflict`. API v1 supports only major version 1; minor additions never change existing required fields or signatures. Unknown optional metadata keys are ignored; unknown enum values are shown as unsupported, never as success. Wire XML is fixed per capability; a major incompatibility is reported before a working provider starts.

## Revision-aware refresh and restart maintenance

The daemon subscribes before its first read and retains the one-second fallback
poll. After a complete snapshot is imported, an unchanged first-page revision
for that watcher's owner and generation skips subsequent pages, entry-list clones
and cache import ([ADR-0122](../adr/0122-revision-aware-provider-polling.md),
[#605](https://github.com/LimeBeck/arinn/issues/605)). A new revision is still read
and validated completely before publication; failures do not advance the accepted
revision. Backend state and known feature flags are refreshed even when entries
do not change. Repeated or stale revision signals are coalesced; missed fallback
ticks are skipped rather than replayed in a burst. These bounds also limit
abandoned first-page leases under the SDK's existing 30-second expiry.

Library catalogue and resume polling track separate revisions. A resume/root
signal does not reread the unchanged catalogue, and a catalogue signal does not
reread unchanged resumes; the timer reconciles both. Roots and additions retain
their independent reads and validation. Media progress and download import retries
continue with cached catalogue data. Unchanged downloads skip import refresh and
row publication until the existing import-retry deadline.

Restart maintenance keeps a lazy SQLite connection shared by blocking workers.
Repeated observations of the same supervisor counter perform no UPDATE. Counter
resets, cumulative counts and manual restart semantics remain unchanged. SQLite's
journal mode and schema are unchanged. These are isolated-work savings, not a
G-5.4 measurement on the target device.

## Chrome: two processes instead of two supervisors of one process

`Chrome MV3 extension ↔ Native Messaging bridge ↔ arinn-plugin-chrome ↔ arinnd`

Chrome starts the short-lived `arinn-chrome-host` on `connectNative`. The host only validates, frames and forwards messages; the provider stays a long-lived systemd service. Host ↔ provider is a separate typed D-Bus interface on the session bus; the plugin-specific transport never reaches the core. After disconnect the host exits, and it also exits when the provider's bridge name loses its owner or moves to a restarted provider, so the extension reconnects through a fresh host; the Chrome plugin keeps running with its backend unavailable or with the remaining browser connections.

Chrome owns the lifetime of the native host and talks over stdin/stdout; `connectNative` creates a persistent port ([Native Messaging](https://developer.chrome.com/docs/extensions/develop/concepts/native-messaging)). So the host is not another instance of the systemd provider.

Extension permissions are `tabs` and `nativeMessaging`; no content scripts and no `host_permissions` for reading pages. The native manifest's `allowed_origins` contains the exact extension ID. Incognito is not connected by default. Each bridge connection gets an ID unique for the provider's lifetime; tabs get an opaque local ID composed by the plugin from the connection identity and the browser ID. The core never parses it. Several browser profiles may produce several connections without creating Arinn profiles.

Extension reconnect: 1, 2, 4, 8, 16, 30 s with jitter, capped at 30 s; after a successful connect a full snapshot, then a debounced full snapshot after every change. This restores the connection; it never repeats a user's mutating command. Old tab IDs are invalid after a browser connection is lost.

The desktop ApplicationProvider owns browser application launch. The Chrome provider declares only `browser`, not `application`; their entities link through a validated platform window token ([ADR-0038](../adr/0038-browser-launch-ownership.md), resolving OQ-03). If the browser is closed, launch goes through that ApplicationProvider; open URL waits for a browser connection within the overall launch deadline. With several connections, the default or last foreground connection is used, if it still exists. Activating a tab inside the browser does not replace compositor activation of the window, which is requested through PlatformService.

### Chrome implementation (2026-09-28, [#82](https://github.com/LimeBeck/arinn/issues/82))

- **Extension** (`plugins/chrome/extension`, installed to `/usr/share/arinn/chrome-extension`):
  - MV3 service worker; `key` fixes the ID `eajlncomibfmpmkhkbcdmajmkpfaaibf`;
  - sends `{"type":"snapshot","windows":[{id,focused}],"tabs":[{id,windowId,title,url,active,audible}]}`, debounced by 150 ms;
  - executes `{"type":"command","serial","action","tabId","url"}` (`activate`, `close`, `open`) and answers `{"type":"result","serial","error"}`;
  - skips incognito windows and tabs;
  - window per activity ([ADR-0050](../adr/0050-window-per-browser-activity.md), [#100](https://github.com/LimeBeck/arinn/issues/100)): every window Arinn manages has the Chrome window type `popup`, so the rules survive a service worker restart without a `storage` permission. Snapshots list `normal` and `popup` windows;
  - `open` focuses a managed window whose active tab has the same origin (scheme, host and port), otherwise creates a `popup` window; either way the window becomes focused and fullscreen. It never adds a tab to an existing window;
  - a tab whose opener is in a managed window (a `target=_blank` link; Chrome places it in a normal window) moves into its own fullscreen `popup` window;
  - a managed window that leaves fullscreen returns to it on `windows.onBoundsChanged`; minimised windows are left alone. Pages' own `window.open` popups are also `popup` windows and are therefore kept fullscreen;
  - KWin reports managed windows with the app ID `chrome-<extension id>-<profile>` and ordinary windows with `google-chrome`; window association ([#101](https://github.com/LimeBeck/arinn/issues/101)) may use this.
- **Host** (`/usr/lib/arinn/arinn-chrome-host`):
  - refuses any origin except the packaged extension (exit status 2) before touching the bus;
  - frames Native Messaging with at most 8 MiB in and 1 MiB out;
  - rejects unknown message types and fields;
  - attaches to the provider's well-known name `org.arinn.ChromeBridge1` and publishes snapshots;
  - exports `org.arinn.ChromeHost1.Command(action, tab, url) → error` only to the bridge owner, with a 5 s result timeout;
  - exits when the port closes, a message is malformed or the provider goes away. It never logs titles or URLs.
- **Provider** (`arinn-plugin-chrome`):
  - owns `org.arinn.ChromeBridge1` (one instance per session) and serves `BrowserProvider1` through the SDK;
  - merges up to 16 connections: tab IDs `c<n>:t<tab>`, window IDs `c<n>:w<window>`; only the most recently focused connection keeps a focused window;
  - cuts long titles, and replaces URLs that break the wire rules with an empty string;
  - `open` goes to the most recently focused connection;
  - backend `unavailable` without a browser;
  - window tokens link browser windows to platform windows by focus agreement ([ADR-0052](../adr/0052-browser-window-association.md), [#101](https://github.com/LimeBeck/arinn/issues/101)). The provider reads `PlatformWindows1` under its registration. One focused browser window and one active platform window that agree for 0.5 s are linked. A link ends when either side, the platform generation or the registration goes away.
- **Packaging:** the same host manifest is installed for Google Chrome (`/etc/opt/chrome/native-messaging-hosts/`) and Chromium (`/etc/chromium/native-messaging-hosts/`).
- **Installing the extension** (owner decision, 2026-09-28): open `chrome://extensions`, enable *Developer mode*, choose *Load unpacked* and select `/usr/share/arinn/chrome-extension`. The ID stays the same across updates; after a package update, press *Reload* on the extension card. Publishing to the Chrome Web Store is a pre-release decision ([#74](https://github.com/LimeBeck/arinn/issues/74)).
- **Daemon routing** ([#83](https://github.com/LimeBeck/arinn/issues/83)):
  - a manifest declares exactly one routed capability, `application` or `browser`; other sets are incompatible (`Unsupported`);
  - for a `browser` provider the daemon imports the tab snapshot and exports `Browser1` at the provider's path;
  - `ActivateTab` asks the provider to select the tab, then activates the tab's compositor window through PlatformWindows1 when its window has a token, which also hides Home; without a token it only hides Home and the browser's own focus request decides;
  - `WebShortcuts1.Open` opens the URL in the first ready provider with a connected browser. It then waits up to 2 s for the provider's next snapshot. If the URL opened a new window, the daemon only hides Home. Otherwise it activates the focused window's token (ADR-0052);
  - `browser` providers may read platform windows, for window association;
  - with no browser connected, `WebShortcuts1.Open` first launches the browser application through the ApplicationProvider ([ADR-0055](../adr/0055-launching-a-closed-browser.md)). The application is `browser_application` in `config.toml`, or else the first launchable entry of the packaged `/usr/share/arinn/browser-applications.toml`. Launch, connection and open share one 30 s deadline.
  - `Browse1` ([ADR-0066](../adr/0066-address-and-search-contract.md), [#177](https://github.com/LimeBeck/arinn/issues/177)):
    - resolves typed text with `browse::resolve_input` and the selected engine, then opens the URL through the same path as `WebShortcuts1.Open`;
    - engines come from the packaged `/usr/share/arinn/search-engines.toml`, read once at start. The daemon setting `search_engine` (provider `arinn`) is an enum over them, and an empty or unknown value uses the first engine. With no engines, the setting is not listed and every search is `invalid`;
    - the same `arinn` source holds the shell's preferences `light_field` and `ambient_timeout` ([ADR-0112](../adr/0112-shell-preferences-as-core-settings.md));
    - an accepted `Open` records the trimmed text in `browse_recent` in `state.sqlite3` (introduced in schema version 4), newest first, deduplicated, keeping five, and emits `RecentChanged`. Typed text and URLs are never logged.

Verification:

- unit tests: framing, messages, snapshot merging and routing;
- `plugins/chrome/tests/chain.rs`: the real host binary between a fake extension and the bridge on a private bus, covering origin refusal, snapshots, command round trips and error mapping, command validation, caller checks, malformed input, port close and provider loss;
- `tests/integration/chrome_extension.test.cjs`: the extension against a mocked `chrome` API, covering the manifest permissions and ID, snapshots without incognito, debouncing, commands, backoff and no replay, and the window-per-activity rules (new window, same-origin reuse, origin matching, adopting opened tabs, keeping fullscreen);
- isolated window-per-activity check (2026-09-29): the extension loaded into Chrome 153 in a nested `kwin_wayland --virtual` on a private session bus, driven through the DevTools protocol. Opening a URL gave a fullscreen 1920×1080 `popup` window; opening the same origin again created no window; a `target=_blank` click moved the new tab into its own fullscreen window; a window set back to normal returned to fullscreen. Page `window.open` popups, DRM services and the real desktop were not tested;
- the package tests check the installed files, the manifests and the origin refusal.

A real-browser check is an owner operator test.

## Desktop implementation status (2026-09-28)

The desktop provider, authenticated KWin inventory backend and session launcher
are implemented. Private-bus tests exercise actual daemon/provider processes,
GIO dispatch, failure propagation, remap, restart/removal and fixture-application
lifetime. The test manager cannot prove cgroup isolation; real KDE/systemd
qualification is outstanding. See the [implementation record](desktop-provider.md).

## Desktop and media edge cases

The desktop provider reports installed entries and running instances. Window identity comes from PlatformService. Closing an application first requests a graceful close of its windows; if that is impossible, `Unsupported` — never a kill by process name. Force kill is not a regular MVP terminate. Several windows are separate instances; activate picks the most recently used available window.

Media session selection: a session explicitly chosen by the user; then a session explicitly linked to the foreground Activity; then the playing session with the most recent observed state change; then the most recent paused one. Never by product name. Position, duration and absolute seek are signed int64 microseconds; unknown duration is −1; position is non-negative. The provider publishes a position anchor (`CLOCK_MONOTONIC` microseconds) on state change or seek and at most once per second while playing; the shell interpolates locally on that clock only while the state is `playing`, and reconciles with anchors. Actions are filtered by the session's action tokens. The wire schema is [ADR-0054](../adr/0054-media-and-library-wire-contract.md).


## Download routing and library import

The daemon routes the single `download` capability through `DownloadProvider1`
and exports `Downloads1` on that provider's daemon path (ADR-0067, AC-17).
Complete provider snapshots are validated before publication, including generation,
revision, duplicate identities, paging bounds and backend state. The shell receives
`DownloadPage`, never provider `JobPage`: output file URIs stay inside the daemon.
Its snapshot revision belongs to the daemon, so a later import result emits
`Daemon1.StateChanged` even when the provider job revision has not changed.
Cached rows remain readable when the backend is unavailable; commands are refused.
Pause, Resume, StartNow and Retry require the current generation and advertised
job action and return `Operation1`. Request identity prevents duplicate execution;
a mutation timeout is reported without replaying the command.

Completed jobs with mapped outputs are submitted to a ready library advertising
`LibraryImport1`, preferring the configured library and otherwise the first eligible
provider. Each candidate identifies the download provider, job and stable output;
requests contain at most `MAX_IMPORTS` candidates and at most 256 KiB of encoded
D-Bus body, including the repeated source identities. Before dispatch the daemon
checkpoints the selected library and a digest of the output identities/mappings in
`state.sqlite3` schema 5. Neither file URIs nor titles are checkpointed or logged.
Retries remain pinned to that library, including after a default change or daemon
restart, so replay uses the same library completion ledger. The library provides
the exactly-once effect; transport delivery itself can repeat after a timeout or
crash. A changed output set is re-evaluated with the same stable completion IDs.

The shell distinguishes `none` (not completed), `pending` (waiting for import),
`imported` with library entity references, `unsupported` (no media results or an
Unsupported reply), and `unavailable` (no mapped outputs). Pending attempts are
throttled to at most one pass per five seconds per download provider. Checkpoints
are pruned only against a complete snapshot with backend `available`, never an
unavailable or degraded snapshot. Import and snapshot work runs asynchronously;
SQLite work runs on blocking workers. Provider generations are checked before and
after import work, and imports are serialized across generation watchers.

Verification is in `runtime/download_tests.rs` (private buses, synthetic providers)
and the storage migration tests. It covers duplicates, restart after an ambiguous
completion, delayed library availability, shell revision changes, bounded batches,
URI exclusion, missing mappings, unsupported content, command guards and XML parity.
This does not qualify AC-17 on a real Transmission/KDE desktop or bind Downloads UI.
