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
filesystem entries and 32 nested directories. Catalogue text has a conservative
16 MiB allocation budget. A failed scan publishes no partial
catalogue. User paths, application arguments and titles are not diagnostic logs.

## Launch ownership

The provider asks the session systemd manager to start `/usr/bin/gio launch`
in a unique `app-arinn-<uuid>.service` in `app.slice`. The desktop-file path is
one argument, using ExecStartEx with no environment expansion. GIO interprets
desktop field codes and D-Bus activation. This is the system application launcher,
not a provider-side Exec interpreter.

The runtime requires GLib/GIO 2.86 or later. Earlier `gio launch` versions
load desktop entries without their filename and therefore lose D-Bus activation
identity ([upstream 2.86 launcher](https://github.com/GNOME/glib/blob/2.86.0/gio/gio-tool-launch.c)).
The Arch package enforces this minimum. On older Ubuntu test runners the activation
fixture checks a normalized failure; successful activation is verified locally
with GIO 2.88 and in the Arch package test with the supported dependency.

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

## Windows and registration lifetime

The provider uses the SDK registration watch to construct WindowClient on its
actual registered connection. It subscribes before reading the inventory and
reconciles signals with a one-second fallback scan; the desktop catalogue is
rescanned at most every two seconds. Every registration change cancels current
reads/controls and clears old clients and running instances. Launch remains
available when KWin is absent; unverified window actions are not published.

KWin's documented desktopFileName basename is converted to an exact `.desktop`
ID. Absolute paths and unproven associations stay empty. No resource class, title,
URL or executable matching is used for application association. The platform
adapter separately excludes the Arinn shell, input-method surfaces and Plasma's
`kscreenlocker_greet` surface (KWin JavaScript does not expose isLockScreen). Each normal external window has an opaque
platform-generation token. Removal/remap replaces that token permanently. The
provider resolves exact IDs against its catalogue and reports distinct windows,
ordered by most recently observed foreground activity. Titles come from the
catalogue, never the compositor.

The daemon authorizes the current provider owner/registration on every platform
request. It atomically commits staged inventories, retains immutable caller-bound
pages and admits controls through the same operation budget as application work.
KWin explicitly acknowledges dispatch or a normalized failure. Provider loss,
adapter generation replacement and heartbeat expiry invalidate pending work.
Before dispatch, the adapter checks the exact window again and fails closed if
session lock state is unavailable. Graceful close never escalates to a process kill.

The backend retains at most 32 paging leases, and at most 16 MiB of encoded
published/leased snapshots plus a bounded staging buffer. Removed-token history
is capped at 100,000 tokens per generation; exhaustion invalidates the session and
requires a fresh adapter generation. Lost callbacks use watchdog reconnection,
never mutation replay. Installation of the KWin script remains explicit under
the existing runbook; this change does not load it into the user's session.

KWin metadata follows its [Window API](https://github.com/KDE/kwin/blob/master/src/window.h).

## Verification status

Local isolated checks on 2026-09-28:

- `cargo test --locked`: temporary XDG trees, locale/visibility, deletion,
  duplicate IDs, oversize/non-regular files, GIO field codes and DBusActivatable,
  exact association/MRU/remap, plus production daemon endpoint tests.
- The production endpoint tests cover unauthorized callers, atomic/incomplete
  inventory, invalid indices, retired tokens, immutable pages, lease capacity,
  XML/live introspection agreement, deduplication, graceful-close errors,
  shared operation limits, provider disconnect, acknowledgement timeout/replay
  and generation replacement.
- `node --test tests/integration/kwin_adapter.test.cjs`: seven deterministic tests
  cover existing Home/Back, exact metadata, remap tokens, revision changes,
  acknowledgements, unsupported/locked controls and late callbacks.
- `dbus-run-session -- python tests/integration/desktop.py`: actual daemon and
  desktop-provider processes with a private manager and fake KWin identity;
  real GIO dispatch to a temporary sentinel, failure propagation, remap and
  inventory loss; provider restart, daemon restart and manifest removal leave
  the sentinel alive. Duplicate launch UUIDs dispatch only once. The existing
  `phase1.py` CI entry point also invokes this suite on a separate private bus.
- The Arch package installs the desktop executable/manifest and requires GIO
  (`glib2>=2.86`). Its smoke test checks discovery through the installed daemon/provider.

The test manager verifies unit requests and fixture-process lifetime, not kernel
cgroup membership. No real KDE/Wayland operator tests were run. AC-07, AC-08,
G-2.1, actual cgroup qualification for G-2.2 and real Activity/remap qualification
for G-2.3 remain outstanding in [qualification task #35](https://github.com/LimeBeck/arinn/issues/35). Phase 2 is not marked Done.
