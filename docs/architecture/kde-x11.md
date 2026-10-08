<!-- SPDX-FileCopyrightText: 2026 Arinn contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Optional KDE Plasma X11 session

Task [#267](https://github.com/LimeBeck/arinn/issues/267) adds an optional KDE
Plasma 6/X11 mode alongside the Wayland MVP target ([ADR-0077](../adr/0077-kde-x11-session.md)).
It uses the same KWin scripting adapter and authenticated daemon transport.
Generic EWMH support for other desktops remains [#186](https://github.com/LimeBeck/arinn/issues/186).

## Starting the shell

The packaged `arinn-shell.service` uses `arinn-shell-launch`. The same script
runs as `ExecStopPost` with `--stopped`: after the shell exits with status 75
(Restart on the "Arinn was updated" Home state), it restarts `arinnd.service`,
and `RestartForceExitStatus=75` restarts the shell ([ADR-0098](../adr/0098-restart-after-an-update.md)).
With
`ARINN_SESSION_BACKEND=auto` (the default), `XDG_SESSION_TYPE=wayland` selects Qt
`wayland` and `XDG_SESSION_TYPE=x11` selects Qt `xcb`. An absent session type
preserves the existing Wayland default. Unknown types are rejected before a shell
starts. An explicit `ARINN_SESSION_BACKEND=wayland` or `x11` overrides detection.
A selection with only the other transport's display variable is rejected with a
fixed reason and exit status 2; the service also uses 2 for duplicate ownership.
When both displays exist (Wayland with XWayland), the explicit selector wins and
cannot distinguish a stale override from an intentional X11 run.

The systemd user manager must have the current graphical environment, including
`XDG_SESSION_TYPE`, `DISPLAY`, `XAUTHORITY` where required, and `WAYLAND_DISPLAY`
on Wayland. Plasma normally imports this environment. After changing sessions,
stop the Arinn target, unset a previous `ARINN_SESSION_BACKEND` override with
`systemctl --user unset-environment ARINN_SESSION_BACKEND`, import the current
`XDG_SESSION_TYPE`, `DISPLAY`, `XAUTHORITY` and `WAYLAND_DISPLAY` where present,
and restart the target in the new session. Explicitly unset display variables
that the new session does not provide; importing only present variables cannot
clear stale values. The package never enables the target.
For a direct development run on Plasma X11, use
`QT_QPA_PLATFORM=xcb build/apps/arinn-shell/arinn-shell` with the existing daemon
and packaged adapter. This is an operator action, not an automated test.

## Playback identity and compositing

Every newly launched Arinn player receives the same per-process launch UUID as
its Wayland app ID and as an X11 instance name. The latter is a lowercase form of
`org.arinn.Playback.<uuid>`, passed through mpv `--x11-name`. The platform adapter
accepts it only from the actual Qt wrapper for `KWin::X11Window`, reading
`resourceName`; it does not infer identity from the generic class, title,
executable, PID or focus. Wayland continues to use `KWin::XdgToplevelWindow` and
`resourceClass`. Unknown wrappers, empty/malformed/nil launch IDs fail closed.
The existing media-window wire types and lifecycle fences remain unchanged.

mpv receives `--x11-bypass-compositor=never`, because the shell's transparent
input/OSD window requires the X11 compositor. The same Present/Home/Return/Release
state machine keeps input in the shell and restores saved window flags.
The existing independent session guard handles lock/suspend; its fail-closed rule
is unchanged. This mode requires KWin compositing to remain enabled.

Sources: [mpv X11 options](https://mpv.io/manual/stable/#options-x11-name),
[mpv WM_CLASS producer](https://github.com/mpv-player/mpv/blob/master/video/out/x11_common.c),
and [KWin X11Window](https://github.com/KDE/kwin/blob/master/src/x11window.cpp).

## Verification limits

Isolated coverage checks launcher selection and argument forwarding, player argv,
real QJSEngine wrapper identity observation and synthetic X11 media
Present/Home/Return. These tests do not prove compositor behavior.

Real KDE/X11 and TV qualification is **not run**. Before claiming support qualified,
verify Home/Return over a browser and a game, video below the focused transparent
shell with visible and hidden OSD, Release/window loss, lock/suspend/recovery and
playback smoothness with compositing enabled. Follow the existing operator runbook
only with explicit owner permission. No G-4.1, acceptance criterion or phase gate
is marked passed by this implementation.
