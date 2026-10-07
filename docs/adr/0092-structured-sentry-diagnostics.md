# ADR-0092: Structured, allowlisted Sentry diagnostics

<!-- SPDX-FileCopyrightText: 2026 Arinn contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

- **Status:** Proposed
- **Date:** 2026-10-07
- **Deciders:** owner-requested in [#370](https://github.com/LimeBeck/arinn/issues/370); owner review of the field list and merge required (privacy posture)
- **Amends / Supersedes:** [ADR-0082](0082-opt-in-sentry.md)
- **Amended by / Superseded by:** —
- **Specification:** §13, §14.1, §14.4

## Context

ADR-0082 rebuilds every Sentry payload from a minimal allowlist. The owner
reported on 2026-10-06 that the result cannot be diagnosed
([#370](https://github.com/LimeBeck/arinn/issues/370)):

- Every daemon failure becomes the same "safe diagnostic description".
- Traces are an undifferentiated `daemon.operation`.
- Every Qt warning is a generic `QtWarning` without a source.
- Hundreds of `QML Image: Icon is unavailable` warnings per session hide the real errors.

The playback failures of [#369](https://github.com/LimeBeck/arinn/issues/369)
(Present timeout, session-card fallback) are therefore invisible in Sentry.

Specification §13 still applies to every field:

- No titles, URLs, paths, file names, tab or window identity.
- No provider payloads and no user content.

What is missing is fixed, non-personal structure. Arinn's own operation names, capabilities and plugin ids are identifiers chosen by the code and manifests. They are not user data.

## Decision

Arinn widens the ADR-0082 allowlist with the exact fields below and nothing
else. A sanitizer rebuilds every value from these lists before the SDK queues
it, and again at the transport boundary. Any other value is dropped.
ADR-0082 otherwise stays in force: the flag, endpoint, consent requirement, cache and crash rules are unchanged.

### Daemon (`arinnd`)

Each admitted operation carries a fixed label. The completion log line gains
the same `kind` and `reason` fields (§14.1).

| Field | Values |
| --- | --- |
| `kind` | One of the operation kinds in `telemetry::KINDS`, for example `library.launch`, `media.present`, `media.release`, `media.command:<action>`, `download.<action>`, `application.<action>`, `browser.activate_tab`, `platform.window.<action>`, `settings.set`, `browse.open`, `shortcut.open`, `daemon.reload`, `provider.restart`. |
| `capability` | A capability from the protocol list (`CAPABILITIES`). |
| `plugin` | The provider's manifest plugin id (`mpv`, `chrome`, …), only if it matches `[a-z0-9][a-z0-9._-]{0,63}`. Never the provider instance id, which a user may name. |
| `outcome` | `succeeded` or `failed`. |
| `error` | A public error code (`Timeout`, `Unavailable`, …). |
| `reason` | A normalized failure reason in `telemetry::REASONS`: `provider_changed`, `platform_timeout` or `platform_channel_closed`. |
| `operation` | Lifecycle literals, as in ADR-0082. |

These fields are used as follows:

- **Events.** One event per failed operation, titled `<kind> failed: <error> (<reason>)`.
  - The fingerprint is `["arinnd", kind, error, reason or "-"]`, so one failure is one issue.
  - The level is `error` for `Timeout`, `Unavailable` and `Internal`, and `warning` for caller-side codes.
  - A supplied fingerprint is never trusted. It is rebuilt from the clean tags.
- **Traces.** One transaction per operation, named by its kind, with the trace op `daemon.operation`. Child spans are limited to 16 per operation.
  - Each span has op `provider.call` (daemon → provider D-Bus call) or `platform.wait` (wait for the platform adapter).
  - Its description is a kind or a name in `telemetry::SPAN_NAMES` (`media.open`, `browser.launch`, `browser.open_url`, `browser.show_window`).
  - A step still running when the operation ends (for example on provider loss) is reported as cut short, not dropped.
  - Each span carries its status and timestamps; a step cut short by a deadline is `cancelled`.
- **Metrics.** `arinn.daemon.operations`, `arinn.daemon.operation.duration` and `arinn.daemon.errors` gain the attributes `kind`, `outcome` and `error`, within the lists above.
- **Breadcrumbs.** `operation.complete` breadcrumbs gain `kind`, `outcome`, `error` and `reason`.

### Shell (`arinn-shell`)

The Qt message handler reduces each warning, critical or fatal message to a
template:

- **`qt.location`.** The Arinn source file name and line, such as `HomeView.qml:42`.
  - It comes from the QML `qrc:/…:<line>:` prefix or from the message context.
  - It is kept only for a `qrc:` resource, or a `.cpp`/`.h` source taken from the compile-time message context (never from message text), whose name matches `[A-Za-z_][A-Za-z0-9_]{0,63}\.(qml|js|mjs|cpp|h)`.
  - `file:` URLs and absolute paths never produce a location.
- **`qt.category`.** The Qt logging category (`[A-Za-z0-9_.-]`, up to 64 characters), except `default`.
- **Exception value: the template.** The message after these replacements, in order:
  1. any token holding non-ASCII or control text becomes `<text>` as a whole;
  2. quoted text becomes `<str>`: for each quote class (`"`, `'`, backtick, typographic quotes) everything from the first to the last quote, or to the end when a quote is left open, so embedded quotes leave nothing between them;
  3. any `scheme:value` token, hierarchical or opaque and with a scheme of any length (`urn:`, `mailto:`, `x:`), becomes `<url>`; a C++ scope such as `QQuickItem::stackAfter` is kept;
  4. any token containing a slash or backslash becomes `<path>`, whatever character precedes it;
  5. addresses become `<str>`;
  6. any dotted name (`Title.desktop`, `host.example`, `qt.qpa.wayland`) becomes `<file>`, whatever the length of its parts; a sentence-ending dot is kept;
  7. any remaining token containing `\ / ? & # % = @` becomes `<str>` as a whole, never split into its parts;
  8. numbers become `<n>`;
  9. finally, only tokens made of words, C++ scopes (`Name::member`), the placeholders above and punctuation are kept; every other token, such as a leading-dot name, becomes `<str>`. The rules above name what to remove; this pass decides what may stay.

  Rules 3 to 7 always replace the whole whitespace-delimited token, brackets and placeholders included, so no part of a path, URL or name survives next to its placeholder. The result is truncated to 160 characters. A template that still contains a digit, quote, slash, `@`, `?`, `&`, `#`, `%`, `=` or a non-printable character is replaced by `Qt message`. The crash-safe SDK callback and the transport filter repeat this check, and check `qt.location` against its full grammar `<name>.<qml|js|mjs|cpp|h>[:<line>]` (or the fixed `unknown.location`), so no other file name passes either boundary.
- **Fingerprint.** `["arinn-shell", <type>, <location>, <template>]`, rebuilt at both boundaries from the cleaned type, location and template, never copied from the payload. The transport filter also runs the template rules again on a Qt exception value, because a raw or cached envelope may claim to be a Qt message without having passed the handler.
- **Rate limit.** Each template is reported at most once per 10 minutes of monotonic time. At most 256 templates are tracked. Repeats increment `arinn.shell.messages.suppressed`. Fatal messages are always reported.
- **Known noise.** `Icon is unavailable`, `Site icon is unavailable`, `Artwork is unavailable` and `QWaylandDataOffer` are never events. They increment `arinn.shell.messages.benign` and leave one `qt.noise` breadcrumb per template window.

### Changing the lists

These categories and their checks are fixed by this ADR. A technical change may add new fixed literals to an existing list: a kind, span name, reason or noise pattern written in Arinn's code. It must update `docs/architecture/telemetry.md` and the sanitizer tests in the same pull request. A new category, or any value derived from user, provider or platform data, needs a new ADR reviewed by the owner.

## Consequences

Sentry groups and explains failures by component, operation kind, error code
and reason. Durations show where an operation waited. The noise from missing
icons no longer hides real errors.

**Residual risk.** The template is still the text a component wrote:

- Plain, unquoted ASCII words in a message survive. An example is a title that code logs without quotes.
- §13 already forbids Arinn code from logging user content.
- Qt's own messages quote values or carry them as URLs.

The shell therefore sends no more than its local log already holds, minus URLs, paths, file names, numbers and quoted text. Code review must keep §13 for every new log line.

**Pending work.**

- #369 adds the platform-side reasons (no matching window, launch id mismatch, stale media channel) under the rule above.
- Shell-side flow transactions (Home load, Downloads read, browser open/return) and the input-layer span follow in later work.
- Using the build id of [#357](https://github.com/LimeBeck/arinn/issues/357) as the Sentry `release`, and uploading debug files from CI, also follow later; the release workflow is owner-merged.

## Alternatives considered

- **Keep ADR-0082 unchanged.** Rejected: the owner cannot diagnose failures.
- **Send raw messages with server-side scrubbing.** Rejected: user data would leave the device before it is scrubbed.
- **A vocabulary allowlist for message words.** Rejected: Qt and QML message texts are open-ended, so most diagnostics would be lost. Fixed placeholders keep the structure and remove the variable parts.
- **Distributed traces across D-Bus.** Deferred: this needs a contract change to propagate trace ids. Daemon-side spans cover the provider and platform waits now.

## Verification

- `cargo test --locked -p arinnd telemetry --lib`:
  - allowlisted fields pass;
  - private plugin labels, kinds, reasons and tags are removed;
  - identical failures share a fingerprint;
  - spans and metric attributes are allowlisted.
- The `sentry-configuration` CTest (`config_test.cpp`):
  - QML templates and locations are built as specified;
  - titles, URLs, paths, file names, `file://` strings and non-ASCII text are removed;
  - known noise is benign;
  - the rate limit and the crash-safe checks hold.
- The `sentry-isolated` CTest (`tests/integration/sentry.py`) inspects real envelopes on a loopback endpoint:
  - a `library.launch` transaction with named `provider.call` spans;
  - its grouped failure event without the private plugin label;
  - one shell event for two identical QML warnings, with the location and template;
  - no event for an icon warning.

Live Sentry ingestion and desktop/HTPC behaviour are not qualified by these tests.
