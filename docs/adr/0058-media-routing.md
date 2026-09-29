# ADR-0058: Daemon routing for media providers

- **Status:** Accepted
- **Date:** 2026-09-29
- **Deciders:** delegated technical choice ([ADR-0037](0037-agent-autonomy-and-owner-gated-decisions.md)) within [ADR-0054](0054-media-and-library-wire-contract.md)
- **Amends / Supersedes:** —
- **Amended by / Superseded by:** —
- **Specification:** [§7.3](../specifications/arinn-specification.md#73-media-sessions-mpris), [§9.5](../specifications/arinn-specification.md#95-operations), [§9.7](../specifications/arinn-specification.md#97-limits-and-validation)

## Context

[ADR-0054](0054-media-and-library-wire-contract.md) fixed the media wire
contract, but the daemon still treated `media` as unrouted
([#126](https://github.com/LimeBeck/arinn/issues/126)). This is the first of
three steps:

1. media routing (this ADR);
2. library routing, including library launch → `MediaOpen1.Open`;
3. forwarding playback progress to the library.

## Decision

- **Registry.** `media` joins the routed capabilities: a manifest may declare
  exactly one of `application`, `browser` or `media`.
- **Import.** The daemon watches `StateChanged`, with a 1 s tick as fallback,
  like the other providers. Each time it reads:
    - the complete `GetSessions` snapshot, with paging, validation and the
      16 MiB and 100 000 entry limits;
    - `BackendState`;
    - `Features`.

  Snapshots are imported through the SDK's `SessionSnapshots`. Unknown feature
  flags are dropped, so an older daemon never calls an interface it does not
  know.
- **Export.** `org.arinn.Media1` is served at the provider's daemon path.
  Reads come from the imported state.
- **Commands** are operations (`Operation1`), deduplicated per caller and
  `request_id`, never retried, with a 5 s timeout.
    - Before admission, a base command (`play`, `pause`, `play-pause`, `seek`,
      `next`, `previous`, `stop`) must have its token in the session's
      `actions`, and an extension command must have its feature flag.
      Otherwise the answer is `Unsupported` at once. A negative seek or
      thumbnail position, or a track kind other than `audio`/`subtitle`, is
      `InvalidArgument`.
    - A successful command's result is the session's `EntityRef`
      (`capability` `media`).
- **Extension reads.** `ListTracks` and `GetTimeline` are direct calls to
  `MediaTracks1` and `MediaTimeline1` with a 5 s timeout. Their replies are
  validated before they reach the shell.
- **Thumbnails.** `RequestThumbnail` returns an operation. On success its
  single result carries the thumbnail's asset id as `local_id`, with the
  provider's id and capability `media`. ADR-0054 does not say how the shell
  receives a thumbnail; this closes that gap on the daemon side.
- **Provider loss.** When the provider disappears, its sessions, features and
  cache are dropped, and commands answer `Unavailable`.
  - `RowStore` in the SDK has no per-client `disconnect`, unlike the stores
    for applications and tabs. The paging leases of a vanished client
    therefore expire by their TTL instead of being freed at once, and the
    lease cap still bounds them. Adding `disconnect` is an SDK change for a
    contract task.

## Consequences

- MPRIS and mpv providers (#123, #125) can be tested end to end through the
  daemon.
- Asset ids (session artwork, thumbnails, library posters) still cannot be
  fetched by the shell. That needs its own contract task.
- The shell's media screens can bind to `Media1` once steps 2 and 3 land.

## Alternatives considered

- **Pass thumbnails inline in the operation.** Operations carry only entity
  references. Inline image data would break the size limits and the rule that
  assets are ids, not bytes (ADR-0054).
- **Let the provider answer `Unsupported` itself.** It does, as a second
  check. Checking in the daemon first means no operation is created for a
  command that cannot succeed, and a provider that wrongly accepts it is never
  called.

## Verification

Private-bus tests in `crates/arinnd/src/runtime/media_tests.rs`:

- sessions are served and commands reach the provider;
- missing actions or feature flags are `Unsupported`, and bad arguments,
  unknown sessions and stale generations are rejected before any provider call;
- tracks are listed and selected through the extension;
- a lost provider makes media `Unavailable`;
- the live `Media1` export matches `docs/plugin-api/media1.xml` method by
  method, including argument types and directions.
