<!-- SPDX-FileCopyrightText: 2026 Arinn contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Desktop application provider

Implementation work for [issue #20](https://github.com/LimeBeck/arinn/issues/20),
specification §§7.1–7.2, 9 and 12, AC-07/AC-08 and G-2.1–G-2.3.
This document records implementation evidence, not Phase 2 acceptance.

## Catalogue

`arinn-plugin-desktop` discovers `applications/` below the absolute XDG data home
and data directories, in priority order. Defaults follow the XDG directory layout.
Desktop-file IDs use relative paths with directory separators replaced by hyphens.
A hidden, malformed or duplicate higher-priority entry suppresses the same lower
ID. Removing that override exposes the lower entry on the next scan.

Names use LC_ALL, LC_MESSAGES, then LANG, with country/modifier fallbacks.
OnlyShowIn, NotShowIn, NoDisplay, Hidden, TryExec and Type control visibility.
Exec and DBusActivatable are delegated to the system launcher; no shell evaluates
an Exec line in the provider. Duplicate groups/keys and malformed booleans are
rejected. Directory symlinks are not traversed; non-regular files are rejected
without blocking on FIFOs. A file is limited to 256 KiB and a scan to 100,000
filesystem entries and 32 nested directories. A failed scan publishes no partial
catalogue. User paths, application arguments and titles are not diagnostic logs.

## Launch ownership

The provider asks the session systemd manager to start `/usr/bin/gio launch`
in a unique `app-arinn-<uuid>.service` in `app.slice`. The desktop-file path is
one argument, using ExecStartEx with no environment expansion. GIO interprets
desktop field codes and D-Bus activation. This is the system application launcher,
not a provider-side Exec interpreter.

The application unit has `Type=exec`, `ExitType=cgroup` and
`CollectMode=inactive-or-failed`. It has no dependency on the provider service.
The provider's registration-token environment is never copied to it. The manager
uses its session environment, as for other graphical session applications.
Launcher stdout/stderr are discarded because raw launcher errors may contain
private paths or arguments; callers receive normalized error codes.

A dedicated D-Bus connection holds an AddRef reference while reading launcher
completion. Properties are read without caching, so an early zero exit timestamp
cannot conceal completion. GIO exit status acknowledges dispatch, not window
creation or the application's eventual exit. The reference is released after the
result or when the connection dies; releasing it never stops the application unit.
Launch mutations are never retried. A timeout can follow a dispatched side effect.

The transient-unit reference mechanism is also used by
[systemd-run](https://github.com/systemd/systemd/blob/main/src/run/run.c).
Desktop semantics are delegated according to the
[Desktop Entry Specification](https://specifications.freedesktop.org/desktop-entry/latest-single/).

## Verification status

Temporary-tree tests cover precedence, locale/visibility, deletion, duplicate IDs,
D-Bus-activatable entries, oversize/non-regular files and directory symlink loops.
A private systemd D-Bus fixture verifies application-unit properties, literal
path arguments, real GIO field-code dispatch to a temporary test script, and
launcher failure. It does not prove real cgroup isolation.

Production platform integration and complete provider lifetime tests are still
being implemented. Real KDE/Wayland and cgroup qualification remain outstanding;
no operator test has been run for this increment.
