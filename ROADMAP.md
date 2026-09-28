# Roadmap

The single source of phase status ([ADR-0028](docs/adr/0028-documentation-governance.md)). Requirements are in the [specification](docs/specifications/arinn-specification.md); this file says **when** they are delivered and **how we know** they are done.

## Rules

- **Statuses:** `Planned` → `In progress` → `Done`. `Blocked` when waiting on a decision or an external dependency (say which).
- **Exit criteria** reference requirement IDs: acceptance criteria (AC-xx), gates (G-x.y), rules (R-xx), open questions (OQ-xx).
- **A phase is Done** only when every exit criterion holds with linked evidence (test, report or evidence file) **and** the documentation is synchronized: specification, ADRs, this file and changelog fragments. Items that are knowingly moved to a later phase are listed under *Carried over* with their target.
- **Open questions** listed under a phase must be decided before that phase starts the affected work.
- **Updating:** a pull request changes only the rows of the gates it closes or moves, and records the date; phase status is changed by the maintainer. This keeps parallel pull requests free of conflicts ([ADR-0036](docs/adr/0036-parallel-development.md)).
- **Tasks:** each phase is split into issues from the *Agent task* template before parallel work starts; issues name their zone and dependencies.
- **Every acceptance criterion** AC-01–AC-19 is assigned to exactly one phase below; `scripts/check-docs.py` enforces it.

## Overview

| Phase | Goal | Status |
| --- | --- | --- |
| [0](#phase-0--architecture-spike) | Architecture spike on KDE/Wayland | **Done** (2026-09-27) |
| [1](#phase-1--core-skeleton) | Production core skeleton, protocol and SDK | **In progress** (2026-09-28) |
| [2](#phase-2--applications) | Desktop applications, Steam, ES-DE | Planned |
| [3](#phase-3--browser) | Chrome integration — first daily-usable build | Planned |
| [4](#phase-4--media) | MPRIS, local library and mpv | Planned |
| [5](#phase-5--product-mvp) | Downloads, full UI, persistence, packaging — MVP | Planned |
| [6](#phase-6--rich-integrations-post-mvp) | Rich integrations after the MVP | Planned |

## Parallel UI work alongside Phase 1

**Scheduling decision:** approved by the project owner on 2026-09-28.
**UI increment status:** Planned; this decision records scope and dependencies,
not implementation or verification evidence.

Phase 0 is complete, so presentation and local navigation work may start while
Phase 1 core/API work continues. Full UI acceptance remains in Phase 5; phases
describe delivery and acceptance milestones, not a requirement to delay all UI
implementation until Phase 5.

| Workstream | Initial scope | Coordination boundary |
| --- | --- | --- |
| Core/API | Phase 1 daemon contracts, runtime, SDK, demo and shell transport migration | Own production D-Bus contracts and the shell C++ transport/model layer |
| UI presentation | QML design tokens, typography, reusable cards/buttons, Home and global navigation; loading, empty, error and unavailable states | Consume explicit presentation models; coordinate shared components before splitting screen work |
| Input and UI verification | Local D-pad/pointer handling, focus scopes and restoration, Qt Quick Test and visual comparison | Assign one owner for shared focus/input code; screen work uses that interface |

### First UI increment

- Implement the QML design system, Home and global navigation using the
  [design contract](docs/specifications/arinn-specification.md#15-design-contract)
  and its handoff references. The current specification governs behaviour,
  including Home/Back; the HTML prototype is a visual reference.
- Exercise populated, empty, loading, error and unavailable presentations with
  explicit development/test fixtures. Fixtures do not become a production backend
  or define new capability contracts.
- Keep focus, navigation and interaction state local to the shell. Production
  data and commands go through the daemon via asynchronous C++ models, respecting
  R-01–R-05 and AC-12. Provider business logic does not move into QML.
- Verify local navigation and focus restoration with Qt Quick Test, and compare
  the implemented states with the handoff at logical 1920×1080 and 4K scale.
  Record evidence for this subset without claiming full Phase 5 acceptance.

### Dependencies and integration

Visual components and fixture-based UI tests need not wait for all Phase 1 gates.
Production binding follows the reviewed versioned contracts (G-1.5), the required
daemon operations and demo fixtures; it is coordinated with the shell migration
to `org.arinn.Daemon1` (G-1.4). Use the demo provider through the daemon for
integration tests, including delayed/unavailable providers and reconnects.

Before parallel implementation, agree on presentation-model fields and actions,
and assign ownership of shared QML components, focus code, C++ models and build
files so separate changes do not compete for the same files.

Work depending on an open question waits for the owner's decision: web shortcuts
(OQ-02), browser launch ownership (OQ-03), hidden-player input (OQ-01), global
media/volume keys (OQ-04) and the live New projection (OQ-05). These dependencies
do not block the first UI increment's generic components and fixture-based views.
Production Activity background capture, session actions and their performance
checks retain their existing phase gates. Real-desktop operator tests still
require explicit permission.

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

**Status:** In progress (2026-09-28).

The production registry, systemd adapter, SDK, application API, operation lifecycle,
persistence, CLI and shell migration are implemented. Local private-bus contracts
cover the demo fixtures, recovery and limits; versioned XML is checked against live
introspection. See the [implementation record](docs/architecture/phase-1-progress.md)
for evidence and remaining qualification. Real systemd/KDE checks also passed ([live evidence](docs/architecture/phase-1-live-evidence.json)).
CI and the maintainer release gate remain pending; Phase 1 is not marked Done.

**Decision resolved (2026-09-28):** OQ-06 uses SHA-256 of `provider_id`
([ADR-0034](docs/adr/0034-provider-data-directory-identity.md)); provisioning and
registry-reset recovery tests are implemented.
G-1.6 also requires maintainer actions; agents do not publish releases.

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
| G-1.1 Contract suite passes against the demo provider for its declared capabilities | Local private-bus coverage; CI pending ([record](docs/architecture/phase-1-progress.md)) |
| G-1.2 Crash/restart, daemon restart, disappear/reappear and stale generation covered by integration tests | Local private-bus coverage; CI pending ([record](docs/architecture/phase-1-progress.md)) |
| G-1.3 Limits, validation and timeouts rejected/handled as specified (malformed, oversized, slow provider) | Local private-bus coverage; CI pending ([record](docs/architecture/phase-1-progress.md)) |
| G-1.4 Shell reads everything from `org.arinn.Daemon1`; no `Spike1` interfaces remain | Shell migrated; local Qt test; CI/review pending |
| G-1.5 Versioned introspection XML for the application capability and the daemon API | [XML contracts](docs/plugin-api/README.md), live introspection comparison |
| AC-11 Dynamic provider: install/remove a manifest and reload without core changes | Local manifest remove/reinstall and reload test; CI pending |
| AC-12 No product-specific conditions in the shell | Local architecture guards and Qt tests; review/CI pending |
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

**Status:** Planned. Decide first: OQ-04. OQ-06 was resolved by [ADR-0034](docs/adr/0034-provider-data-directory-identity.md).

Goal: everything needed to use Arinn as the HTPC shell — the full design, Downloads, persistence, diagnostics, error UX, power and packaging.

The [parallel UI workstream](#parallel-ui-work-alongside-phase-1) starts with a
bounded presentation/navigation increment alongside Phase 1. This phase integrates
and accepts the full UI; early component work does not close its criteria.

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
