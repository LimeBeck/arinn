# ADR-0075: Bounded provider-owned web shortcut site icons

- **Status:** Accepted
- **Date:** 2026-10-01
- **Deciders:** project owner explicitly approved site favicon downloads, ADR updates and PR preparation on 2026-10-01 at 19:20:21 UTC; technical bounds under delegated authority, task #224
- **Amends / Supersedes:** amends [ADR-0043](0043-web-shortcuts.md) (network icons for web shortcuts) and [ADR-0048](0048-browser-wire-contract.md) (separate optional provider icon API, no tab signature or extension permission change)
- **Amended by / Superseded by:** —
- **Specification:** §5.1, §5.5, §7.1, §9.7, §11, §13, §18

## Context

Web shortcuts currently omit their icons from the shell tile model. ADR-0043
forbids favicon requests; ADR-0048 carries no favicon in the tab contract.
ADR-0059 covers installed applications only. The owner explicitly requested site
favicon discovery and caching, together with the corrected Kinopoisk streaming
URL, and authorized this change to the prior decisions.

## Decision

The browser provider discovers site icons for saved web shortcuts, independently
of a running browser, through new optional interfaces described in the
[site-icon contract](../architecture/web-shortcut-icons.md). The daemon routes a
shortcut ID and current configuration context to a current provider; the shell
receives validated image bytes through the daemon, never a network URL or cache
path. The existing shortcut and browser-tab signatures remain unchanged.

Network access is restricted to public HTTPS origins, with validated and pinned
DNS addresses, revalidated redirects, no ambient credentials, five-second total
deadlines, two concurrent discoveries and explicit byte/image/request limits.
Inert HTML link metadata, apple-touch icons, a bounded web manifest and favicon
fallback supply candidates. Normalize bounded raster input to a small static
PNG. Cache only hashes and normalized images, with bounded disk usage and TTLs.
Missing or unsafe data keeps the monogram. Existing installed-app icons are
unchanged. No browser extension permissions or content scripts are added.

Correct the packaged streaming URL with a one-time, data-driven exact-match
migration that preserves customized and deleted shortcuts. Persist neither
fetched references nor browsing history in shortcut state. Ship no downloaded
third-party marks in the repository or package.

## Consequences

This deliberately permits provider-owned network access that ADR-0043 excluded.
Requests disclose the user's IP and the saved site's origin to that site; no
third-party favicon aggregator, cookie/profile access or path/query disclosure
is introduced. Fetching is optional presentation work, never a launch dependency.
Production implementation must enforce the full bounds and test graceful failure.

The owner merges this security/product change. Acceptance of the decision is
not evidence of implementation, clean CI or real KDE/TV qualification.

## Alternatives considered

- Shell-side downloads: rejected because they bypass provider ownership and
  expose navigation to remote image loading.
- Extension favicon permission: not required for saved shortcuts and would not
  discover a new shortcut while the browser is closed.
- Reusing installed-app local paths: rejected; it crosses a different trust and
  lifetime boundary and exposes provider cache paths to the shell.

## Verification

Protocol and private-bus fixtures cover typed optional interfaces and stale or
unauthorized contexts. Production tests must exercise network restrictions,
cache/image budgets, manifest/touch candidates, migration preservation and actual
model-to-image rendering with fallback. See the contract's explicit handoff.
No real desktop operator test is authorized by this decision.
