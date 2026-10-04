# Optional Sentry diagnostics

<!-- SPDX-FileCopyrightText: 2026 Arinn contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

Specification §13–14; R-01–R-05, R-09, R-10;
[ADR-0082](../adr/0082-opt-in-sentry.md); [task #301](https://github.com/LimeBeck/arinn/issues/301).

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

## Captured diagnostics

- `arinnd`: startup and typed operation errors; successful/failed operation
  duration breadcrumbs; startup, ready and shutdown; unwinding Rust panics with
  bounded stack traces.
- `arinn-shell`: Qt warnings, critical/fatal messages and native fatal signals;
  debug/info messages as breadcrumbs; application lifecycle, window visibility,
  activation, focus and input event kinds as breadcrumbs. No text, key code,
  pointer position, window identity or title is recorded.
- Component, release version, level, timestamp, bounded operation/error codes,
  stack instruction addresses and native module build identifiers. Rust stack
  symbols have source paths and local variables removed. Native symbol names
  are resolved on the server from separately managed debug artifacts.

Opt-in traces measure `daemon.startup`, `daemon.operation` and `shell.startup`.
Metrics count daemon operations/errors and shell diagnostic messages/errors, and
record daemon startup/operation and shell startup durations in milliseconds.
Names are fixed; attributes retain only the component. No provider, request,
window or browser identity is attached. All these measurements use the same
flag, with full sampling while development diagnostics are enabled.

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

`cargo test --locked -p arinnd telemetry --lib` tests defaults and adversarial
payloads. `sentry-isolated` in CTest runs `tests/integration/sentry.py` against a
loopback server and disposable native/Rust fixtures. It checks actual envelope
contents, traces/metrics, disabled/malformed settings, invalid DSNs, raw-envelope callback bypass, recursive crash fallback and cache privacy before replay;
fixtures have core dumps disabled. Build Rust examples before CTest as specified
in CONTRIBUTING.md. No request reaches the configured production Sentry project.

Transport queues are bounded by the SDKs; normal shutdown flushes for at most two
seconds. Loss of network or invalid telemetry setup must not prevent application
startup. Native in-process crash reporting is best effort; SIGKILL/OOM kills,
Rust aborting panics and a broken process may prevent delivery. Reports are useful
without symbol uploads, but native source-level symbolication requires matching
build debug artifacts. Automated upload, public consent UI, live Sentry ingestion
and desktop/TV qualification remain untested or pending; no roadmap gate closes.
