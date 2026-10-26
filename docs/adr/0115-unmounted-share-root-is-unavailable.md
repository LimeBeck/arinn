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

At every scan, and when an import picks its root, the library provider reads `/etc/fstab` and `/proc/self/mountinfo`. A root that can be read also counts as unavailable when both of these hold:

1. **The root is empty.** `read_dir` yields no entry at all. This is exactly the symptom: the empty folder a mount point leaves behind. A folder with any entry is never treated as an away share.
2. **The share above it is away.** The nearest mount point that fstab lists at or above the root (not `/`, and not swap) has nothing mounted on it, nor on any folder between it and the root, by mountinfo.

Then the provider handles the root exactly as an unreadable one under ADR-0056. Its items keep their ids and resume and become unavailable together, and `LibraryRoots1` reports the root unavailable. When the share is mounted again, the next scan finds the same items available.

Details:

- **Any mount counts.** A mount that fstab does not list (a systemd mount unit, `mount` by hand) between the listed point and the root counts as mounted.
- **Paths are compared in canonical form.** The kernel records mount points with symbolic links resolved: on ostree desktops `/mnt` is `/var/mnt`. So the root, and each listed point that covers it as written or as resolved, go through `canonicalize`, falling back to the text when a path does not resolve. Mountinfo paths are taken as the kernel writes them. Only the listed points that cover the root are resolved, so an unrelated, hung share is never touched.
- **Parsing.** Fields are split on whitespace (fstab) or single spaces (mountinfo, field 5), and `\ooo` escapes such as `\040` are decoded. Comments and blank lines are skipped.
- **Automount.** An `x-systemd.automount` entry shows as an `autofs` mount while idle, so it counts as mounted, even when the share behind it is away. The decision then falls to reading the root, which triggers the automount. When the share is away, that read can block the scan until the automount times out, as it did before this ADR.
- **Unreadable tables.** A missing fstab lists nothing. An unreadable mountinfo is "unknown", and then no root is treated as away.
- **Imports.** An import picks the most specific configured root that contains the file, as a scan would. If that root is unavailable, the import is refused with `Unavailable`; it never falls back to an outer root, whose walk skips the inner one.
- The provider never mounts anything (§5.6) and logs no path (§13).

## Consequences

- The common NAS case now shows the folder as unavailable in Settings, and its films stay in the library as unavailable (handoff LF-4).
- A folder that no fstab entry covers keeps ADR-0056's rule. A folder the user really emptied stays available, and its items are missing one by one.
- A folder with files is always read normally. That covers a stale `nofail` entry whose mount point now holds files on the parent filesystem, and a share mounted under an away fstab entry.
- A share mounted another way (a manual `mount`, a desktop's user mount) is not in fstab, so its empty mount point still reads as available. A removable drive's mount point under `/run/media` is removed on unmount, so it already reads as unreadable.
- A root written through a symbolic link that an fstab entry's text does not share (fstab `/mnt/nas`, root `/var/mnt/nas/films`) is not matched. It keeps ADR-0056's rule, a missed detection rather than a false one.
- A `noauto` fstab entry that is not mounted, with an empty mount point, is unavailable until it is mounted.
- The provider reads two small system files per scan and per import. A store can be given the tables (`Store::use_mount_tables`, `MountTables::from_text`), so tests do not depend on the host and need no mount.

## Alternatives considered

- **"Had items and became empty" is unavailable.** Rejected: a folder the user emptied on purpose would be shown as unavailable. Requiring emptiness *and* an fstab entry that is away keeps that folder available.
- **The root's `st_dev` equals its parent's.** Rejected as the only test: a bind mount, or a share's subfolder chosen as the root, has the same device as its parent while it is healthy. Mountinfo names the mount points directly.
- **Comparing fstab and mountinfo paths as written.** Rejected after review: mountinfo holds resolved paths, so a healthy share under a symlinked `/mnt` was reported away.
- **Checking only the listed point in mountinfo.** Rejected after review: a share mounted below an away fstab entry was reported away.
- **Remembering earlier mountinfo entries.** Rejected: the provider would need state across restarts for a case that fstab already describes.
- **Mounting or probing the share.** Rejected by §5.6: Arinn never mounts or browses network shares.

## Verification

- `plugins/library/src/mounts.rs` unit tests:
  - a listed share with nothing mounted;
  - a mount between the listed point and the root;
  - a mount point reached through a symbolic link (temporary folders);
  - the nearest listed point;
  - `/`, swap, no fstab and an unreadable mountinfo never mark a root;
  - escapes.
- `plugins/library/tests/catalogue.rs`:
  - `an_unmounted_share_is_unavailable_not_empty`: the films become unavailable together, keep their ids, and come back;
  - `a_really_empty_folder_stays_available`;
  - `a_folder_with_files_under_an_away_fstab_entry_stays_available`;
  - `import_under_an_unavailable_nested_root_is_refused`.
- Each case fails when its part of the rule is removed: the emptiness test, the canonical comparison, the nested-mount check and the import selection.
- The catalogue and store tests use empty injected tables, so they do not read the host's fstab or mountinfo.
- A real NAS on the HTPC is an operator check (AC-15). Not run.
