# ADR-0038: Browser launch ownership

- **Status:** Accepted
- **Date:** 2026-09-28
- **Deciders:** project owner, explicit answer to OQ-03 in the Codex conversation; recorded in [issue #19](https://github.com/LimeBeck/arinn/issues/19)
- **Amends / Supersedes:** [ADR-0011](0011-chrome-integration.md), [ADR-0021](0021-plugin-runtime-ownership.md) (browser launch ownership only)
- **Amended by / Superseded by:** [ADR-0043](0043-web-shortcuts.md) — web shortcuts (OQ-02)
- **Specification:** [§7.1–7.2](../specifications/arinn-specification.md#71-browser-chromechromium)

## Context

OQ-03 identified conflicting ownership in the historical developer specification
and the current plugin runtime: declaring both `application` and `browser` on the
Chrome provider could duplicate the desktop provider's browser application entry.
The owner approved the recommended separation on 2026-09-28.

## Decision

The desktop ApplicationProvider launches the browser application through the
session application launcher, in its own application unit or scope, outside the
provider service cgroup. `arinn-plugin-chrome` declares only the `browser`
capability and owns browser-level entities and operations.

The application and browser entities link through a validated platform window
token. The daemon does not infer this relationship from titles, URLs or
product-specific conditions. Window activation remains a PlatformService action.

Opening a URL with a closed browser uses the desktop ApplicationProvider for the
application launch and waits for a browser connection within the overall launch
deadline, as specified in the [plugin runtime](../architecture/plugin-runtime.md).
This decision does not select web-shortcut ownership (OQ-02) or define new wire
methods; required contract changes are reviewed separately.

## Consequences

Phase 2 can implement browser application discovery and launch alongside ordinary
applications. Phase 3 adds browser-level operations without a second application
listing from the Chrome provider. Restarting/removing the provider must not kill
applications it launched (G-2.2).

The current platform bridge tracks active/return window state. Production window
enumeration, association, activation and graceful close still need their reviewed
contract and implementation before full desktop-provider acceptance.

## Alternatives considered

Have the Chrome provider declare both `application` and `browser`. The owner chose
the desktop provider for application launch to avoid duplicate ownership and
application listings.

## Verification

Phase 2 verifies browser/Steam/ES-DE launch, Home/Back, application lifetime
independence and window remapping (AC-07, AC-08, G-2.1–G-2.3). Phase 3 verifies
browser-only capability registration and browser/application association through
the platform token, including failure to associate rather than title-based guesses.

This ADR records a decision, not implementation evidence. No phase or gate is
closed; real-desktop operator tests still require explicit owner permission.

<!-- SPDX-FileCopyrightText: 2026 Arinn contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->
