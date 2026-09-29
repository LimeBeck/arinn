# ADR-0050: One fullscreen browser window per browser activity

- **Status:** Accepted
- **Date:** 2026-09-29
- **Deciders:** project owner, explicit choice of the window-per-activity option with an application-launch fallback in the session of 2026-09-29 ([issue #99](https://github.com/LimeBeck/arinn/issues/99)); technical details under delegated authority ([ADR-0037](0037-agent-autonomy-and-owner-gated-decisions.md))
- **Amends / Supersedes:** amends [ADR-0048](0048-browser-wire-contract.md) (`OpenUrl` semantics only; the wire contract is unchanged)
- **Amended by / Superseded by:** [ADR-0051](0051-address-and-search-entry.md) — the generic browser entry becomes the address and search screen
- **Specification:** [§7.1](../specifications/arinn-specification.md#71-browser-chromechromium)

## Context

The browser is about 70% of usage time (spec §2). A regular Chrome window on a
TV shows a tab strip, an address bar and small controls that do not work from
3 metres. Arinn must not build its own browser engine (spec §3.2, §16), and
[ADR-0011](0011-chrome-integration.md) limits the extension to `tabs` and
`nativeMessaging`. [ADR-0048](0048-browser-wire-contract.md) opens a URL as a new
tab in the focused window, so every web shortcut launch adds a tab.

Three options were discussed with the owner:

1. embed a browser engine (QtWebEngine or CEF) in the shell;
2. show the tabs of an ordinary Chrome window;
3. show every browser activity in its own fullscreen Chrome window and let Arinn
   be the tab switcher.

An isolated check on 2026-09-29 (nested `kwin_wayland --virtual` on a private
session bus, Chrome 153, 1920×1080, a test extension with the `tabs` permission
only) showed that option 3 works:

- `chrome.windows.create` and `chrome.windows.update({state: "fullscreen"})`
  produce separate KWin windows at 0,0 1920×1080;
- the screenshot shows only page content, with no tab strip, toolbar or frame;
- a tab created in an occupied window can be moved into a new fullscreen window
  with `chrome.windows.create({tabId})`;
- focusing a window through the extension makes it the active KWin window.

The check did not cover real `target=_blank` clicks, `window.open` popups, the
user leaving fullscreen, DRM services or the real desktop.

## Decision

Arinn presents every browser activity as its own fullscreen browser window.

- **Opening.** `OpenUrl` focuses an existing managed window whose active tab has
  the same origin (scheme, host and port) as the URL. Otherwise it creates a new
  window with the URL and makes it fullscreen. Web shortcuts therefore never
  create duplicates of an open service.
- **One activity per window.** A tab that appears in a managed window that
  already has a tab is moved into its own fullscreen window.
- **Fullscreen is kept.** A managed window that leaves fullscreen is returned to
  fullscreen by the extension.
- **Switching.** Arinn lists these windows and their tabs through the existing
  `Browser1` snapshot and switches between them with `ActivateTab`; closing a tab
  closes its window. The browser's own tab strip is not part of the TV
  experience.
- **Ordinary windows.** A window the user opens as a regular browser window (the
  generic browser entry, for free browsing) is not managed and keeps the normal
  browser UI.
- **Scope.** The behaviour lives in the extension and uses only the `tabs` and
  `nativeMessaging` permissions. The `Browser1`/`BrowserProvider1` wire contract
  and its types do not change.
- **Fallback.** If this model does not hold on a real KDE desktop (operator
  test), Arinn falls back to launching Chrome as an ordinary application through
  the desktop ApplicationProvider ([ADR-0038](0038-browser-launch-ownership.md))
  and records that in a new ADR.

Linking each browser window to its compositor window
(`BrowserWindow.window_token`) remains open and is tracked in
[issue #101](https://github.com/LimeBeck/arinn/issues/101).

## Consequences

- Home, Back and Continue work per browser activity at window level, like games
  and applications.
- The extension must track which windows it manages and react to window and tab
  events; its tests cover each rule.
- With one tab per window, the browser window and the activity correspond one to
  one, which simplifies the link to a compositor window.
- Navigation inside a page still depends on the air mouse or a keyboard (spec
  §3.1); this decision does not change that.
- Many open services mean many browser windows; memory use is similar to the
  same number of tabs.

## Alternatives considered

- **Embedded browser engine.** Rejected: spec §3.2 and §16 forbid an own browser engine;
  QtWebEngine has no Widevine by default, which rules out most streaming
  services; the user's profile, logins and extensions would be lost; Arinn would
  own engine security updates.
- **Tabs of an ordinary browser window.** Rejected as the TV presentation: the
  browser UI is visible and one window holds many activities. It remains the
  fallback.
- **Chrome `--app` windows launched per URL.** Not chosen: every launch goes
  through the ApplicationProvider and a new process invocation, and links opened
  from such windows still need handling; the extension already has the window
  API.

## Verification

- Node tests of the extension with a mocked `chrome` API
  ([issue #100](https://github.com/LimeBeck/arinn/issues/100)).
- An isolated nested-KWin check of the implemented extension.
- Owner operator test on the real desktop with streaming services (AC-03,
  AC-04, AC-06). Accepting this ADR does not mean these checks have passed.
