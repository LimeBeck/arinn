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
| [1](#phase-1--core-skeleton) | Production core skeleton, protocol and SDK | **Done** (2026-09-28) |
| [2](#phase-2--applications) | Desktop applications, Steam, ES-DE | **Done** (2026-09-28) |
| [3](#phase-3--browser) | Chrome integration — first daily-usable build | Planned |
| [4](#phase-4--media) | MPRIS, local library and mpv | Planned |
| [5](#phase-5--product-mvp) | Downloads, full UI, persistence, packaging — MVP | Planned |
| [6](#phase-6--rich-integrations-post-mvp) | Rich integrations after the MVP | Planned |

## Parallel UI work alongside Phase 1

**Scheduling decision:** approved by the project owner on 2026-09-28.
**UI increment status:** In progress (2026-09-28). The reusable component subset
and isolated fixture gallery are implemented ([component contract](docs/architecture/ui-components.md),
[issue #10](https://github.com/LimeBeck/arinn/issues/10)). Fixture-driven Home and
global navigation extend that foundation ([issue #15](https://github.com/LimeBeck/arinn/issues/15));
production binding and complete design verification remain pending. No Phase 5 gate is closed.

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

Hidden-player input (OQ-01), global media/volume keys (OQ-04) and the New projection (OQ-05) were decided by the owner on 2026-09-28; New is not part of the MVP. These dependencies
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

**Status:** Done (2026-09-28), under explicit owner authorization.

The production registry, systemd adapter, SDK, application API, operation lifecycle,
persistence, CLI and shell migration are implemented. Local private-bus contracts
cover the demo fixtures, recovery and limits; versioned XML is checked against live
introspection. See the [implementation record](docs/architecture/phase-1-progress.md)
for verification evidence. Real systemd/KDE checks also passed ([live evidence](docs/architecture/phase-1-live-evidence.json)).
CI and clean Arch package checks passed on 2026-09-28 ([CI evidence](docs/architecture/phase-1-progress.md#release-qualification)).
Branch protection is enabled and verified. The [v0.1.0 pre-release](https://github.com/LimeBeck/arinn/releases/tag/v0.1.0)
was published through the [successful release pipeline](https://github.com/LimeBeck/arinn/actions/runs/36416052425)
on 2026-09-28; all Phase 1 exit criteria are met.

**Decision resolved (2026-09-28):** OQ-06 uses SHA-256 of `provider_id`
([ADR-0034](docs/adr/0034-provider-data-directory-identity.md)); provisioning and
registry-reset recovery tests are implemented.

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
| G-1.1 Contract suite passes against the demo provider for its declared capabilities | Local and CI private-bus coverage passed (2026-09-28; [evidence](docs/architecture/phase-1-progress.md#release-qualification)) |
| G-1.2 Crash/restart, daemon restart, disappear/reappear and stale generation covered by integration tests | Local and CI private-bus coverage passed (2026-09-28; [evidence](docs/architecture/phase-1-progress.md#release-qualification)) |
| G-1.3 Limits, validation and timeouts rejected/handled as specified (malformed, oversized, slow provider) | Local and CI private-bus coverage passed (2026-09-28; [evidence](docs/architecture/phase-1-progress.md#release-qualification)) |
| G-1.4 Shell application/provider integration data and commands use `org.arinn.Daemon1` and its daemon-owned endpoints; no executable `Spike1` interfaces remain. Independent session observation follows ADR-0041 | Source review passes the clarified boundary (2026-09-28; [review](docs/architecture/phase-1-shell-review.md), [owner decision](docs/adr/0041-independent-session-lifecycle-guard.md)); prior Qt/private-bus [CI evidence](docs/architecture/phase-1-progress.md#release-qualification). R-05 lifecycle transport extraction was merged in [#32](https://github.com/LimeBeck/arinn/pull/32), closing [#29](https://github.com/LimeBeck/arinn/issues/29) |
| G-1.5 Versioned introspection XML for the application capability and the daemon API | [XML contracts](docs/plugin-api/README.md), live introspection comparison |
| AC-11 Dynamic provider: install/remove a manifest and reload without core changes | Local and CI manifest remove/reinstall and reload tests passed (2026-09-28; [evidence](docs/architecture/phase-1-progress.md#release-qualification)) |
| AC-12 No product-specific conditions in the shell | Production shell source review passed (2026-09-28; [review](docs/architecture/phase-1-shell-review.md)); prior local/CI test evidence remains in the [implementation record](docs/architecture/phase-1-progress.md#release-qualification) |
| G-1.6 Branch protection on `master` requires all CI checks and the CLA; the first `v0.1.0` pre-release is published through the release pipeline | Pass (2026-09-28): required checks and CLA enforced by verified branch protection; `v0.1.0` published by the successful release pipeline, with downloaded asset checksums verified ([evidence](docs/architecture/phase-1-progress.md#release-qualification)) |

## Phase 2 — Applications

OQ-03 was resolved by the owner on 2026-09-28:
the desktop provider launches the browser; the Chrome provider declares only
`browser` ([ADR-0038](docs/adr/0038-browser-launch-ownership.md)).
Implementation is tracked in [issue #20](https://github.com/LimeBeck/arinn/issues/20);
the provider and platform backend are implemented with [isolated verification](docs/architecture/desktop-provider.md).

**Status:** Done (2026-09-28), by owner decision: the owner's manual checks of
the v0.1 package on their KDE Plasma 6 / Wayland machine (commit `e97879b`)
count as the real KDE qualification for [#35](https://github.com/LimeBeck/arinn/issues/35).
The owner reported that every check below passed; no agent-run operator test and
no evidence JSON exist for them. Kernel cgroup separation was not inspected
directly; only application survival across a provider restart was checked.

Goal: launch and return to ordinary applications, Steam and ES-DE through `arinn-plugin-desktop`.

| Exit criterion | Evidence |
| --- | --- |
| AC-07 Steam launches from its own tile | Passed, owner's manual check (2026-09-28) |
| AC-08 EmulationStation launches from its own tile | Passed, owner's manual check (2026-09-28) |
| G-2.1 Applications → Chrome/Steam/ES-DE → Home → Back → the same application | Passed, owner's manual check (2026-09-28): launch from Applications puts the window above the shell ([#72](https://github.com/LimeBeck/arinn/pull/72)), switching to an open window works ([#73](https://github.com/LimeBeck/arinn/pull/73)), Home → Return reaches the same application; Steam and ES-DE from their tiles |
| G-2.2 Restarting or removing the desktop plugin does not kill launched applications (separate scopes) | Private-bus fixture lifetime passed (2026-09-28; [evidence](docs/architecture/desktop-provider.md#verification-status)); on the owner's KDE session a launched application survived `arinnctl provider restart desktop` (owner's manual check, 2026-09-28). Kernel cgroups not inspected |
| G-2.3 Production Activity matching survives window remap (carried over from Phase 0) | Daemon/provider and adapter remap fixtures passed (2026-09-28; [evidence](docs/architecture/desktop-provider.md#verification-status)); on real KDE, activation of an open window and Home → Return passed with a stable adapter connection (owner's manual check, 2026-09-28). A window remap was not specifically exercised |

## Phase 3 — Browser

**Status:** Planned. Browser launch ownership (OQ-03) was resolved by
[ADR-0038](docs/adr/0038-browser-launch-ownership.md); online-service tiles (OQ-02)
were resolved on 2026-09-28 as Arinn-owned web shortcuts ([ADR-0043](docs/adr/0043-web-shortcuts.md)).

Goal: the main daily scenario — Arinn becomes usable every day.

| Exit criterion | Evidence |
| --- | --- |
| AC-03 Chrome or an online service opens from Home | Operator test |
| AC-04 Existing tabs are shown and switchable | Integration + operator test |
| AC-05 Global Home over Chrome | Operator test |
| AC-06 Return to the same browser activity (tab level) | Operator test |
| AC-10 Chrome provider crash does not crash shell or daemon | Pass (integration, CI): `plugins/chrome/tests/isolation.rs` `chrome_provider_crash_does_not_crash_the_daemon` kills the provider with SIGKILL; the daemon stays up and the browser becomes unavailable. The shell shows the unavailable state without failing (`browser_catalog_test`, [#296](https://github.com/LimeBeck/arinn/pull/296)). Recorded 2026-10-06 ([#353](https://github.com/LimeBeck/arinn/issues/353)) |
| G-3.1 Extension uses only `tabs` and `nativeMessaging`; native host limits and reconnect as specified | Pass (review + integration, CI): the manifest requests only `tabs` and `nativeMessaging` (`tests/integration/chrome_extension.test.cjs`, checked again by review). Framing limits are 1 MiB to the browser and 8 MiB from it (`plugins/chrome/src/framing.rs` `frames_round_trip_and_enforce_limits`). The real native host links the extension and the provider bridge (`plugins/chrome/tests/chain.rs`). The extension reconnects with capped exponential backoff and never replays commands (`chrome_extension.test.cjs`). Recorded 2026-10-06 ([#353](https://github.com/LimeBeck/arinn/issues/353)) |

## Phase 4 — Media

**Status:** Planned. OQ-01 and OQ-05 were resolved by the owner on 2026-09-28 ([ADR-0046](docs/adr/0046-shell-input-layer-during-playback.md); New is not part of the MVP).

Goal: MPRIS media control, the local library and playback in external mpv with Arinn's OSD.

| Exit criterion | Evidence |
| --- | --- |
| AC-09 MPRIS session shown and controlled | Integration + operator test |
| AC-15 Library scan of local and mounted folders, fallback metadata, unavailable mount, film and episode start | Pass (integration, CI). Scan, names, NFO and artwork: `plugins/library/tests/catalogue.rs` (`scans_films_series_artwork_and_hostile_nfo`, `tv_season_folders_form_one_series`). Fallback metadata: `missing_ffprobe_keeps_the_catalogue_and_unknown_duration`. Unavailable root keeps entries: `missing_file_and_unavailable_root_keep_the_uuid`. Film and episode start through the daemon: `crates/arinnd/src/runtime/library_tests.rs` `a_launch_opens_the_item_in_the_media_provider`; in the shell, `library_catalog_test` `seriesLaunchResolvesToAnEpisode`. A mounted folder is an ordinary configured root; no network mount was exercised. Recorded 2026-10-06 ([#353](https://github.com/LimeBeck/arinn/issues/353)) |
| AC-16 Play/pause/seek, audio/subtitle selection, resume after restart, Continue | Integration + operator test |
| G-4.1 Media integration gate: OSD over video without losing playback, shell input layer per ADR-0046, Home/Return | #141: managed input layer implemented with synthetic adapter/private-bus coverage; #309 adds confirmed Stop, fresh-process Continue and retained final-frame artwork (ADR-0083 proposed); owner-run KDE Wayland/TV operator test pending |

## Phase 5 — Product MVP

**Status:** Planned. OQ-04 was resolved by the owner on 2026-09-28 (Plasma handles media/volume keys outside the shell). OQ-06 was resolved by [ADR-0034](docs/adr/0034-provider-data-directory-identity.md).

Goal: everything needed to use Arinn as the HTPC shell — the full design, Downloads, persistence, diagnostics, error UX, power and packaging.

The [parallel UI workstream](#parallel-ui-work-alongside-phase-1) starts with a
bounded presentation/navigation increment alongside Phase 1. This phase integrates
and accepts the full UI; early component work does not close its criteria.

| Exit criterion | Evidence |
| --- | --- |
| AC-01 Autostart after login shows Home | Passed on KDE Plasma 6 / Wayland, owner's manual check on the target HTPC (reported 2026-10-06, [#353](https://github.com/LimeBeck/arinn/issues/353)). No agent-run operator test and no evidence JSON exist for it. Opt-in session units: [ADR-0042](docs/adr/0042-opt-in-session-autostart.md), [#52](https://github.com/LimeBeck/arinn/issues/52); offline unit and loader checks run in the package test. XFCE/X11 remains under G-5.5 |
| AC-02 Full D-pad navigation | UI tests + operator test |
| AC-13 No profile selection or login | Pass (review, 2026-10-06, [#353](https://github.com/LimeBeck/arinn/issues/353)): the shell starts at Home. The QML and C++ sources of `apps/arinn-shell` contain no profile, account, login or password flow |
| AC-14 Focus and primary actions readable from 3 m | Operator test on a TV |
| AC-17 Downloads with Transmission, import into Library exactly once | Integration + operator test. Shell binding of the Downloads screen to `Downloads1` with catalog and `Main.qml` tests ([#233](https://github.com/LimeBeck/arinn/issues/233)). Partial, owner's manual check on the KDE HTPC (snapshot `1bd20d6`, 2026-10-06). The screen shows the real Transmission queue. A completed series download shows "Done · Added to the library", and Watch opens it for playback. The library on the device holds that series exactly once: one series row, 8 episodes, 8 distinct files (agent read-only count over SSH). Not yet confirmed on the device: pause/resume/start now/retry, and the error and service-unavailable states |
| AC-18 Home/Back table with standard keyboard input, Esc/Backspace exception | Pass (UI tests, CI) for the rows inside the shell. `tests/ui/tst_home_back.qml` (`HomeBackTable`) covers modal/sheet/navigation, nested screen, Home with and without a return Activity, and Backspace in a field. Home and Menu on every screen, including from a focused text field and an open Settings editor, are covered by `library_catalog_test` `menuOpensNavigationOverSectionScreens` and `SettingsView::test_home_from_editors_and_sheets` ([#334](https://github.com/LimeBeck/arinn/pull/334)). The external-application row is platform behaviour, covered by AC-05/G-0.3–G-0.4. Recorded 2026-10-06 ([#353](https://github.com/LimeBeck/arinn/issues/353)) |
| AC-19 Suspend, wake, lock screen and ambient (carried over G-0.8) | Operator test |
| G-5.1 UI matches the design handoff states and the 22 screenshots | Design review |
| G-5.2 Activity background capture lifecycle and budget (carried over G-0.7) | Measurement report |
| G-5.3 Arch/EndeavourOS package installs, starts and uninstalls cleanly | Packaging test |
| G-5.4 Performance budgets of spec §6.5 measured on the target device | [Measurement harness](docs/architecture/performance-harness.md) with offscreen/private-bus smoke checks; target-device measurement report remains pending |
| G-5.5 Operator acceptance of AC-01–AC-19 repeated on an X11/EWMH desktop (XFCE) in addition to KDE Plasma 6/Wayland (P-09, [ADR-0080](docs/adr/0080-mvp-platforms.md)) | Operator test on each platform |
| G-5.6 On-screen keyboard: every text field can be filled with the remote alone (spec §6.2, [ADR-0103](docs/adr/0103-on-screen-keyboard.md), [#418](https://github.com/LimeBeck/arinn/issues/418)) | Partial (UI and C++ tests, 2026-10-09, [ui-keyboard.md](docs/architecture/ui-keyboard.md)): Site or search, both shortcut editor fields and string settings type with the panel. Open: path fields (KB-5, [#426](https://github.com/LimeBeck/arinn/issues/426)), the pixel comparison with `kb-*.png` at 1080p and 4K, and the operator test with the remote |
| G-5.7 Quick menu: a global key (`quick_menu_shortcut`) opens a side sheet over the current Activity with Now playing, volume and mute, network status, Sleep and the power menu, Open Home and Close Arinn; Back returns to the same Activity; degraded without the daemon (spec §5.7, §6.2, §6.3, [#439](https://github.com/LimeBeck/arinn/issues/439)) | Open: Qt UI tests (open, Back returns, Open Home, focus scope, degraded state), adapter tests for the global key on KDE and on EWMH, the design handoff addition for the sheet (G-5.1), and the operator test on the HTPC with the remote |

## Phase 6 — Rich integrations (post-MVP)

**Status:** Planned. Starts after the Plugin API is declared stable (specification 1.0).

Candidates: Steam provider (library, artwork, recent games), ES-DE provider, Jellyfin provider, TV/webOS control, gamepad input, adding torrents from Arinn. Each must fit existing capabilities or go through an API review (R-09, R-10).

## Backlog

- Multi-monitor edge cases: hot unplug, mirrored outputs, moving the Activity while Home is open.
- KWin compositor restart while Arinn runs.
- Live compositor texture instead of the last frame.
