# ADR-0052: Browser windows are linked to platform windows by focus agreement

- **Status:** Accepted
- **Date:** 2026-09-29
- **Deciders:** delegated technical choice ([ADR-0037](0037-agent-autonomy-and-owner-gated-decisions.md)) within [ADR-0038](0038-browser-launch-ownership.md), [ADR-0048](0048-browser-wire-contract.md) and [ADR-0050](0050-window-per-browser-activity.md)
- **Amends / Supersedes:** —
- **Amended by / Superseded by:** —
- **Specification:** [§7.1](../specifications/arinn-specification.md#71-browser-chromechromium), [§12](../specifications/arinn-specification.md#12-platform-integration-kde-plasma-6--wayland)

## Context

`BrowserWindow.window_token` ([ADR-0048](0048-browser-wire-contract.md)) links a
browser window to its compositor window, so that the daemon can bring exactly
that window forward. [ADR-0038](0038-browser-launch-ownership.md) requires a
validated platform window token and forbids linking by titles, URLs or
product-specific core logic ([#101](https://github.com/LimeBeck/arinn/issues/101)).

The extension knows browser window IDs, but not compositor windows. The
platform inventory ([ADR-0039](0039-platform-window-contract.md)) has opaque
tokens, a desktop ID, `active` and `fullscreen`, but no titles. All Chrome
windows share one process, and Arinn's managed windows share one Wayland app ID
(`chrome-<extension id>-<profile>`), so none of these identifies a window.

Two facts do identify one:

- Chrome reports a window as `focused` only while it has the operating system's
  focus;
- the platform reports at most one `active` external window.

## Decision

- **Where.** `arinn-plugin-chrome` makes the link. It reads `PlatformWindows1`
  under its registration, which browser providers may already do. The core and
  the shell do not change.
- **Rule.** When exactly one browser window is focused and exactly one platform
  window is active, and this pair stays the same for 0.5 s, the two are linked.
  The wait covers the moment in which one snapshot has moved on and the other
  has not. A token belongs to at most one browser window: a new link takes the
  token away from an older one.
- **Validation and end.**
    - The provider publishes a token only while it is in the current platform
      snapshot.
    - A link ends when the browser window or the token disappears, when the
      platform generation changes, and when the provider re-registers.
    - Remapped windows get new tokens (ADR-0039) and are linked again the next
      time they have focus.
- **Refresh.** The provider re-evaluates on every browser change, on
  `WindowsChanged`, while a pair is settling, and at least once per second.
- **Opening a URL.** The daemon no longer shows the window that was focused
  before `OpenUrl`. It waits up to 2 s for the provider's next snapshot.
    - If that snapshot has a new window, the URL opened in it (ADR-0050). The
      daemon only hides Home: a new window has no link yet, and the browser
      raises it itself.
    - Otherwise the daemon activates the focused window's token, the window
      that already showed the origin.

## Consequences

- Links form without titles, URLs or process details, and without extension
  permissions.
- A window is linked only after it has had the focus once. Until then the
  daemon falls back to hiding Home, as before this decision.
- A focus change that the two sides report in the wrong order for longer than
  0.5 s could link the wrong pair. The next settled pair corrects it, because
  every focus change is observed again.
- Whether KWin gives a new Chrome window the focus while Home is shown depends
  on focus-stealing prevention. The real desktop check is an owner operator
  test.

## Alternatives considered

- **Titles, URLs or a marker in the title.** Forbidden by ADR-0038. A title
  marker would also need a content script.
- **Wayland app ID or desktop ID.** Shared by all managed windows, so it does
  not identify one.
- **Creation order.** A new browser window and a new platform window appear at
  about the same time, but other applications can open windows meanwhile, and
  Chrome restores windows at start-up. Focus agreement is observed repeatedly,
  so it corrects itself; a creation-order guess does not.
- **Asking the extension to focus each window to probe it.** It would steal
  focus from what the user is watching.

## Verification

- `plugins/chrome/src/association.rs` unit tests: a settled pair links; a
  changing pair restarts the wait; no link without exactly one focused and one
  active window; links end when either side disappears; a token moves to its
  new owner; links are cleared.
- The daemon test `urls_open_in_the_connected_browser_and_raise_the_window_that_shows_them`:
  an already open origin raises its own window, not the previously focused one
  (it fails without the change), and a new window only hides Home.
- The real KDE check of Home and return with linked windows is an owner
  operator test. It has not been run.
