# Changelog

All notable changes to this project are documented here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). Arinn has no releases yet; entries are grouped by roadmap phase until the first release, after which [Semantic Versioning](https://semver.org/) applies.

Unreleased changes are not written here: every pull request adds a fragment under [`changes/`](changes/README.md), and `python scripts/release.py collect VERSION` moves them into a release section ([ADR-0036](docs/adr/0036-parallel-development.md)).

## [Unreleased]

## [0.1.0] - 2026-09-28

### Added

- Reusable QML UI foundation: Onest typography, design tokens, logical canvas, buttons, content cards, app tiles, progress and status components; isolated fixture gallery and offscreen Qt Quick component tests. Production screen integration remains pending.
<!-- SPDX-FileCopyrightText: 2026 Arinn contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

- Add a standalone Home preview with Continue, Favorites and New rows, global
navigation, local Home/Back and focus restoration, explicit failure/loading
fixtures, and Qt Quick navigation tests. Production provider binding is pending.
- Add the desktop application provider with bounded XDG discovery, system GIO launch in independent application units, exact window association, activation and graceful close through the production KWin platform backend. Private-bus, adapter and package fixtures cover integration and lifetime handling; real KDE/cgroup qualification remains outstanding.
<!-- SPDX-FileCopyrightText: 2026 Arinn contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

- Define the pre-stable platform window snapshot, activation and graceful-close contract, with typed protocol and SDK bindings and private-bus fixtures. Production daemon/KDE implementation remains pending (ADR-0039).
- Define the internal KWin inventory/control wire contract and SDK registration contexts for desktop-provider integration, with isolated contract and lifecycle tests. Production backend and KDE qualification remain pending (#24, G-1.5, G-2.1–G-2.3).
<!-- SPDX-FileCopyrightText: 2026 Arinn contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

- Add Home, Library and player design screenshots to the English and Russian READMEs, with captions distinguishing the target design from the implemented Qt shell.
- English [specification v0.4](docs/specifications/arinn-specification.md), consolidating developer specification v0.3 with all accepted decisions, including acceptance criteria AC-01–AC-19 and open questions OQ-01–OQ-06.
- [ROADMAP.md](ROADMAP.md) with phase status, exit criteria and evidence links.
- ADR-0028 (documentation governance), ADR-0029 (licensing), ADR-0030 and ADR-0031 (clarifications previously edited into ADR-0023 and ADR-0026), ADR-0032 (KWin script adapter), and an ADR template.
- Licensing: `GPL-3.0-or-later` for the core, `MIT OR Apache-2.0` for `arinn-protocol` and interface XML, `CC-BY-SA-4.0` for documentation; SPDX headers, `REUSE.toml`, `LICENSES/`, `deny.toml`.
- Contributor documentation: README (English and Russian), CONTRIBUTING, CLA, AGENTS/CLAUDE, SECURITY, Code of Conduct, issue and pull request templates.
- CI/CD (ADR-0033): Rust build, tests and lints; Qt build and private-bus integration suite on Arch Linux; KWin adapter tests; REUSE, cargo-deny, documentation and version checks; Arch package build with a clean-install smoke test; nightly runs against the latest Arch packages; tag-gated releases to GitHub Releases; CLA check; Dependabot.
- Arch Linux `PKGBUILD`, package smoke test and `scripts/release.py`.
- Rules and tooling for parallel development by several agents ([ADR-0036](docs/adr/0036-parallel-development.md)): changelog fragments, reserved ADR numbers with a generated register, work zones and an agent task issue template.
- Production `org.arinn.Daemon1`, application proxies and asynchronous operation objects with versioned XML, caller-bound paging, request deduplication and deadlines (ADR-0035).
- Systemd transient provider supervision, registration authentication, generation invalidation, daemon recovery and persisted restart diagnostics.
- Versioned TOML/SQLite storage, SHA-256 provider directories, ordered favorites and bounded launch recents.
- Production SDK demo fixtures and a private-bus contract suite covering recovery, limits, reload and Qt clients; package manifest and smoke-test integration.
- Phase 1 foundation: XDG plugin manifest discovery, deterministic overrides and duplicate diagnostics, bounded file parsing, executable/capability validation and a declarative reload plan.
- `arinnd --check-plugins`: offline JSON diagnostics without D-Bus or plugin execution; launch arguments are omitted from output.
- Permissive `arinn-plugin-sdk` snapshot primitives: immutable revision paging, actual D-Bus body-size checks, caller-bound expiring tokens and bounded leases. The production transport and SDK demo are integrated.

### Changed

- Agents work autonomously ([ADR-0037](docs/adr/0037-agent-autonomy-and-owner-gated-decisions.md)): they create and claim their own tasks from the roadmap, take over stale claims after 48 hours, and merge their own pull requests when all checks are green; open questions, product scope, licensing, releases, operator tests, repository administration and agent rules stay with the owner and are marked `needs-owner`. Repository labels are declared in `.github/labels.yml` and synced by a workflow.
<!-- SPDX-FileCopyrightText: 2026 Arinn contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

- Resolve OQ-03: the desktop ApplicationProvider owns browser launch; the Chrome provider declares only `browser`, with entities linked through validated platform window tokens ([ADR-0038](docs/adr/0038-browser-launch-ownership.md)). Implementation and Phase 2/3 acceptance remain pending.
<!-- SPDX-FileCopyrightText: 2026 Arinn contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

- Record the Phase 1 shell capability-boundary review and the unresolved Daemon1-only gate conflict with the direct session lifecycle guard; Phase 1 remains in progress.
<!-- SPDX-FileCopyrightText: 2026 Arinn contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

- Clarify the owner-approved shell boundary: application/provider integration uses Daemon1, while fail-closed lock/sleep observation remains daemon-independent; track platform transport isolation separately (ADR-0041).
<!-- SPDX-FileCopyrightText: 2026 Arinn contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

- Isolate asynchronous lock/sleep observation in the platform session layer while preserving daemon-independent shell hiding; add private-bus coverage for stale lock replies and absent or unresponsive daemon services (R-05, ADR-0041).
<!-- SPDX-FileCopyrightText: 2026 Arinn contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

- Update GitHub Actions for checkout, Python setup, artifact upload/download, REUSE checks and release publishing to current stable major versions.
- Make the private-bus desktop activation test wait for the application to handle dispatched activation, with a gated fixture that covers completion after the provider disconnects.
<!-- SPDX-FileCopyrightText: 2026 Arinn contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

- Record verified Phase 1 branch protection and completed platform lifecycle extraction; identify the remaining v0.1.0 release preparation and publication gate.
- Record successful Phase 1 CI and clean Arch package checks, with source links and the remaining branch-protection and maintainer release requirements.
- All living documentation translated to English. ADR-0001–ADR-0019 accepted.
- `docs/architecture/review-map.md` and `docs/architecture/product-decisions.md` folded into the specification and the roadmap.
- Shell, KDE adapter and CLI now use the production daemon API. CLI provides human/JSON output, paged diagnostics and capability commands; active Spike1 interfaces are removed.
- Documented the owner-approved parallel UI workstream alongside Phase 1: QML design system, Home, global navigation, local focus/input and fixture-based checks, with explicit API/open-question dependencies. Full UI acceptance remains in Phase 5; no implementation gate is closed by this scheduling decision.
- Specification v0.5 resolves OQ-06 with owner approval: provider data/state directories use the SHA-256 of `provider_id`, independent of `state.sqlite3` (ADR-0034). Provisioning and registry-reset recovery tests are implemented.

### Removed

- Third-party service logos from the design prototype and screenshots; replaced by neutral monograms, screenshots re-rendered from the prototype.

### Fixed

- Declare Qt SVG as a required build and Arch package dependency, and install it in clean CI environments so the UI artwork loading and pixel regression test can decode SVG fixtures. Missing Qt SVG now fails at configuration instead of during UI tests.
<!-- SPDX-FileCopyrightText: 2026 Arinn contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

- Isolate the Home navigation tests from pointer positions left by previous fixtures, preventing intermittent focus failures in CI.
- Ambient auto-start condition made consistent ("external Activity **or** playing media").

## Phase 0 — architecture spike (2026-09-27)

### Added

- Rust daemon `arinnd`, demo provider, `arinnctl` and a Qt Quick shell connected over session D-Bus (`org.arinn.Spike1`, experimental).
- Temporary KWin script adapter: global Home (Meta+F12), Home on the Activity's output, focus return, recovery after daemon crashes.
- Session guard hiding the shell on lock and sleep.
- Activity background experiment with `WindowThumbnail` and near/far rendering in a separate Qt process.
- Phase 0 report, gate matrix, runbook and evidence.
