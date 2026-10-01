# ADR-0073: Optional Return-to-platform-window association

- **Status:** Accepted
- **Date:** 2026-10-01
- **Deciders:** Delegated technical choice within owner-authorized contract task #212
- **Amends / Supersedes:** —
- **Amended by / Superseded by:** —
- **Specification:** §9, §12; existing opaque identity and platform ownership requirements

## Context

Home's Return target is an opaque KWin UUID. BrowserWindow.window_token names a
PlatformWindows1 identity, currently a platform-generation/counter token, while
Status.generation is a daemon epoch. Comparing these strings does not establish
identity. The conservative #213 correction preserves Return; it cannot finish
#207's cross-source deduplication. ADR-0039 owns platform identity, ADR-0052 owns
browser links and ADR-0069 owns explicit Home intent. None defines their missing
Return association, and their existing signatures must remain compatible.

## Decision

Define optional `ReturnAssociation1` read-only daemon metadata: a coherent
`Get` snapshot carries daemon epoch, monotonic revision, opaque Return target,
and zero or one typed `(platform generation, token)` reference. A separate
`Changed(epoch, revision)` invalidates reads; it never requests Home presentation.

Define optional `KWinReturn1.Report` for the trusted adapter to assert the exact
Return-object/token relationship against its connected inventory generation and
committed revision. It uses bounded scalar arguments and the existing KWin-owner/
inventory-session authentication. Neither public nor private existing interfaces
change. The daemon validates context/lifetimes; only the platform resolves
compositor identity. Full [wire/lifecycle semantics](../architecture/return-association.md)
are normative for later implementations.

Unresolved or unsupported association preserves Return. Focus loss alone does
not revoke exact window identity. Removal, remap, session/owner/generation change
and Home context updates invalidate old evidence. Browser joins use current
registered provider snapshots and require a single unambiguous candidate;
provider restart clears that join, not the platform-owned Return action. No
name/title/URL/last-focus guessing, no new integration-specific core routing, and
no new provider IPC in local navigation are introduced.

## Consequences

Contract-only fixtures can test the distinct identity domains without claiming
production behaviour. The daemon/platform producer and asynchronous shell
consumer are separate follow-ups, explicitly listed in the contract document.
No deduplication is enabled until both work. Missing optional interfaces retain
legacy behaviour, and existing clients continue using unchanged signatures.

No product requirement, trust policy or control permission changes. The existing
specification remains accurate and is referenced rather than rewritten; #212's
zone contains protocol and technical documentation, not product requirements.
Media-session/window association remains independently scoped in #189.

## Alternatives considered

- Comparing Return directly to a BrowserWindow token or treating Status.generation
  as a platform generation: compares different identity domains.
- Inferring from current or last focus in the shell: loses the identity when Home
  takes focus and can select an unrelated window.
- Extending required Status/BrowserWindow structs: unnecessary incompatible
  signature changes for optional evidence.
- Adding methods to existing daemon/bridge XML without their implementation in
  this contract-only increment: would make production introspection drift. New
  optional interfaces can land without claiming they are already exported.
- A provider-specific Return method: makes platform identity depend on one
  integration and does not establish general window ownership.

## Verification

Protocol validation/roundtrip tests preserve the old Status signature. Private-bus
fixtures compare both new XML files to live exports, exercise typed calls/signals,
reject untrusted or obsolete adapter contexts, and demonstrate absent-interface
compatibility. Reference lifecycle fixtures cover stale generation/revision,
window removal/remap, owner/epoch change, provider restart, ambiguity and a linked
browser window after Home removes focus. These are contract fixtures, not tests
of production daemon/platform/shell support. Follow-up UI and explicitly
owner-authorized real-KDE evidence remain required; no AC or gate is closed here.
