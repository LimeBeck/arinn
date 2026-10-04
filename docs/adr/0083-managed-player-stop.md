# ADR-0083: Terminate the managed player on Stop and retain its final frame

- **Status:** Proposed
- **Date:** 2026-10-04
- **Deciders:** Owner request in issue #309; implementation prepared by Codex, owner review/merge pending
- **Amends / Supersedes:** ADR-0057 (explicit Stop lifecycle)
- **Amended by / Superseded by:** —
- **Specification:** §6.4, §7.4; AC-16, AC-18

## Context

The owner reported playback controls over Home and unreliable player reopening,
and requested a fresh process after stopping with the last frame on Continue.
The previous provider sent stop and left mpv idle with a window alive. That is
implementation evidence, not proof of the compositor symptom's root cause.

## Decision

Explicit Stop and Back that leaves playback terminate only the Arinn-owned player.
Pause and global Home preserve it. Continue launches through the existing library
resume path. The provider actor serializes Open behind shutdown; it freezes and
samples position, best-effort captures the displayed video frame, sends quit,
and awaits owned-unit/socket exit before publishing stopped. It fences old IPC
events and clears launch identity. A shutdown timeout is an error, not success.

Frame capture uses private JSON IPC screenshot-to-file with the video flag. The
provider validates the JPEG against the existing asset size limit, resizes it
through its independent extractor, and exposes an opaque AssetProvider1 ID.
Preparation uses two 200 ms property deadlines; capture/normalization share one
second; quit acknowledgement has 300 ms, owned-unit shutdown two seconds, and
socket closure 500 ms. These limits fit the existing five-second media command
budget. Missing/failed/oversized frames retain the previous artwork. Temporary
files are removed even if the bounded extraction future is dropped. Frames are
local cache data and excluded from all telemetry.

The shell keeps Home hidden and player input disabled while Stop is pending.
Only confirmed success releases presentation and returns to Home/Detail; failure
shows retry feedback in the player. No public API change is required.

## Consequences

There is no idle player window after successful Stop. Reopening has a new process
and native window identity, with startup latency on every Continue. The library
remains the resume authority. Active provider crashes still reconnect to a live
owned player; intentional Stop does not trigger automatic reconnect or degradation.

## Alternatives considered

Keeping the idle player reproduces the lifecycle the owner asked to change.
A background ffmpeg extract alone samples a timestamp, whereas private screenshot
capture obtains the final displayed video frame. The previous frame remains the
fallback so extraction failure cannot prevent stopping.

## Verification

Private IPC regressions cover fresh-peer Open, resume/artwork retention, invalid
capture fallback, late shutdown events and failed quit retry. A synthetic private
systemd service checks terminal-state acknowledgement. Headless mpv tests check
actual process exit and fresh PID. Production shell tests cover pending/failed Stop,
Home/player visibility and retry. Required checks and real-device qualification
are recorded in PR evidence; no phase or operator gate is marked passed here.
