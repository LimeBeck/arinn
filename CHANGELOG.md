# Changelog

All notable changes to this project are documented here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). Arinn has no releases yet; entries are grouped by roadmap phase until the first release, after which [Semantic Versioning](https://semver.org/) applies.

## [Unreleased]

### Added

- Production `org.arinn.Daemon1`, application proxies and asynchronous operation objects with versioned XML, caller-bound paging, request deduplication and deadlines (ADR-0035).
- Systemd transient provider supervision, registration authentication, generation invalidation, daemon recovery and persisted restart diagnostics.
- Versioned TOML/SQLite storage, SHA-256 provider directories, ordered favorites and bounded launch recents.
- Production SDK demo fixtures and a private-bus contract suite covering recovery, limits, reload and Qt clients; package manifest and smoke-test integration.

- Phase 1 foundation: XDG plugin manifest discovery, deterministic overrides and duplicate diagnostics, bounded file parsing, executable/capability validation and a declarative reload plan.
- `arinnd --check-plugins`: offline JSON diagnostics without D-Bus or plugin execution; launch arguments are omitted from output.
- Permissive `arinn-plugin-sdk` snapshot primitives: immutable revision paging, actual D-Bus body-size checks, caller-bound expiring tokens and bounded leases. The production transport and SDK demo are integrated.

- English [specification v0.4](docs/specifications/arinn-specification.md), consolidating developer specification v0.3 with all accepted decisions, including acceptance criteria AC-01–AC-19 and open questions OQ-01–OQ-06.
- [ROADMAP.md](ROADMAP.md) with phase status, exit criteria and evidence links.
- ADR-0028 (documentation governance), ADR-0029 (licensing), ADR-0030 and ADR-0031 (clarifications previously edited into ADR-0023 and ADR-0026), ADR-0032 (KWin script adapter), and an ADR template.
- Licensing: `GPL-3.0-or-later` for the core, `MIT OR Apache-2.0` for `arinn-protocol` and interface XML, `CC-BY-SA-4.0` for documentation; SPDX headers, `REUSE.toml`, `LICENSES/`, `deny.toml`.
- Contributor documentation: README (English and Russian), CONTRIBUTING, CLA, AGENTS/CLAUDE, SECURITY, Code of Conduct, issue and pull request templates.
- CI/CD (ADR-0033): Rust build, tests and lints; Qt build and private-bus integration suite on Arch Linux; KWin adapter tests; REUSE, cargo-deny, documentation and version checks; Arch package build with a clean-install smoke test; nightly runs against the latest Arch packages; tag-gated releases to GitHub Releases; CLA check; Dependabot.
- Arch Linux `PKGBUILD`, package smoke test and `scripts/release.py`.

### Changed

- Shell, KDE adapter and CLI now use the production daemon API. CLI provides human/JSON output, paged diagnostics and capability commands; active Spike1 interfaces are removed.

- Documented the owner-approved parallel UI workstream alongside Phase 1: QML design system, Home, global navigation, local focus/input and fixture-based checks, with explicit API/open-question dependencies. Full UI acceptance remains in Phase 5; no implementation gate is closed by this scheduling decision.

- Specification v0.5 resolves OQ-06 with owner approval: provider data/state directories use the SHA-256 of `provider_id`, independent of `state.sqlite3` (ADR-0034). Provisioning and registry-reset recovery tests are implemented.

- All living documentation translated to English. ADR-0001–ADR-0019 accepted.
- `docs/architecture/review-map.md` and `docs/architecture/product-decisions.md` folded into the specification and the roadmap.

### Removed

- Third-party service logos from the design prototype and screenshots; replaced by neutral monograms, screenshots re-rendered from the prototype.

### Fixed

- Ambient auto-start condition made consistent ("external Activity **or** playing media").

## Phase 0 — architecture spike (2026-09-27)

### Added

- Rust daemon `arinnd`, demo provider, `arinnctl` and a Qt Quick shell connected over session D-Bus (`org.arinn.Spike1`, experimental).
- Temporary KWin script adapter: global Home (Meta+F12), Home on the Activity's output, focus return, recovery after daemon crashes.
- Session guard hiding the shell on lock and sleep.
- Activity background experiment with `WindowThumbnail` and near/far rendering in a separate Qt process.
- Phase 0 report, gate matrix, runbook and evidence.
