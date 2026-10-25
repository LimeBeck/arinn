# ADR-0105: Global on-screen keyboard: platform text delivery

<!-- SPDX-FileCopyrightText: 2026 Arinn contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

- **Status:** Accepted
- **Date:** 2026-10-21
- **Deciders:** project owner for the product behaviour and the security posture ([#434](https://github.com/LimeBeck/arinn/issues/434)):
  - 2026-10-10: the keyboard becomes global;
  - 2026-10-11: a separate global key `keyboard_shortcut` owned by the platform layer, no auto-show, the recommended security posture, an empty suggestion row, option C (hybrid), part of the MVP;
  - 2026-10-21: the remote's microphone button is an additional default key;
  - 2026-10-22: the four points under *Owner decisions* accepted as proposed ([#480](https://github.com/LimeBeck/arinn/pull/480#issuecomment-6334717118)). The four points as proposed, the KDE deliverer trust rule among them, are listed in the [#434 status comment of 2026-10-21](https://github.com/LimeBeck/arinn/issues/434#issuecomment-6328896721).

  The rest of the mechanism is a delegated technical choice ([ADR-0037](0037-agent-autonomy-and-owner-gated-decisions.md)).
- **Amends / Supersedes:** amends [ADR-0103](0103-on-screen-keyboard.md): the keyboard is no longer shell-only. It stays one shell component, and the platform layer delivers its text to external applications.
- **Amended by / Superseded by:** —
- **Specification:** §5.7, §6.2, §8.5, §12.5, §13, §14.4; AC-02, AC-18; R-02, R-03, R-05

## Context

ADR-0103 made the on-screen keyboard a shell component that types only into shell fields. The owner wants the remote alone to type into external applications too: first Chrome (site search, login forms, YouTube search). The SteamOS keyboard summoned over any game is the model ([#434](https://github.com/LimeBeck/arinn/issues/434)).

Constraints:

- **Who may deliver input.** The shell never talks to third-party applications (R-03, §8.2), and plugins never supply UI or reach the platform (R-02, R-04). Delivering input into a foreign window is a new privilege. The owner limited it:
  - only the trusted platform adapter delivers;
  - only into the window that had focus when the panel opened;
  - the panel closes, and delivery stops, when that window loses focus or disappears;
  - nothing is delivered while the session is locked or its lock state is unknown (ADR-0041);
  - typed text is never logged, persisted or sent to diagnostics.
- **Wayland.** A client cannot send input to another client. On KDE Plasma 6 only the compositor can do it, through one of two channels:
  - the input method that KWin starts itself (`zwp_input_method_v1`);
  - fake input or libei, through the RemoteDesktop portal.

  What KWin 6 supports was checked against its sources (`src/inputmethod.cpp`, `src/wayland_server.cpp`, `src/plugins/eis`, branches Plasma/6.3 to master):
  - KWin implements `zwp_input_method_v1` and `zwp_input_panel_v1`, and only for the process it starts from `kwinrc [Wayland] InputMethod`. It does not implement `input-method-unstable-v2` or `zwp_virtual_keyboard_v1`.
  - `org.kde.kwin.VirtualKeyboard.forceActivate()` activates an input-method context even when the focused application has no text-input.
  - A context's keyboard grab receives every key after the global-shortcut filter, so global Home still works.
  - `commit_string` goes through text-input v2/v3 when the application enabled it. Otherwise KWin turns the string into key events. Since Plasma 6.6 this works for any keysym through a temporary keymap; 6.3–6.5 manage only characters of the active layout.
  - A focus change re-evaluates the context and deactivates it when the new window has no text-input.
  - `org_kde_kwin_fake_input` is restricted. libei text (`EIS_EVENT_TEXT_UTF8`) exists only on master (after Plasma 6.7).
- **X11/EWMH.** Any client can inject key events with XTest. The layout decides which keycode types which character.
- **Focus.** The panel must never take focus from the application. So the remote's arrows, OK and Back must reach the panel without the panel being focused.
- **The microphone.** The owner's remote (Geniatron G60S PRO BT, input ID bus 0x3, vendor 0x1915, product 0x1001) sends MSC_SCAN `c00cf` → `KEY_VOICECOMMAND` (582) for the microphone button. Neither the shell nor a global shortcut can use it:
  - on Wayland, xkb gives it the keysym XF86VoiceCommand, which Qt does not name (`key() == 0`), so KGlobalAccel cannot bind it;
  - X11 does not deliver evdev codes above 247 at all.

  ADR-0079 and ADR-0094 left udev hwdb remapping as the workaround.

## Decision

### Keys

- **`keyboard_shortcut`** in `config.toml` uses the syntax and the parser of `home_shortcut` (ADR-0094, `platform/key_sequence.rs`). Its proposed default is **`Meta+F10`**:
  - Meta+F12 is Home;
  - Meta+F11 is the quick menu (ADR-0106);
  - Plasma has no default binding on Meta+F10.
- **The microphone.** Each adapter also binds **Launch1** (Qt `Key_Launch1`, X11 `XF86Launch1`) as the microphone button, as HomePage is bound for Home.
- **Conflicts.** An invalid value falls back to the default with one warning. A keyboard key that is also a Home key or a quick-menu key (ADR-0106, including the remote's Menu keys) is left out, and the conflict is logged: Home and the quick menu win a shared key. An edited value applies when the daemon (KDE) or the X11 adapter restarts, as for Home.
- **The microphone remap.** The package installs `/usr/lib/udev/hwdb.d/70-arinn-remote.hwdb`:

  ```
  evdev:input:b0003v1915p1001*
   KEYBOARD_KEY_c00cf=prog1
  ```

  It maps the microphone's scan code to `KEY_PROG1` (148). xkeyboard-config names that key XF86Launch1 on both X11 and Wayland, and Qt names it Launch1. The entry matches only this vendor and product, and only this one usage code.
  - Arch's pacman hook rebuilds the hwdb database; the remote picks the change up when it reconnects or after `udevadm trigger`.
  - The owner's capture on the HTPC confirms or corrects the match.
- **Behaviour of the key.** It works only over an unlocked external application window. Inside the shell (Home visible, or the shell focused) it does nothing. A second press while the panel is open hides the panel.

### Roles and transport

```text
 shell (panel, never focused) ── org.arinn.ExternalKeyboard1 ──▶ arinnd ◀── org.arinn.KeyboardBridge1 ── deliverer
        ▲  Opened / Input / Closed                                  │            (X11 adapter or KDE input method)
        └──────────────────────────────────────────────────────────┘                    │ XTest / zwp_input_method_v1
                                                                                         ▼
                                                                          the window focused when the panel opened
```

- **The shell** draws the panel with the one keyboard component of ADR-0103, in an *external* mode:
  - the suggestion row is empty and there is no context row;
  - the main key is Enter;
  - language and last key are remembered per process, never the text.

  It sends every typed key to the daemon. It never sends input to an application.
- **The daemon** routes: one session at a time, a bounded queue, no storage. It never logs text. Its Sentry traces have no spans for these methods.
- **The deliverer** is part of the trusted platform adapter. It binds the target, captures the panel's keys while the panel is open, delivers text and reports the end of the session.
- **The request** comes from the platform adapter that owns the global key:
  - the KWin script on KDE;
  - `arinn-platform-x11` on X11/EWMH, which is also the deliverer there.

The two internal interfaces live at `/org/arinn/Daemon1` on `org.arinn.Daemon1`. Neither is a plugin capability.

**`org.arinn.ExternalKeyboard1`** (shell-facing; [external-keyboard1.xml](../plugin-api/external-keyboard1.xml)):

| Member | Meaning |
| --- | --- |
| `Opened(s session)` | The deliverer bound the target and captures the panel keys: show the panel |
| `Input(s session, s key)` | A captured panel key: `up`, `down`, `left`, `right`, `select`, `back` |
| `Closed(s session)` | The session ended (target lost, lock, key pressed again, deliverer gone): hide the panel |
| `Type(s session, s text)` | Deliver text: 1–256 UTF-8 bytes, no control characters (Unicode category Cc) |
| `Press(s session, s key)` | Deliver an editing key: `backspace`, `return`, `left`, `right` |
| `Close(s session)` | The shell hid the panel (Back, Home, the shell on screen or a lock) |

**`org.arinn.KeyboardBridge1`** (adapter-facing; [keyboard-bridge1.xml](../plugin-api/keyboard-bridge1.xml)):

| Member | Caller | Meaning |
| --- | --- | --- |
| `Request()` | platform owner | The keyboard key was pressed over an unlocked application window. Opens a session or, while one is open, closes it. Another caller receives `PermissionDenied`; without a connected deliverer the call returns `Unavailable`. |
| `Connect(s instance) → s generation` | deliverer | Attach a deliverer instance, named by a fresh canonical non-nil UUID (anything else is `InvalidArgument`); a new instance replaces the old one and ends its session. |
| `Next(s generation) → as` | deliverer | Waits at most 1 s. Returns one record: `[]` idle, `[open, S]`, `[type, S, text]`, `[press, S, key]` or `[close, S]`. |
| `Opened(s generation, s session)` | deliverer | The target is bound and the panel keys are captured. |
| `Input(s generation, s session, s key)` | deliverer | A captured panel key. |
| `Closed(s generation, s session, s reason)` | deliverer | `focus` (the target lost focus or disappeared while the session is unlocked), `locked` (the session is locked or its lock state is unknown, whatever else happened), `unavailable` (cannot bind or capture). |

Rules:

- **Sessions.** A session ID is a fresh UUID. A deliverer that does not confirm `Opened` within 3 s, or does not poll `Next` for 5 s, ends the session.
  - Late calls for an ended session or an old generation return `Conflict`.
  - At most 64 records wait for the deliverer; beyond that `Type` and `Press` return `Unavailable`.
  - `Type`, `Press` and `Close` before `Opened` also return `Conflict`: the shell learns a session only from `Opened`. A session that never opens ends by the 3 s rule.
- **Authentication.**
  - `Request` must come from the current owner of the selected platform name: `org.kde.KWin` or `org.arinn.X11` (`ARINN_PLATFORM_BACKEND`).
  - The deliverer must be:
    - on EWMH, the owner of `org.arinn.X11`;
    - on KDE, the owner of `org.arinn.InputMethod` whose process is a direct child of the `org.kde.KWin` owner's process. KWin starts the configured input method itself, so a process that merely claims the name is not trusted.
  - `ExternalKeyboard1` methods reject a caller whose connection is a registered provider's, the platform adapter's or the deliverer's (`PermissionDenied`). This is a rule per bus connection, as for `SystemControl1`: a provider's registered connection can never type. A plugin process that opens another connection is inside the trust model of §13, like any same-user process.
  - Other same-user processes are inside the existing trust boundary (§13). The session ID guards against stale and foreign sessions; it does not authenticate the shell.
- **No text in the daemon's state.** The daemon keeps a queued text only until the deliverer takes it. Logs carry the operation, the outcome and a fixed reason.

### X11/EWMH delivery (`arinn-platform-x11`)

A keyboard worker with its own X connection does the following:

1. **The key.** It grabs the keyboard keys on the root window, including lock-modifier variants, as for Home. A press is sent to the daemon as `Request()` only after an explicit unlocked ScreenSaver reply (the probe order of the EWMH adapter).
2. **Open.** The target is `_NET_ACTIVE_WINDOW`. It must be a listed normal client and not the shell; otherwise the worker reports `Closed(unavailable)`.
3. **Panel keys.** The worker passively grabs these keys on the root window: Up, Down, Left, Right, Return, KP_Enter, Escape and XF86Back.
   - Unmodified and with lock modifiers only.
   - A key another client holds is skipped. The session opens only when all four directions, at least one Select key (Return or KP_Enter) and at least one Back key (Escape or XF86Back) are grabbed; otherwise the worker releases its grabs and reports `unavailable`, so the remote can always move, select and leave.
   - Passive grabs leave focus with the application, so all other keys, Backspace and a hardware keyboard included, reach it.
   - The X server's auto-repeat repeats grabbed arrows.
4. **Delivery.** Before **each character** of a `type` record and before each editing key, the worker checks two things:
   - **The lock:** an explicit unlocked ScreenSaver reply. A screen locker usually holds an active keyboard grab, which `GetInputFocus` does not report, so the lock is checked per character and not only per record. Otherwise the rest of a record could reach the locker's password prompt.
   - **The focus:**
     - `_NET_ACTIVE_WINDOW` is still the target;
     - the X input focus (`GetInputFocus`) is the target or one of its descendants. XTest events follow the input focus, not `_NET_ACTIVE_WINDOW`; `PointerRoot` or `None` fails the check.

   At the first failed check the worker drops the rest of the record and delivers nothing more. It ends the session with `locked` when the lock check failed, otherwise with `focus` (the reason table applies).

   Each character, and each editing key, is typed through one of up to eight scratch keycodes, keycodes that have no keysym:
   - a scratch keycode that already carries the keysym is reused;
   - otherwise the next one in rotation is mapped to the keysym in every column (`ChangeKeyboardMapping`), so neither the layout nor Shift matters, and a client still reading the previous mapping never sees its character change under it;
   - the keysym is the Latin-1 value or `0x01000000 + code point`;
   - the worker sends an XTest press and release;
   - when the session ends, every scratch keycode gets its empty mapping back.

   This is how `xdotool type` handles characters missing from the layout; Cyrillic needs no layout switch.
5. **End.** A change of `_NET_ACTIVE_WINDOW` or of the input focus away from the target, the target's destruction, a lock or an unknown lock state ends the session. The worker reports it and releases its panel grabs.

The shell shows the panel in an **override-redirect** window with `WM_HINTS input = False` at the bottom of the screen:

- the window manager never focuses or lists it, so the shell's EWMH identity and the inventory are unchanged;
- it is above the application without a stacking request.

### KDE Plasma 6/Wayland delivery

- **The key.** The KWin script registers the action `ArinnKeyboard` ("Arinn: On-screen keyboard"). The daemon writes the keyboard keys to it through KGlobalAccel when the adapter connects, after Home and the quick menu, with the conflict checks of ADR-0094 and ADR-0106.
  - The callback requires an explicit unlocked ScreenSaver reply.
  - It also requires an active normal application window that is neither the shell nor the panel, and Home must be hidden.
  - Then it calls `Request()`.
- **The deliverer** is `arinn-input-method`, KWin's input method:
  - the package installs `/usr/lib/arinn/arinn-input-method` and `/usr/share/applications/org.arinn.InputMethod.desktop` with `X-KDE-Wayland-VirtualKeyboard=true`;
  - KWin starts it with `WAYLAND_SOCKET`;
  - it binds `zwp_input_method_v1`, owns `org.arinn.InputMethod` and connects to the daemon.
- **Open.** The helper calls `forceActivate()` on KWin's `org.kde.kwin.VirtualKeyboard`. It waits up to 1 s for `activate(context)`, then calls `grab_keyboard` on the context. The context belongs to the surface that has keyboard focus, which is the target. Without a context the session is `unavailable`.
- **Panel keys.** While a session is open, the grab delivers every key.
  - On press, the panel keys (evdev Up, Down, Left, Right, Enter, KP Enter, OK, Select, Esc, Back) go to the daemon.
  - Every other key and every modifier change goes back to the application through the context (`key`, `modifiers`), so Backspace and a hardware keyboard keep working.
  - Global shortcuts run before the grab, so Home, the keyboard key and the quick-menu key keep working.
- **Delivery.** The helper first checks for an explicit unlocked ScreenSaver reply.
  - `Type` uses `commit_string`. KWin passes it to an application with text-input, and as key events to one without (layout-independent since Plasma 6.6).
  - `Press` uses `keysym` (BackSpace, Return, Left, Right).
- **End.** KWin's `deactivate` (focus left the target, the target closed, the screen locked) ends the session. The helper then asks the lock service: an explicit unlocked reply gives `focus`; a locked, unknown or missing reply gives `locked`. It delivers nothing after `deactivate`, whatever the answer. Closing a session releases the grab and sets `VirtualKeyboard.active` to false.
- **The panel window** is a separate shell window titled `org.arinn.KeyboardPanel`. Two pieces of KDE configuration keep it from taking focus:
  - **A window rule.** The daemon's KDE platform code keeps one rule `arinn-keyboard-panel` in `kwinrulesrc`: window class `org.arinn.Shell`, exact title `org.arinn.KeyboardPanel`, *Accept focus: No* (forced). It then asks KWin to reconfigure. Only this group and the `[General]` `rules`/`count` entries are touched; a file that cannot be parsed safely is left alone.
  - **The KWin script.** It excludes that title from the shell identity and the inventory. It places the panel at the bottom of the shell's output, keeps it above and skips taskbar, pager and switcher.
- **The input method setting.** When `kwinrc [Wayland] InputMethod` is empty, the daemon sets it to the Arinn desktop file and sends KConfig's change notification, so KWin starts the helper. It does this **once**: a marker in Arinn's state directory records that it wrote the value, and it never writes it again. Plasma stores *None* by deleting the key, so a user who later picks *None* keeps it. The daemon never replaces another input method. Uninstalling Arinn reverses neither the value nor the window rule. Both are harmless: the rule matches only Arinn's panel, and a missing desktop file only turns KWin's input method off. In that case `Request()` reports `Unavailable`, the log names the reason, and the user can select *Arinn* under System Settings → Keyboard → Virtual Keyboard.
- **KDE/X11** (ADR-0077) has no input method and no XTest deliverer, so the key reports `Unavailable` there.

### Owner decisions

These points go beyond the owner's answers of 2026-10-11 and 2026-10-21 on #434. The owner accepted all four on 2026-10-22 ([#480](https://github.com/LimeBeck/arinn/pull/480#issuecomment-6334717118)):

1. **The default key** is `Meta+F10`, next to `home_shortcut` `Meta+F12` and `quick_menu_shortcut` `Meta+F11`. A second press hides the panel; inside the shell the key does nothing.
2. **The microphone** is the second default key, through the packaged udev hwdb entry. It changes what the button sends system-wide, for this vendor and product only, from `KEY_VOICECOMMAND` to `KEY_PROG1`. A configuration binding cannot do this: X11 drops the key and Qt cannot name it (ADR-0079).
3. **KDE configuration.** The daemon sets `kwinrc [Wayland] InputMethod` once, only when it is empty, and keeps one window rule for its own panel window.
4. **The trust rule for the KDE deliverer:** the owner of `org.arinn.InputMethod` must be a direct child of the KWin process.

## Consequences

- One keyboard component types everywhere; R-03 holds, because the shell only talks to the daemon.
- **Two internal interfaces and a new deliverer process on KDE.** `daemon1.xml` is unchanged.
- **Text is bounded but not filtered beyond Cc.** Tab, newline and carriage return are control characters, so they reach the application only as keys (`return`) or not at all. Other separators and format characters (U+2028, U+2029, category Cf such as bidi controls) pass as text. The shell's panel never produces them; a same-user caller that sends them is inside §13, and the deliverer types them as characters.
- **Wayland needs the application to have keyboard focus and KWin's input method.**
  - On Plasma 6.3–6.5 an application without text-input receives only characters of the active layout.
  - Chrome without its Wayland IME flag is such an application; Cyrillic there needs Plasma 6.6 or the IME flag.
- **XTest remaps up to eight unused keycodes during a session.** Clients see MappingNotify events, as with `xdotool type`, and the keycodes are emptied again when the session ends.
- **A panel that loses its rule takes focus on Wayland.** Then KWin deactivates the context and the session ends at once: fail closed, logged.
- The hwdb entry is the first system-wide file Arinn installs for an input device. Other remotes need their own entry or the configured key.
- Operator tests on the HTPC are still needed: the real KWin input-method path, Chrome on Wayland and on XFCE, and the hwdb remap.

## Alternatives considered

- **Key injection on Wayland (fake input, libei through the RemoteDesktop portal).** Rejected:
  - fake input is restricted;
  - the portal asks for consent and gives layout-dependent keycodes;
  - libei text exists only after Plasma 6.7;
  - none of them captures the remote's keys for the panel.
- **The panel as an input-panel surface of the input method.** The helper would have to draw the keyboard itself, or receive frames rendered by the shell. That means two renderers or a frame channel for one panel.
- **LayerShellQt for a non-focusable panel.** It uses private QtWayland API. The package is built against the Qt floor (ADR-0104) and runs on a newer Qt, so the private ABI would not match.
- **Automatic show on text-field focus.** The owner rejected it on 2026-10-11.
- **A global grab of the whole keyboard on X11.** It would also take Backspace and hardware typing from the application, and an XTest event would hit Arinn's own grab.
- **Matching characters to existing keycodes on X11.** It depends on the active XKB group and on Shift, so it fails for Cyrillic on an English layout. The scratch keycode does not.
- **Registering the microphone directly.** Not possible: Qt has no name for XF86VoiceCommand, and X11 drops the code.

## Verification

- `arinn-protocol` tests: bounds of text, keys, reasons and `Next` records; live introspection against the two XML files.
- `arinnd` private-bus tests:
  - session lifecycle;
  - toggle;
  - the 3 s open timeout and the 5 s poll timeout;
  - deliverer replacement;
  - provider and adapter callers rejected;
  - the queue bound;
  - no text in logs;
  - the KDE deliverer trust rule;
  - the kwinrc/kwinrulesrc writes;
  - the keyboard keys written to KGlobalAccel.
- `arinn-platform-x11` Xvfb tests:
  - the key requests a session only while unlocked;
  - grabbed panel keys reach the daemon while focus stays on the target;
  - XTest types ASCII and Cyrillic into a focused test window with an English keymap;
  - focus loss and lock end the session and stop delivery.
- `arinn-input-method` tests against a fake compositor and a fake KWin: activation, grab, key routing, forwarding, commit and deactivation.
- Qt UI tests:
  - the external mode (empty suggestions, Enter, typing through the daemon);
  - Back, Home and Menu with the panel open;
  - the panel window never becoming the focus window.
- Node tests of the KWin script: the key and the panel's exclusion from shell identity and inventory.
- Not run here: the operator test on the HTPC — a YouTube search in managed Chrome with the remote alone, on KDE/Wayland and on XFCE (G-5.9). It needs the owner's permission.
