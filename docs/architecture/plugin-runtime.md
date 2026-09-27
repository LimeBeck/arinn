# Plugin runtime: discovery, supervision and the Chrome bridge

Part of [ADR-0021](../adr/0021-plugin-runtime-ownership.md), 2026-09-27. Status: chosen for implementation; the production runtime is not implemented yet (Phase 0 has a single hard-wired demo provider).

## Manifest and discovery

A manifest is TOML with the required fields `id`, `name`, `version`, `api_version = 1`, `executable` and `capabilities`, and optional `args` (array of strings). After installation `executable` is an absolute path to a regular executable file. Neither `executable` nor `args` is a shell expression. Capabilities: `application`, `browser`, `media`, `library`, `action` and `download` ([ADR-0025](../adr/0025-downloads-mvp.md)); their API major version is 1. An unknown capability is never registered silently: the manifest gets `Unsupported` in diagnostics.

Discovery reads `*.toml` directly in `$XDG_DATA_HOME/arinn/plugins`, then in `arinn/plugins` of each `$XDG_DATA_DIRS` entry in order. XDG defaults apply as specified. A user manifest overrides a system manifest with the same ID; an invalid user manifest disables the provider with an explicit error and no surprising fallback. Duplicate IDs at the same level are `Conflict`; no file is picked at random. The source path is shown in diagnostics.

The registry is re-read at startup and on `arinnctl providers reload`; file watching and timed hot reload are not required for the MVP. Removing a manifest and reloading stops the provider unit and removes its capabilities; user applications it launched keep running. Changing `executable`, the API version or capabilities requires a provider restart; changing display metadata updates the registry without a restart. Favorites stay, marked unavailable.

## Units and ownership

**Daemon:** a regular `arinnd.service` bound to the graphical session, without user lingering. The shell is started by the session autostart launcher after Plasma login and connects to the same daemon. Autostart must not create a second shell or daemon; packaging tests graphical session shutdown.

**Provider:** a systemd **transient user service** created by the daemon through the systemd D-Bus API. The unit name is derived from the registry UUID; `ExecStart` is the validated binary plus argv, never a shell string. Only systemd restarts the process; the daemon controls handshake and availability.

Defaults: `Type=exec`, `Restart=on-failure`, `RestartSec=2s`, `StartLimitIntervalSec=60s`, `StartLimitBurst=5`, `TimeoutStopSec=5s`. After the limit is exhausted the provider is failed, the error is in diagnostics and a manual restart is needed; there is no infinite loop. Only an explicit request clears the failed/start-limit state. A handshake timeout stops the hung plugin and counts against the same restart budget.

Provider runtime states: discovered → starting → ready; on crash restarting or failed; on explicit stop stopped, without auto-restart; an incompatible manifest or API gives incompatible and no working capability. Backend state is separate: available, unavailable or degraded. A closed Chrome or no MPRIS player is not a plugin crash.

Running external applications are not in the plugin service cgroup: a desktop application is launched by the session application launcher in its own application unit or scope. An integration test must confirm that restarting or removing the desktop plugin does not kill Chrome or Steam. The launcher parses `.desktop` files, `DBusActivatable` and field codes according to the [Desktop Entry Specification](https://specifications.freedesktop.org/desktop-entry/latest-single/).

systemd documents the transient service settings ([transient settings](https://systemd.io/TRANSIENT-SETTINGS/)); the defaults above are Arinn's choice. Restart semantics are checked against the installed `systemd.service(5)` and `systemd.unit(5)` during implementation.

## Handshake and recovery

1. The registry validates the manifest and creates a unit with the `provider_id` and a one-time registration token in runtime credentials or a file (mode 0600, never logged).
2. The plugin connects to the session bus and exports its provider object.
3. The plugin calls `Register(plugin_id, provider_id, api_major, api_minor, version, capabilities, object_path, token)`. The daemon takes the sender from the D-Bus envelope and checks the UID, the launch token and that the PID belongs to the expected unit/cgroup. The token is not a sandbox.
4. The daemon accepts only capabilities listed in the manifest and returns `daemon_epoch`, `generation` and negotiated limits. The provider becomes available only after successful registration and its first valid snapshot.
5. `NameOwnerChanged` invalidates the provider. After a daemon restart, providers notice the owner change, receive a new registration challenge for the existing unit and register again with a new generation. The registry never starts a duplicate unit.

Token replay and a second active owner of one provider give `PermissionDenied` or `Conflict`. API v1 supports only major version 1; minor additions never change existing required fields or signatures. Unknown optional metadata keys are ignored; unknown enum values are shown as unsupported, never as success. Wire XML is fixed per capability; a major incompatibility is reported before a working provider starts.

## Chrome: two processes instead of two supervisors of one process

`Chrome MV3 extension ↔ Native Messaging bridge ↔ arinn-plugin-chrome ↔ arinnd`

Chrome starts the short-lived `arinn-chrome-host` on `connectNative`. The host only validates, frames and forwards messages; the provider stays a long-lived systemd service. Host ↔ provider is a separate typed D-Bus interface on the session bus; the plugin-specific transport never reaches the core. After disconnect the host exits; the Chrome plugin keeps running with its backend unavailable or with the remaining browser connections.

Chrome owns the lifetime of the native host and talks over stdin/stdout; `connectNative` creates a persistent port ([Native Messaging](https://developer.chrome.com/docs/extensions/develop/concepts/native-messaging)). So the host is not another instance of the systemd provider.

Extension permissions are `tabs` and `nativeMessaging`; no content scripts and no `host_permissions` for reading pages. The native manifest's `allowed_origins` contains the exact extension ID. Incognito is not connected by default. Each bridge connection gets a UUID; tabs get an opaque local ID composed by the plugin from the connection identity and the browser ID. The core never parses it. Several browser profiles may produce several connections without creating Arinn profiles.

Extension reconnect: 1, 2, 4, 8, 16, 30 s with jitter, capped at 30 s; after a successful connect a full snapshot, then events. This restores the connection; it never repeats a user's mutating command. Old tab IDs are invalid after a browser connection is lost.

If the browser is closed, launch goes through the selected ApplicationProvider (see [OQ-03](../specifications/arinn-specification.md#21-open-questions)); open URL waits for a browser connection within the overall launch deadline. With several connections, the default or last foreground connection is used, if it still exists. Activating a tab inside the browser does not replace compositor activation of the window, which is requested through PlatformService.

## Desktop and media edge cases

The desktop provider reports installed entries and running instances. Window identity comes from PlatformService. Closing an application first requests a graceful close of its windows; if that is impossible, `Unsupported` — never a kill by process name. Force kill is not a regular MVP terminate. Several windows are separate instances; activate picks the most recently used available window.

Media session selection: a session explicitly chosen by the user; then a session explicitly linked to the foreground Activity; then the playing session with the most recent observed state change; then the most recent paused one. Never by product name. Position, duration and absolute seek are signed int64 microseconds; unknown duration is −1; position is non-negative. The provider publishes a position anchor on state change or seek and at most once per second while playing; the shell interpolates locally on a monotonic clock and reconciles with anchors. Actions are filtered by capabilities.
