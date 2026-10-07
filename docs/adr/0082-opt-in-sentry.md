# ADR-0082: Optional Sentry diagnostics for the shell and daemon

<!-- SPDX-FileCopyrightText: 2026 Arinn contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

- **Status:** Proposed
- **Date:** 2026-10-04
- **Deciders:** owner-requested preparation in [#301](https://github.com/LimeBeck/arinn/issues/301); owner review and merge required
- **Amends / Supersedes:** —
- **Amended by / Superseded by:** [ADR-0092](0092-structured-sentry-diagnostics.md) (structured fields, proposed)
- **Specification:** §11, §13, §14

## Context

The owner requests Sentry in the Qt shell and Rust daemon, with broad technical
diagnostics but no personal data. The owner clarified that the consent UI is a
pre-release requirement; the present development increment needs a configuration
flag defaulting to false. The [publication checklist](https://github.com/LimeBeck/arinn/issues/74)
tracks informed consent, refusal and withdrawal without claiming them implemented.

The existing daemon config has a strict schema and is concurrently modified by
[#64](https://github.com/LimeBeck/arinn/issues/64). Both SDKs must decide whether to
initialize before relying on daemon IPC. Diagnostic setup must not depend on the
shell having a working daemon connection.

## Decision

Use the official Rust SDK in `arinnd` and checksum-pinned Sentry native in the
shell. Both read `[telemetry] enabled` from the independent
`$XDG_CONFIG_HOME/arinn/telemetry.toml`; missing, malformed or false means disabled.
Following the owner's additional request in [#303](https://github.com/LimeBeck/arinn/issues/303),
both binaries embed the supplied public ingestion DSN from one shared source.
Endpoint precedence is `ARINN_SENTRY_DSN`, configuration `dsn`, then the embedded
public project endpoint. Explicit empty/invalid overrides disable reporting and
do not fall back. A DSN cannot activate telemetry; `enabled=true` is still required.
Changes take effect after process restart. No account/API authentication token
or user-specific endpoint is embedded. This ADR remains Proposed for owner review.

Capture daemon startup/operation error codes, panic stack traces, lifecycle and
operation breadcrumbs; native crashes, Qt warnings/critical/fatal events and
bounded shell lifecycle/input/focus breadcrumbs. Under the same flag, collect
fixed startup/daemon-operation traces and numeric counts/duration metrics, with
full development sampling and only a component attribute. SDK auto sessions,
profiling, replay, screenshots, memory dumps and automatic log capture
remain disabled. Event filters reconstruct an allowlisted technical payload,
removing free-form exception/log messages, user/device identity, requests,
provider payloads, source paths, source context and local variables. No raw key
text, key codes, pointer positions or window titles are captured.

Every envelope is rebuilt before queue/cache and again at the native HTTP
boundary. A checked patch to the pinned SDK routes pre-queue envelopes through
that privacy filter and discards recursive inproc failures that skip callbacks.
Transaction and metric callbacks clear scope data before batching; unsupported
patch anchors fail configuration. This local hardening must be reviewed with
any SDK upgrade. Transport gzip is forced off for the JSON boundary.

The native SDK uses its in-process backend, which emits JSON crash events without
minidumps or an external crash daemon. Keep instruction addresses and module
build identifiers for symbolication. Use the system libunwind and libcurl; SDK
and toml++ sources are build dependencies pinned by version and SHA-256. The Rust
SDK uses the system TLS implementation; the rustls/platform-verifier transport
introduced a license outside the current allowlist during verification. The
license policy is unchanged.

## Consequences

Telemetry failure is non-fatal and transport work happens off the UI thread.
Normal shutdown has a bounded two-second flush. Native crash handling remains
best effort in a damaged process; Rust panic handling does not report SIGKILL,
OOM kills or `panic=abort`. Public release still requires consent and withdrawal
controls, a privacy/retention notice and symbol artifact operations. No phase or
operator acceptance is marked passed.

A separate configuration file avoids making SDK setup part of the D-Bus contract
or changing the current storage schema. Native reports may be buffered in a
private cache; a disabled launch never initializes the SDK and cannot upload old
reports. Runtime flag changes require restart; immediate consent withdrawal is a
future public-release gate.

## Alternatives considered

- A shared custom HTTP envelope client: rejected in favor of maintained SDK
  transports, queues and crash/panic support.
- Native minidump backends: excluded because memory dumps can contain private
  data even when JSON fields are filtered.
- Main daemon config now: would require changing actively claimed storage code
  and coupling early shell startup to that schema. Revisit consolidation with
  the consent UI if needed.

## Verification

Rust unit tests verify configuration defaults, DSN validation and sanitization.
An isolated loopback mock endpoint inspects actual Rust/native SDK envelopes,
including trace/metric payloads, raw envelope bypass, synthetic panics, native crashes, recursive privacy-hook failures and cache contents before replay. Disabled and malformed paths
must send no requests and create no telemetry cache. The full checks and
independent review are recorded on the pull request, not implied by this ADR.
Live Sentry ingestion, debug-symbol upload, desktop/TV operation and public consent
UI are not qualified by these tests.
