# Lock, sleep and graphical session shutdown

The platform session adapter listens to `org.freedesktop.ScreenSaver.ActiveChanged` on the session bus and `org.freedesktop.login1.Manager.PrepareForSleep` on the system bus. On lock or before sleep the window is hidden regardless of the daemon state; the Activity context is kept. After wake the shell first re-checks the lock state; an old `GetActive` reply cannot override a newer signal. If ScreenSaver is unavailable, the state counts as locked until confirmed otherwise.

The Phase 0 transient units have `PartOf=graphical-session.target` and `After=graphical-session.target`: stopping the graphical session stops the daemon, demo and shell. Stopping the whole desktop target was not tested. The packaged session units use the same binding and are additionally `PartOf=arinn-session.target` ([ADR-0042](../adr/0042-opt-in-session-autostart.md)); logout with them is an operator test pending AC-01.

## Boundary and implementation status

The owner approved daemon-independent observation in [ADR-0041](../adr/0041-independent-session-lifecycle-guard.md) on
2026-09-28. G-1.4 applies to application/provider integration through Daemon1;
this observer is a narrow session-lifecycle exception. It does not replace the
system lock screen, control applications or execute suspend/power commands.
Platform-specific transport belongs in the platform layer under R-05. The
implementation now lives in
[`platform/session`](../../platform/session/sessionlifecycle.h) ([task #29](https://github.com/LimeBeck/arinn/issues/29)).
The shell constructs the platform observer through `createSessionLifecycle` and
consumes only its `blocked` property and `changed` signal. The public Qt interface
contains no native bus details; `DbusSessionLifecycle` owns ScreenSaver/login1
subscriptions, asynchronous queries and stale-reply epochs. The existing
`org.arinn.Session1.blocked` shell export and QML visibility binding are preserved.
The observer executes independently of the application's daemon transport.

## Verified

The production shell constructs `StartupHome` after loading its window. It waits
for the independent lifecycle guard to confirm an unlocked session, then sends
the existing `Daemon1.Home` command asynchronously. Unavailable daemon/platform
responses retry once per second with at most one outstanding request. A successful
acceptance ends the startup intent for that shell process; subsequent state
refreshes, daemon reconnects and lock/unlock never replay it. Visibility continues
to follow daemon state and the lifecycle guard, without a synthetic Home result.
This implements the startup Home requirement of AC-01; actual login/TV acceptance
remains an operator test. `shell-startup-home` launches the production executable
on private session/system buses with an offscreen display and synthetic daemon
and ScreenSaver services to test startup, delayed platform readiness, locked and
unknown initial states, and absence of later replay.

The `session-lifecycle` CTest runs on a dedicated private bus and uses real D-Bus subscriptions to fake ScreenSaver and login1 services: lock/unlock, sleep/wake, unlock during sleep, ScreenSaver owner disappearing and returning. It also covers absent/unknown lock state, stale unlocked replies after lock,
owner loss or sleep/wake, and production window hiding with Daemon1 absent or
owning its name without replying. Wake keeps the window hidden until a fresh
lock query completes. These fixtures never connect to the real system bus.
The IPC/input suite is verified separately against the demo provider.

Historical Phase 0 evidence (not rerun for the extraction): on an unlocked KDE session `blocked=false`, 20 Chrome Home/Return cycles and the actual `PartOf`/`After` properties of all three units were checked. This verifies the reaction to events, not physical suspend or the compositor lock screen.

## Deferred by the project owner

On 2026-09-27 the project owner chose to run the real checks later. Real lock, suspend/wake and logout were not executed. These acceptance items stay open (Phase 5, AC-19); this is not a missing product decision.

For an operator-approved lock/unlock check there is `tests/integration/kwin_lock.py`: open Home over a disposable Activity first, then run the test. It **locks the real screen** and waits up to 180 s for a manual unlock. It checks the `org.arinn.Session1.blocked` property and that the return target is kept. The operator visually confirms the shell is not shown over the lock screen. Suspend is checked separately through the regular KDE menu without bypassing inhibitors; the test suite never suspends or logs out automatically.

A complete MVP power action still needs inhibitor/refusal reporting and provider recovery after wake. `SessionLifecycle` does not implement the suspend command itself.

## Desktop entry relaunch

Closing the shell through its menu ends the shell process without stopping the
Arinn session target or enabling/disabling autostart. The packaged `arinn-open`
launcher checks `arinn-shell.service` separately from the KDE/EWMH target and
starts a missing shell before sending Home. An already active shell is preserved;
a failed shell start fails the launch instead of reporting Home success without
a window. Before Home, the launcher performs a bounded round trip to the existing
GUI-thread `Shell1.connected` property and rechecks the unit. The property value
does not need to be true: the reply proves the shell has entered its event loop
after loading QML. Name ownership alone is insufficient for a `Type=dbus` unit.
Stub launcher tests cover both session targets, failed startup after name ownership,
and delayed shell readiness, without operating the real graphical session.

Normal desktop-entry relaunch after a clean shell exit resets the unit start
counter before starting it. The launcher requires `ActiveState=inactive` and
`Result=success`; failed units retain the service start limit and crash-loop
protection. Repeated menu close/reopen cycles therefore remain available.

## Package upgrade

An upgrade replaces the binaries on disk, but running processes keep the old
ones until they restart ([#355](https://github.com/LimeBeck/arinn/issues/355)).
A new shell against an old daemon cannot load Home, and KWin keeps running the
script it loaded at login. Both packages therefore restart the running Arinn
user services after an upgrade with `/usr/lib/arinn/arinn-restart-user-services`:

- **Arch:** the libalpm hook `/usr/share/libalpm/hooks/arinn-restart-user-services.hook`
  runs it `PostTransaction`, only on `Operation = Upgrade` of the daemon, shell,
  EWMH adapter, anything under `/usr/lib/arinn/` (providers, launchers, the
  Chrome host) or the KWin script. `post_upgrade` in `arinn.install` prints one
  line saying the restart follows at the end of the transaction.
- **Debian** ([ADR-0091](../adr/0091-debian-package-ci-artifact.md)): `postinst configure`
  runs it when `$2` names the previous version, that is on an upgrade.

Install and removal start or stop nothing; `pre_remove` keeps its message.

The script lists the running `user@<uid>.service` managers of the system
manager and, for each user whose `arinnd`, KWin, EWMH or shell unit is active,
runs `systemctl --user -M <user>@ try-restart` on `arinnd.service`,
`arinn-kwin.service`, `arinn-ewmh.service` and `arinn-shell.service`, in that
order and one at a time. `try-restart` starts no unit that was not running, so
a user who runs only one session type keeps it. The daemon restarts its
providers on start, and the reloaded KWin adapter serves the new daemon. The
managed mpv scope may close with the daemon, so the output says that playback
in progress is interrupted. Without a running systemd (chroot, container,
build root) the script does nothing, and a failed restart prints a warning
without failing the transaction.

`tests/package/test_restart_user_services.py` (CTest `restart-after-upgrade`)
runs the script, `post_upgrade` and the Debian `postinst` against stub
`systemctl` and `getent`, and checks that the hook runs after upgrades only and
that its targets match the binaries the PKGBUILD installs. The Arch package test
(`tests/package/session.py`) repeats the stub test against the installed script
and matches the hook targets against the files `pacman -Qlq arinn` lists. A
restart under a real user manager is an operator test and was not run.
