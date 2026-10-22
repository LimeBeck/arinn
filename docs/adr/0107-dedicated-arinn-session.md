# ADR-0107: Dedicated Arinn login session and an unprivileged switch to the desktop

- **Status:** Accepted
- **Date:** 2026-10-21
- **Deciders:** project owner. The owner made the product decision on 2026-10-10 ([#438](https://github.com/LimeBeck/arinn/issues/438)). On 2026-10-21 the owner approved, in chat, the unprivileged privilege boundary and security posture below ([recorded on #476](https://github.com/LimeBeck/arinn/pull/476#issuecomment-6328132714)), and confirmed Return to Arinn as an application-menu launcher. Technical details are under delegated authority.
- **Amends / Supersedes:** amends [ADR-0042](0042-opt-in-session-autostart.md) (adds a second, session-based start path next to the opt-in autostart)
- **Amended by / Superseded by:** —
- **Specification:** §4.2, §5.7, §11, §12.4, §13, §18

## Context

Today Arinn runs inside an ordinary Plasma or XFCE session. On Plasma the opt-in
user units start it ([ADR-0042](0042-opt-in-session-autostart.md)); on XFCE an
opt-in XDG autostart helper starts the EWMH target
([ADR-0080](0080-mvp-platforms.md), [x11-ewmh.md](../architecture/x11-ewmh.md)).
On 2026-10-10 the owner decided ([#438](https://github.com/LimeBeck/arinn/issues/438)) that the MVP adds the following:

- an **Arinn** login session for KDE Plasma on Wayland
  (`/usr/share/wayland-sessions/arinn.desktop`) and for XFCE on X11
  (`/usr/share/xsessions/arinn-xfce.desktop`). Each starts the desktop with the
  Arinn units;
- **Switch to desktop** in the power menu. It shows the live list of
  applications that will close, with Cancel as the default focus (§5.7). It
  ends the Arinn session and makes the next login use the plain desktop session;
- a **Return to Arinn** launcher on the desktop, which does the reverse.

The opt-in autostart inside an ordinary session keeps working. Plugins never
get the switch action (§13). The owner asked this ADR to decide the privilege
boundary between two options: a narrow polkit-guarded helper that switches only
between the two known session names, or logging out to the greeter with the
target session preselected, which needs no privileges.

How the two display managers of the MVP platforms choose a session (verified against upstream sources on 2026-10-21):

- **SDDM** (Plasma). Autologin happens only when the daemon first starts, unless
  `[Autologin] Relogin=true`. It uses `[Autologin] Session=` and, when that
  is empty, `Last.Session` from `/var/lib/sddm/state.conf`. The SDDM daemon
  writes `Last.Session` after every successful login, and the greeter
  preselects it. Both files are root-owned, and the greeter has no per-user
  session preference.
- **LightDM** (XFCE). An autologin session uses `autologin-session=` and, when
  that is unset, the user's `XSession` from AccountsService. A greeter login
  uses the session the greeter sent, falling back to the same chain. LightDM
  then stores the chosen session as the user's `XSession`. A user can change
  their own `XSession` through AccountsService (`SetXSession`). That needs the
  polkit action `org.freedesktop.accounts.change-own-user-data`, which is
  allowed without authentication in an active local session.

So on SDDM, which is the display manager of the Plasma HTPC, the owner's
option 2 cannot preselect the plain session without privileges. A logout lands
on a greeter that preselects the last session, the Arinn one, and SDDM logs in
automatically again only with `Relogin=true`. Option 1 does not achieve "the
next login" alone either. A new `[Autologin] Session=` applies at the next
autologin, which happens at the next boot unless `Relogin=true`. After a plain
logout, the greeter still preselects the Arinn entry.

## Decision

A third variant with the privilege model of option 2, approved by the owner
on 2026-10-21. **No privileged code and no display-manager configuration.** The
Arinn session entry itself honours a per-user choice, and the switch is that
user's own logout.

1. **Session entries.** The package installs:
   - `/usr/share/wayland-sessions/arinn.desktop` (`Name=Arinn`,
     `Exec=/usr/lib/arinn/arinn-session plasma`,
     `TryExec=startplasma-wayland`, `DesktopNames=KDE`);
   - `/usr/share/xsessions/arinn-xfce.desktop` (`Name=Arinn (XFCE)`,
     `Exec=/usr/lib/arinn/arinn-session xfce`, `TryExec=startxfce4`,
     `DesktopNames=XFCE`).

   `TryExec` hides an entry when its desktop is not installed. The package
   enables nothing. Choosing the Arinn entry in the greeter, or setting it as
   the autologin session, stays the administrator's step. The documentation
   recommends `Relogin=true` for SDDM autologin.
2. **Session start** (`arinn-session`, POSIX shell, `packaging/bin`). It reads
   `$XDG_STATE_HOME/arinn/session-mode`, where `~/.local/state` replaces an
   unset `XDG_STATE_HOME`:
   - If the file holds exactly `desktop` (at most one trailing newline), the
     wrapper `exec`s the plain desktop
     (`startplasma-wayland` or `startxfce4`) with nothing from Arinn.
   - Anything else, including a missing or damaged file, starts Arinn. A broken
     file must not hide the TV shell. The wrapper starts the desktop as a child
     and waits up to 120 s until the desktop is ready for Arinn. Plasma is
     ready when it has activated `graphical-session.target`, which is the
     requisite of `arinn-session.target`. XFCE is ready when
     `org.xfce.SessionManager` is owned on the session bus. The wrapper then
     exports `ARINN_SESSION_ENTRY=plasma|xfce` to the user manager. On Plasma it
     starts `arinn-session.target`. On XFCE it starts the existing
     `arinn-xfce-session` helper, which owns the EWMH target and its lifetime.
   - If the desktop never becomes ready, or the start fails, the desktop keeps
     running without Arinn (fail open, with a fixed log line). The user still
     has a working desktop and the Return to Arinn launcher.
   - The wrapper forwards `TERM`/`HUP`/`INT` to the desktop and exits with the
     desktop's status. When the desktop exits, it stops the Plasma target and
     removes `ARINN_SESSION_ENTRY`.
   - Together with `ARINN_SESSION_ENTRY` it exports `ARINN_SESSION_MODE`, the
     absolute path of the mode file it read. The switch writes that file, so
     an `XDG_STATE_HOME` set only in the login environment (`~/.profile`) or
     only in the user manager (`environment.d`) cannot split the two.
   - The desktop starts with SIGINT and SIGQUIT at their defaults
     (`env --default-signal=INT,QUIT`, coreutils 8.31 or later). Without job
     control, sh would start a background command with both ignored, and the
     whole desktop session would inherit that.
   - In the desktop mode the wrapper first removes `ARINN_SESSION_ENTRY` from
     the user manager, in case an earlier wrapper was killed while the manager
     outlived it (lingering, or SDDM `Relogin=true`). It names the mode file in
     `ARINN_SESSION_MODE`, both in the user manager and in the desktop's own
     environment, so Return to Arinn writes the file this login read.
   - Without `DBUS_SESSION_BUS_ADDRESS`, the wrapper exports the user bus
     `$XDG_RUNTIME_DIR/bus` when it exists, so the XFCE helper and the desktop
     share one bus.
3. **Switch** (`arinn-session-switch arinn|desktop [--prompt]`, POSIX shell).
   It writes the mode atomically into the file named by
   `ARINN_SESSION_MODE`, or else into the user's own state directory (file
   `0600` via a temporary file and rename; a missing directory is created
   `0700`, an existing one keeps its mode). It then asks the
   running desktop to log out on the session bus:
   - Plasma: `org.kde.Shutdown.logout`, or `org.kde.LogoutPrompt.promptLogout`
     with `--prompt`;
   - XFCE: `org.xfce.Session.Manager.Logout(show_dialog, allow_save=true)`.

   The desktop is `ARINN_SESSION_ENTRY` when it is set; otherwise the standard
   desktop name in `XDG_CURRENT_DESKTOP` (`KDE`, `XFCE`) identifies it. An
   unsupported desktop changes nothing and fails. If the desktop refuses the
   logout request (the D-Bus call fails), the previous choice is restored as
   the session would read it (`desktop` only for exactly `desktop`, otherwise
   `arinn`), or
   the file is removed if there was none, and the switch fails. The caller
   shows the error, and the session continues.

   A logout the desktop **accepts and cancels later** is not detected. Plasma's
   `logout()` returns at once and an application's unsaved-work prompt can
   cancel it afterwards; a second, concurrent request is ignored and still
   returns success; XFCE's `Logout` behaves alike. The session then continues
   with `desktop` recorded, without an error, and the next login starts the
   plain desktop. This is an open point for the daemon and power-menu
   follow-up (G-5.8). Directions: a pending choice that the session wrapper
   commits only when the desktop exits soon after the request, or reporting
   "Logout was cancelled" when the session is still running after a bound and
   restoring the previous choice; otherwise accept and document it.
4. **Callers.**
   - "Switch to desktop" calls `arinn-session-switch desktop` without a
     desktop prompt, because Arinn's own confirmation with the live
     application list comes first. The shell never executes it, since QML runs
     no shell commands (§13). Only the daemon's platform layer starts it, with
     a fixed path and fixed arguments. That method is a separate
     `contract-change` next to the power actions of `SystemControl1`. It
     refuses provider connections, so plugins never get the action, and it is
     offered only when `ARINN_SESSION_ENTRY` is set, which means inside the
     dedicated session.
   - "Return to Arinn" (`org.arinn.ReturnToArinn.desktop`, in the application
     menu of KDE and XFCE) runs `arinn-session-switch arinn --prompt`. The
     desktop's own logout confirmation lets the user save work or cancel.
     Cancelling keeps the current desktop session, but the recorded choice
     stays. The label means "the next login starts Arinn", so this is the
     intended reading.
5. **Persistence.** The choice persists until the user switches again, as the
   owner's decision says ("the next login uses the plain desktop session").

**Security posture (for the owner):**

- No root code, setuid helper, polkit policy or system D-Bus service is added.
  The package writes no `/etc` file and no display-manager state. All writes
  go to the calling user's own `$XDG_STATE_HOME`.
- The switch can do nothing the user cannot already do: log out of their own
  session through the desktop's own logout API, and pick the session that
  user's Arinn entry starts.
- A process running as the user, including a plugin, could write the file or
  call the desktop's logout API directly. This ADR does not add that ability;
  the user's own processes already have it, consistent with §13's "not a
  hostile-plugin sandbox". What Arinn controls is that its daemon never offers
  the action to a provider connection.
- Fail-safe defaults: anything but an exact `desktop` starts Arinn. A refused
  logout restores the previous choice. A desktop that is not ready leaves the
  user with a working desktop.

## Consequences

- One mechanism covers SDDM, LightDM and any other display manager that reads
  `wayland-sessions`/`xsessions`. With SDDM `Relogin=true`, or LightDM
  autologin into the Arinn entry, the switch works both ways without a
  password and without the greeter.
- The greeter label "Arinn" can start the plain desktop after a switch. A user
  who picks the stock "Plasma"/"Xfce Session" entry in the greeter always
  gets the plain desktop. SDDM then remembers that entry as the last session,
  so Return to Arinn from there needs the Arinn entry to be chosen once in the
  greeter.
- The dedicated session and opt-in autostart are independent. A user who also
  enabled the autostart gets Arinn in the plain desktop as well. The install
  message tells users of the Arinn session not to enable it.
- New persisted file: `$XDG_STATE_HOME/arinn/session-mode` (spec §11).
- Open point (G-5.8 follow-up): a logout cancelled after the desktop accepted
  it leaves `desktop` recorded without an error (decision 3).
- Operator-test notes:
  - On XFCE the wrapper starts `arinn-xfce-session` as soon as
    `org.xfce.SessionManager` is owned, earlier than the autostart template.
    If xfwm4 is not up yet, `arinn-platform-x11` fails once with "no EWMH
    window manager" and recovers through `Restart=on-failure`.
  - Both entries start `startplasma-wayland`/`startxfce4` directly; Plasma 6's
    own entry wraps it in `plasma-dbus-run-session-if-needed`, which is a
    no-op where a systemd user bus exists, as Arinn already requires. The
    display managers set `XDG_SESSION_DESKTOP`/`DESKTOP_SESSION` to
    `arinn`/`arinn-xfce` instead of the stock values.
- Plasma readiness depends on Plasma's systemd startup
  (`graphical-session.target`), the same requirement the ADR-0042 units already
  have.
- Pending in separate pull requests: the daemon method (contract change after
  `SystemControl1`, [ADR-0108](https://github.com/LimeBeck/arinn/pull/470)), the
  power-menu entry (after the #148 shell work), and the operator test.

## Alternatives considered

- **Option 1: a polkit-guarded root helper** that writes only
  `/etc/sddm.conf.d/zz-arinn-session.conf` (`[Autologin] Session=`) or
  `/etc/lightdm/lightdm.conf.d/90-arinn-session.conf`
  (`autologin-session=`) with one of the two known names. This is what SteamOS
  does with SDDM, where it also sets `Relogin=true`. It needs a root
  executable or system bus service, a polkit action with a default (likely
  `allow_active=yes`), display-manager detection, and changes to
  administrator-owned configuration. After a plain logout it still does not
  change the greeter's preselection. It adds privileged attack surface for no
  functional gain over the decision above. Recommended only if the owner wants
  the display manager itself to show the plain session as the autologin
  session.
- **Option 2 as stated: log out to the greeter with the target preselected
  through AccountsService.** `SetXSession`/`SetSession` on the own user is
  unprivileged and works for LightDM, both for the greeter preselection and
  for autologin without `autologin-session`. SDDM has no per-user preference,
  so on the Plasma HTPC the user would have to pick the session by hand in the
  greeter. Rejected as the only mechanism. It remains a possible addition for
  LightDM greeters (making Return to Arinn preselect the Arinn entry after a
  stock-session login).
- **Selecting Arinn through `XDG_CURRENT_DESKTOP`** (for example
  `DesktopNames=KDE;Arinn` and an `OnlyShowIn=Arinn` autostart entry): relies
  on every desktop and display manager propagating the extra name, and changes
  what applications see as the desktop. Rejected.
- **A runtime enablement** (`systemctl --user enable --runtime
  arinn-session.target` before the desktop starts): event-driven, but the link
  persists for the lifetime of the user manager, for example with lingering,
  and would start Arinn in a later plain session. Rejected for the
  readiness wait above.
- **One-shot desktop mode** (SteamOS: Desktop Mode lasts until the next boot):
  a reboot would always return the TV to Arinn. Not chosen, because the owner's
  decision makes the choice persist until Return to Arinn. The owner may prefer
  it.

## Verification

- `tests/package/test_session_entry.py` (CTest `arinn-session-entry`, and
  against the installed copies from `tests/package/session.py`) runs both
  scripts with stub `systemctl`, `busctl` and desktop commands. Beyond the
  cases below it checks that the desktop gets default SIGINT/SIGQUIT in both
  modes even under a parent that ignores them, that the desktop mode clears
  stale markers, that the switch writes the file the session exported, and
  that the write replaces the file by rename and leaves no temporary file
  when it fails:
  - Plasma and XFCE start only after readiness, with the marker exported
    first;
  - the desktop mode starts the plain desktop only, and damaged choices
    start Arinn;
  - a desktop that never becomes ready, or a failed target start, keeps the
    desktop running;
  - signals are forwarded and the shared bus address is used;
  - the switch writes `0600`/`0700` files atomically, uses each desktop's
    logout call with and without a prompt, refuses unsupported desktops and
    bad arguments, restores the previous choice when the logout is refused,
    and round-trips through the session entry.
- `tests/package/session.py` checks the installed session entries and launcher.
  When `desktop-file-validate` is available it validates them, ignoring only
  its unknown-key report for the session key `DesktopNames`
  (desktop-file-utils issue 61). The unchanged units still pass
  `systemd-analyze --user verify`.
- Operator test on the HTPC, with the owner's permission: log in to Arinn,
  switch to the desktop, and return, on Plasma/SDDM and XFCE/LightDM (G-5.8,
  G-5.5). Accepting this ADR passes none of these.
