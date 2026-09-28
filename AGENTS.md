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
- **Never decide an open question (OQ-xx) or any other owner-gated matter yourself** ([ADR-0037](docs/adr/0037-agent-autonomy-and-owner-gated-decisions.md)). Label the issue or pull request `needs-owner`, state your recommendation and the options, and continue with other work. Record the owner's answer in the specification (and an ADR if architectural).
- **Never claim verification you did not run.** Write "not tested" or "not run" and say why. Do not mark acceptance criteria or gates as passed without evidence.
- **Keep the architecture rules R-01–R-10:** no product names in core routing or QML business logic, no direct calls from the shell to third-party applications, no plugin-supplied UI, platform code only in the platform layer.
- **Keep documentation in sync in the same change:** specification, ADRs, roadmap and technical documents as the table in CONTRIBUTING.md requires, plus a changelog fragment in `changes/` — never edit `CHANGELOG.md` directly.
- **English** for code, comments, commits, pull requests and documentation. Answer the user in the language they write in.
- **Never create release tags or publish releases**; only the maintainer releases (CONTRIBUTING.md, *Releases*).
- **Every bug fix needs a regression test** that fails without the fix.
- **SPDX header** on every new file (`GPL-3.0-or-later`; `MIT OR Apache-2.0` in the protocol crate, SDK and interface XML).
- **Secrets and privacy:** never log or commit URLs, tab titles, tokens, credentials or personal paths; never commit `.phase0/`, `build/` or `target/`.

## Autonomy

You are expected to work on your own ([ADR-0037](docs/adr/0037-agent-autonomy-and-owner-gated-decisions.md)). Do not wait for the owner for anything that is not owner-gated.

**Do yourself:**

- **Find or create the task.** Pick an open, unclaimed Agent task issue for the current or next roadmap phase. If none fits, create one from the roadmap scope and exit criteria (Agent task template): zone, dependencies, requirement IDs, reserved ADR number. Search open issues and pull requests first so tasks and zones do not overlap.
- **Claim it:** add `agent:<your name>` and comment "claimed" with your branch name. A claim with no activity for 48 hours may be taken over after commenting on the issue.
- **Decide technical details** inside the specification and accepted ADRs; record significant technical choices as ADRs accepted under delegated authority.
- **Finish the pull request:** merge `master` into it, fix CI (also failures inherited from `master`, by porting the fix), answer and resolve review threads, update your gate rows in `ROADMAP.md` with evidence, open follow-up issues for problems you find.
- **Merge it yourself** (squash, then delete the branch and close the issue) when every CI, changelog and CLA check is green on a branch up to date with `master`, no review thread is unresolved, there is no `needs-owner` label, and the pull request touches no owner-gated area.

**Owner-gated — label `needs-owner`, explain, recommend, move on:** open questions and product scope (acceptance criteria, MVP, product behaviour in the specification); marking a phase Done; non-technical ADRs (product, licensing, security posture, process); licensing files and policy (`LICENSE*`, `REUSE.toml`, `CLA.md`, `deny.toml` license policy); releases, version bumps and tags; operator tests on a real desktop, HTPC or TV; repository settings, branch protection, secrets and history rewrites on `master`; the rules for agents themselves (`AGENTS.md`, `CONTRIBUTING.md`, `CODEOWNERS`, merge/release/CLA workflows). The owner merges those pull requests.

## Parallel work

Other agents work on this repository at the same time. Follow *Parallel work* in [CONTRIBUTING.md](CONTRIBUTING.md#parallel-work) ([ADR-0036](docs/adr/0036-parallel-development.md)). In short:

- **One claimed issue, one branch, one pull request.**
- **Change only paths in the issue's zone.** If you must touch something else, create a separate task or widen the zone in the issue with a comment before editing.
- **Contracts** (`crates/arinn-protocol`, SDK, `docs/plugin-api/`) change only in a `contract-change` task; never alongside other work, and only one such pull request is open at a time.
- **ADR numbers** come from the issue (`python scripts/adr.py next` when reserving, after checking other open tasks); run `python scripts/adr.py register` after adding or amending an ADR.
- **Do not bump the specification version, edit other tasks' gates in `ROADMAP.md`, or resolve `Cargo.lock` by hand.**
- **Never push to another agent's branch.** Take over an issue only through the stale-claim rule.
- **Labels** come from `.github/labels.yml`; add new ones there, never by hand.

## Working style

- Prefer small, reviewable commits that each build and pass tests.
- When a requirement is ambiguous or two documents disagree, stop and report the conflict with file and line references instead of guessing.
- Reference requirement IDs (AC-xx, G-x.y, R-xx, OQ-xx) in commit bodies and pull request descriptions.
- Add a `Co-Authored-By:` trailer naming the agent to your commits.
