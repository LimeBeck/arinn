# ADR-0081: MPRIS export of Arinn-managed playback

- **Status:** Accepted
- **Date:** 2026-10-04
- **Deciders:** delegated technical choice ([ADR-0037](0037-agent-autonomy-and-owner-gated-decisions.md)) for a bug fix ([#300](https://github.com/LimeBeck/arinn/issues/300)) within [ADR-0013](0013-mpris-media.md), [ADR-0054](0054-media-and-library-wire-contract.md) and [ADR-0057](0057-mpv-ipc-and-process-scope.md)
- **Amends / Supersedes:** —
- **Amended by / Superseded by:** —
- **Specification:** [§6.2](../specifications/arinn-specification.md#62-keys), [§7.3](../specifications/arinn-specification.md#73-media-sessions-mpris), [§7.4](../specifications/arinn-specification.md#74-local-library-and-mpv), AC-16

## Context

The remote's play/pause key did nothing, even during Arinn playback. On the
device, a uinput KEY_PLAYPAUSE never reached the focused shell window. Plasma
registers the media keys as global shortcuts and sends them to the current
MPRIS player (OQ-04: Plasma owns media and volume keys). The managed mpv runs
without scripts or MPRIS ([ADR-0057](0057-mpv-ipc-and-process-scope.md)), so
the key reached no player, or another one such as a browser tab. Neither the
shell's `Key_MediaTogglePlayPause` handling nor `[input.bindings]`
([ADR-0079](0079-shell-input-bindings.md)) can see it.

## Decision

- **The provider is the MPRIS player.** `arinn-plugin-mpv` itself exports
  `org.mpris.MediaPlayer2` and `org.mpris.MediaPlayer2.Player` at
  `/org/mpris/MediaPlayer2` on its session-bus connection. mpv stays without
  scripts and IPC beyond ADR-0057.
- **The bus name exists only while playback can be controlled.**
  `org.mpris.MediaPlayer2.arinn_mpv.instance<pid>` is requested while the
  provider's current session is playing or paused. It is released when the
  session is stopped or ended or none is left, so an idle Arinn, or one that
  keeps a stopped session, never takes the media keys from other players.
- **Commands reuse the session path.**
  - `PlayPause`, `Play`, `Pause`, `Stop`, `Seek` and `SetPosition` become the
    same `MediaAction`s the shell sends, with the same allowed-action checks.
  - `PlayPause` uses `play-pause` when the session allows it, otherwise
    `pause` or `play` by state.
  - `Next` and `Previous` follow the session's actions.
  - A successful `Seek` or `SetPosition` emits `Seeked` with the target
    position, because clients extrapolate `Position`.
  - `OpenUri` is `NotSupported`; MPRIS cannot open media in Arinn.
  - `Raise` and `Quit` are not offered.
- **Properties come from the provider's view.**
  - `PlaybackStatus` follows the session state.
  - `Metadata` carries `mpris:trackid` (a path made from the session id),
    `xesam:title` and `mpris:length`.
  - `Position` is extrapolated from the snapshot anchor and is not signalled.
  - The `Can*` properties follow the session's actions.
  - `Identity` is "Arinn" and `DesktopEntry` is `org.arinn.Shell`.
  - Changes are signalled on every view change.
- **The `arinn_` prefix is reserved.** Names under
  `org.mpris.MediaPlayer2.arinn_` belong to Arinn providers.
  `arinn-plugin-mpris` skips them, so a managed session is never listed twice.
  The rule is a prefix, not a product check (R-01).

## Consequences

- Plasma's media keys, the media widget and KDE Connect control Arinn
  playback, whether or not the shell has focus.
- Arinn playback appears in Plasma's media controls under "Arinn" while it
  runs. Titles reach the session bus as they already do through
  `MediaProvider1`. They are not logged.
- Another provider that exports MPRIS must use the `arinn_` prefix to stay
  out of the MPRIS provider.

## Alternatives considered

- **Enable mpv's own MPRIS script.** Rejected: ADR-0057 runs mpv without
  scripts. mpv's player would also bypass the provider's state and allowed
  actions.
- **Grab the media keys in the shell or the platform layer.** Rejected: OQ-04
  leaves media keys to Plasma, and a grab would break other players.
- **Rebind the key with `[input.bindings]`.** Not possible: the key never
  reaches the window.

## Verification

- Unit tests in `plugins/mpv/src/mpris.rs`: status, track id, position,
  metadata, and that only a playing or paused session keeps the name.
- `plugins/mpris/src/map.rs` checks that the reserved prefix is skipped.
- The `plugins/mpv/tests/bus.rs` scenario checks three things:
  - while a session plays, the name is on the bus;
  - `PlaybackStatus` is `Playing` and `DesktopEntry` is `org.arinn.Shell`;
  - MPRIS `Seek` reaches mpv and emits `Seeked`;
  - MPRIS `PlayPause` pauses mpv.

  Disabling the name request or the `Seeked` signal fails it.
- Not covered: the actual release of the name on the bus after Stop (the
  decision is unit-tested), and a real KEY_PLAYPAUSE through Plasma on the
  device (operator check).
