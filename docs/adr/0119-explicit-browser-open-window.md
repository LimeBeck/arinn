# ADR-0119: Activate the browser window selected by the open command

- **Status:** Accepted
- **Date:** 2026-10-29
- **Deciders:** delegated technical choice under ADR-0037, implementing the existing focus-or-create requirement; independent review required before merge
- **Amends / Supersedes:** amends [ADR-0052](0052-browser-window-association.md), the OpenUrl completion mechanism only
- **Amended by / Superseded by:** —
- **Specification:** [§7.1](../specifications/arinn-specification.md#71-browser-chromechromium), AC-03, AC-04, AC-06

## Context

Issue [#585](https://github.com/LimeBeck/arinn/issues/585) reports that opening
an existing shortcut raises the most recently opened service. A successful
extension command currently returns no target. The daemon selects a focused
window from the first newer snapshot, but Home may prevent the requested browser
focus change, and unrelated changes can advance the revision. The focused window
therefore does not identify the command's result.

The browser snapshot does not distinguish ordinary and managed windows. Matching
an origin in the daemon cannot reproduce the extension's selection, particularly
with ordinary windows, duplicates and redirects.

## Decision

The extension returns the selected numeric window ID and whether it created the
window in the result of that exact command. These fields stay on the private
Native Messaging and ChromeHost1 link. The bridge qualifies the result with the
connection that received the command; IDs from different profiles never mix.

For an existing window the Chrome provider uses its settled association and the
existing PlatformWindows1 client to activate the exact platform token, and waits
for the operation. Registration, unique daemon owner and platform generation
fence the lookup and activation. Missing links fail as Unavailable; old extension
results without a target fail rather than guessing. A new window keeps the
browser's own mapping/focus path, without a guessed token.

After OpenUrl succeeds the daemon rechecks the provider owner/generation and only
dismisses Home. It never performs a second activation inferred from focused
snapshots. ActivateTab keeps its existing explicit-tab activation path. No public
protocol, SDK or XML changes are needed. Focus agreement remains the association
mechanism; URLs and titles never establish platform links.

## Consequences

Opening an existing service no longer depends on the browser stealing desktop
focus from Home. The extension, native host and provider must be updated together.
During an incomplete update or before an existing window has a settled link,
opening it can report Unavailable; the user may retry after the link is restored.
There is no automatic replay of the mutating open command.

The complete provider open has one eight-second deadline, within the daemon's
ten-second call budget. Timing out never replays an activation already dispatched;
as with other mutating IPC, an in-flight action may have an uncertain outcome.

## Alternatives considered

- Wait for another focused snapshot: neither a revision nor elapsed time proves
  which window this command selected.
- Match URLs in the daemon or shell: duplicates, ordinary windows and redirects
  prevent this from identifying the extension's selected managed window.
- Add a public result interface: unnecessary for this implementation fix because
  the provider can invoke the existing daemon-owned platform action.

## Verification

The extension regression covers A → B → A → new C → A while its mock deliberately
keeps the old focused flag. The private Native Messaging/D-Bus chain checks exact
platform tokens, missing links, platform restart, registration loss, missing
target fields and host disappearance. The daemon regression checks that an
unrelated focused snapshot queues no compositor activation.

These synthetic checks do not qualify KDE/TV behaviour. Real-desktop operator
tests require separate owner permission; accepting this ADR is not evidence that
they passed.
