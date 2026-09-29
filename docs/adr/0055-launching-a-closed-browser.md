# ADR-0055: Launching a closed browser before opening a URL

- **Status:** Accepted
- **Date:** 2026-09-29
- **Deciders:** delegated technical choice ([ADR-0037](0037-agent-autonomy-and-owner-gated-decisions.md)) within [ADR-0038](0038-browser-launch-ownership.md) and [ADR-0043](0043-web-shortcuts.md)
- **Amends / Supersedes:** —
- **Amended by / Superseded by:** —
- **Specification:** [§7.1](../specifications/arinn-specification.md#71-browser-chromechromium), [§9.5](../specifications/arinn-specification.md#95-operations), [§11](../specifications/arinn-specification.md#11-state-ownership-and-persistence)

## Context

[ADR-0038](0038-browser-launch-ownership.md) says a URL opened with a closed
browser is handled in three steps:

1. the desktop ApplicationProvider launches the browser application;
2. the daemon waits for a browser connection;
3. the URL is opened.

All three happen within the overall launch deadline. Until now the daemon
returned `Unavailable` instead ([#116](https://github.com/LimeBeck/arinn/issues/116)).
The core must not know which product is the browser (R-01). Application IDs
come from the ApplicationProvider: the desktop provider publishes desktop-file
IDs.

## Decision

- **Which application.** `browser_application` in `config.toml` names one
  application ID. When it is empty (the default), the daemon reads a candidate
  list from `arinn/browser-applications.toml`, in the first XDG data root that
  has one. The package installs it at `/usr/share/arinn/`, with Google Chrome
  and then Chromium, the two supported browsers. Product IDs live only in this
  packaging data, as the default web shortcuts do (ADR-0043). The first
  candidate that a ready ApplicationProvider lists as launchable is used.
- **When.** Only when no ready `browser` provider has a connected browser, but
  a `browser` provider is ready. With no browser provider there is nothing to
  open the URL in, so nothing is launched and the result is `Unavailable`.
  Without a launchable candidate the result is also `Unavailable`.
- **How.** The daemon calls the provider's `Launch`, then waits for the browser
  provider to report a connected backend, then opens the URL as before
  (ADR-0052). Launch, connection and open share one 30 s deadline, the launch
  timeout of spec §9.5. When the deadline passes, the result is `Timeout`. The
  launch is never retried.
- **Concurrency.** One launch at a time. An open that arrives during a launch
  waits for it and then finds the browser connected.

## Consequences

- A web shortcut works right after the TV starts, with the browser closed.
- The launched browser may also show its own start-up window next to the
  managed window of the URL (ADR-0050). This is not handled here.
- A user with another Chromium-based browser that has the Arinn extension
  installed sets `browser_application`.

## Alternatives considered

- **The session's default `https` handler.** It would need a new
  ApplicationProvider method or field, which is a contract change. It could
  also name a browser without the Arinn extension, which never connects.
- **The first application in the `online` group.** That group holds every web
  browser the desktop provider finds, so the choice would be arbitrary.
- **Launching from the Chrome provider.** Rejected by ADR-0038: application
  launch belongs to the ApplicationProvider.

## Verification

- `a_closed_browser_is_launched_before_the_url_opens`: one launch, then the
  URL opens, and a second open does not launch again. It fails without this
  change.
- `nothing_is_launched_without_a_known_browser_application`.
- `packaged_browser_candidates_come_from_the_first_root_that_has_them`.
- The package smoke test checks that the candidate file is installed.
- The real-desktop check is an owner operator test and has not been run.
