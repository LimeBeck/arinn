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
random UUID tokens. Properties are type checked and bounded (4,096 clients,
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
before the first Home, as observed on live xfwm4 (#283). Startup Home (AC-01)
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
shell's mask is selected again on each snapshot. Output names are currently empty: this
initial backend does not implement RandR output placement.

## Home, Return and video

Meta+F12 uses a passive root key grab, including Caps/Num/Scroll lock modifier
combinations. The shortcut derives Super modifiers from the keyboard/modifier
mapping, falling back to native Meta only if no mapped Super exists; it does not
assume Mod4. Keyboard/modifier mapping changes rebuild the grabs. An existing grab
conflict makes the backend unavailable. Home snapshots the exact opaque target
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

XComposite activity background remains the separate work in #149. Multi-output
placement, real lock/suspend behavior and the transparent overlay on actual
compositors remain qualification work. No AC or roadmap gate is marked passed.

## Opt-in setup

The package installs the EWMH executable, `arinn-ewmh.service` and
`arinn-ewmh-session.target`; it enables none of them. Select the backend in the
systemd user-manager environment **before starting the daemon**. For a manual
trial inside an X11 graphical session:

```sh
systemctl --user stop arinn-session.target arinn-ewmh-session.target
systemctl --user import-environment DISPLAY XAUTHORITY
systemctl --user set-environment XDG_SESSION_TYPE=x11 ARINN_PLATFORM_BACKEND=ewmh ARINN_SESSION_BACKEND=x11
systemctl --user start arinn-ewmh-session.target
```

The two session targets conflict. Shared daemon and shell units stop with either
target. A daemon started separately must be stopped before changing selection.
Stock XFCE may not activate `graphical-session.target`, and LightDM may not import
`XDG_SESSION_TYPE`; the manual recipe explicitly supplies the session type, and the EWMH
target pulls the graphical target through its dependency. The graphical target
has `RefuseManualStart=yes`, so do not start it directly. Run it only from
inside the intended X11 session. Stop `arinn-ewmh-session.target` before logging
out of a desktop that does not manage the graphical target. Enabling the EWMH
target alone is insufficient for persistent XFCE startup; persistent session
integration remains distribution-specific qualification work. To return to KDE/Wayland, stop the EWMH target, unset the two
Arinn environment variables, import the graphical session environment again,
and start the existing Arinn session target.

## Automated verification

```sh
cargo test -p arinn-platform-x11
ARINN_XVFB=/absolute/path/to/Xvfb cargo test -p arinn-platform-x11 -- --include-ignored
ARINN_PLATFORM_BACKEND=ewmh cargo test -p arinnd selected_adapter_owner_excludes_the_other_backend
```

The five ignored native tests each spawn their own Xvfb using `-displayfd`, create
synthetic EWMH metadata and clients, and never read the inherited `DISPLAY`.
They verify inventory, typed identity, activation/close and Home-key dispatch,
manager invalidation, Home/Return and video/compositor/lock lifecycle. They include partial-presentation rollback and compositor-loss regressions and do
not substitute for real-desktop operator tests, which require owner permission.

The Arch Qt CI job installs Xvfb and runs the native tests with `--include-ignored`.
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
