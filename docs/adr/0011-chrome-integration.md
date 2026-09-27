# ADR-0011: Chrome/Chromium through an MV3 extension and Native Messaging

- **Status:** Accepted
- **Date:** 2026-09-27 (proposed); accepted 2026-09-27 by [ADR-0028](0028-documentation-governance.md)
- **Deciders:** project owner (Developer Specification v0.3)
- **Amends / Supersedes:** —
- **Amended by / Superseded by:** [ADR-0021](0021-plugin-runtime-ownership.md) — separate native bridge process and reconnect rules
- **Specification:** [§7.1](../specifications/arinn-specification.md)

## Context

The browser workflow is the main daily MVP scenario. Tabs must be manageable without building a browser engine into the shell.

## Decision

Arinn uses a regular Chrome/Chromium. Chain: Chrome Extension API → Arinn Chrome Extension (Manifest V3) → Native Messaging → `arinn-plugin-chrome` → Plugin API → `arinnd` → shell.

The extension requests the minimum permissions. The integration works with windows, tabs, URL, title, favicon and active state. Version 1 needs no page scraping, DOM automation, arbitrary JavaScript or content analysis.

The MVP can launch Chrome, open a URL, list windows and tabs, find the active tab, switch/create/close tabs, show title, URL and favicon, and return to an existing browser activity. A tab may carry id, windowId, title, url, favicon, active and audible; IDs are opaque.

Online services are separate Home tiles that open a URL. BrowserProvider owns browser-level entities; ApplicationProvider owns launching the application. Packaging installs the Native Messaging manifest.

## Consequences

Browser integration reuses the existing browser. The extension and the native host have their own lifecycles that must be reconciled with daemon supervision (ADR-0021).

## Alternatives considered

An own WebView/browser engine is excluded. CDP and DOM automation are not selected.

## Verification

AC-03, AC-04, AC-05, AC-06: tab operations, online-service tiles and returning to the browser activity. Phase 3 must deliver a usable daily browser workflow.
