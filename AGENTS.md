# Instructions for AI coding agents

This file is for AI coding agents (Codex, Claude Code and others). Everything in [CONTRIBUTING.md](CONTRIBUTING.md) applies to you as well; this file adds the rules that matter most for autonomous work.

## Read first

1. [Specification](docs/specifications/arinn-specification.md) — current requirements; the only source of truth for behaviour.
2. [ROADMAP.md](ROADMAP.md) — the current phase, its exit criteria and the open questions it depends on.
3. [ADR register](docs/adr/README.md) — why things are the way they are.
4. The technical contract for the area you touch: `docs/architecture/`, `docs/plugin-api/`.

## Project map

| Path | Content |
| --- | --- |
| `crates/arinnd` | Daemon (Rust, Tokio, zbus) |
| `crates/arinn-protocol` | Shared D-Bus types — licensed `MIT OR Apache-2.0` |
| `plugins/demo` | Demo provider |
| `tools/arinnctl` | CLI |
| `apps/arinn-shell` | Qt 6 / QML shell with a C++ QtDBus layer |
| `platform/kde` | KWin script adapter |
| `changes/` | Changelog fragments, one per pull request |
| `tests/package` | Smoke test of the installed Arch package |
| `packaging/arch` | Arch Linux PKGBUILD |
| `.github/workflows` | CI, package, release and CLA pipelines ([ADR-0033](docs/adr/0033-ci-cd-and-releases.md)) |
| `tests/integration` | Private-bus suite (`phase0.py`), JS adapter tests, operator tests (`kwin_*.py`) |
| `docs/` | Specification, ADRs, architecture, plugin API, design |

## Commands

```sh
cargo build --locked
cargo fmt --all --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cmake -S . -B build -G Ninja && cmake --build build
dbus-run-session -- python tests/integration/phase0.py
node --test tests/integration/kwin_adapter.test.cjs
python scripts/check-docs.py
python scripts/adr.py check
python scripts/release.py check
reuse lint
cargo deny check licenses
```

## Hard rules

- **Never run operator tests without explicit permission from the user.** `scripts/phase0-session.py` and `tests/integration/kwin_*.py` load KWin scripts, kill services, change window placement or **lock the real screen**.
- **Never edit historical sources:** `docs/specifications/developer-specification-v0.3.md`, `docs/design/handoff/`, `docs/design/archive/`, `docs/design/brand/brand-guide-v0.1.txt`.
- **Never edit an accepted ADR** except its *Amended by / Superseded by* field or a typo. Write a new ADR from `docs/adr/template.md`.
- **Never decide an open question (OQ-xx) yourself.** Ask the user; record the answer in the specification (and an ADR if architectural).
- **Never claim verification you did not run.** Write "not tested" or "not run" and say why. Do not mark acceptance criteria or gates as passed without evidence.
- **Keep the architecture rules R-01–R-10:** no product names in core routing or QML business logic, no direct calls from the shell to third-party applications, no plugin-supplied UI, platform code only in the platform layer.
- **Keep documentation in sync in the same change:** specification, ADRs, roadmap and technical documents as the table in CONTRIBUTING.md requires, plus a changelog fragment in `changes/` — never edit `CHANGELOG.md` directly.
- **English** for code, comments, commits, pull requests and documentation. Answer the user in the language they write in.
- **Never create release tags or publish releases**; only the maintainer releases (CONTRIBUTING.md, *Releases*).
- **Every bug fix needs a regression test** that fails without the fix.
- **SPDX header** on every new file (`GPL-3.0-or-later`; `MIT OR Apache-2.0` in the protocol crate, SDK and interface XML).
- **Secrets and privacy:** never log or commit URLs, tab titles, tokens, credentials or personal paths; never commit `.phase0/`, `build/` or `target/`.

## Parallel work

Other agents work on this repository at the same time. Follow *Parallel work* in [CONTRIBUTING.md](CONTRIBUTING.md#parallel-work) ([ADR-0034](docs/adr/0034-parallel-development.md)). In short:

- **Work only on a claimed issue** (Agent task template): assign it, add `agent:<your name>`, use one branch and one pull request for it. If there is no issue, ask the user to create or approve one.
- **Change only paths in the issue's zone.** If you must touch something else, stop and say so in the issue instead of editing it.
- **Contracts** (`crates/arinn-protocol`, SDK, `docs/plugin-api/`) change only in a `contract-change` task; never alongside other work.
- **ADR numbers** come from the issue (`python scripts/adr.py next` only when reserving); run `python scripts/adr.py register` after adding or amending an ADR.
- **Do not bump the specification version, edit other gates in `ROADMAP.md`, or resolve `Cargo.lock` by hand.**
- **Merge `master` into your branch** before opening the pull request and whenever it conflicts; re-run the checks after merging.
- **Never push to another agent's branch** or take over its issue.

## Working style

- Prefer small, reviewable commits that each build and pass tests.
- When a requirement is ambiguous or two documents disagree, stop and report the conflict with file and line references instead of guessing.
- Reference requirement IDs (AC-xx, G-x.y, R-xx, OQ-xx) in commit bodies and pull request descriptions.
- Add a `Co-Authored-By:` trailer naming the agent to your commits.
