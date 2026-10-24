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

## Restart limit notice

`arinnd.service` and `arinn-shell.service` restart on failure up to five times
in 60 s (`StartLimitBurst=5`, `StartLimitIntervalSec=60s`). After that, systemd
leaves the unit failed. On a TV with only a remote, Arinn would just disappear,
so both units declare `OnFailure=arinn-session-failed@%n.service`
([#437](https://github.com/LimeBeck/arinn/issues/437)). The template is a
oneshot that runs `/usr/lib/arinn/arinn-session-failed <unit>`. Both packages
install it.

- **When the handler runs.** Since systemd 254 (`RestartMode=normal`, the
  default), every failure passes through a transient failed state before its
  automatic restart, so `OnFailure=` starts the handler on every crash. In a
  crash loop that is about six runs in 10 s. The template therefore sets
  `StartLimitIntervalSec=0`, so the user manager's default start limit cannot
  swallow the last run, the only one that matters. Systemd 253 and older start
  it only when the unit stays failed.
- **When it shows the notice.** The handler reads the unit with
  `systemctl --user show`, and also uses the `$MONITOR_*` variables that
  systemd passes to a template handler. It shows the notice when:
  - the result is `start-limit-hit` (systemd 260 and newer), or
  - the unit is finally failed (`ActiveState=failed`, `SubState=failed`) and
    `NRestarts` has reached `StartLimitBurst`. Before systemd 260, a unit that
    hits its limit through automatic restarts keeps the result of its last
    crash (`exit-code`, `signal`, `core-dump`), so the restart count is the only
    sign of the limit.
- **When it stays silent.** Missing or non-numeric properties and every earlier
  run (sub-state `failed-before-auto-restart` or `auto-restart`, or fewer
  restarts) produce one journal line and nothing else. Exit status 75
  (`RestartForceExitStatus=`, [ADR-0098](../adr/0098-restart-after-an-update.md))
  is always restarted, so it reaches the notice only at the start limit.
- **Exit status 2.** `RestartPreventExitStatus=` also leaves the shell failed.
  That status means a duplicate shell or a wrong backend, not a crash loop. When
  the main process exited with a status listed in `RestartPreventExitStatus`,
  the handler stays silent, even after earlier restarts, as before.
- **One notice per failed run.** The run before the refused restart and the run
  for the final failure share the unit's invocation ID. The handler keeps the
  last reported ID in `$XDG_RUNTIME_DIR/arinn-session-failed.<unit>` (mode 0600).
- **Known limit.** A unit that already has `NRestarts` at the burst and then
  fails without being restarted for another reason (for example, a pending stop
  at logout) would also show the notice.
- At the start limit it sends `org.freedesktop.Notifications.Notify` with
  `busctl --user`, which works on Plasma and XFCE. The summary is "Arinn stopped
  after repeated failures". The body names the unit, the diagnostics commands
  (`journalctl --user -u <unit>`, `arinnctl diagnostics`, spec §14.3) and how to
  start again (`systemctl --user reset-failed <unit>`, then
  `systemctl --user start <unit>`). The notification is critical and does not
  expire. Without a notification server, the handler logs that the notice was
  not shown.
- The message and the journal lines contain only the unit name (§13). The
  handler restarts nothing and takes no power action. It has no `Restart=` and
  no `OnFailure=` of its own.

`tests/package/test_session_failed.py` (CTest `session-failure-notice`) runs
the handler with stub `systemctl` and `busctl`. It covers the start limit as
both systemd behaviours report it: `start-limit-hit`, or the crash result
(`exit-code`/`signal`/`core-dump`) with `NRestarts` at the burst. It also
covers:

- silence before the limit (each transient state);
- silence for exit status 2, also after earlier restarts;
- silence for missing properties;
- one notice per invocation;
- a missing notification server;
- refused unit names.

When `busctl` and `dbus-monitor` are available, it also captures the real
`Notify` call on a private bus. Its unit checks pin `OnFailure=`, the start
limit, `RestartForceExitStatus=75`, `RestartPreventExitStatus=2` and the
oneshot handler with `StartLimitIntervalSec=0`. The package test
(`tests/package/session.py`) repeats them against the installed handler, and
its `systemd-analyze --user verify` run includes the template. That run
instantiates the template and checks the handler executable.

A real user manager reaching the start limit was not run: the CI and dev
containers have read-only cgroups and cannot run `systemd --user`. On a desktop
it is an operator test. The systemd behaviour described above comes from the
systemd sources (`service.c`/`unit.c`, v254–v260).

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

## Dedicated Arinn session

[ADR-0107](../adr/0107-dedicated-arinn-session.md) (accepted by the owner on
2026-10-21) adds two login session entries next to the opt-in autostart of
[ADR-0042](../adr/0042-opt-in-session-autostart.md):

| Entry | Display manager directory | Starts |
| --- | --- | --- |
| `arinn.desktop` ("Arinn") | `/usr/share/wayland-sessions` | `startplasma-wayland`, then `arinn-session.target` |
| `arinn-xfce.desktop` ("Arinn (XFCE)") | `/usr/share/xsessions` | `startxfce4`, then `arinn-xfce-session` (EWMH target) |

Both run `/usr/lib/arinn/arinn-session <plasma|xfce>`. The wrapper reads
`$XDG_STATE_HOME/arinn/session-mode`. When the file holds exactly `desktop`
(at most one trailing newline), the wrapper replaces itself with the plain desktop. Otherwise it starts the
desktop as a child, waits up to 120 s for readiness, exports
`ARINN_SESSION_ENTRY` and `ARINN_SESSION_MODE` (the mode file it read) to the
user manager and starts Arinn. Readiness means
`graphical-session.target` is active on Plasma, or `org.xfce.SessionManager`
is owned on XFCE. If Arinn cannot start, the desktop keeps running. When the
desktop exits, the wrapper stops the Plasma target, removes the marker and
exits with the desktop's status. In both modes the desktop starts with
SIGINT and SIGQUIT at their defaults, as with the stock entries. The desktop
mode clears an `ARINN_SESSION_ENTRY` a killed wrapper may have left in the
user manager and exports `ARINN_SESSION_MODE` there and to the desktop, so
Return to Arinn writes the same file.

`/usr/lib/arinn/arinn-session-switch arinn|desktop [--prompt]` records the
choice, then asks the desktop to log out through its own session-bus API:
`org.kde.Shutdown.logout` or `org.kde.LogoutPrompt.promptLogout` on Plasma,
`org.xfce.Session.Manager.Logout` on XFCE. It writes the file named by
`ARINN_SESSION_MODE` when set, so it matches the file the session read. If the
desktop refuses the request, the previous choice is restored as the session
would read it (damaged content becomes `arinn`) and the command
exits 1. Without `--prompt` the logout call is bounded to 5 s
(`busctl --timeout=5`), and a desktop that does not answer counts as a
refusal. A choice that cannot be recorded exits 3 without a logout request;
usage errors exit 2. A logout the desktop accepts and an application cancels later is not
detected: the session continues with `desktop` recorded and no error. That is
an open point for the daemon and power-menu follow-up (G-5.8). The "Return to Arinn"
menu launcher (`org.arinn.ReturnToArinn.desktop`) runs it with `--prompt`.
"Switch to desktop" in the power menu reaches it only through the daemon's
platform layer: `SystemControl1.GetSessionSwitch` and `SwitchToDesktop`
([system control](system-control.md#switch-to-desktop)). It is offered only
when the daemon's environment has `ARINN_SESSION_ENTRY` set.
Nothing here needs privileges or changes display-manager configuration.

Administrator setup, once per HTPC:

- **SDDM** (Plasma): for a TV without a login prompt, use
  `/etc/sddm.conf.d/autologin.conf` with `[Autologin]`,
  `User=<user>`, `Session=arinn.desktop` and `Relogin=true`. With
  `Relogin=true` a switch logs in again at once; without it, the greeter
  appears with the Arinn entry preselected.
- **LightDM** (XFCE): `autologin-user=<user>` and
  `autologin-session=arinn-xfce` in `[Seat:*]`, or choose "Arinn (XFCE)"
  once in the greeter.
- Users of the Arinn session should not also enable the opt-in autostart.
  Otherwise Arinn starts in the plain desktop as well.

Verified offline: `tests/package/test_session_entry.py` (CTest
`arinn-session-entry`, and against the installed files from
`tests/package/session.py`) runs both scripts against stub `systemctl`,
`busctl` and desktop commands. Not verified: a real login, switch and return
on the HTPC with SDDM and with LightDM. That operator test needs the owner's
permission (G-5.8). Notes for it: on XFCE the EWMH adapter may fail once
with "no EWMH window manager" when it starts before xfwm4, and recovers
through `Restart=on-failure`; the display managers set
`XDG_SESSION_DESKTOP`/`DESKTOP_SESSION` to `arinn`/`arinn-xfce`.

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
