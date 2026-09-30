# ADR-0060: Daemon routing for library providers and library launch

- **Status:** Accepted
- **Date:** 2026-09-30
- **Deciders:** delegated technical choice ([ADR-0037](0037-agent-autonomy-and-owner-gated-decisions.md)) within [ADR-0054](0054-media-and-library-wire-contract.md)
- **Amends / Supersedes:** amends [ADR-0058](0058-media-routing.md) (step 2 of the media and library routing)
- **Amended by / Superseded by:** —
- **Specification:** [§5.3](../specifications/arinn-specification.md#53-library), [§7.4](../specifications/arinn-specification.md#74-local-library-and-mpv), [§9.5](../specifications/arinn-specification.md#95-operations), [§13](../specifications/arinn-specification.md#13-security)

## Context

[ADR-0058](0058-media-routing.md) routed `media`. The library, and starting a
film or episode from it, are the next step of
[#126](https://github.com/LimeBeck/arinn/issues/126). ADR-0054 specifies the
flow: library `Launch` returns a file URI, which the daemon passes to a media
provider's `MediaOpen1.Open`. The URI must never reach the shell.

## Decision

- **Registry.** `library` joins the routed capabilities.
- **Import.** The daemon watches `StateChanged` (and `LibraryResume1`'s own
  `StateChanged`), with a 1 s tick as fallback.
    - It reads the complete paged catalogue (16 MiB and 100 000 entry limits)
      through the SDK's `LibrarySnapshots`, plus `BackendState` and `Features`.
      Unknown flags are dropped.
    - It reads resumes through `ResumeSnapshots`, only when the provider
      declares `LibraryResume1`.
    - A resume change alone also emits the daemon's `StateChanged`.
- **Export.** `org.arinn.Library1` at the provider's daemon path. Without
  `LibraryResume1`, `GetResumes` and `RestartFromBeginning` answer
  `Unsupported`.
- **Launch** is an operation with the 30 s launch timeout (spec §9.5), never
  retried. It proceeds in this order:
    1. Refuse an unknown item (`NotFound`) or an unavailable one
       (`Unavailable`) before any provider call.
    2. Call the library's `Launch`.
    3. Validate the returned `PlaybackSource`. Its `local_id` must be the
       requested item.
    4. Choose the media provider. It is the one named by `defaults.media` in
       `config.toml` if that provider is ready and declares `MediaOpen1`,
       otherwise the first ready media provider that declares it, by provider
       id. With none, the operation fails `Unavailable`.
    5. Call `Open` with the content ref of the library item, the URI, the
       start position and the languages.
    6. The operation's result is the new media session
       (`EntityRef{media provider, "media", session id}`). The URI is never an
       operation result, a daemon reply or a log line.
- **`RestartFromBeginning`** is an operation calling `LibraryResume1` with a
  5 s timeout.
- **Provider loss** clears the catalogue, the resumes and both caches.
  Library `ImportCompleted` and playback progress to `SetProgress` are not
  part of this step: progress forwarding is step 3 of #126, and import comes
  with the download routing.

## Consequences

- A film or episode can be started from the library as soon as a library
  provider (#124) and a player with `MediaOpen1` (#125) are registered.
- The shell's Library screen can bind to `Library1` (#140).
- `defaults.media` in `config.toml` gets its first consumer.
- Resume positions are written only when step 3 lands.

## Alternatives considered

- **Return the URI to the shell and let the shell start the player.** This is
  forbidden: the shell never receives file paths and never starts
  applications (spec §13, R-03).
- **Open with every media provider that has `MediaOpen1`.** This could start
  two players. One deterministic choice, which the owner can configure, is
  enough.

## Verification

Private-bus tests in `crates/arinnd/src/runtime/library_tests.rs`:

- the catalogue and resumes are served, and restart reaches the provider;
- a launch opens the item in the media provider with the right URI, start
  position, languages and content ref, and returns the session without any
  `file://`;
- unavailable, unknown and stale items are refused, and so are resume calls
  without the flag, all before any provider call;
- a launch without a player that can open fails `Unavailable`;
- the live `Library1` export equals `docs/plugin-api/library1.xml`.
