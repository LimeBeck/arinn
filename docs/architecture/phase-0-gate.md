# Phase 0 gate: KDE/Wayland

Gate matrix for Phase 0 of the [roadmap](../../ROADMAP.md). Evidence: [Phase 0 report](phase-0-report.md); commands: [runbook](phase-0-runbook.md).

**Result: architecture gate passed (G-0.1–G-0.6).** G-0.7 passed at feasibility level; G-0.8 is deferred by the project owner and carried into Phase 5 (AC-19). The target HTPC has one screen ([ADR-0031](../adr/0031-single-screen-target.md)); two monitors are only on the development machine.

## Chosen path

A packaged KWin script as part of the platform layer ([ADR-0032](../adr/0032-kwin-script-platform-adapter.md)): global shortcut through KWin, foreground window tracking, shell activation and restoring the previous window. A typed daemon ↔ adapter bridge accepts only predefined operations, never scripts from plugins. Product-specific window logic stays in providers; compositor operations in the adapter.

The KWin scripting API documents `registerShortcut` and the workspace/window APIs. Near/far effects were researched separately through KWin effects and `WindowThumbnail` inside the compositor ([KWin API](https://develop.kde.org/docs/plasma/kwin/api/), [KWin effects](https://develop.kde.org/docs/plasma/kwineffect/)).

## Gate matrix

| ID | Check | Success condition | Result |
| --- | --- | --- | --- |
| G-0.1 | Qt/QML ↔ Rust over D-Bus | Async status/snapshot and operation without UI blocking | PASS: Qt Test on a private bus, focus moves during a slow action |
| G-0.2 | External demo process | Register, snapshot, crash/restart with a new generation | PASS: contract suite and SIGKILL of the transient unit |
| G-0.3 | Chrome Home/Return | Global Home works from the browser; Return reaches the same window | PASS for the window: 20 cycles on each of two outputs; DOM/tab state not checked separately |
| G-0.4 | Steam and a fullscreen game | Home over the activity, focus restored, the application keeps running | PASS: Steam 20/20; Terraria fullscreen 20/20 on each output after the remap fix |
| G-0.5 | Target window removed | Return never activates some other window; Home stays available | PASS on an isolated Chrome fixture |
| G-0.6 | Repeated Home | No duplicate shell; input scope correct | PASS for target/output in all cycles; full modal scopes are Phase 5 UI work |
| G-0.7 | Texture/near/far effects | Real external window; limits and cost measured | PASS (feasibility): [experiment](background-experiment.md); production capture lifecycle and performance → Phase 5 |
| G-0.8 | Lock/sleep/session shutdown | No overlay over the lock screen; units stop correctly | Deferred by the project owner: SessionGuard and private-bus tests done, real lock/suspend/logout not run ([session lifecycle](session-lifecycle.md)) → Phase 5, AC-19 |

A focus failure fails the gate; a missing graphical effect is a separate open design requirement. The report records KDE, Qt, GPU and driver versions, native Wayland/XWayland mode of each application, monitor scale and fullscreen mode.

## Environment check

2026-09-27, read-only: `XDG_SESSION_TYPE=wayland`, `XDG_CURRENT_DESKTOP=KDE`; `kwin_wayland`, `qdbus6`, `systemctl`, Rust/Cargo and CMake available. `kwin_wayland --version` 6.7.5; Qt6Core/Qt6DBus/Qt6Quick 6.11.2 (pkg-config); systemd 261.3. The Plasma Shell version was not established reliably.

The project owner allowed the runtime spike on this machine as technically equivalent to the HTPC ([ADR-0030](../adr/0030-phase0-machine-and-standard-input.md)). The temporary adapter and shortcut were loaded only for the session; no permanent autostart was installed.
