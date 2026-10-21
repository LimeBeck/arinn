<!-- SPDX-FileCopyrightText: 2026 Arinn contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Global on-screen keyboard transport

The on-screen keyboard also types into the focused external application
(specification §6.2, §12.5;
[ADR-0105](../adr/0105-global-on-screen-keyboard-platform-text-delivery.md),
[#434](https://github.com/LimeBeck/arinn/issues/434)). The shell draws the
panel ([UI keyboard](ui-keyboard.md)). The daemon routes. A trusted platform
deliverer binds the target window, captures the panel's keys and delivers
the text. This document is the wire contract of the two internal interfaces.
Neither is a plugin capability.

- **[ExternalKeyboard1](../plugin-api/external-keyboard1.xml):** shell-facing.
- **[KeyboardBridge1](../plugin-api/keyboard-bridge1.xml):** adapter-facing.

Both live at `/org/arinn/Daemon1` on `org.arinn.Daemon1`. The types and
validation are in `arinn_protocol::keyboard`.

## Session lifecycle

```text
key pressed ─▶ Request() ─▶ [open, S] to the deliverer ─▶ Opened(S) ─▶ ExternalKeyboard1.Opened(S)
                                                                  │
   Type/Press(S) ─▶ [type|press, S, …] ─▶ deliverer checks lock and target, then delivers
   deliverer key  ─▶ Input(S, key) ─▶ ExternalKeyboard1.Input(S, key)
                                                                  │
end: Close(S) from the shell, Request() again, Closed(S, reason) from the deliverer,
     no Opened within 3 s, no Next for 5 s, a new deliverer ─▶ ExternalKeyboard1.Closed(S)
```

- **One session at a time.** S is a fresh non-nil UUID. A second `Request()` while a session exists ends it: the key toggles the panel. A session ended by the shell or by `Request()` is sent to the deliverer as `[close, S]`.
- **Shell signals.** The shell sees only `Opened`, `Input` and `Closed` for the current S. A `Closed` for another session is stale and ignored. On a daemon owner change the shell hides the panel.
- **Errors.** `Type`, `Press` and `Close` for anything but the current opened session return `Conflict`. A session not yet opened counts as not current: the shell learns S only from `Opened`, and a session that never opens ends after 3 s.
  - `Close` of an ended session is not an error for the shell to report: the panel is already hiding.
  - `Type` and `Press` return `Unavailable` when 64 records already wait.
- **The deliverer's lane.**
  - `Next(generation)` waits at most 1 s and returns one record. The records are `[]` (idle), `[open, S]`, `[type, S, text]`, `[press, S, key]` and `[close, S]`. They are fixed records, never a command line.
  - A poll refreshes the deliverer's heartbeat.
  - At most one poll may be pending per deliverer; another returns `Conflict`.
- **The request.** `Request()` from a caller that is not the current platform owner returns `PermissionDenied`. Without a connected deliverer it returns `Unavailable`, and no session starts.
- **Deliverer loss.** `Connect(instance)` takes a fresh non-nil UUID in canonical form (lowercase, hyphenated) (anything else is `InvalidArgument`) and returns a new generation, and a new instance replaces the previous deliverer. Replacement, 5 s without a poll, or the loss of the deliverer's bus name ends the current session. Calls with an old generation return `Conflict`.
- **Limits.**
  - Text: 1–256 UTF-8 bytes and no control characters (`char::is_control`, Unicode category Cc). Tab, newline and carriage return are therefore never text; Enter is the `return` key. Other separators and format characters (U+2028, U+2029, category Cf) are not rejected: the shell's panel never produces them, and a deliverer types them as characters.
  - Input keys: `up`, `down`, `left`, `right`, `select`, `back`.
  - Press keys: `backspace`, `return`, `left`, `right`.
  - Session IDs, generations and instances: non-nil UUIDs in canonical form only (lowercase, hyphenated); uppercase, braced, URN and unhyphenated spellings are `InvalidArgument`.
  - Close reasons: `focus` (the target lost focus or disappeared while the session is unlocked), `locked` (locked or unknown lock state, whatever else happened), `unavailable` (cannot bind the target or capture the panel keys).

## Authorization

| Caller | Allowed members |
| --- | --- |
| Current owner of the selected platform name (`org.kde.KWin` or `org.arinn.X11`, `ARINN_PLATFORM_BACKEND`) | `Request` |
| The deliverer: on EWMH the owner of `org.arinn.X11`; on KDE the owner of `org.arinn.InputMethod` whose process is a direct child of the `org.kde.KWin` owner's process | `Connect`, `Next`, `Opened`, `Input`, `Closed` |
| Any other connection except a registered provider's, the platform adapter's and the deliverer's | `ExternalKeyboard1.Type`, `Press`, `Close` |

- Other callers receive `PermissionDenied`.
- The rules are per bus connection, as for `SystemControl1`. A provider's registered connection can never type; a plugin process that opens another connection is a same-user process like any other (§13).
- Same-user processes stay inside the existing trust boundary (§13). The session ID protects against stale and foreign sessions; it does not authenticate the shell.

## Privacy

- **Text in transit only.** Typed text exists in the shell's call, in at most 64 queued records, and in the deliverer's single delivery. Nothing stores it.
- **Logs.** Records name only the operation (`external-keyboard`), the outcome (`opened`, `closed`, `rejected`) and a fixed reason. They never contain the text, the key, window identity or titles.
- **Sentry.** The daemon's Sentry traces have no spans or breadcrumbs for these members (§14.4).
- **The shell** keeps no history of external input. The suggestion row is empty, and the per-field memory of ADR-0103 (language, last key) never stores text.

## Delivery

The deliverer delivers only into the target bound at `[open, S]`. Before every record, and on X11 before every character, it needs an explicit unlocked reply from the lock service; a locked or unknown state ends the session (`locked`). As soon as the target loses focus, nothing more is delivered, also within one `type` record. Each platform is described in detail in ADR-0105:

- **X11/EWMH:** `arinn-platform-x11`.
  - The target is `_NET_ACTIVE_WINDOW`.
  - Passive root grabs capture the panel keys; focus stays with the application. The session opens only when all four directions, one Select key (Return or KP_Enter) and one Back key (Escape or XF86Back) are grabbed; otherwise it is `unavailable`.
  - XTest types every character through one of up to eight scratch keycodes mapped to its keysym, independent of the layout; they are emptied when the session ends.
  - Before each character and each editing key the worker checks two things. The lock: an explicit unlocked reply; a locker's keyboard grab is invisible to `GetInputFocus`. The focus: `_NET_ACTIVE_WINDOW` is still the target, and the X input focus (`GetInputFocus`) is the target or one of its descendants. At the first failed check it drops the rest of the record and ends the session (`locked` or `focus`).
  - The session ends on focus change or destruction of the target.
- **KDE Plasma 6/Wayland:** `arinn-input-method`, the input method KWin starts.
  - `forceActivate()` binds the context of the focused surface.
  - A keyboard grab captures the panel keys and forwards all other keys to the application.
  - `commit_string` and `keysym` deliver.
  - KWin's `deactivate` ends the session at once; the helper reports `locked` unless the lock service explicitly answers unlocked, then `focus`.

## KDE Plasma 6/Wayland

The pieces:

- **The KWin script** registers `ArinnKeyboard`. The daemon writes the keys to it through KGlobalAccel (`platform/kde/global_home.rs`).
  - The script calls `Request()` only after an explicit unlocked ScreenSaver reply, over an active normal application window, with Home hidden.
  - It excludes the panel window (shell identity and title `org.arinn.KeyboardPanel`) from the shell, the inventory and the Return target.
  - It places the panel along the bottom of the active output and keeps it above.
- **The daemon's KDE code** (`platform/kde/keyboard_config.rs`) runs on each new KWin adapter connection:
  - it sets `kwinrc [Wayland] InputMethod` to `org.arinn.InputMethod.desktop` only when the entry is empty, and sends KConfig's change notification;
  - it keeps the `arinn-keyboard-panel` window rule in `kwinrulesrc` (accept focus, above, no border and the skip flags, all forced) and asks KWin to reconfigure;
  - it never writes a file with immutable markers;
  - it logs `Conflict` when another input method is chosen.
- **`arinn-input-method`** (`platform/kde/input-method`) is what KWin starts. A pure state machine (`machine.rs`) decides; the I/O layer carries out its decisions.
  - `[open, S]`: lock check, `forceActivate()`, wait 1 s for the context, `grab_keyboard`, `Opened`.
  - Panel keys (evdev 103, 108, 105, 106, 28, 96, 352, 353, 1, 158) become `Input` on press; held arrows repeat at the grab's rate.
  - Every other key is forwarded with `key`, and modifiers with `modifiers`.
  - Text goes through `commit_string` with the context's latest `commit_state` serial; editing keys through `keysym`.
  - A context deactivated by KWin ends the session (`focus`). A lock (checked before each delivery and every 500 ms) ends it (`locked`). Closing releases the context with `VirtualKeyboard.active = false`.
  - A context without a session (an application's own text-input) shows nothing and forwards every key.

## Verification status

- **Contract.**
  - `arinn-protocol` validation tests;
  - live introspection of both interfaces against their XML (`arinn-protocol` fixture and the daemon export).
- **Daemon** (`runtime::keyboard_routing::tests`, private buses):
  - lifecycle and toggle;
  - the trust rules, including the KDE child-process check and provider callers;
  - the open timeout, the poll timeout and deliverer replacement;
  - a vanished deliverer;
  - the queue bound and concurrent polls;
  - no typed text or key name in log records.
- **KDE configuration** (`keyboard_config` tests) and KGlobalAccel writes (`global_home` tests, `adapter_connection_applies_configured_home_shortcut`).
- **X11/EWMH:** see [x11-ewmh.md](x11-ewmh.md#on-screen-keyboard) (Xvfb tests).
- **KDE input method** (`arinn-input-method` tests): the state machine, and a scenario against a fake `zwp_input_method_v1` compositor, a fake `VirtualKeyboard`, lock service and daemon:
  - no auto-show;
  - force, grab and Opened;
  - panel keys and forwarding;
  - commit with the serial, and keysyms;
  - a lock ends the session and refuses a new one;
  - focus loss;
  - close releases the context.
- **KWin script:** Node tests.
- **Shell:** see [ui-keyboard.md](ui-keyboard.md#verification).
- **Not verified here.** No KWin, no real compositor and no hardware are available here. These need the owner's operator test on the HTPC (G-5.9):
  - KWin 6 starting the input method from `kwinrc`;
  - `forceActivate` and the keyboard grab with Chrome;
  - the window rule keeping the panel unfocused, and the script's placement (it assigns `frameGeometry` from a plain object);
  - commits in Chrome with and without its Wayland IME;
  - XFCE with a real window manager;
  - the hwdb remap of the microphone.
