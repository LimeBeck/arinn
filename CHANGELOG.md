# Changelog

All notable changes to this project are documented here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). Arinn has no releases yet; entries are grouped by roadmap phase until the first release, after which [Semantic Versioning](https://semver.org/) applies.

## [Unreleased]

### Added

- English [specification v0.4](docs/specifications/arinn-specification.md), consolidating developer specification v0.3 with all accepted decisions, including acceptance criteria AC-01–AC-19 and open questions OQ-01–OQ-06.
- [ROADMAP.md](ROADMAP.md) with phase status, exit criteria and evidence links.
- ADR-0028 (documentation governance), ADR-0029 (licensing), ADR-0030 and ADR-0031 (clarifications previously edited into ADR-0023 and ADR-0026), ADR-0032 (KWin script adapter), and an ADR template.
- Licensing: `GPL-3.0-or-later` for the core, `MIT OR Apache-2.0` for `arinn-protocol` and interface XML, `CC-BY-SA-4.0` for documentation; SPDX headers, `REUSE.toml`, `LICENSES/`, `deny.toml`.
- Contributor documentation: README (English and Russian), CONTRIBUTING, CLA, AGENTS/CLAUDE, SECURITY, Code of Conduct, issue and pull request templates.
- CI: Rust build, tests and lints; Qt build and private-bus integration suite; KWin adapter tests; REUSE, cargo-deny and documentation checks; CLA check.

### Changed

- All living documentation translated to English. ADR-0001–ADR-0019 accepted.
- `docs/architecture/review-map.md` and `docs/architecture/product-decisions.md` folded into the specification and the roadmap.

### Fixed

- Ambient auto-start condition made consistent ("external Activity **or** playing media").

## Phase 0 — architecture spike (2026-09-27)

### Added

- Rust daemon `arinnd`, demo provider, `arinnctl` and a Qt Quick shell connected over session D-Bus (`org.arinn.Spike1`, experimental).
- Temporary KWin script adapter: global Home (Meta+F12), Home on the Activity's output, focus return, recovery after daemon crashes.
- Session guard hiding the shell on lock and sleep.
- Activity background experiment with `WindowThumbnail` and near/far rendering in a separate Qt process.
- Phase 0 report, gate matrix, runbook and evidence.
