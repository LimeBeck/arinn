# ADR-0063: The `ended` session state

- **Status:** Accepted
- **Date:** 2026-09-30
- **Deciders:** delegated technical choice ([ADR-0037](0037-agent-autonomy-and-owner-gated-decisions.md)) within [ADR-0054](0054-media-and-library-wire-contract.md) and the [media MVP](../architecture/media-mvp.md)
- **Amends / Supersedes:** amends [ADR-0054](0054-media-and-library-wire-contract.md) (session states)
- **Amended by / Superseded by:** —
- **Specification:** [§7.3](../specifications/arinn-specification.md#73-media-sessions-mpris), [§7.4](../specifications/arinn-specification.md#74-local-library-and-mpv), [§9.8](../specifications/arinn-specification.md#98-extensibility-and-versioning)

## Context

ADR-0054 has the daemon forward "a confirmed-EOF flag" to
`LibraryResume1.SetProgress`, but a session could not report a confirmed end:
the known states were `playing`, `paused` and `stopped`. The mpv provider
mapped end of file to `paused`, so progress forwarding
([ADR-0061](0061-playback-progress-forwarding.md)) never sent `ended`. As a
result, items of 180 s or less, or of unknown duration, could never be marked
watched ([#158](https://github.com/LimeBeck/arinn/issues/158)). media-mvp.md
forbids inferring the end from the position: a seek to the end is not a
confirmation.

## Decision

- **New token.** A session may report state `ended`: the player confirmed that
  playback reached the end of the content (for mpv, `end-file` with reason
  `eof`, or `eof-reached`). It is added to `media::STATES` with the constant
  `STATE_ENDED` and the helper `media::ended()`.
- **Behaviour.** Like `paused`, it does not advance on the shell clock. The
  position should be the last position the player reported.
- **Providers that cannot confirm an end** (MPRIS) never report it. A provider
  never reports `ended` after a seek to the end without playback reaching it.
- **Compatibility.** The state is already a free token. There is no signature
  or XML change, and an older consumer shows `ended` as an unsupported state
  that does not advance (spec §9.8).
- **Follow-ups, outside this contract change:**
    - the daemon forwards `ended = media::ended(state)` in `SetProgress`
      (ADR-0061);
    - the mpv provider reports `ended` instead of `paused` at end of file
      (#125).

## Consequences

- Short items and items of unknown duration can be marked watched once the
  follow-ups land.
- The shell shows `ended` like a paused session at its end position until a
  design says otherwise.

## Alternatives considered

- **A boolean in the session struct.** Rejected: it changes the signature and
  the XML, for information that is a state.
- **Inferring the end from position and duration.** Rejected: media-mvp.md
  requires a confirmed end.

## Verification

Protocol tests: `ended` validates, does not advance, and is recognised only
for the exact token. The follow-up tasks add daemon and provider tests.
