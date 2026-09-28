# ADR-0046: The shell is the input layer during managed playback

- **Status:** Accepted
- **Date:** 2026-09-28
- **Deciders:** project owner (answer to OQ-01 in the Claude Code session, recommended option); technical details under delegated authority ([ADR-0037](0037-agent-autonomy-and-owner-gated-decisions.md))
- **Amends / Supersedes:** amends [ADR-0024](0024-local-library-mpv.md) (input path of the managed mpv session)
- **Amended by / Superseded by:** —
- **Specification:** [§6.4](../specifications/arinn-specification.md#64-player), [§6.5](../specifications/arinn-specification.md#65-performance-principles)

## Context

In a managed mpv session, mpv draws the video in its own window and Arinn draws
the OSD. The media MVP design routed keys pressed while the OSD was hidden from
the mpv window through the plugin and the daemon to the shell, which then moved
focus to the timeline. That is the chain *keypress → D-Bus → plugin → response →
focus change* that §6.5 forbids, and it was recorded as OQ-01.

## Decision

During a managed playback session the shell keeps keyboard focus as a
transparent fullscreen input layer over the mpv window, both while the OSD is
hidden and while it is visible.

- Every key is handled locally in the shell by the §6.4 rules. Only commands
  (seek, play/pause, stop, track selection) go to the MediaProvider through the
  daemon.
- mpv never receives keys: the shipped mpv configuration disables its default
  key bindings and the OSC, and the provider never forwards input.
- The layer applies only to sessions Arinn manages. External applications keep
  their own input (§6.2), and global Home stays a platform action.

## Consequences

- Input latency during playback is local, as in the rest of the shell.
- The platform layer must keep the shell stacked and focused above the mpv
  window during a managed session, and lower it when the session ends. This is
  platform code (R-10).
- A transparent Wayland surface above the video must not reduce playback
  smoothness or break direct scan-out on the target HTPC. That is unverified and
  is part of the media integration gate.

## Alternatives considered

- **Keys through mpv and the plugin** (the earlier media MVP design): rejected,
  it violates §6.5 and makes focus depend on provider IPC.
- **Giving focus to mpv while the OSD is hidden** and taking it back on the
  first key: rejected, the first key would be lost or would need the same
  forbidden chain.

## Verification

G-4.1 media integration gate: an OSD over video without dropped frames compared
with mpv alone, the §6.4 key table with the OSD hidden and visible, and
Home/Return during playback. Accepting this ADR does not mean the gate passed.
