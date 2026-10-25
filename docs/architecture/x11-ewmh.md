<!-- SPDX-FileCopyrightText: 2026 Arinn contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Optional generic X11/EWMH platform

The owner requested the generic adapter in addition to KDE/X11 on 2026-10-03
(issues #186 and #268). [ADR-0078](../adr/0078-x11-ewmh-adapter.md) describes the
proposed implementation. KDE/Wayland remains the MVP platform. This backend is
implemented and tested with an isolated synthetic EWMH manager; actual XFCE,
other desktop, compositor and TV qualification is **not run**.

## Runtime and identity

`arinn-platform-x11` is a separate Rust process under `platform/x11`, using
[x11rb](https://docs.rs/x11rb/0.14.0/x11rb/) on a blocking worker. Native requests
never block a Tokio executor or the shell UI thread. The daemon selects one
trusted adapter owner at startup through `ARINN_PLATFORM_BACKEND`: absent or
`kde` trusts `org.kde.KWin`, `ewmh` trusts `org.arinn.X11`, any other value fails
closed. Both cannot be trusted simultaneously. This authenticates ownership of a selected
D-Bus name, not the executable or systemd unit: another process running as the
same user can claim an unowned name. No unit or cgroup attestation is provided. Adapter loss invalidates the
existing bridge generation, inventory and pending operations.

The generic adapter reuses the internal scalar KWinBridge1, KWinMedia1 and
KWinReturn1 transport names. Their wire shapes and the provider-facing
PlatformWindows1 contract are unchanged. All reports and polls are pinned to
the current unique daemon owner; reconnect allocates new window tokens and an
adapter instance. There are no shell calls to applications or native XIDs in
provider records.

The adapter checks the root and manager `_NET_SUPPORTING_WM_CHECK` properties
and requires advertised client-list, activation, fullscreen and above support.
A manager change causes a reconnect. It observes `_NET_CLIENT_LIST` normal
clients and `_NET_ACTIVE_WINDOW`; destruction and inventory removal retire
random UUID tokens. `_NET_ACTIVE_WINDOW` may hold one or two CARD32s, as xfwm4
writes two: the first non-zero word is the active window, and `[0]` or `[0, 0]`
(xfwm4 with nothing focused, #379) mean no active window. Properties are type checked and bounded (4,096 clients,
512 supported atoms, 512 metadata bytes); oversized required properties make
the backend unavailable rather than publish a partial inventory.

Exact desktop metadata comes from `_NET_WM_DESKTOP_FILE`,
`_KDE_NET_WM_DESKTOP_FILE` or `_GTK_APPLICATION_ID`. Unknown metadata stays empty;
WM_CLASS, title, process name and executable are not association fallbacks.
Desktop-file IDs retain their `.desktop` suffix for exact provider matching.
Qt/KDE and GTK metadata sources define application basenames, so a missing
`.desktop` suffix is added for those sources only. Already suffixed values are
preserved. Unsuffixed `_NET_WM_DESKTOP_FILE` remains unknown; paths are rejected.
This follows [Qt desktopFileName](https://doc.qt.io/qt-6/qguiapplication.html#desktopFileName-prop)
and [GNOME application ID naming](https://developer.gnome.org/documentation/tutorials/application-id.html).
The shell has its separate explicit identity.
`org.arinn.Shell` and `org.arinn.Shell.desktop` identify the shell, which is excluded
from inventory. The hidden shell (`Window.Hidden`) is a withdrawn window, which
xfwm4 leaves out of `_NET_CLIENT_LIST`; its native X window exists unmapped
before the first Home (#283). Qt creates a native window only on the first show,
so the shell creates it explicitly at startup while still hidden (#379). Startup Home (AC-01)
depends on finding it, because the shell asks for Home while still hidden. The adapter
therefore also looks for the shell among direct root-window children, mapped or
not, one level deep. Inventory still comes only from `_NET_CLIENT_LIST`. The
scan is bounded like the client list: more than 4096 root children fail closed.
Its requests are pipelined (one batch of attributes, one batch of identity
properties), because every window-manager frame is also a root child.

A shell found only among root children serves Home, but it has no lifetime
token: lifetimes, the shell's included, end when a window leaves
`_NET_CLIENT_LIST`, even if the XID returns. Managed playback therefore requires
a listed shell, and a shell that hides during playback ends the presentation
(ADR-0046: the shell is the input layer). Home on a hidden shell works in two
steps. The EWMH requests in `raise_shell` reach a withdrawn window but the
manager ignores them; the explicit Home publish makes the shell show itself,
and the next refresh raises the now-listed shell above while Home is active.

Two matching shell windows, listed or not, make identity ambiguous and trigger a
fail-closed reconnect; run one shell instance, and keep it to one native
top-level window with the shell identity. Event masks are selected once per
newly observed XID and reset when its inventory lifetime ends; the unlisted
shell's mask is selected again on each snapshot. Window output names are currently empty: this
initial backend does not implement RandR output placement. Home reports the
fixed output `x11-screen`, which names the X screen rather than a RandR output.
The Activity background needs a non-empty Home output to bind its frame
(ADR-0087). No Qt screen has that name, so the shell never moves for it.

## Home, Return and video

Global Home uses passive root key grabs for `home_shortcut` (read only from
`config.toml`, default Meta+F12) and XF86HomePage ([ADR-0094](../adr/0094-configurable-global-home-keys.md)),
including Caps/Num/Scroll lock modifier
combinations. A sequence whose key or modifier is not mapped is skipped. Meta derives Super modifiers from the keyboard/modifier
mapping, falling back to native Meta only if no mapped Super exists; it does not
assume Mod4. Keyboard/modifier mapping changes rebuild the grabs. A sequence another
client already grabs is skipped and logged; the backend is unavailable only
when no Home sequence can be grabbed. `quick_menu_shortcut` (default Meta+F11)
and the remote's Menu keys (`Menu`, `XF86MenuKB`) are grabbed the same way next to
the Home keys. The Menu keys are released while the shell is the active window and
grabbed again at the next snapshot after another window becomes active; a press is reported with
`ReportQuickMenuRequest` after the same unlocked check and Home state, with the
same output (`x11-screen`) as a Home request, so it arms the Activity background too.
For a quick-menu press the adapter raises and activates the shell only after that
request is published, so the shell never has focus before the request; a
quick-menu key that cannot be grabbed is skipped without stopping the backend
([ADR-0106](../adr/0106-quick-menu-presentation.md)). The power keys
(`XF86PowerOff`, and `XF86PowerDown` where mapped) are read only while the shell
owns `org.arinn.Shell1` (`power_owner.rs`), never on the ordinary XFCE desktop
(an owner of `org.xfce.SessionManager` or `org.xfce.PowerManager` on the session
bus without `ARINN_SESSION_ENTRY=xfce`, which the dedicated session's wrapper
sets; a name that appears later gives the key back), and only after logind's
`handle-power-key` inhibitor is held. They are grabbed after the
other keys, also while the shell is active, and a press is reported with
`ReportPowerMenuRequest` after the same unlocked check and Home state; the shell
is raised at once, as for Home. A power key another client holds is skipped and
logged and stays that client's; when Arinn lets go of a grab while
xfce4-power-manager runs without its own grab (its one grab at start failed),
the adapter restarts it over D-Bus so it grabs again. In the dedicated Arinn
XFCE session (owner decision of 2026-10-24) the power keys are never grabbed
but read from XInput2 raw key presses on the root window (only the power
keycodes are looked at), and only once that works the adapter sets xfce4-power-manager's
`/xfce4-power-manager/power-button-action` to 0 ("do nothing") through xfconf,
keeping the user's value in `$XDG_STATE_HOME/arinn/xfce-power-button` until it is
restored: when the shell is lost or the reader stops, on a clean stop, by
`ExecStopPost=-/usr/bin/arinn-platform-x11 --release-power-key`, at the next start
in any X11/EWMH session, and at every XFCE login by the system autostart entry
`org.arinn.PowerButtonRestore.desktop` (`--restore-power-button`); a per-user
autostart entry that needs only `sh` and `xfconf-query` restores it after the
package is removed ([ADR-0111](../adr/0111-power-key.md)). Home snapshots the exact opaque target
and keeps it across repeated Home presses. Return validates that token in the
current inventory before dispatch. The shell receives all-desktops, fullscreen/above and
activation EWMH messages; ordinary activation and close also use EWMH messages.
An operation acknowledges checked request dispatch; the window manager retains
focus and placement policy, so this is not proof of desktop qualification.
See the [EWMH specification](https://specifications.freedesktop.org/wm/latest-single/).

Managed video uses the exact canonical non-nil UUID from the native WM_CLASS
instance `org.arinn.playback.<launch-uuid>` supplied by mpv in #267. Generic class
and title are never evidence. Presentation requires the screen's compositor
selection owner, makes video fullscreen and gives the transparent shell input
focus above it. Release restores the video fullscreen/below flags changed by
presentation. Home over playback saves that exact managed target; Return keeps
the shell as the playback input layer. Stale release tokens cannot release a
new presentation. Compositor loss, lock or unavailable lock state releases
presentation, lowers its shell overlay and clears daemon-visible media identity
until presentation is available again. Home and controls require an explicit unlocked ScreenSaver reply.
Unrelated focus releases the presentation without reactivating the shell. Shell
loss, replacement or an ended inventory lifetime restores the exact player
state; presentation retains an opaque shell lifetime token, not just its XID.
The process cleans up presentation on SIGINT/SIGTERM and reconnect; native
identity is refreshed before restoring a saved window.

Both adapter and Qt shell probe lock APIs in this order: freedesktop service at
`/org/freedesktop/ScreenSaver`, the same service at `/ScreenSaver`, then
`org.xfce.ScreenSaver` at `/org/xfce/ScreenSaver` with its matching interface.
The first successful `GetActive` reply wins. An absent service or unsupported
endpoint allows the next probe; timeout or malformed replies fail closed. A
working lock API is a hard runtime requirement: absence is never evidence of an
unlocked session. See the [XFCE implementation](https://github.com/xfce-mirror/xfce4-screensaver/blob/master/src/gs-listener-dbus.c).
Startup reports the selected fixed backend name. Daemon permission rejection
reports a backend-selection hint; restoration failure reports a fixed reason
before reconnecting. These diagnostics contain no window metadata.

### Activity background

The adapter produces the Activity background of specification §12.3 (#149,
[ADR-0087](../adr/0087-bounded-activity-background-frames.md),
[contract](activity-background.md)) in `platform/x11/src/background.rs`. An
optional task follows the daemon's `Background1`, pinned to the owner the
session connected to. For a pending attempt it captures only the window whose
current inventory token is both the Return target and the context's window
token.

- **Capture.** The capture runs off the async runtime on its own X connection.
  It requires a compositing manager (`_NET_WM_CM_Sn` owned), XComposite ≥ 0.2,
  a viewable window, an LSB-first 32-bit TrueColor pixmap format (depth 24 or
  32) and at most 16 Mi pixels (a 64 MiB reply). It names the composited pixmap of the
  window's top-level frame with `NameWindowPixmap` and reads it with
  `GetImage`. The frame must show that window alone. A virtual root
  (`__SWM_VROOT`), or a container that also holds another viewable client
  window (one with `WM_STATE`), is declined, and so is a frame in which the
  target is no longer found or no longer viewable. It never reads the root
  window or another window.
- **Conversion.** One pass reads the server's bytes, box-filters them into
  1920×1080 in premultiplied space (depth 24 is opaque), and unpremultiplies
  each output pixel, so transparent edges add no dark fringes and no full-size
  copy is made. The result is published as one sealed memfd on the adapter's
  own connection, the `org.arinn.X11` owner the daemon authenticates.
- **No frame.** A locked or unknown session, a missing window, no compositor, an
  unredirected window (some compositors unredirect fullscreen windows), an
  unsupported format or the 1500 ms local guard cancels the attempt. Home then
  keeps its normal background.
- **One worker.** A capture that outlives the guard keeps its worker slot until
  it finishes, then drops its frame and descriptor at once. A drop guard frees
  the slot even if the worker panics. Any request that arrives meanwhile is cancelled at once as
  `busy`, so a stalled or grabbed server never accumulates threads or X
  connections.
- **Lifetime.** Each request is handled once. A failed `Get` (an older daemon,
  an exhausted epoch) is logged and waits for the next `Changed`. The task
  never ends the adapter session.
- **Diagnostics.** Normalized `background-capture` lines with an outcome and
  `duration_ms` are the hook for the G-5.2 report.
- **Tests.**
  - Unit tests: pixel conversion, bounds and sealing.
  - Xvfb tests: the fixture acts as a manual compositing manager. One test
    captures a composited window, and confirms no capture without a compositor
    or for an unmapped window. Two more run the whole task against a fake
    daemon and screensaver on a private bus. One covers publication of the
    exact window, and cancellation when locked or for a target that is not a
    current window. The other holds a server grab: the stalled attempt is
    cancelled at the deadline, the next one is cancelled at once without a
    second capture, and a request after the release is published.

  Another Xvfb test declines a container shared by two clients and a virtual
  root, while capturing a frame that holds its client alone.

  They do not qualify xfwm's compositor, DPI or colour on the TV.

## On-screen keyboard

The adapter is the X11 deliverer of the global on-screen keyboard
([ADR-0105](../adr/0105-global-on-screen-keyboard-platform-text-delivery.md),
[transport](external-keyboard.md)). A keyboard worker (`keyboard.rs`) runs on
its own thread with its own X connection; `keyboard_lane.rs` connects it with
`KeyboardBridge1`. A daemon without the bridge leaves the keyboard off and the
rest of the adapter unchanged.

- **The key.** The worker grabs `keyboard_shortcut` (read-only from
  `config.toml`, default Meta+F10) and XF86Launch1 on the root window, with the
  lock-modifier variants. Launch1 is the remote's microphone after the packaged
  hwdb remap. A key that is also a Home or quick-menu key is left out (Home
  and the quick menu win, ADR-0106), and a key another client holds is
  skipped. A press requests a session only after an explicit
  unlocked ScreenSaver reply.
- **Open.** The target is `_NET_ACTIVE_WINDOW`. It must be listed in
  `_NET_CLIENT_LIST`, of normal type and not the shell; otherwise the session is
  `unavailable`. The worker then passively grabs Up, Down, Left, Right, Return,
  KP_Enter, Escape and XF86Back, unmodified and with lock variants. The session
  opens only when all four directions, Return or KP_Enter, and Escape or
  XF86Back are grabbed; otherwise every grab is released and the session is
  `unavailable`. Focus stays with the application; every other key, Backspace
  and hardware typing included, reaches it.
- **Delivery.** One character or editing key at a time. Before each one the
  lane needs an explicit unlocked reply (a screen locker's keyboard grab is
  invisible to the focus check), and the worker checks that the target still
  has focus: it is `_NET_ACTIVE_WINDOW`, and the X input focus
  (`GetInputFocus`) is the target or one of its descendants, never `None` or
  `PointerRoot`. The first failed check drops the rest of the record and ends
  the session (`locked` or `focus`).
  - Each character is typed through one of up to eight scratch keycodes (keycodes without
    keysyms), mapped to its keysym in every column: Latin-1 directly,
    otherwise `0x01000000 + code point`.
  - A keycode that already carries the keysym is reused; otherwise the next one in rotation is remapped.
  - The mappings are restored when the session ends.
- **End.** A change of `_NET_ACTIVE_WINDOW`, a destroyed target, a lock or an
  unknown lock state ends the session within 500 ms. The worker releases its
  grabs and reports the reason. Nothing typed is logged.

The shell's panel window is override-redirect, so it is never in
`_NET_CLIENT_LIST`, never a shell candidate and never focused.

Multi-output placement, real lock/suspend behavior and the transparent overlay
on actual compositors remain qualification work. No AC or roadmap gate is
marked passed.

## Opt-in setup

The package installs the EWMH executable, `arinn-ewmh.service` and
`arinn-ewmh-session.target`; it enables none of them. Select the backend in the
systemd user-manager environment **before starting the daemon**. For a manual
trial inside an X11 graphical session:

```sh
systemctl --user stop arinn-session.target arinn-ewmh-session.target
systemctl --user import-environment DISPLAY XAUTHORITY
systemctl --user set-environment XDG_SESSION_TYPE=x11 ARINN_PLATFORM_BACKEND=ewmh ARINN_SESSION_BACKEND=x11
systemctl --user unset-environment ARINN_SESSION_ENTRY
systemctl --user start arinn-ewmh-session.target
```

Clearing `ARINN_SESSION_ENTRY` drops a dedicated-session marker a killed
`arinn-session` may have left in a lingering user manager, so a manual trial is
never taken for the dedicated Arinn XFCE session ([ADR-0111](../adr/0111-power-key.md)).

The two session targets conflict. Shared daemon and shell units stop with either
target. A daemon started separately must be stopped before changing selection.
Stock XFCE may not activate `graphical-session.target`, and LightDM may not import
`XDG_SESSION_TYPE`; the manual recipe explicitly supplies the session type, and the EWMH
target pulls the graphical target through its dependency. The graphical target
has `RefuseManualStart=yes`, so do not start it directly. Run it only from
inside the intended X11 session. Stop `arinn-ewmh-session.target` before logging
out of a desktop that does not manage the graphical target. Enabling the EWMH
target alone is insufficient for persistent XFCE startup. To return to KDE/Wayland, stop the EWMH target, unset the two
Arinn environment variables, import the graphical session environment again,
and start the existing Arinn session target.

### Persistent XFCE/X11 startup

The package installs `arinn-xfce-session` and an XDG autostart **template** under
`/usr/share/arinn/autostart/`. It installs no global autostart entry and enables
no units (ADR-0042). Opt in once as the intended desktop user:

```sh
mkdir -p "${XDG_CONFIG_HOME:-$HOME/.config}/autostart"
cp /usr/share/arinn/autostart/org.arinn.XfceSession.desktop \
  "${XDG_CONFIG_HOME:-$HOME/.config}/autostart/"
```

Log out and log back into XFCE/X11. The entry runs only in XFCE. The helper
requires `DISPLAY` and the XFCE session manager's D-Bus owner; it rejects an
explicit non-X11 session type. LightDM's missing session type is supplied as
`x11`. Before starting the target it imports `DISPLAY` and, when present,
`XAUTHORITY`; absent authority clears a stale manager value so Xlib can use its
default. It sets the two Arinn backend selectors before starting the shared
daemon. The existing shell startup path requests Home (AC-01).

The helper remains in the desktop autostart session, owning
`org.arinn.XfceSession` to refuse duplicate launchers. It subscribes to XFCE
owner changes before reading the initial owner. XFCE owner loss/replacement,
session-bus loss, SIGTERM or SIGINT stops the EWMH target, including a partial
failed startup, and clears the imported session/backend environment. Systemctl
commands have a 30-second deadline. No window titles, display names, authority
paths or raw bus errors are logged.

One graphical session per Unix user is supported: systemd services and manager
environment are shared. The helper refuses an already running Arinn session,
daemon, shell or EWMH adapter without stopping it. Disable other Arinn autostart
targets before choosing this path; do not start another session concurrently or
configure this helper itself as a lingering systemd user service. Other desktop
clients must import their own environment on their next login.

To disable future startup, or before removing the package:

```sh
rm "${XDG_CONFIG_HOME:-$HOME/.config}/autostart/org.arinn.XfceSession.desktop"
```

Then log out to stop the current integration. Package removal does not edit
per-user configuration. A user copy survives removal; `TryExec` prevents it
from running when the helper is absent. Real XFCE login, Home visibility, logout
and repeated login qualification remains **not run** and needs owner permission.

## Automated verification

```sh
cargo test -p arinn-platform-x11
ARINN_XFCE_SESSION="$PWD/target/debug/arinn-xfce-session" python tests/package/test_xfce_session.py
ARINN_XVFB=/absolute/path/to/Xvfb cargo test -p arinn-platform-x11 -- --include-ignored
ARINN_PLATFORM_BACKEND=ewmh cargo test -p arinnd selected_adapter_owner_excludes_the_other_backend
```

The keyboard's ignored tests spawn their own Xvfb too. They check that:

- the key reaches the worker and not the application;
- XTest types ASCII and Cyrillic with an English keymap;
- the panel keys are grabbed while other keys pass;
- the shell and unlisted windows are never targets;
- focus loss and destruction stop delivery, also when only the X input focus moves away;
- a session without every direction, a Select and a Back key does not open and keeps no grab;
- through the lane, with a fake daemon and lock service:
  - a locked key requests nothing, a session opens and types, and a lock ends it;
  - each character, and each editing key, waits for its own lock check, without the 500 ms tick;
  - the worker ends a session whose target loses focus (`focus`), types nothing more and gives the panel keys back.

The five ignored native tests each spawn their own Xvfb using `-displayfd`, create
synthetic EWMH metadata and clients, and never read the inherited `DISPLAY`.
They verify inventory, typed identity, activation/close and Home-key dispatch,
manager invalidation, Home/Return and video/compositor/lock lifecycle. They include partial-presentation rollback and compositor-loss regressions and do
not substitute for real-desktop operator tests, which require owner permission.

The Arch Qt CI job installs Xvfb and runs the native tests with `--include-ignored`.
The installed-package session check also runs the XFCE helper suite on a private
D-Bus with a synthetic session owner and stub systemctl. It checks login ordering,
missing LightDM variables, logout, signals, duplicate refusal, failed-start
cleanup and refusal of incompatible/pre-existing sessions. No real user manager
or desktop is contacted.
Private-bus adapter and Qt tests cover all three lock endpoints, priority, signals
and loss of the lock service.

Inventory revisions compare only serialized window fields. Media identity reports
are independent and reuse the current inventory revision on lock/unlock or
launch-ID changes, avoiding the daemon's identical-inventory conflict.

When a presented player leaves the inventory, restoration uses its previous row
before replacing the snapshot. Native launch identity is revalidated before any
state change. Destroyed or changed-identity players retire without changing
another window; restoration failure for the exact live player remains observable.

Shutdown and reconnect cleanup use the same live-identity restoration before
replacing the inventory snapshot, and report restoration failures explicitly.
