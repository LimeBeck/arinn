# ADR-0059: Application icon references

- **Status:** Accepted
- **Date:** 2026-09-29
- **Deciders:** delegated technical choice ([ADR-0037](0037-agent-autonomy-and-owner-gated-decisions.md)) within specification §5.5, §7.2, §9.7 and §18
- **Amends / Supersedes:** amends [ADR-0035](0035-phase1-application-wire-contract.md)
- **Amended by / Superseded by:** —
- **Specification:** [§5.5](../specifications/arinn-specification.md#55-applications), [§7.2](../specifications/arinn-specification.md#72-applications-steam-and-es-de), [§9.7](../specifications/arinn-specification.md#97-limits-and-validation), [§18](../specifications/arinn-specification.md#18-packaging-distribution-and-licensing)

## Context

Application records are `(id, title, can_launch, group)` ([ADR-0035](0035-phase1-application-wire-contract.md)). Tiles therefore keep the shell's neutral placeholder. Installed desktop icons should be able to replace that placeholder later, without a provider-specific shell and without bundling third-party logos ([#48](https://github.com/LimeBeck/arinn/issues/48)).

The Plugin API is pre-stable (specification §22). Adding `group` already changed the entry signature. Web-shortcut icons stay an opaque packaged reference ([ADR-0043](0043-web-shortcuts.md)). Media artwork stays an opaque asset id until its byte transport exists ([#131](https://github.com/LimeBeck/arinn/issues/131)).

## Decision

An application record is `(id, title, can_launch, group, icon)`, signature `(ssbss)`. `icon` is a string of at most 1024 bytes.

- Empty means absent. [`icon_presentation`](../../crates/arinn-protocol/src/application.rs) returns `Neutral`.
- A non-empty value is either a theme icon name or an absolute local file path. A theme name is ASCII `[A-Za-z0-9._-]` and contains at least one letter or digit. A file path starts with `/`, is not a network path (`//`), and has no empty, `.` or `..` segment. Anything else, including a URL, a `file:` URI and a relative path, is `InvalidArgument`.
- `Neutral` is the consumer's existing placeholder. A theme name or path that the consumer cannot load as a local image is also `Neutral`. The consumer does not fetch a remote icon and does not call the provider to obtain one (R-03).
- The reference is data, not image bytes. The core does not interpret product names inside it (R-01). It is not logged. This field does not carry web-shortcut icons or service defaults.

Provider discovery of desktop `Icon` keys, and shell loading of a usable reference, are later tasks. Until then in-tree providers send an empty reference and the shell decodes the field and discards it, so the placeholder is unchanged.

`Application1` and `Provider1` are pre-stable, so this signature change is the same class as the group field. It is not a minor addition to a released Plugin API (specification §9.8).

## Consequences

- A provider that omits the field no longer matches `GetApplications`. An empty string remains valid.
- Catalogue text budgets count the reference. Page and body limits are unchanged.
- The later renderer must open a file reference only as an image, with a size cap, and must treat a missing file, a non-image or an unknown theme name as `Neutral`. It must not execute the path.
- [#131](https://github.com/LimeBeck/arinn/issues/131) still defines how artwork bytes move. An application icon is not that asset id.

## Alternatives considered

- **Opaque artwork asset id.** Rejected for this field. It needs the byte transport in #131, and a theme icon does not need a second round trip.
- **Inline image bytes.** Rejected. Specification §9.7 passes artwork as references, and a page of images breaks the 256 KiB body limit.
- **Remote URL.** Rejected. No remote fetching, and the repository does not bundle third-party marks (specification §18).
- **Theme names only.** Rejected. A desktop `Icon` key may be an absolute path; dropping that form would make those icons unrepresentable without another contract change.
- **`a{sv}` metadata.** Rejected. It still changes the signature and leaves the reference untyped.

## Verification

Protocol tests cover an empty reference, a theme name, an absolute path, rejected URLs and relative paths, the 1024-byte bound, `Neutral` for absent and invalid values, and the `(ssbss)` signature. SDK publish rejects an invalid reference and counts its bytes. The shell catalog fixture decodes a theme name and a path and does not expose either on the QML model or issue a new provider call. No operator test is run. Discovery and rendering are not claimed.
