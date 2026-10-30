# Optional Sentry diagnostics

<!-- SPDX-FileCopyrightText: 2026 Arinn contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

Specification §13–14; R-01–R-05, R-09, R-10;
[ADR-0082](../adr/0082-opt-in-sentry.md), [ADR-0093](../adr/0093-structured-sentry-diagnostics.md);
tasks [#301](https://github.com/LimeBeck/arinn/issues/301) and
[#370](https://github.com/LimeBeck/arinn/issues/370).

## Configuration

The shell and daemon independently read
`$XDG_CONFIG_HOME/arinn/telemetry.toml`, falling back to
`$HOME/.config/arinn/telemetry.toml`. Absolute roots are required. This separate
file does not alter the main daemon config schema. Missing/invalid configuration,
a non-boolean flag, malformed DSN setting or an invalid explicit DSN disables
Sentry without stopping Arinn.

```toml
[telemetry]
enabled = false
# Optional override for a different project:
# dsn = "https://PUBLIC_KEY@HOST/PROJECT_ID"
```

Set `enabled = true` explicitly to enable development diagnostics. No local DSN
is needed: both binaries embed the owner-provided public ingestion endpoint from
`observability/sentry-default-dsn.txt`. This is an intentional public project key,
not an account/API authentication token. The endpoint alone never enables the SDK.

Endpoint precedence is `ARINN_SENTRY_DSN` → configuration `dsn` → embedded default.
An empty or invalid explicit override disables the endpoint rather than falling
back. Keep custom endpoints and account/API credentials out of source; only the
owner-approved public product endpoint is embedded. DSNs require HTTPS; literal loopback HTTP is allowed for
isolated tests. Passwords, query strings and fragments are refused. Do not place
account credentials or Sentry auth tokens here. Restart the shell and daemon
through the session target after changing the flag; existing processes do not
hot-reload it. Setting the flag to false and restarting stops collection and
transmission, including cached reports. This increment has no consent dialog.

The [publication checklist](https://github.com/LimeBeck/arinn/issues/74) requires
informed opt-in consent, a refusal path, review/withdrawal controls, immediate
revocation behavior and a privacy/retention notice before release. A development
flag is not evidence that those requirements are implemented.

For publication checklist #74, the owner confirmed a 90-day retention period on
2026-10-27. The public privacy notice must state this period, along with the
recipient, purpose, data categories and consent withdrawal controls. The
maintainer must ensure that the receiving Sentry project uses this retention
period. This documents the publication requirement; it does not implement the
consent UI or verify the remote project settings.

## Captured diagnostics

- `arinnd`:
  - startup errors and failed operations;
  - successful and failed operation breadcrumbs with their duration;
  - startup, ready and shutdown;
  - unwinding Rust panics with bounded stack traces.
- `arinn-shell`:
  - Qt warnings and critical/fatal messages as templates;
  - native fatal signals;
  - debug/info messages as breadcrumbs;
  - application lifecycle, window visibility, activation, focus and input event kinds as breadcrumbs.

  No key code, pointer position, window identity or title is recorded.
- Component, release (`arinn@<build id>`, ADR-0097), level, timestamp, bounded operation/error codes,
  stack instruction addresses and native module build identifiers. Rust stack
  symbols have source paths and local variables removed. Native symbol names
  are resolved on the server from separately managed debug artifacts.

Opt-in traces measure `daemon.startup`, one transaction per daemon operation named
by its kind, `shell.startup` and the shell flows below. Metrics count daemon operations/errors and shell
diagnostic messages/errors, and record daemon startup/operation and shell startup
durations in milliseconds. Names are fixed. No provider instance, request, window or
browser identity is attached. All these measurements use the same flag, with full
sampling while development diagnostics are enabled.

### Structured fields (ADR-0093)

Every field below is a fixed value written by Arinn's code or manifests. The
sanitizers drop any other value. The lists live in `crates/arinnd/src/telemetry.rs`
(`KINDS`, `SPAN_OPS`, `SPAN_NAMES`, `REASONS`) and `observability/native/qt_template.cpp`.

**Daemon tags, breadcrumb data and metric attributes:**

| Field | Values |
| --- | --- |
| `component` | `arinnd` |
| `kind` | `application.launch`, `application.activate`, `application.terminate`, `browser.activate_tab`, `browser.close_tab`, `browse.open`, `daemon.reload`, `provider.restart`, `download.pause`, `download.resume`, `download.start-now`, `download.retry`, `library.launch`, `library.restart`, `media.command:play`, `media.command:pause`, `media.command:play-pause`, `media.command:seek`, `media.command:next`, `media.command:previous`, `media.command:stop`, `media.command:select-track`, `media.command:disable-subtitles`, `media.command:thumbnail`, `media.present`, `media.release`, `platform.window.activate`, `platform.window.close`, `settings.set`, `shortcut.open` |
| `capability` | A protocol capability (`media`, `library`, `download`, `browser`, `application`, …) |
| `plugin` | Manifest plugin id matching `[a-z0-9][a-z0-9._-]{0,63}`; never the provider instance id |
| `outcome` | `succeeded`, `failed` |
| `error` | `NotFound`, `Unavailable`, `Unsupported`, `PermissionDenied`, `Timeout`, `InvalidArgument`, `Conflict`, `Internal`, `Cancelled` |
| `reason` | `provider_changed` (owner or generation changed during the operation), `platform_timeout` (no platform answer within 5 s, or the platform expired the request itself), `platform_channel_closed` (the platform dropped the request, or answered `Unavailable` after an adapter reset or disconnect); for managed playback also `platform_not_polling`, `platform_window_missing`, `platform_conflict`, `session_locked` and `platform_cancelled` ([managed playback diagnostics](managed-playback.md#diagnostics)) |
| `operation` | Lifecycle: `startup`, `ready`, `shutdown`, `seed-web-shortcuts`, `operation.complete` |

Metric attributes are `kind`, `operation`, `outcome` and `error` only. Operation counts and durations carry `kind`, `outcome` and `error`. Error counts carry `error` with either `kind` (an operation) or `operation` (a lifecycle failure such as `startup`).

**Daemon transactions.** A transaction is named by the operation kind, with the trace op `daemon.operation`. It has up to 16 child spans:

- `provider.call`: a daemon → provider D-Bus call.
- `platform.wait`: a wait for the platform adapter.

A span description is a kind, `media.open`, `browser.launch`, `browser.open_url` or `browser.show_window`. A library launch has the spans `provider.call library.launch` and `provider.call media.open`. A Present has `platform.wait media.present`. A URL open (`browse.open`, `shortcut.open`) names the chosen browser plugin and has `provider.call browser.launch` (when the browser was closed), `provider.call browser.open_url` and `platform.wait browser.show_window`. A span cut short by the operation deadline, or still running when the operation ends, has status `cancelled`.

**Daemon events.** A failed operation produces one event titled `<kind> failed: <error> (<reason>)`.

- Fingerprint: `["arinnd", kind, error, reason or "-"]`.
- Level: `error` for `Timeout`, `Unavailable` and `Internal`, otherwise `warning`.

The completion log line on stderr carries the same `kind` and `reason`.

**Shell Qt and QML messages.** Warnings, critical and fatal messages become events of type `QtWarning`, `QtCritical` or `QtFatal`:

- **`qt.location` tag.** The Arinn source name and line (`HomeView.qml:42`), from a `qrc:` QML prefix or the compile-time `.cpp`/`.h` context. It is never derived from `file:` URLs, absolute paths or C++ file names inside message text.
- **`qt.category` tag.** The Qt logging category, for example `qml` or `qt.qpa.wayland`.
- **Exception value.** The template: the message with non-ASCII text, quoted text, URLs, paths, addresses, file names and numbers replaced by `<text>`, `<str>`, `<url>`, `<path>`, `<file>` and `<n>`. Any `scheme:value` token is a URL, any dotted name is a file name, any token with a slash or backslash is a path whatever precedes it, and a token still holding a query or address separator becomes `<str>`; each of these replaces the whole whitespace-delimited token, brackets included. A final allowlist keeps only words, C++ scopes, placeholders and punctuation; any other token becomes `<str>`. Both privacy boundaries accept `qt.location` only as `<name>.<qml|js|mjs|cpp|h>[:<line>]`. It is at most 160 characters. A template that still holds a digit, quote, slash or query character becomes `Qt message`.
- **Fingerprint.** `["arinn-shell", type, location, template]`, rebuilt from the cleaned fields at both boundaries. The transport filter templates a Qt exception value again, so a raw or cached envelope cannot pass its own text off as a template.
- **Rate limit.** One event per template per 10 minutes, with at most 256 templates tracked. Repeats count in `arinn.shell.messages.suppressed`. Fatal messages are never suppressed.
- **Known noise.** `Icon is unavailable`, `Site icon is unavailable`, `Artwork is unavailable` and `QWaylandDataOffer` never become events. They count in `arinn.shell.messages.benign` and leave one `qt.noise` breadcrumb per template window.

**Shell flows.** A user-visible shell flow is one transaction with the trace op `shell.flow`, a status, an `outcome` tag and up to 8 child spans with the op `shell.step`. The lists live in `observability/native/flows.h`. The shell models record a flow with `apps/arinn-shell/flow_trace.h`, which does nothing unless the telemetry startup hook installs its sink, so builds and tests without telemetry are unaffected. Nothing else is recorded: no IDs, titles, URLs or paths.

| Flow | Starts | Ends | Steps |
| --- | --- | --- | --- |
| `home.load` | Home data becomes `loading` | the first other Home state | — |
| `downloads.read` | a Downloads read starts | the read is published; reported only when it fails or changes the state the screen shows, not on every 30 s poll | — |
| `browse.open` | an address or search open | its operation ends | `request`, `operation.wait` |
| `shortcut.open` | a web shortcut open | its operation ends, it is cancelled, or the daemon owner changes | `request`, `operation.wait` |
| `activity.return` | Return to the activity | the daemon hides Home, refuses, or 5 s pass (`Timeout`) | — |
| `playback.present` | `present()` for a managed player | the window check confirms the input layer, or the presentation fails or is cleared | `window.resolve`, `platform.present`, `window.check` |

The outcome is `ok`, `failed`, `updated` (Home found another daemon build) or a public error code (`NotFound`, `Unavailable`, `Unsupported`, `PermissionDenied`, `Timeout`, `InvalidArgument`, `Conflict`, `Internal`, `Cancelled`); any other value is reported as `failed`. The status follows the outcome (`ok`, `not_found`, `unavailable`, `unimplemented`, `permission_denied`, `deadline_exceeded`, `invalid_argument`, `aborted`, `internal_error`, `cancelled` or `unknown_error`). A step still open when its flow ends takes the flow's outcome. The SDK callback and the transport filter both keep only these names, statuses and outcomes.

Unquoted words that code writes into a message survive templating. §13 forbids such logging of user content, so the template holds no more than the local log.

Every event, transaction and metric is rebuilt from allowed fields before transport. Free-form messages
and exceptions are replaced by safe diagnostic descriptions. Users, hostnames,
HTTP requests, unrelated contexts, source filenames/absolute paths, source text, locals,
arbitrary tags/extras, unknown breadcrumbs, provider content, URLs and tab titles
are removed. Automatic sessions, profiles, logs, screenshots,
replay, attachments and minidumps are not enabled. The remote ingestion service
necessarily sees network connection metadata such as the source IP; the public
privacy notice must cover that as well as retention.

Breadcrumbs are bounded to 100, stack frames to 128 and exception chains to four.
Native SDK cache data is under `$XDG_CACHE_HOME/arinn/sentry-shell` (fallback
`$HOME/.cache/arinn/sentry-shell`) in a directory with mode 0700. Only sanitized
JSON diagnostics may be buffered. No SDK, transport, panic/crash handler or cache
is initialized when the flag is false, even with a valid DSN. The daemon's offline
`--check-plugins` mode never initializes telemetry.

## Package-matched debug files

[ADR-0116](../adr/0116-package-matched-sentry-debug-symbols.md) prepares the remaining
CI upload part of #370. It is proposed and owner-merged. CI collects split ELF
debug files for `arinnd` and `arinn-shell` with release optimization and
line-level debug information. Arch splits the staged binaries before stripping.
Debian collects the matching files from `dh_strip`'s debug root after stripping:
`debugedit` may rewrite the build IDs during that step. Collection selects each
file by the final staged binary's build ID and rejects missing, symlinked or
mismatched files. After extraction, each finished package's build IDs must match
those files.
Arch and Debian have separate manifests and seven-day symbol artifacts. The
product packages and GitHub release assets do not gain debug/source archives.

The manifest records `arinn@<build id>` and the file digests/build IDs. Sentry
matches native debug IDs, not the release name alone: no separate rebuild or
Sentry release creation can substitute for these exact files.

After all CI and package-install checks pass, `master` and version-tag pushes
upload to `limebeckdev / arinn`. The owner configured `SENTRY_AUTH_TOKEN`; the
release workflow passes it explicitly into reusable CI, and only the trusted
upload subprocess receives it. A missing token emits a notice and skips upload.
A configured upload/validation failure blocks publication. PRs, nightlies and
manual CI runs never upload, even when the repository has the token.

The helper verifies manifests, hashes, component names, line tables and workflow
commit before running a SHA-256-pinned Sentry CLI 3.8.2 in a private home. It
uploads only the four explicit ELF `.debug` files with `--no-sources` and waits
up to 300 seconds for processing. It never adds `--include-sources` or prints CLI
output/token-bearing diagnostics. Debug information contains build-time Arinn
and toolchain source locations/symbol names, not user-device data or local-variable
descriptions; C/C++ checkout prefixes are remapped to `/usr/src/arinn`. Runtime
consent, event filters and cache privacy remain unchanged.

`python -m unittest discover -s tests/ci -p test_debug_symbols.py` checks synthetic
C/Rust binaries and a recording CLI, without Sentry access or an Arinn build.
Actual package collection/verification is exercised by a manual CI run on the
branch. Live upload and source-level symbolication require the first trusted
build after owner merge and remain unverified by these tests.

## Build and verification

CMake downloads Sentry native 0.17.1 and toml++ 3.4.0 with fixed SHA-256 hashes.
`observability/native/harden_sdk.py` applies checked, idempotent source guards:
recursive inproc crashes that skip privacy callbacks are discarded; every
submitted envelope is rebuilt before the SDK queues or persists it. Unsupported
source anchors fail the build. The HTTP boundary filters again, including replay
of older cached envelopes. SDK gzip is explicitly disabled for this JSON filter.
Transaction/metric callbacks also remove SDK scope data before batching.
For offline builds, supply existing source directories with
`FETCHCONTENT_SOURCE_DIR_SENTRY_NATIVE` and `FETCHCONTENT_SOURCE_DIR_TOMLPLUSPLUS`.
The package links the native SDK statically and retains its and toml++'s license
notices, including MPack, jsmn and the portable stb_sprintf vendored notices.
`package_notices.py` extracts the complete upstream permission/copyright blocks
into the build directory; PKGBUILD installs them with the SDK/toml++ notices.
libcurl, libunwind and OpenSSL development files are required.

`cargo test --locked -p arinnd telemetry --lib` tests defaults, adversarial
payloads, structured fields, fingerprints, spans and metric attributes.
`sentry-configuration` also tests Qt message templates, locations, noise and the
rate limit. `sentry-isolated` in CTest runs `tests/integration/sentry.py` against a
loopback server and disposable native/Rust fixtures. It checks actual envelope
contents, traces/metrics, disabled/malformed settings, invalid DSNs, raw-envelope callback bypass, recursive crash fallback and cache privacy before replay;
fixtures have core dumps disabled. Build Rust examples before CTest as specified
in CONTRIBUTING.md. No request reaches the configured production Sentry project.

Transport queues are bounded by the SDKs; normal shutdown flushes for at most two
seconds. Loss of network or invalid telemetry setup must not prevent application
startup. Native in-process crash reporting is best effort; SIGKILL/OOM kills,
Rust aborting panics and a broken process may prevent delivery. Reports are useful
without symbol uploads, but native source-level symbolication requires matching
build debug artifacts. Automated upload is prepared in ADR-0116 but not live-qualified. Public consent UI,
live Sentry ingestion and desktop/TV qualification remain untested or pending;
no roadmap gate closes.
