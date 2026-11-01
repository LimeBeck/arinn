# ADR-0127: Configurable decoding for managed video

- **Status:** Proposed
- **Date:** 2026-11-01
- **Deciders:** project owner for configurable hardware decoding (explicit request, task #673); per-file application is a delegated technical choice under ADR-0037
- **Amends / Supersedes:** ADR-0057 managed player options
- **Amended by / Superseded by:** —
- **Specification:** §5.6, §7.4, §11; AC-09, AC-16; R-01–R-05

## Context

The managed player disables external configuration and scripts, but does not
select hardware decoding. Its software default can add CPU load for supported
video. The owner requested a persistent setting rather than a fixed launch flag.
The existing provider settings contract already supplies enum controls, storage,
validation, startup delivery and live Apply; no API addition is needed.

## Decision

The mpv provider declares `hardware_decoding` in the `playback` category through
ProviderSettings1. Values are `auto` (automatic hardware decoding when available)
and `no` (software). The default remains `no` to preserve existing behaviour.
Settings1 persists the value in `[providers.<id>.settings]`; the SDK supplies
validated startup defaults and rejects unsupported values on Apply. No raw
player arguments, custom backends or external mpv configuration are accepted.

Apply updates the preference for future Open operations without sending a
command to the current video or restarting a player/provider. Every Open sends
its selected `hwdec` as a per-file `loadfile` option alongside resume position and
language choices. One choice is frozen across the modern and legacy command
forms. This works with a fresh process, an already-running owned player and an
adopted player after a provider restart. Reconnection alone never replays Open
or changes decoding of current playback. An Open already being prepared uses
the preference when its file options are constructed.

Automatic means the mpv-supported automatic decoder selection with software
fallback; it does not promise a hardware decoder for every codec/device. Keep
`--no-config`, script and input restrictions, process ownership and private IPC.
No provider names or decoding logic are added to core routing or QML. Generic
Settings renders the enum; there is no plugin-supplied UI or new wire method.

## Consequences

The preference can change without rebuilding Arinn. Current playback continues;
opening a video applies the new choice. A real supported GPU/driver is still
needed to use hardware decoding. Existing configurations stay in software until
the user selects Automatic. No installer or host update is part of this task.

## Alternatives considered

- Loading mpv.conf would also enable unrelated player configuration, violating
  the managed-player isolation policy.
- A hardcoded auto flag would not provide the requested choice.
- Changing a live decoder on Apply could seek/reinitialize current playback.
- Applying only launch arguments would miss the reused player process.

## Verification

Schema/startup sanitation and invalid values; private-bus schema authorization,
stale generations, startup preferences and Apply followed by Open; modern and
legacy per-file options and unchanged current playback; headless generated-video
playback with automatic selected and an empty hardware codec allowlist to prove
software playback remains available without a real GPU test. Full required
checks and independent review are recorded in the task/PR, not assumed here.
No real-desktop/operator or device decoding qualification is claimed.
