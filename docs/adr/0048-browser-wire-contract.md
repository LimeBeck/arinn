# ADR-0048: Browser wire contract (Browser1 and BrowserProvider1)

- **Status:** Accepted
- **Date:** 2026-09-28
- **Deciders:** delegated technical choice ([ADR-0037](0037-agent-autonomy-and-owner-gated-decisions.md)) within the owner decisions of 2026-09-28: support Google Chrome and Chromium, an unpacked extension with a fixed ID for now, and the extension permissions `tabs` and `nativeMessaging` only
- **Amends / Supersedes:** amends [ADR-0011](0011-chrome-integration.md) (no favicon in version 1)
- **Amended by / Superseded by:** [ADR-0050](0050-window-per-browser-activity.md) — `OpenUrl` focuses or creates a fullscreen window; [ADR-0051](0051-address-and-search-entry.md) — the shell opens typed addresses and searches through a daemon method
- **Specification:** [§7.1](../specifications/arinn-specification.md#71-browser-chromechromium), [§9.1](../specifications/arinn-specification.md#91-capabilities), [§9.7](../specifications/arinn-specification.md#97-limits-and-validation), [§13](../specifications/arinn-specification.md#13-security)

## Context

Phase 3 needs a wire contract for the `browser` capability. It serves the
Chrome provider, the daemon routing ([#83](https://github.com/LimeBeck/arinn/issues/83))
and the shell. The application contract ([ADR-0035](0035-phase1-application-wire-contract.md))
already sets the patterns this contract follows:

- paged snapshots with a generation and revision;
- commands that return `Operation1` objects;
- one daemon-side endpoint per provider.

## Decision

**Types** (`arinn-protocol::browser`):

- `BrowserWindow = (id, window_token, focused)`, signature `(ssb)`.
  `window_token` is the validated PlatformWindows1 token of the matching
  compositor window, or empty when the provider has no validated link
  ([ADR-0038](0038-browser-launch-ownership.md), [ADR-0040](0040-kwin-inventory-transport.md)).
- `Tab = (id, window_id, title, url, active, audible)`, signature `(ssssbb)`.
  IDs are opaque. `url` is what the browser shows (any scheme, possibly empty),
  single-line and at most 16 KiB.
- `TabPage = (generation, revision, windows, tabs, next_token)`, signature
  `(sta(ssb)a(ssssbb)s)`. It has the same paging, lease and size limits as
  `ApplicationPage`.
- A complete snapshot has unique window and tab IDs. Every tab belongs to a
  listed window, each window has at most one active tab, and at most one window
  is focused.

**Provider side:** `org.arinn.BrowserProvider1` at `/org/arinn/Provider1`, served
only to the daemon's unique name.

- `GetTabs(token)`, `BackendState()` and `StateChanged(generation, revision)`.
  A change in backend state alone also publishes a new revision.
- `OpenUrl(generation, url)` opens a new tab. It accepts only `http` and
  `https` (`web::open_url`: the web shortcut syntax, but plain `http` for any
  host, spec §7.1).
- `ActivateTab(generation, id)` selects the tab and focuses its browser window.
  Compositor activation stays a PlatformWindows1 action of the daemon.
- `CloseTab(generation, id)`.
- Errors:
  - `Unavailable`: no browser connection;
  - `NotFound`: unknown tab;
  - `Conflict`: stale generation;
  - `InvalidArgument`: bad URL or ID;
  - `PermissionDenied`: any caller other than the daemon.

**Daemon side:** `org.arinn.Browser1` at the provider's
`/org/arinn/Daemon1/providers/p_<uuid_hex>` path, like `Application1`.

- `GetTabs(token)`.
- `ActivateTab(generation, id, request_id)` and `CloseTab(generation, id,
  request_id)` return `Operation1` objects. A successful `ActivateTab` also
  activates the compositor window and hides Home.
- There is no generic open-URL method for the shell. The shell opens only web
  shortcuts, through `WebShortcuts1.Open`, which routes to the default provider.

**No favicon in version 1.** Reading favicons in Manifest V3 needs a third
permission (`favicon`) or fetching arbitrary URLs. Both widen the §7.1
permission set. The shell uses the generic browser presentation.

**SDK:**

- The snapshot store is generic over a `Catalog`. `Snapshots` (applications)
  keeps its name and API; `TabSnapshots` serves browsers; `Store::touch`
  publishes unchanged entries under a new revision.
- `BrowserService` with a `BrowserHandler`, run with
  `run_browser`/`run_browser_on`. It shares the application service's
  registration loop and registers the `browser` capability.

Titles and URLs of tabs are private browsing data. They reach the daemon and the
shell, but never logs or diagnostics (§13).

## Consequences

- The Chrome provider (#82) implements only `BrowserHandler` and translates
  Native Messaging. The daemon (#83) gets a second capability endpoint in the
  same style as applications.
- The browser can be closed while the provider runs. Backend `unavailable`
  with an empty snapshot is the normal state, not a failure (AC-10).
- Associating browser windows with compositor windows is the provider's job.
  Until it is implemented, `window_token` is empty and the daemon activates
  the browser application's window instead.

## Alternatives considered

- **One aggregated tab list across all providers on the daemon root:** rejected.
  It would need provider-scoped IDs and a merged revision; the per-provider
  endpoint matches `Application1`.
- **Favicon as a bounded data URL:** deferred. It needs the `favicon` permission
  (owner-gated security posture) and a size policy.
- **A generic `OpenUrl` for the shell:** rejected. The shell opens only
  Arinn-owned shortcuts (ADR-0043), so arbitrary URLs never cross the shell
  boundary.

## Verification

- `arinn-protocol` unit tests: signatures, round trip, limits and snapshot
  consistency; `web::open_url` accepts public `http` and rejects other schemes.
- `arinn-plugin-sdk` store tests: tab paging and `touch`.
- `tests/browser_service.rs` on a private bus:
  - registration with `browser`;
  - revisions, including a backend-only change;
  - command checks before the handler: `NotFound`, `Conflict`,
    `InvalidArgument`, `Unavailable`;
  - daemon-only access;
  - `browser-provider1.xml` matches the live export.
- The daemon's `Browser1` export is verified against `browser1.xml` in #83.
