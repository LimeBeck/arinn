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
`Categories=` sets the Applications group: `WebBrowser` is Online, then `Game` is
Games, then `AudioVideo`, `Audio` or `Video` is Media; the first match in that
order wins and everything else is Other (spec §5.5).
The optional desktop `Icon` key is unescaped and validated against
[ADR-0059](../adr/0059-application-icon-references.md). Valid theme names and
absolute file references pass through unchanged; invalid references become empty
without hiding the application. Icon bytes count toward the discovery budget.
Discovery does not open or download the referenced image.
Exec is delegated to the system launcher and D-Bus activation to the session bus;
no shell evaluates an Exec line in the provider. Duplicate groups and malformed booleans are
rejected. A repeated key keeps its last value, as GLib and the launcher read it
([#201](https://github.com/LimeBeck/arinn/issues/201)). Directory symlinks are not traversed; non-regular files are rejected
without blocking on FIFOs. A file is limited to 256 KiB and a scan to 100,000
filesystem entries and 32 nested directories. Catalogue text has a conservative
16 MiB allocation budget. A failed scan publishes no partial
catalogue. User paths, application arguments and titles are not diagnostic logs.

## Launch ownership

The provider asks the session systemd manager to start `/usr/bin/gio launch`
in a unique `app-arinn-<uuid>.service` in `app.slice`. The desktop-file path is
one argument, using ExecStartEx with no environment expansion. GIO interprets
desktop field codes. This is the system application launcher, not a provider-side
Exec interpreter.

**D-Bus activatable entries** are not launched through GIO. `gio launch` sends
`org.freedesktop.Application.Activate` asynchronously, flushes asynchronously and
returns, so the process can exit before the call leaves it; the activation is then
lost while GIO still exits 0 ([issue #59](https://github.com/LimeBeck/arinn/issues/59),
[GLib 2.86 `g_desktop_app_info_launch_uris_internal`](https://github.com/GNOME/glib/blob/2.86.0/gio/gdesktopappinfo.c)).
Instead the provider applies GLib's rule itself: an entry with
`DBusActivatable=true` whose file basename without `.desktop` is a valid
well-known bus name is activated on that name, at the object path formed by
replacing `.` with `/` and `-` with `_`. The provider first checks that the name
is owned or activatable and otherwise returns `Unavailable`. It then sends
`Activate` with empty platform data and `NO_REPLY_EXPECTED` from a dedicated
connection; the message is written to the bus socket before the call returns, so
closing the connection cannot lose it. The bus, not the provider, starts the
application, so it never joins the provider's cgroup (with systemd activation, as
used by dbus-broker, it runs in its own unit). As with GIO, success acknowledges
dispatch, not handling by the application. Other `DBusActivatable` entries fall
back to GIO, as GLib does.

The Arch package requires GLib/GIO 2.86 or later
([upstream 2.86 launcher](https://github.com/GNOME/glib/blob/2.86.0/gio/gio-tool-launch.c)).
The activation fixture no longer depends on the GIO version and runs on every CI
job.

The application unit has `Type=exec`, `ExitType=cgroup` and
`CollectMode=inactive-or-failed`. It has no dependency on the provider service.
The provider's registration-token environment is never copied to it. The manager
uses its session environment, as for other graphical session applications.
Launcher stdout/stderr are discarded because raw launcher errors may contain
private paths or arguments; callers receive normalized error codes.

A dedicated D-Bus connection holds an AddRef reference while reading launcher
completion. Properties are read without caching, so an early zero exit timestamp
cannot conceal completion. For GIO launches, the exit status acknowledges dispatch
only; the launch lifetime below decides the result. The reference is released when
the launch ends or when the connection dies; releasing it never stops the
application unit. Launch mutations are never retried. A dispatch timeout (30 s) can
follow a dispatched side effect.

## Launch lifetime and cancellation

Spec §6.5 and §7.2, [ADR-0110](../adr/0110-application-launch-lifetime-and-cancellation.md),
[#436](https://github.com/LimeBeck/arinn/issues/436). Before this change the
provider answered `Launch` when GIO exited, so a slow game "succeeded" before any
window existed and an application that exited without a window also "succeeded".

After GIO exits with status 0, the provider keeps its reference on the unit and
waits (`plugins/desktop/src/launches.rs`):

- **Window.** The launch succeeds when the window inventory shows a window that
  was not there when the launch started and is not proven to belong to another
  catalogue application: its desktop ID is the launched entry's, unknown or empty.
  Games, Proton and wrapper scripts often report no exact desktop ID; the
  Activity itself is still associated exactly, as above. A new platform
  generation renames every window, so it resets the starting set instead of
  counting as a new window.
- **Unit ended.** With `ExitType=cgroup` the unit stays active after GIO exits
  until everything the launch started has exited. When its `ActiveState` becomes
  `inactive` or `failed`, the provider reads the inventory once more. The launch
  fails at once with `Unavailable` unless a window of the application exists. An
  existing window counts here because the launch was handed to an instance that
  was already running (Steam, single-instance applications).
- **No deadline.** While the unit runs with no window, the launch stays pending.
  The 30 s budget bounds dispatch only; the daemon applies no deadline to an
  application launch.
- **Not observable.** Without a window inventory (no platform adapter), and for
  D-Bus activated entries, which have no Arinn unit, the launch succeeds at
  dispatch as before. If the inventory stays missing for 5 s during the wait
  (a reconnecting platform adapter is shorter), the launch ends the same way.

`CancelLaunch(generation, id)` signals every unfinished launch of that desktop ID.
Each one stops its own unit with `StopUnit(…, "replace")`, so systemd stops the
processes of that launch's cgroup and nothing else. Its `Launch` reply then fails
with `Cancelled`. `CancelLaunch` answers after the stops, or `NotFound` when no
launch was pending or each one ended another way first (for example, its window
appeared). A cancel during dispatch takes effect when dispatch completes.
Losing the registration ends the wait with `Unavailable` and only releases the
reference; the application keeps running.

The daemon keeps the public operation `running` and `cancellable` meanwhile.
`Operation1.Cancel` from the caller forwards to `CancelLaunch` and ends the
operation as `cancelled`. The shell shows "Opening <app>…", then after 8 s
"Starting <app>…" with Cancel. Cancel asks for confirmation, with *Keep starting*
as the default focus; Home hides the overlay without cancelling, and choosing the
tile again shows it again. A failed launch shows the error dialog with Retry.

Known limitation: a game entry that hands off to a client that is already running
elsewhere (for example a `steam steam://rungameid/…` entry while Steam runs)
ends its own unit at once. If no window of that entry exists yet, the launch
fails, although the client may still start the game.

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
ID. Absolute paths and unproven associations stay empty. A managed player window (an exact `org.arinn.Playback.<launch>` identity, ADR-0074) is Arinn's own playback surface: it stays in the inventory for presentation but always has an empty desktop ID, so it is never a running application or a Continue Activity ([#376](https://github.com/LimeBeck/arinn/issues/376)). No resource class, title,
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
- D-Bus activation (2026-09-28, [issue #59](https://github.com/LimeBeck/arinn/issues/59)):
  a discovered activatable entry is dispatched 20 times on a private bus with no
  systemd manager, each time closing the provider connection before the fixture
  application handles `Activate`; all activations arrive. Unknown names return
  `Unavailable`. The test fails when the entry is routed through the GIO launcher.
  The release-mode test binary passed 30 consecutive runs locally.
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

Launch lifetime ([#436](https://github.com/LimeBeck/arinn/issues/436), 2026-10-21):

- `cargo test -p arinn-plugin-desktop`: window rules, an ended unit, hand-off to
  a running instance, a new platform generation, no inventory, and cancel that
  stops only the launches of one application.
- `tests/integration/desktop.py` with the extended test manager, which keeps an
  application unit active while its process group runs (the stand-in for
  `ExitType=cgroup`) and stops only that group: a launch stays running and
  cancellable until its window appears; a window that appears after 31 s
  completes it; a unit that exits with status 0 and no window fails in under 5 s;
  a hand-off to an existing window succeeds; Cancel from another connection is
  `PermissionDenied`; Cancel by the caller ends the operation as `cancelled`,
  stops that fixture and leaves a concurrent launch running. On `master` the
  suite fails at the first "still starting" check: the launch had already
  succeeded.

The test manager verifies unit requests and fixture-process lifetime, not kernel
cgroup membership. No agent-run KDE/Wayland operator tests were run.

Real KDE qualification ([#35](https://github.com/LimeBeck/arinn/issues/35)), 2026-09-28:
the owner checked the v0.1 package (commit `e97879b`) on their KDE Plasma 6 /
Wayland machine and reported that all checks passed. By owner decision these
manual checks count as the qualification, and Phase 2 is Done. The checks:

- Steam, EmulationStation and an ordinary application launch from their tiles,
  and the launched window appears above the shell;
- activating an open window from Home works;
- Home → Return reaches the same application;
- a launched application survives `arinnctl provider restart desktop`;
- the KWin adapter no longer reconnects on idle control polls.

Not covered: kernel cgroup membership was not inspected, and a window remap was
not specifically exercised.
