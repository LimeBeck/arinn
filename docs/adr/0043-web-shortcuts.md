# ADR-0043: Arinn-owned web shortcuts for online-service tiles

- **Status:** Accepted
- **Date:** 2026-09-28
- **Deciders:** project owner, explicit choice of option A for OQ-02 in [issue #53](https://github.com/LimeBeck/arinn/issues/53); technical details under delegated authority ([ADR-0037](0037-agent-autonomy-and-owner-gated-decisions.md))
- **Amends / Supersedes:** amends [ADR-0022](0022-state-and-interaction-baseline.md) (Arinn-owned state), [ADR-0038](0038-browser-launch-ownership.md) (opens its deferred OQ-02 point only)
- **Amended by / Superseded by:** —
- **Specification:** [§5.1](../specifications/arinn-specification.md#51-home), [§7.1](../specifications/arinn-specification.md#71-browser-chromechromium), [§11](../specifications/arinn-specification.md#11-state-ownership-and-persistence)

## Context

The specification requires streaming services such as Kinopoisk, IVI and YouTube
to appear as separate tiles; the user never goes Browser → Bookmark → service
(§5.1, AC-03). Favorites may store only stable launch targets declared by a
provider, and Arinn does not archive URLs, but no provider declares these
services. The design handoff offers "Add application — any site as an app".
OQ-02 asked where these tiles come from; the owner chose option A on 2026-09-28.

## Decision

Arinn owns **web shortcuts**: user-visible entries with a title, one `https` URL
and an optional icon reference. They are Arinn state, not provider state.

- **Storage.** Web shortcuts live in `state.sqlite3` with a schema migration. A
  shortcut has a stable daemon-generated ID, so Favorites can pin it like any
  other stable launch target; pinning, order and unavailability follow the
  existing Favorites rules.
- **Validation.** Only absolute `https` URLs without user information are
  accepted; length limits follow the daemon's string limits. The daemon rejects
  everything else with `InvalidArgument`.
- **Opening.** Activating a shortcut is a daemon operation that calls the generic
  `open` of the default BrowserProvider with the stored URL. Browser launch
  follows [ADR-0038](0038-browser-launch-ownership.md): a closed browser is
  launched by the desktop ApplicationProvider. With no available BrowserProvider
  the shortcut is shown as unavailable; it is never opened by the shell or by
  `xdg-open`.
- **Defaults.** A default set is shipped as packaging data under
  `/usr/share/arinn/` and seeded into state once, on first run of a state schema
  that has no shortcuts yet. A shortcut the user removed is not re-added by an
  upgrade. Service names and URLs never appear in core code or QML business logic
  (R-01).
- **Management.** The user adds, edits, removes and pins shortcuts from the TV
  through the shell, which calls the daemon API. The daemon API for this is a
  separate `contract-change` task.
- **Icons.** Default shortcuts reference packaged icons. Arinn does not fetch
  favicons or any network resource for shortcuts; a shortcut without an icon uses
  the generic tile presentation.
- **Privacy.** Shortcut URLs are user configuration, not browsing history. They
  are the only URLs Arinn stores. They MUST NOT appear in logs, diagnostics,
  `arinnctl` diagnostic output or crash reports; browser tab titles, URLs and
  history remain unarchived.

## Consequences

AC-03 becomes reachable without a new capability and without product names in
the core. Arinn gains a small daemon-owned entity type, a state migration and a
daemon API extension, which must go through contract review. The "URLs are not
archived" rule gains one explicit exception for user-created shortcuts.

Shortcuts open as ordinary browser tabs, so tab-level return (AC-06) and the
browser tab list work unchanged. Shortcuts depend on an available BrowserProvider;
without one they are unavailable, not hidden.

No implementation exists yet; accepting this ADR closes no gate.

## Alternatives considered

- **Browser-installed web apps as desktop applications** (option B). Needs no
  code, because the desktop provider already discovers their `.desktop` files,
  but they must be set up outside Arinn, open in a separate app window outside
  the browser tab model and are browser-specific. It remains usable as a manual
  workaround.
- **A separate web-shortcut provider plugin** (option C). Keeps the core
  unchanged but conflicts with browser launch ownership (ADR-0038), needs
  cross-provider launching and is configured only through files, not from the TV.

## Verification

- Daemon tests: URL validation (non-`https`, user information, oversized input),
  migration, one-time seeding that does not re-add removed defaults, Favorites
  pinning and unavailable state without a BrowserProvider.
- Private-bus integration test: activating a shortcut calls `open` on a fixture
  BrowserProvider with the stored URL.
- Log and diagnostics review: no shortcut URL is emitted.
- AC-03 operator test in Phase 3.

These tests are pending.
