# Roadmap

The single source of phase status ([ADR-0028](docs/adr/0028-documentation-governance.md)). Requirements are in the [specification](docs/specifications/arinn-specification.md); this file says **when** they are delivered and **how we know** they are done.

## Rules

- **Statuses:** `Planned` → `In progress` → `Done`. `Blocked` when waiting on a decision or an external dependency (say which).
- **Exit criteria** reference requirement IDs: acceptance criteria (AC-xx), gates (G-x.y), rules (R-xx), open questions (OQ-xx).
- **A phase is Done** only when every exit criterion holds with linked evidence (test, report or evidence file) **and** the documentation is synchronized: specification, ADRs, this file, `CHANGELOG.md`. Items that are knowingly moved to a later phase are listed under *Carried over* with their target.
- **Open questions** listed under a phase must be decided before that phase starts the affected work.
- **Updating:** change this file in the same pull request that changes a phase status, closes a gate or moves an item. Record the date.
- **Every acceptance criterion** AC-01–AC-19 is assigned to exactly one phase below; `scripts/check-docs.py` enforces it.

## Overview

| Phase | Goal | Status |
| --- | --- | --- |
| [0](#phase-0--architecture-spike) | Architecture spike on KDE/Wayland | **Done** (2026-09-27) |
| [1](#phase-1--core-skeleton) | Production core skeleton, protocol and SDK | Planned — next |
| [2](#phase-2--applications) | Desktop applications, Steam, ES-DE | Planned |
| [3](#phase-3--browser) | Chrome integration — first daily-usable build | Planned |
| [4](#phase-4--media) | MPRIS, local library and mpv | Planned |
| [5](#phase-5--product-mvp) | Downloads, full UI, persistence, packaging — MVP | Planned |
| [6](#phase-6--rich-integrations-post-mvp) | Rich integrations after the MVP | Planned |

## Phase 0 — Architecture spike

**Status:** Done (2026-09-27). Evidence: [gate matrix](docs/architecture/phase-0-gate.md), [report](docs/architecture/phase-0-report.md), [runbook](docs/architecture/phase-0-runbook.md).

Goal: prove the architecture on KDE Plasma 6 / Wayland before significant UI work — Qt/QML shell, Rust daemon, D-Bus, external plugin process, global Home over Chrome and a game, focus return.

| Exit criterion | Result |
| --- | --- |
| G-0.1 Qt/QML ↔ Rust over D-Bus, no UI blocking | Pass |
| G-0.2 External demo process: register, snapshot, crash/restart with a new generation | Pass |
| G-0.3 Chrome Home/Return | Pass (window level) |
| G-0.4 Steam and a fullscreen game Home/Return | Pass |
| G-0.5 Removed target window handled | Pass |
| G-0.6 Repeated Home, no duplicate shell | Pass |
| G-0.7 Activity background feasibility | Pass (feasibility) |
| G-0.8 Lock/sleep/session shutdown | Deferred by the project owner → Phase 5 |

**Carried over:** production Activity matching after window remap (→ Phase 2); background capture lifecycle and performance (→ Phase 5); real lock/suspend/logout (→ Phase 5, AC-19); multi-monitor edge cases and compositor restart (→ Backlog).

## Phase 1 — Core skeleton

**Status:** Planned — next.

Goal: replace the spike interfaces with the production core, so every later phase adds providers instead of changing the core.

Scope:

- `arinnd`: `org.arinn.Daemon1` API; plugin registry with manifest discovery, transient units, registration and generations; snapshots with paging; asynchronous operations, errors and limits ([implementation baseline](docs/architecture/implementation-baseline.md), [plugin runtime](docs/architecture/plugin-runtime.md)).
- `arinn-protocol` and `arinn-plugin-sdk` (permissive license); `arinn-plugin-demo` rebuilt on the SDK with the fixtures of spec §17.
- `arinnctl`: `status`, `providers`, `capabilities`, `activities`, `diagnostics`, `provider restart`, `providers reload`, `--json`, exit codes.
- Shell receives all data only through the daemon.
- `config.toml` and `state.sqlite3` with migrations; structured JSON logging.
- CI/CD per [ADR-0033](docs/adr/0033-ci-cd-and-releases.md) is in place (pipeline, nightly, package install test, gated releases); Phase 1 extends the test suites it runs.

| Exit criterion | Evidence |
| --- | --- |
| G-1.1 Contract suite passes against the demo provider for its declared capabilities | CI |
| G-1.2 Crash/restart, daemon restart, disappear/reappear and stale generation covered by integration tests | CI |
| G-1.3 Limits, validation and timeouts rejected/handled as specified (malformed, oversized, slow provider) | CI |
| G-1.4 Shell reads everything from `org.arinn.Daemon1`; no `Spike1` interfaces remain | Code review, CI |
| G-1.5 Versioned introspection XML for the application capability and the daemon API | `docs/plugin-api/` |
| AC-11 Dynamic provider: install/remove a manifest and reload without core changes | Integration test |
| AC-12 No product-specific conditions in the shell | Code review, CI grep check |
| G-1.6 Branch protection on `master` requires all CI checks and the CLA; the first `v0.1.0` pre-release is published through the release pipeline | GitHub settings, release page |

## Phase 2 — Applications

**Status:** Planned. Decide first: OQ-03.

Goal: launch and return to ordinary applications, Steam and ES-DE through `arinn-plugin-desktop`.

| Exit criterion | Evidence |
| --- | --- |
| AC-07 Steam launches from its own tile | Operator test |
| AC-08 EmulationStation launches from its own tile | Operator test |
| G-2.1 Applications → Chrome/Steam/ES-DE → Home → Back → the same application | Operator test, evidence JSON |
| G-2.2 Restarting or removing the desktop plugin does not kill launched applications (separate scopes) | Integration test |
| G-2.3 Production Activity matching survives window remap (carried over from Phase 0) | Integration test |

## Phase 3 — Browser

**Status:** Planned. Decide first: OQ-02, OQ-03.

Goal: the main daily scenario — Arinn becomes usable every day.

| Exit criterion | Evidence |
| --- | --- |
| AC-03 Chrome or an online service opens from Home | Operator test |
| AC-04 Existing tabs are shown and switchable | Integration + operator test |
| AC-05 Global Home over Chrome | Operator test |
| AC-06 Return to the same browser activity (tab level) | Operator test |
| AC-10 Chrome provider crash does not crash shell or daemon | Integration test |
| G-3.1 Extension uses only `tabs` and `nativeMessaging`; native host limits and reconnect as specified | Review, integration test |

## Phase 4 — Media

**Status:** Planned. Decide first: OQ-01, OQ-05.

Goal: MPRIS media control, the local library and playback in external mpv with Arinn's OSD.

| Exit criterion | Evidence |
| --- | --- |
| AC-09 MPRIS session shown and controlled | Integration + operator test |
| AC-15 Library scan of local and mounted folders, fallback metadata, unavailable mount, film and episode start | Integration test |
| AC-16 Play/pause/seek, audio/subtitle selection, resume after restart, Continue | Integration + operator test |
| G-4.1 Media integration gate: OSD over video without losing playback, input path per OQ-01, Home/Return | Operator test |

## Phase 5 — Product MVP

**Status:** Planned. Decide first: OQ-04, OQ-06.

Goal: everything needed to use Arinn as the HTPC shell — the full design, Downloads, persistence, diagnostics, error UX, power and packaging.

| Exit criterion | Evidence |
| --- | --- |
| AC-01 Autostart after login shows Home | Operator test on the target HTPC |
| AC-02 Full D-pad navigation | UI tests + operator test |
| AC-13 No profile selection or login | Review |
| AC-14 Focus and primary actions readable from 3 m | Operator test on a TV |
| AC-17 Downloads with Transmission, import into Library/New exactly once | Integration + operator test |
| AC-18 Home/Back table with standard keyboard input, Esc/Backspace exception | UI tests |
| AC-19 Suspend, wake, lock screen and ambient (carried over G-0.8) | Operator test |
| G-5.1 UI matches the design handoff states and the 22 screenshots | Design review |
| G-5.2 Activity background capture lifecycle and budget (carried over G-0.7) | Measurement report |
| G-5.3 Arch/EndeavourOS package installs, starts and uninstalls cleanly | Packaging test |
| G-5.4 Performance budgets of spec §6.5 measured on the target device | Measurement report |

## Phase 6 — Rich integrations (post-MVP)

**Status:** Planned. Starts after the Plugin API is declared stable (specification 1.0).

Candidates: Steam provider (library, artwork, recent games), ES-DE provider, Jellyfin provider, TV/webOS control, gamepad input, adding torrents from Arinn. Each must fit existing capabilities or go through an API review (R-09, R-10).

## Backlog

- Multi-monitor edge cases: hot unplug, mirrored outputs, moving the Activity while Home is open.
- KWin compositor restart while Arinn runs.
- Live compositor texture instead of the last frame.
