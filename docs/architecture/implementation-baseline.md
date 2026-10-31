# Implementation baseline

Part of [ADR-0020](../adr/0020-dbus-contract-baseline.md) and [ADR-0022](../adr/0022-state-and-interaction-baseline.md). Chosen 2026-09-27 as a delegated technical decision. Numeric limits below are project defaults, not requirements of third-party specifications. Requirements summary: [specification §8–§11, §14, §17](../specifications/arinn-specification.md).

Implementation status: the Phase 1 application bindings are specified by [ADR-0035](../adr/0035-phase1-application-wire-contract.md) and [Plugin API XML](../plugin-api/README.md). Media and library wire bindings are [ADR-0054](../adr/0054-media-and-library-wire-contract.md); the daemon routes `media` and `library` ([ADR-0058](../adr/0058-media-routing.md), [ADR-0060](../adr/0060-library-routing.md)). See the [implementation record](phase-1-progress.md) for verification and remaining qualification.

## Stack and boundaries

- **Shell:** Qt 6 Quick/QML with a small C++ layer for QtDBus and typed models. The UI thread never makes blocking D-Bus calls. CMake, Qt Quick Test and Qt Test.
- **Core, CLI and first plugins:** a Rust workspace with Tokio, zbus, and serde for configuration files; `cargo test` and a shared contract harness. Dependency versions are pinned by the lockfile; no floating production dependencies.
- **Transport:** session D-Bus for shell/CLI ↔ daemon and daemon ↔ plugins. Public name `org.arinn.Daemon1`, root `/org/arinn/Daemon1`, interface names carry the major version suffix `1`. A provider has a unique bus owner and exports `/org/arinn/Provider1`; the core proxy is visible to clients at `/org/arinn/Daemon1/providers/p_<uuid_hex>`. The UUID is stored in the registry; opaque plugin IDs are never turned into object paths directly.
- Only the daemon is the application/provider integration API of the shell. Clients never call provider endpoints, MPRIS or KWin directly. The daemon-independent platform lifecycle guard observes lock/sleep state to hide the shell even during daemon failure; it supplies no application data or commands ([ADR-0041](../adr/0041-independent-session-lifecycle-guard.md)).
- The initial build target is the current stable Arch/EndeavourOS with KDE 6 on Wayland. Portability to other desktops is deferred.

