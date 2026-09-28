# Changelog

All notable changes to this project are documented here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). Arinn has no releases yet; entries are grouped by roadmap phase until the first release, after which [Semantic Versioning](https://semver.org/) applies.

Unreleased changes are not written here: every pull request adds a fragment under [`changes/`](changes/README.md), and `python scripts/release.py collect VERSION` moves them into a release section ([ADR-0036](docs/adr/0036-parallel-development.md)).

## [Unreleased]

## Phase 0 — architecture spike (2026-09-27)

### Added

- Rust daemon `arinnd`, demo provider, `arinnctl` and a Qt Quick shell connected over session D-Bus (`org.arinn.Spike1`, experimental).
- Temporary KWin script adapter: global Home (Meta+F12), Home on the Activity's output, focus return, recovery after daemon crashes.
- Session guard hiding the shell on lock and sleep.
- Activity background experiment with `WindowThumbnail` and near/far rendering in a separate Qt process.
- Phase 0 report, gate matrix, runbook and evidence.
