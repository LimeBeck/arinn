# ADR-0134: Native music catalogue and provider-owned playback

- **Status:** Proposed
- **Date:** 2026-11-02
- **Deciders:** Maintainer; owner requested unified music implementation in #707/#710
- **Amends / Supersedes:** —
- **Amended by / Superseded by:** —
- **Specification:** §5, §8.4, §9, R-01–R-10

## Context

The owner selected the native music design and its single-row layout B, then
requested local playback and Yandex account connection in one MR. Existing
video-library resume and completion semantics do not model music queues.
The [research](../architecture/music-discovery.md) records service-access limits;
a working unofficial client does not qualify Arinn's deployment identity.

## Decision

Introduce the pre-stable `music` capability. The shell calls daemon-owned
`org.arinn.Music1` endpoints; the daemon authenticates provider owner and
generation before and after forwarding to `org.arinn.MusicProvider1`.
Metadata, opaque IDs and a safe device-code prompt cross this interface.
Filesystem paths, stream URLs, device credentials and OAuth tokens do not.
The [wire document](../plugin-api/music.md) defines bounds and command semantics.

Providers own their catalogue, account, audio process and finite or continuous
queue. Local music uses descriptor-based file reopening and a separately owned,
audio-only mpv process. It does not add audio rules to the video-library provider.
A catalogue scan publishes one immutable snapshot; playback opens from the last
published snapshot without waiting for a filesystem scan.

The daemon serializes admitted music commands globally. Starting playback
requires confirmation of the existing provider/session before stopping it;
only a confirmed stop permits the new start. Browsing another source does not
change the active session. `MusicController1.GetActive` projects that session
independently from the catalogue selection.

Yandex uses device-code authorization and Secret Service. The owner-supplied
public Arinn client identity is the default deployment configuration; it remains
subject to live device-flow and Music API qualification. No third-party client defaults are
bundled. An explicitly empty identity disables Connect. Optional client secrets
use a unique unlocked Secret Service item; user tokens are never pasted into UI. This ADR does not claim live provisioning or streaming qualification.

## Consequences

The native section can share one UI across providers while retaining separate
account and catalogue state. Queue occurrence identity, bounded snapshots,
caller-scoped retries and replacement fencing add protocol and test obligations.
This remains proposed for owner review and does not declare Plugin API v1 stable,
change an MVP acceptance criterion or mark a roadmap phase complete.

## Alternatives considered

- Browser shortcuts: do not meet the requested native music experience.
- Extending video-library kinds: couples music to video resume/completion rules.
- Reusing the existing external mpv Activity: does not supply an independent
  background audio session and risks conflicting with video-provider work.

## Verification

Private-bus tests must cover authorization, owner/generation changes, command
retry identity and snapshot cursors. Owned fake audio IPC tests must distinguish
load acknowledgement, file readiness, errors and natural EOF. Synthetic catalogue
fixtures must cover missing mounts and duplicate finite-playlist tracks. Native
QML tests must cover source selection, layout B, focus and account states.
Live authorization, package installation and actual audio playback require
separate recorded qualification; accepting this ADR does not imply they passed.
