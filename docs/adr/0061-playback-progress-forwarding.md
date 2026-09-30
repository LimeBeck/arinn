# ADR-0061: Forwarding playback progress to the library

- **Status:** Accepted
- **Date:** 2026-09-30
- **Deciders:** delegated technical choice ([ADR-0037](0037-agent-autonomy-and-owner-gated-decisions.md)) within [ADR-0054](0054-media-and-library-wire-contract.md) and the [media MVP](../architecture/media-mvp.md)
- **Amends / Supersedes:** amends [ADR-0060](0060-library-routing.md) (step 3 of the media and library routing)
- **Amended by / Superseded by:** —
- **Specification:** [§7.4](../specifications/arinn-specification.md#74-local-library-and-mpv), [§11](../specifications/arinn-specification.md#11-state-ownership-and-persistence)

## Context

The library provider owns resume (ADR-0054). The daemon feeds it through
`LibraryResume1.SetProgress` with the linked session's position and duration
and a confirmed-EOF flag. media-mvp.md asks for updates every 5 s and on
pause, seek, end and stop. This is step 3 of
[#126](https://github.com/LimeBeck/arinn/issues/126).

## Decision

- **Which sessions.** Media sessions with `has_content` and a content ref of
  capability `library`. The update goes to the library provider named in the
  ref, and only when that provider is ready and declares `LibraryResume1`.
- **When.** The daemon evaluates after every session import and watcher tick
  (at least once per second) and sends when:
    - it sees a linked session for the first time;
    - the session changes between playing and not playing;
    - the linked content changes;
    - the position differs from the extrapolated position by more than 2 s
      (a seek);
    - the session is playing and 5 s have passed since the last update;
    - the session disappears (a stop): the last extrapolated position is
      sent.
- **Position.** A playing session advances from its `CLOCK_MONOTONIC` anchor,
  as on the shell; the position never goes below 0 or past a known duration.
- **`ended`.** It is always false. The session contract has no confirmed
  end-of-file signal: `STATES` is `playing`, `paused` and `stopped`, and a
  position at the end is not a confirmation (media-mvp.md). The signal is a
  contract task ([#158](https://github.com/LimeBeck/arinn/issues/158)). Until
  then only the 90-second rule can mark items watched.
- **Delivery.** Best effort with a 2 s timeout and no retry: the next update
  corrects a missed one. The state is in memory only; after a daemon restart
  every linked session is new and is sent once.

## Consequences

- Resume works for films and episodes longer than 180 s as soon as a library
  provider and mpv are registered.
- Short items and items with unknown durations cannot be marked watched until
  #158 lands.
- One `SetProgress` call per playing session every 5 s, plus one per state
  change.

## Alternatives considered

- **Mark items watched when the position reaches the end.** Rejected:
  media-mvp.md forbids completion without playback or EOF confirmation, and a
  seek to the end would count.
- **Let the player call the library directly.** Rejected: providers talk only
  to the daemon, and the library must not depend on a specific player.

## Verification

Private-bus tests in `crates/arinnd/src/runtime/progress_tests.rs`:

- first sight, a seek while paused and a stop are forwarded, with no updates
  while nothing changes, and `ended` is never true;
- a playing session is forwarded from its extrapolated anchor position and
  then at least every 5 s;
- sessions without library content are not forwarded.
