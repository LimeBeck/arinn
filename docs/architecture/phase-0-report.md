# Phase 0 report

Date: 2026-09-27. The development machine was used, with the project owner's permission, as technically equivalent to the HTPC. A vertical slice was implemented and verified; gate status is in the [gate matrix](phase-0-gate.md). [Build and run](phase-0-runbook.md).

## Environment

KDE/Wayland, KWin 6.7.5, Qt 6.11.2, Rust 1.98.1, systemd 261.3. Intel Iris Xe (Tiger Lake GT2), i915 kernel driver, Mesa 26.2.3, KWin OpenGL/EGL compositor. The Plasma Shell version was not confirmed separately.

| Output | Physical mode | Scale | Logical geometry |
| --- | --- | --- | --- |
| DP-1 | 1920×1080, ~75 Hz | 1 | 0,0 — 1920×1080 |
| eDP-1 | 2880×1800, 90 Hz | 1.75 | 1920,140 — 1646×1029 |

The display configuration was not changed. The shell selects the `QScreen` by output name; KWin handles placement and activation. Tests set fullscreen on the test window through KWin; this confirms compositor fullscreen, not exclusive renderer mode or direct scanout.

The target HTPC has **one screen** ([ADR-0031](../adr/0031-single-screen-target.md)). The two-screen run is a development-machine check; a single-screen smoke test is repeated at deployment.

## Confirmed results

| Check | Result / evidence |
| --- | --- |
| Typed Rust → D-Bus → Qt/QML | A real snapshot read by Qt Test; standalone demo in a separate process. Private-bus suite PASS. |
| Async actions | A 7-second provider gives `Timeout` after 5 s; status answers without waiting for the action. Left/Right focus in QML changes within 500 ms during the operation. This is a functional test limit, not an FPS or input-latency benchmark. |
| Chrome, DP-1 | 20/20 Home → repeated Home → Back, same UUID and output. Fullscreen; Chrome with `--ozone-platform=wayland`, a separate profile and a local fixture page. [JSON](evidence/chrome-DP-1.json) |
| Chrome, eDP-1 | 20/20, fullscreen and scale 1.75. [JSON](evidence/chrome-eDP-1.json) |
| Steam, DP-1 | 20/20, client window, not fullscreen. [JSON](evidence/steam-DP-1.json). Wayland/XWayland mode of this window not measured. |
| Terraria, DP-1 and eDP-1 | 20/20 each, compositor fullscreen, SDL Wayland (confirmed by log). [DP-1](evidence/terraria-DP-1.json), [eDP-1](evidence/terraria-eDP-1.json). Game window/menu only, no gameplay. |
| Provider crash | SIGKILL demo → unavailable → systemd restart → new generation; daemon and shell PIDs unchanged. [JSON](evidence/live-failures.json) |
| Removed Activity | Closing a separate Chrome fixture while Home is open clears the return target; Back activates no other window. [JSON](evidence/live-failures.json) |
| Validation/recovery | Wrong token, foreign platform caller, unknown item and stale generation rejected. After a daemon restart the demo registers again. Private-bus suite PASS. |
| Daemon crash recovery | The KWin bridge tracks the D-Bus owner, retries its long-poll, drops stale replies and republishes the saved Activity context. SIGKILL checked with Home hidden and open, without reloading the adapter. [JSON](evidence/daemon-recovery.json) |
| Activity background | Real window → `WindowThumbnail` → near/far → PNG → separate Qt renderer. [Experiment](background-experiment.md) |
| Session guard | The shell hides on lock/sleep and re-checks after wake; private-bus tests with fake ScreenSaver/login1 services. [Session lifecycle](session-lifecycle.md) |
| Build | cargo build/test, clippy `-D warnings`, fmt check, CMake/Ninja, the Qt integration suite and the JS remap/closed-window regression passed. |

Home was triggered through `KGlobalAccel.invokeShortcut` of the registered Meta+F12, not a physical key. Return was checked by KWin window UUID and output. This does not check DOM/input preservation in an arbitrary tab and is not a Chrome extension test. The `cycle_ms` arrays include three commands, polling and CLI process startup and cannot be compared with an input-to-photon budget.

The remote emits standard keys (Esc, Backspace, …); no dedicated integration is needed and the physical remote does not block Phase 0 ([ADR-0030](../adr/0030-phase0-machine-and-standard-input.md)). Qt Test also checks opening navigation with Backspace and closing it with Esc.

## Carried over

| Item | Target |
| --- | --- |
| Returning to a window with a new UUID through production Activity matching (the spike keeps a detached UUID and restores only the same KWin identity after remap; 40 cycles passed after the fix) | Phase 2 |
| Automatic capture lifecycle for Home and its performance: PNG export ~0.3 s, mixed DPI needs a physical size limit | Phase 5 |
| Real lock/suspend/logout, inhibitors and wake (deferred by the project owner) | Phase 5, AC-19 |
| Unplug/mirror outputs and display layout changes while Home is open; compositor restart | Backlog (not MVP, [ADR-0031](../adr/0031-single-screen-target.md)) |
| Production core: manifest discovery, SDK, persistence, autostart, capability routing, error names, subscriptions, cancellation, backpressure, transport size limit. `Spike1` interfaces are not stable. | Phase 1 |
| UI: two cards and a navigation placeholder; design system and modal focus scopes not implemented | Phase 5 |

## Clean-up

After the tests the temporary KWin script was unloaded and the `arinn-phase0-*` units were stopped (empty `list-units`, `isScriptLoaded=false`). Steam/Proton settings and game saves were not edited; Terraria ran with a separate save directory in `/tmp`.

## Technical conclusions

A KWin script is suitable for the initial Home/Return with two displays; the [KWin API](https://develop.kde.org/docs/plasma/kwin/api/) provides window/output tracking and activation. Async QtDBus with zbus works directly. Returned structs needed a one-element tuple in zbus, otherwise the wire body did not match the introspection and Qt decoding; the fix is covered by an integration test and the [actual XML](../plugin-api/phase0.xml).

Production core, SDK and provider lifecycle can proceed (Phase 1).
