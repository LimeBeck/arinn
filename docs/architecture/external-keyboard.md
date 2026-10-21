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
- **Errors.** `Type`, `Press` and `Close` for anything but the current opened session return `Conflict`.
  - `Close` of an ended session is not an error for the shell to report: the panel is already hiding.
  - `Type` and `Press` return `Unavailable` when 64 records already wait.
- **The deliverer's lane.**
  - `Next(generation)` waits at most 1 s and returns one record. The records are `[]` (idle), `[open, S]`, `[type, S, text]`, `[press, S, key]` and `[close, S]`. They are fixed records, never a command line.
  - A poll refreshes the deliverer's heartbeat.
  - At most one poll may be pending per deliverer; another returns `Conflict`.
- **Deliverer loss.** `Connect(instance)` returns a new generation, and a new instance replaces the previous deliverer. Replacement, 5 s without a poll, or the loss of the deliverer's bus name ends the current session. Calls with an old generation return `Conflict`.
- **Limits.**
  - Text: 1–256 UTF-8 bytes and no control characters (`char::is_control`). Line breaks and tabs are keys, never text.
  - Input keys: `up`, `down`, `left`, `right`, `select`, `back`.
  - Press keys: `backspace`, `return`, `left`, `right`.
  - Close reasons: `focus`, `locked`, `unavailable`.

## Authorization

| Caller | Allowed members |
| --- | --- |
| Current owner of the selected platform name (`org.kde.KWin` or `org.arinn.X11`, `ARINN_PLATFORM_BACKEND`) | `Request` |
| The deliverer: on EWMH the owner of `org.arinn.X11`; on KDE the owner of `org.arinn.InputMethod` whose process is a direct child of the `org.kde.KWin` owner's process | `Connect`, `Next`, `Opened`, `Input`, `Closed` |
| Any other client except registered providers and the platform adapter | `ExternalKeyboard1.Type`, `Press`, `Close` |

- Other callers receive `PermissionDenied`.
- Same-user processes stay inside the existing trust boundary (§13). The session ID protects against stale and foreign sessions; it does not authenticate the shell.

## Privacy

- **Text in transit only.** Typed text exists in the shell's call, in at most 64 queued records, and in the deliverer's single delivery. Nothing stores it.
- **Logs.** Records name only the operation (`external-keyboard`), the outcome (`opened`, `closed`, `rejected`) and a fixed reason. They never contain the text, the key, window identity or titles.
- **Sentry.** The daemon's Sentry traces have no spans or breadcrumbs for these members (§14.4).
- **The shell** keeps no history of external input. The suggestion row is empty, and the per-field memory of ADR-0103 (language, last key) never stores text.

## Delivery

The deliverer delivers only into the target bound at `[open, S]`. Before every delivery it needs an explicit unlocked reply from the lock service; a locked or unknown state ends the session (`locked`). Each platform is described in detail in ADR-0105:

- **X11/EWMH:** `arinn-platform-x11`.
  - The target is `_NET_ACTIVE_WINDOW`.
  - Passive root grabs capture the panel keys; focus stays with the application.
  - XTest types every character through a scratch keycode mapped to its keysym, independent of the layout.
  - The session ends on focus change or destruction of the target.
- **KDE Plasma 6/Wayland:** `arinn-input-method`, the input method KWin starts.
  - `forceActivate()` binds the context of the focused surface.
  - A keyboard grab captures the panel keys and forwards all other keys to the application.
  - `commit_string` and `keysym` deliver.
  - KWin's `deactivate` ends the session.

## Verification status

- **Implemented here:**
  - `arinn-protocol` validation tests;
  - a private-bus check that a fixture export matches both XML files.
- **Not implemented yet:** the daemon routing, the deliverers and the shell's external mode. They follow in the implementation of [#434](https://github.com/LimeBeck/arinn/issues/434), which records its own verification and the open operator test (G-5.9).