Qt offers asynchronous D-Bus calls and zbus connects to the session bus; this is the reason for the bindings, not a test of Arinn: [QtDBus](https://doc.qt.io/qt-6/qdbusconnection.html), [zbus](https://docs.rs/zbus/latest/zbus/connection/struct.Builder.html).

## Identifiers and routing

`plugin_id` is the stable package ID from the manifest. `provider_id` is the stable ID of a configured plugin instance; by default it equals `plugin_id`. Additional instances are declared in daemon configuration. One provider may register several capabilities.

`EntityRef = (provider_id, capability, local_id)`, three separate strings. The core knows the capability but never parses `local_id`. Ephemeral entities also carry a `generation`; a stale generation returns `Conflict` and is never redirected to a new object with the same ID. Favorites store only stable launch targets declared by a provider or Arinn-owned web shortcuts, never temporary tab IDs ([ADR-0043](../adr/0043-web-shortcuts.md)).

A command on an entity always goes to its provider. A generic command (for example, open a URL without an entity) goes to the saved default provider of the capability; with exactly one available provider it is chosen automatically. With several and no default, the user chooses in Settings; there is no hidden product-specific priority. All providers can be aggregated into one list.

## Snapshots and reconnect

A revision is scoped to `(provider_id, capability)`. `generation` is a UUID issued by the daemon for every new registration, including re-registration after a daemon restart. A revision is a `uint64` that starts at 1 and grows strictly, only on changes within one generation. Identical snapshots need no new revision. Overflow requires a new registration and generation.

`StateChanged` carries `provider_id`, `capability`, `generation` and `revision`. A snapshot atomically carries the same generation/revision pair and typed entries. A consumer installs the signal match before reading the first snapshot, buffers notifications, accepts the snapshot and requests the next one if it has already seen a higher revision. Events may be coalesced; stale replies are dropped. A new owner or generation invalidates the whole old snapshot and pending operations, then a full reload follows.

The daemon may retain a complete imported snapshot when a validated first page has the same generation/revision. Backend metadata still refreshes; the optimization and signal coalescing are defined in [ADR-0122](../adr/0122-revision-aware-provider-polling.md).

Large lists are read in pages of at most 200 entries or 256 KiB of encoded body, whichever comes first. A page token is bound to an immutable generation/revision and expires after 30 s; `Conflict` means start over. The previous complete snapshot stays visible until the new one is complete; pages of different revisions are never mixed. Signal dispatch and snapshot publication are atomic with respect to the internal state commit.

D-Bus unique names and `NameOwnerChanged` track connection lifetime; generation and revisions are Arinn's own ([D-Bus specification](https://dbus.freedesktop.org/doc/dbus-specification.html)).

## Commands and errors

Typed capability methods take a `request_id` (a UUID from the caller) and return an operation object path immediately after validation and queueing. An operation has the states queued, running, succeeded, failed and cancelled, a typed error, start and end times, and result entity refs. The client reads the operation after subscribing, because the signal may arrive before the method reply. `InvalidArgument`, `NotFound`, `Unsupported` and `PermissionDenied` found during validation are returned immediately as D-Bus errors.

Errors: `NotFound`, `Unavailable`, `Unsupported`, `PermissionDenied`, `Timeout`, `InvalidArgument`, `Conflict`, `Internal`, `Cancelled`, in the namespace `org.arinn.Error1.*`. The operation state `cancelled` corresponds to `Cancelled`. Error texts are safe for the UI; stack traces stay in diagnostic logs without payload.

Default timeouts: IPC admission/read 3 s; controls, activate and seek 5 s; launch, open and createTab 30 s; handshake 10 s. The launch timeout bounds dispatch: an application launch whose application is still starting has no deadline and stays cancellable ([ADR-0110](../adr/0110-application-launch-lifetime-and-cancellation.md)). After a timeout the result of a command is unknown until reconciled with a snapshot. A successful application launch means a window of the application appeared, or, where windows cannot be observed, that the launch was dispatched or a running instance was found; it does not mean that the page or video is ready. The UI waits for the actual Activity before transitioning.

Cancellation is best effort and only for operations declared cancellable; it does not roll back external side effects. The daemon never retries mutating commands automatically. Within one daemon epoch, repeating `(caller unique owner, request_id)` returns the same operation; different content with the same ID is `Conflict`. Operations are kept for 5 minutes after completion; deduplication is not guaranteed across reconnects. On disconnect or generation change, pending operations end with `Unavailable` and late replies are ignored.

## Limits and validation

Message body up to 256 KiB; ID strings up to 256 bytes of UTF-8; ordinary text up to 8 KiB; URLs up to 16 KiB. Artwork and video are never inline in snapshots: a provider reports a typed asset reference and the daemon loads or serves the cached asset. No arbitrary HTML or JavaScript reaches QML. Optional metadata is a bounded `a{sv}` with scalar values and string arrays, at most 32 keys; core models stay typed.

The native bridge applies the same limit, pages large snapshots and never logs to stdout. Both directions validate before allocating by an untrusted length. Admission separates live work from retained history: at most 128 queued/running operations per caller and 32 per provider. Completed operations keep their full five-minute deduplication/read lifetime and share the unchanged global ceiling of 1024 retained objects; they do not occupy the caller's concurrency slots. A full global retention budget still rejects new work. Existing requests deduplicate before capacity checks. Overflow returns `Unavailable` with retryable metadata; the UI does not retry automatically.

Validation covers type, length, provider ownership and whether a command is allowed. Generic `open` accepts only `http` and `https` URLs; `javascript:`, `data:`, `file:` and arbitrary schemes are rejected. Other schemes are possible only through explicitly declared typed provider actions. `.desktop` entries are launched by the system desktop-entry mechanism without shell interpolation from QML.

Regular logs contain no URLs, tab titles, payloads, page content or secrets — only IDs, operation, outcome and a safe error code. Debug logging does not lift this rule.

## Storage

`$XDG_CONFIG_HOME/arinn/config.toml`: UI preferences, provider configuration and defaults, Home settings. Written through a temporary file in the same directory, fsync and atomic rename. The shell changes configuration through the daemon. `schema_version` and migration run before providers start.

`$XDG_STATE_HOME/arinn/state.sqlite3`: favorites and their order, web shortcuts ([ADR-0043](../adr/0043-web-shortcuts.md)), the stable provider-instance mapping, recent Arinn launch targets and diagnostic counters. One SQLite transaction per change and schema migrations; a failed migration keeps the previous database and turns on diagnostics/degraded state. SQLite was chosen for transactions; no ORM is required ([SQLite atomic commit](https://www.sqlite.org/atomiccommit.html)).

Recent launch targets: at most 100, kept for 30 days. Browser tab titles, URLs and history are not archived. User-configured web shortcut URLs are the only stored URLs and never appear in logs or diagnostics. Favorites survive removal of their provider and are marked unavailable; only the user deletes them. Secrets never go into the TOML file: a credential reference into the system secret storage is added when an integration needs it.

`$XDG_CACHE_HOME/arinn/`: artwork with an LRU budget of 512 MiB; clearing the cache loses no preferences. Provider databases belong to providers under `$XDG_DATA_HOME/arinn/providers/<digest>/` and `$XDG_STATE_HOME/arinn/providers/<digest>/`. The digest is the full 64-character lowercase hexadecimal SHA-256 of the exact UTF-8 bytes of the validated `provider_id`, without normalization; it is independent of the registry UUID and database ([ADR-0034](../adr/0034-provider-data-directory-identity.md), resolving OQ-06). The opaque ID is never used directly as a path component. Playback state is not duplicated in the core. Snapshots of live entities are kept in memory.

## Observability and tests

Logs are JSON on stderr, collected by journald; levels error, warn, info and debug, default info. Missing `provider_id`, `request_id` or `error` are `null`. `duration_ms` is a monotonic elapsed duration. Diagnostics contain runtime state, backend availability, generation, versions, capabilities, the last safe error with code and time, `restarts_since_daemon_start` and `cumulative_restart_count`. The last error stays until the next error or a manual clear and gets `recovered_at` after successful recovery.

CLI: human-readable text by default and `--json` for all read commands. Exit codes: 0 success, 1 runtime failure, 2 invalid usage, 3 daemon unavailable. `provider restart <id>` and `providers reload` are part of the baseline.

The contract harness starts any provider from its manifest and checks only the declared capabilities. Demo fixtures: empty/populated, delayed, invalid ID, crash, reconnect, revision race, unavailable backend. Rust unit and integration tests run with `cargo test`; QML interaction with Qt Quick Test; the IPC adapter with Qt Test.

Initial engineering budgets on the target device: p95 input-to-focus ≤ 50 ms with a warm UI at 60 Hz; p95 frame time ≤ 16.7 ms outside heavy initial loads; a slow provider does not affect focus. These are measurement targets, not results. Performance regressions are judged on one fixed environment; regular CI never presents synthetic FPS as TV acceptance.

CLI help uses the same command metadata as argument validation. `arinnctl --help`,
`-h`, `help`, and command/group help (for example `browser activate --help`)
print structured text to stdout with exit code 0 before connecting to D-Bus. Help
includes descriptions, ID sources, examples and exit codes; `--json` keeps help
as text. Invalid usage prints a concise stderr hint and exits with code 2. Omitting a
command still requests status. The process regressions use an invalid bus address
to verify that help and validation are independent of daemon availability.
