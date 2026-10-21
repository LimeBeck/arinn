# ADR-0042: Opt-in session autostart through packaged systemd user units

- **Status:** Accepted
- **Date:** 2026-09-28
- **Deciders:** project owner (bring the autostart mechanism forward to run the current build on the TV, [issue #52](https://github.com/LimeBeck/arinn/issues/52)); unit layout and KWin loading under delegated authority ([ADR-0037](0037-agent-autonomy-and-owner-gated-decisions.md))
- **Amends / Supersedes:** amends [ADR-0021](0021-plugin-runtime-ownership.md) (daemon and shell units), [ADR-0032](0032-kwin-script-platform-adapter.md) (how the packaged KWin script is loaded)
- **Amended by / Superseded by:** [ADR-0077](0077-kde-x11-session.md) (proposed optional X11 shell transport; owner approval pending); [ADR-0107](0107-dedicated-arinn-session.md) (dedicated Arinn login session)
- **Specification:** [§18](../specifications/arinn-specification.md#18-packaging-distribution-and-licensing)

## Context

The specification requires that `arinnd` runs as a user service bound to the
graphical session, that the shell starts after Plasma login without duplicate
instances, and that packaging installs the KWin script (§18, AC-01). The Arch
package so far contained only binaries; the daemon, shell and KWin adapter could
be started only by the developer script `scripts/phase0-session.py`, which loads
everything for one session. The owner asked to deliver the mechanism before the
Phase 5 MVP so the build can run on the TV. AC-01 acceptance stays in Phase 5.

## Decision

The package installs four systemd user units into `/usr/lib/systemd/user/`:

- `arinn-session.target` — the only unit with an `[Install]` section,
  `WantedBy=graphical-session.target`; it `Wants=` the three services and is
  `Requisite=`/`PartOf=` the graphical session.
- `arinnd.service` — `Type=dbus`, `BusName=org.arinn.Daemon1`, bounded
  `Restart=on-failure` (2 s, 5 starts per 60 s).
- `arinn-shell.service` — `Type=dbus`, `BusName=org.arinn.Shell1`, ordered after
  the daemon, `QT_QPA_PLATFORM=wayland`, same bounded restart.
- `arinn-kwin.service` — `Type=oneshot` with `RemainAfterExit=yes`; runs
  `/usr/lib/arinn/arinn-kwin-adapter start|stop`, which loads the packaged KWin
  script into the running KWin through `org.kde.kwin.Scripting` under the name
  `arinn` and unloads it on stop. It changes no KWin configuration.

Every service is `PartOf=graphical-session.target arinn-session.target`, so
logging out or stopping the target stops all of them. Uniqueness comes from the
user manager and the bus names: a second daemon or shell cannot own its name.

Autostart is **opt-in per user**: the package enables nothing. A user enables it
once with `systemctl --user enable arinn-session.target` and disables it with
`systemctl --user disable arinn-session.target`. The package prints these
commands on install and before removal. Installing the package on a workstation
therefore never takes over that desktop.

The KWin loader waits up to 10 s for KWin scripting, does nothing if the script
is already loaded, and refuses to load while the developer adapter
`arinn-phase0` is loaded; `scripts/phase0-session.py` refuses to start while the
packaged session is running. Platform-specific loading stays in `platform/kde`.

## Consequences

The current build can start after login on the HTPC without the developer script.
Enabling is one command per user; there is no global preset. Package removal
cannot touch per-user enablement links, so users disable the target first.

The mechanism is verified offline in CI. Real login, logout and repeated
login on KDE Plasma are operator tests and remain open for AC-01 in Phase 5.
Whether Home is visible after login depends on the shell's initial state and the
KWin adapter's first report; that is AC-01 behaviour, not part of this decision.

## Alternatives considered

- **Enabled by default** (a global preset or `systemctl --global enable` in the
  install script): rejected; it would take over every desktop where the package
  is installed, including developer workstations.
- **XDG autostart `.desktop` entry for the shell**: rejected; it gives no restart
  policy, no ordering against the daemon and no clean stop with the session.
- **Enabling the KWin script through `kwinrc`**: rejected; it persists a KWin
  setting outside the package and outside the Arinn target, so disabling Arinn
  would leave the adapter active.

## Verification

`tests/package/session.py` runs after the package is installed in CI: units are
present and not enabled, `systemd-analyze --user verify` reports no problem with
them, the dependency and bus-name layout is as above, and the KWin loader is
exercised against a fake `busctl` (load and run, already loaded, refusal, competing
developer adapter, stop without KWin). The uninstall step checks that the units
and the loader are removed. Real session start, logout and duplicate-instance
checks on KDE are operator tests pending AC-01.
