# ADR-0115: An unmounted share is an unavailable library root

<!-- SPDX-FileCopyrightText: 2026 Arinn contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

- **Status:** Accepted
- **Date:** 2026-10-26
- **Deciders:** delegated technical choice ([ADR-0037](0037-agent-autonomy-and-owner-gated-decisions.md)) within spec §5.6 and AC-15; task [#556](https://github.com/LimeBeck/arinn/issues/556)
- **Amends / Supersedes:** amends [ADR-0056](0056-local-library-scan-metadata-and-resume.md) (when a root counts as unreadable)
- **Amended by / Superseded by:** —
- **Specification:** [§5.6](../specifications/arinn-specification.md#56-settings), [§7.4](../specifications/arinn-specification.md#74-local-library-and-mpv), AC-15

## Context

AC-15 requires that library entries survive an unavailable mount, and spec §5.6 shows a library folder as unavailable when the library provider reports it ([ADR-0071](0071-settings-editor-contract.md)). ADR-0056 treats a root as unavailable only when it cannot be read.

A network share that `/etc/fstab` mounts with `nofail` (the usual NAS entry under `/mnt`) leaves its mount point behind as an empty folder when the share is away. That folder can be read. So the provider reported the root as available, marked its items missing one by one, and Settings never showed the folder as unavailable. The review of #553 found this ([#556](https://github.com/LimeBeck/arinn/issues/556)).

## Decision

At every scan, and when an import picks its root, the library provider reads `/etc/fstab` and `/proc/self/mountinfo`. A root also counts as unavailable when:

- the nearest mount point that fstab lists at or above the root (not `/`, and not swap) has nothing mounted on it in mountinfo now.

Then the provider handles the root exactly as an unreadable one under ADR-0056: its items keep their ids and resume and become unavailable together, and `LibraryRoots1` reports the root unavailable. When the share is mounted again, the next scan finds the same items available.

Details:

- Fields are split on whitespace (fstab) or single spaces (mountinfo, field 5), and `\ooo` escapes such as `\040` are decoded. Comments and blank lines are skipped. Paths are compared as written, without following symbolic links.
- An automount (`x-systemd.automount`) shows as an `autofs` entry in mountinfo while idle, so it counts as mounted. Reading the root then mounts it as before.
- A missing fstab lists nothing. An unreadable mountinfo is "unknown", and then no root is treated as unmounted.
- The provider never mounts anything (§5.6) and logs no path (§13).

## Consequences

- The common NAS case now shows the folder as unavailable in Settings, and its films stay in the library as unavailable (handoff LF-4).
- A folder that no fstab entry covers keeps ADR-0056's rule. A folder the user really emptied stays available, and its items are missing one by one.
- A share mounted another way, such as a manual `mount` or a desktop's user mount, is not in fstab. When its mount point remains as an empty folder, it still reads as available. A removable drive's mount point under `/run/media` is removed on unmount, so it already reads as unreadable.
- A `noauto` fstab entry that is not mounted is unavailable until it is mounted. That matches what the user can play from it.
- The provider reads two small system files per scan and per import. The tables are injectable (`MountTables::from_text`) for tests, because CI cannot mount.

## Alternatives considered

- **"Had items and became empty" is unavailable.** Rejected: a folder the user emptied on purpose would be shown as unavailable.
- **The root's `st_dev` equals its parent's.** Rejected as the only test: a bind mount, or a share's subfolder chosen as the root, has the same device as its parent while it is healthy. Mountinfo names the mount points directly.
- **Remembering earlier mountinfo entries.** Rejected: the provider would need state across restarts for a case that fstab already describes.
- **Mounting or probing the share.** Rejected by §5.6: Arinn never mounts or browses network shares.

## Verification

- `plugins/library/src/mounts.rs` unit tests: a listed share that is not mounted; escapes; the nearest listed mount point; `/`, swap, no fstab and an unreadable mountinfo never mark a root.
- `plugins/library/tests/catalogue.rs`:
  - `an_unmounted_share_is_unavailable_not_empty`: the share's items become unavailable together, keep their ids, and come back. It fails with the rule disabled.
  - `a_really_empty_folder_stays_available`.
- A real NAS on the HTPC is an operator check (AC-15). Not run.
