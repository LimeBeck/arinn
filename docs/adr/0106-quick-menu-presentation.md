# ADR-0106: Quick menu key and presentation request

<!-- SPDX-FileCopyrightText: 2026 Arinn contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

- **Status:** Accepted
- **Date:** 2026-10-21
- **Deciders:** project owner for the product behaviour (2026-10-10, [#439](https://github.com/LimeBeck/arinn/issues/439): option 2, a quick menu with its own configurable global key, `quick_menu_shortcut`, owned by the platform layer; 2026-10-21, [#439](https://github.com/LimeBeck/arinn/issues/439#issuecomment-6312358857): the remote's ≡ Menu button opens the quick menu outside the shell, Menu keeps its meaning inside the shell, and `Meta+F11` is the default); the mechanism is a delegated technical choice ([ADR-0037](0037-agent-autonomy-and-owner-gated-decisions.md))
- **Amends / Supersedes:** —
- **Amended by / Superseded by:** —
- **Specification:** [§5.7](../specifications/arinn-specification.md#57-system-ui-power-and-ambient), [§6.2](../specifications/arinn-specification.md#62-keys), [§6.3](../specifications/arinn-specification.md#63-home-and-back), [§12.1](../specifications/arinn-specification.md#121-global-home)

## Context

The owner added a quick menu to the MVP (#439): a second global key opens a side sheet over the current Activity, without the fullscreen Home. Back closes the sheet and activates the same Activity, as Back from Home does (§6.3).

The platform side of that key is the platform side of global Home. The KWin script and the X11/EWMH adapter remember the Return Activity and its output, raise the shell and report the state to the daemon. An explicit Home press goes through `Daemon1.ReportHomeRequest`, which commits that state, emits `Changed` and then `HomeRequested(epoch, target, output)` ([ADR-0069](0069-explicit-home-presentation-requests.md)). The shell cannot tell a quick-menu press from a Home press in `Status` or `Changed`, and it must not infer intent from them. It needs its own explicit request.

The Home keys are configurable through `home_shortcut` in `config.toml` ([ADR-0094](0094-configurable-global-home-keys.md)). The owner asked for the same for the quick menu.

## Decision

**Contract.** `org.arinn.Daemon1` gains two members; every existing signature stays:

- `ReportQuickMenuRequest(target: s, output: s)`. It uses the authentication and validation of `ReportHomeRequest`: only the current platform adapter may call it, and both strings follow the ordinary-text bounds. It commits the same state as `ReportHomeRequest` (Home visible with that Return target and output), emits `Changed`, then emits one `QuickMenuRequested(epoch: s, target: s, output: s)`, also for identical repeats. After the signal it arms the Activity background capture exactly as `ReportHomeRequest` does ([ADR-0087](0087-bounded-activity-background-frames.md), §12.3), so Home opened from the quick menu shows the Activity's last frame. A rejected call changes no state, emits nothing and arms nothing.
- `QuickMenuRequested` is a transient presentation intent, like `HomeRequested`: it carries the daemon epoch and the request's opaque Return target and output, and it is never persisted, replayed to a late subscriber or synthesized from a state report. The two requests never emit each other's signal.

`Status` is unchanged. While the quick menu is open the platform state is the Home state: the shell is raised over the Activity and the Return target is remembered. Whether the shell shows Home or the quick menu is shell-local state.

**Keys.** `config.toml` gains `quick_menu_shortcut` (default `Meta+F11`), in the syntax of `home_shortcut` (`platform/key_sequence.rs`). An empty or invalid value means the default, with one warning that leaves out the value. The remote's Menu key follows it, as HomePage follows `home_shortcut`. Which code the remote (G60S PRO BT) sends for ≡ is not captured yet, so both are bound: Qt `Key_Menu` (evdev `KEY_COMPOSE`, X11 `Menu`) and Qt `Key_MenuKB` (evdev `KEY_MENU`, X11 `XF86MenuKB`). A quick-menu sequence that is also a Home key is left out, so Home wins.

**Menu inside the shell.** Menu toggles global navigation inside the shell, and a long press opens the tile action sheet (spec §6.2). A global grab would take it from the shell, so the adapters bind the two Menu keys only while a window other than the shell has focus. The configured key stays bound everywhere and opens the quick menu over the shell too.

**KDE.** The KWin script registers a second action, `ArinnQuickMenu` ("Arinn: Quick menu"). Its callback runs the Home path (unlocked-session check, remember the Return Activity and output, raise the shell) and reports through `ReportQuickMenuRequest` instead of `ReportHomeRequest`. When an adapter connection first polls `TakeCommand`, the daemon writes the keys to that action through KGlobalAccel, after the Home keys, with the checks of ADR-0094: a key held by another action is left out, and with no free key the stored keys stay. A missing action is logged as `NotFound`; the Home keys are unaffected. The daemon follows the adapter's `ReportWindow`: when the shell's window (`org.arinn.Shell`, or `arinn-shell` under KDE/X11) gains or loses focus, it writes the action's keys again, without or with the Menu keys. Only changes are written, and the latest focus always wins.

**X11/EWMH.** `arinn-platform-x11` reads `quick_menu_shortcut` read-only from `config.toml` and grabs its sequences next to the Home sequences. When a snapshot finds the shell active, it grabs again without the Menu keys, and with them once another window is active. A press reports `ReportQuickMenuRequest` after the same unlocked check as Home. A quick-menu sequence that is not mapped or is held by another client is skipped and logged. Unlike Home, a quick-menu key that cannot be grabbed never stops the adapter.

**Shell.** The shell subscribes to `QuickMenuRequested` before it reads `Status`, and handles a request only for the current daemon owner and epoch. It opens the quick menu over what it shows. Over an Activity the fullscreen Home stays hidden. Back uses the existing `Daemon1.Back` Return path. An explicit `HomeRequested` while the sheet is open replaces it with Home. Media and volume keys outside the shell are not registered (spec §5.7, OQ-04).

**Applying a change.** An edited `quick_menu_shortcut` applies when the daemon (KDE) or the X11 adapter restarts, as for `home_shortcut`.

## Consequences

- The quick menu reuses the whole Home presentation path: lock check, Return target, output, Back and the Activity background. Only the request that names the intent is new.
- An old shell ignores `QuickMenuRequested`. With a new adapter the quick-menu key then shows the shell with Home, the state it reports. An old daemon answers `UnknownMethod`, and the KWin script, whose error callbacks KWin drops, shows nothing. The adapter, daemon and shell are installed together.
- The remote's Menu key no longer reaches other applications, for example as a context-menu key. On KDE, focus changes cost a few KGlobalAccel calls; on X11, the grabs follow focus at the adapter's next snapshot, up to 0.5 s later.
- Users can move the key in `config.toml`. A change made in KDE's Shortcuts settings is replaced the next time the adapter connects.
- A key Qt cannot name still cannot be a global shortcut; udev hwdb remapping remains the workaround (ADR-0094).

## Alternatives considered

- **A presentation kind argument on a new generic method** (`ReportPresentationRequest(kind, target, output)` → `PresentationRequested(epoch, kind, target, output)`). More general, but it needs a kind vocabulary, validation of unknown kinds and a rule for `kind = "home"` next to the existing `HomeRequested`. Two named members are explicit and old consumers ignore them. Reconsidered on 2026-10-23 for the remote's power key ([#481](https://github.com/LimeBeck/arinn/issues/481), spec §5.7 *Power button*), which needs the same path for the power menu: a kind would save that key one contract change, but the shell would dispatch on a string the daemon must still validate, and the quick-menu implementation in #439 already consumes the named members. The power key adds its own additive pair in a separate `contract-change` task, following this pattern.
- **Change `ReportHomeRequest`/`HomeRequested` to carry a kind.** Breaks existing signatures.
- **Infer the quick menu in the shell** from `Changed` or `Status`. Forbidden by ADR-0069: state recovery must never become a user action.
- **A shell-local key only.** The shell does not receive keys while an external application has focus (§12.1).
- **Menu bound globally at all times.** The shell would never receive Menu, so navigation and the tile action sheet would break (spec §6.2).
- **A long press of Home.** KWin global shortcuts and X11 grabs report presses, not their duration, and a long press delays every ordinary Home.
- **Show the quick menu for media and volume keys outside the shell.** Rejected by the owner: Plasma keeps them (OQ-04).

## Verification

- `runtime::window_tests::quick_menu_requests_are_distinct_from_home_requests`: untrusted calls and an oversized target or output emit nothing; repeated requests each emit `Changed` and then `QuickMenuRequested` with the epoch, target and output, and commit the Home state; neither request emits the other's signal; a late subscriber receives no history.
- `runtime::background::tests::quick_menu_request_arms_the_activity_background`: a quick-menu request arms the Activity background capture for its Return target and output, a rejected one arms nothing, and Home pressed afterwards on the same context keeps that attempt.
- `runtime::input_tests::build_info_reports_this_build_and_matches_the_xml` and `tests/integration/phase1.py` compare the live export with `daemon1.xml`.
- The key configuration, the KWin action, the X11 grab and the shell sheet are not part of the contract pull request ([#467](https://github.com/LimeBeck/arinn/issues/467)); they follow in [#439](https://github.com/LimeBeck/arinn/issues/439), together with their tests. Those tests do not exist before #439 merges: the Menu keys following the shell's focus on KDE (`global_home` `remote_menu_keys_follow_the_shell_focus`, and the quick-menu keys in `window_tests` `adapter_connection_applies_configured_home_shortcut`, which exists today for the Home keys only) and on X11 (`remote_menu_opens_the_quick_menu_only_outside_the_shell`).
- Not tested: real KDE Plasma 6 and X11 sessions with the remote (operator tests need the owner's permission).
