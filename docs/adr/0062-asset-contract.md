# ADR-0062: Fetching assets by id

- **Status:** Accepted
- **Date:** 2026-09-30
- **Deciders:** delegated technical choice ([ADR-0037](0037-agent-autonomy-and-owner-gated-decisions.md)) within [ADR-0054](0054-media-and-library-wire-contract.md)
- **Amends / Supersedes:** amends [ADR-0054](0054-media-and-library-wire-contract.md) (how asset ids are resolved)
- **Amended by / Superseded by:** —
- **Specification:** [§9.1](../specifications/arinn-specification.md#91-capabilities), [§9.7](../specifications/arinn-specification.md#97-limits-and-validation), [§13](../specifications/arinn-specification.md#13-security)

## Context

[ADR-0054](0054-media-and-library-wire-contract.md) made these opaque asset
ids, never inline bytes:
- media `artwork`;
- thumbnails (ADR-0058);
- library `poster` and `fanart`.

No interface resolved an id, so the shell could not show any real artwork
([#131](https://github.com/LimeBeck/arinn/issues/131)). Application icons are
a different case ([ADR-0059](0059-application-icon-references.md)): theme
names or local paths that the shell loads itself.

## Decision

- **Provider side.** `media` and `library` providers may export the optional
  `org.arinn.AssetProvider1`:
    - it has one method, `GetAsset(generation, id) → (mime, data)`, with
      signature `(say)`;
    - a provider that exports it lists `AssetProvider1` in `Features`;
    - the method is served only to the registered daemon, in the current
      generation;
    - an unknown id is `NotFound`.
- **Daemon side.** The daemon exposes `org.arinn.Assets1.GetAsset(generation,
  id)` on the provider's daemon path. It forwards the call to the provider,
  validates the result, and answers `Unsupported` when the provider does not
  list the flag. The daemon implementation and shell loading are separate
  tasks; this decision fixes the contract.
- **What an asset is.**
    - Image bytes, at most 8 MiB.
    - The type is `image/jpeg`, `image/png` or `image/webp`.
    - The declared type must equal the type sniffed from the bytes' own
      signature. SVG, HTML and disguised content are rejected.
    - The SDK validates the handler's result and turns a bad image into
      `Internal`, so a faulty provider cannot send one.
    - There are no file paths or URLs in either direction, and assets are
      never logged.
- **Transport.** Plain D-Bus bytes. File descriptors are not used.
- **Ids.** The same opaque ids as in ADR-0054. Their lifetime is the
  provider's; an id that no longer resolves is `NotFound`.

## Consequences

- Artwork, posters, fanart and thumbnails become displayable once the daemon
  and shell tasks land.
- Each image costs one round trip. The shell caches by
  (provider, generation, id).
- MPRIS artwork (`mpris:artUrl`) can only be served if the MPRIS provider
  reads local files and returns their bytes. It never fetches remote URLs.

## Alternatives considered

- **Pass a file descriptor to a read-only file.** It is more efficient, but
  the provider would decide what the shell reads. The daemon would have to
  reopen the fd and check it before it could hand it on, and the shell would
  need fd handling in QtDBus. A later version can add it if profiling shows
  the byte copy matters.
- **Return a path or URL for the shell to load.** Rejected: the shell never
  receives provider file paths or URLs (spec §13, R-03).
- **Accept any image type.** Rejected: SVG and other formats bring script and
  parser risks into the shell.

## Verification

- Protocol unit tests: the `(say)` signature round trip; JPEG, PNG and WebP
  accepted; a mismatched type, SVG, HTML, an empty or oversized image and a
  truncated WebP header rejected.
- SDK private-bus tests (`media_service`, `library_service`):
    - the flag appears in `Features` only with a handler;
    - a valid image is served;
    - an unknown id is `NotFound`;
    - a disguised image is `Internal`;
    - an empty id is `InvalidArgument`;
    - a stale generation is `Conflict`;
    - the base service does not export the interface;
    - the live `AssetProvider1` equals `docs/plugin-api/asset-provider1.xml`.
