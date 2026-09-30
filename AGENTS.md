# Instructions for AI coding agents

This file is for AI coding agents (Codex, Claude Code and others). Everything in [CONTRIBUTING.md](CONTRIBUTING.md) applies to you as well; this file adds the rules that matter most for autonomous work.

## Task scope

Start from the user's request and stay within its scope:

- **Review or explanation:** inspect and report findings. Do not edit files, claim development tasks, or publish issues or pull requests unless requested.
- **Directed change:** implement the requested change. Use its existing issue or create a scoped Agent task issue after checking for overlap. Finish that task without selecting unrelated roadmap work.
- **Autonomous roadmap work:** select an open, unclaimed task for the current or next phase only when the user asks you to choose or continue roadmap work.

Honor an explicit local-only request: prepare and verify local changes without publishing them. If GitHub is unavailable, continue independent local work, record the intended zone and dependencies, and report the missing coordination before publishing or merging. Never claim that an issue was reserved, claimed or checked when it was not.

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

## Verification

[CONTRIBUTING.md — Required checks](CONTRIBUTING.md#required-checks) is the canonical list of commands and their scope. Use the documentation-only checks for Markdown/changelog changes and the full checks for code changes. Record failures, skipped checks and environmental limitations; a failed check is not a pass because an isolated rerun passed. Real-desktop operator tests always require explicit permission.

## Code discovery

Prefer codebase-memory-mcp graph tools for code discovery when available: `search_graph`, `trace_path`, `get_code_snippet`, then `query_graph` or `get_architecture`. Index an unindexed repository first. Check relevant snippets against the current checkout before editing; the graph may be stale. Use `rg` and file reads for strings, configuration and documentation, or when graph tools are unavailable, stale or insufficient. Report a tool limitation only when it affects the result; it does not block independent work.

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
- **SPDX coverage** on every new file according to [CONTRIBUTING.md](CONTRIBUTING.md#code-conventions) and the existing `REUSE.toml` policy: GPL for implementation code, permissive protocol/SDK/XML, CC-BY-SA for documentation. A file already covered by REUSE annotations does not need a conflicting header.
- **Secrets and privacy:** never log or commit real user URLs, tab titles, tokens, credentials or personal paths. Synthetic fixtures and public documentation links are allowed; runtime logging still follows specification §13. Never commit `.phase0/`, `build/` or `target/`.

## Autonomy

You are expected to work on your own ([ADR-0037](docs/adr/0037-agent-autonomy-and-owner-gated-decisions.md)). Do not wait for the owner for anything that is not owner-gated.

**Do yourself:**

- **Find or create the task within the authorized scope.** Follow *Task scope* above. Search open issues and pull requests before claiming or creating a task; record its zone, dependencies, requirement IDs and reserved ADR number in the Agent task template.
- **Claim it:** add `agent:<your name>` and comment "claimed" with your branch name. A claim with no activity for 48 hours may be taken over after commenting on the issue.
- **Decide technical details** inside the specification and accepted ADRs; record significant technical choices as ADRs accepted under delegated authority.
- **Finish the pull request:** merge `master` into it, fix CI within the task's zone, answer and resolve review threads, and update your gate rows in `ROADMAP.md` with evidence. For inherited or unrelated failures outside the zone, follow the scope procedure in *Parallel work* before editing. Record unresolved failures as blockers; do not silently enlarge the change or mark it done.
- **Merge it yourself** (squash, then delete the branch and close the issue) when every CI, changelog and CLA check is green on a branch up to date with `master`, no review thread is unresolved, there is no `needs-owner` label, and the pull request touches no owner-gated area.

**Owner-gated — label `needs-owner`, explain, recommend, move on:** open questions and product scope (acceptance criteria, MVP, product behaviour in the specification); marking a phase Done; non-technical ADRs (product, licensing, security posture, process); licensing files and policy (`LICENSE*`, `REUSE.toml`, `CLA.md`, `deny.toml` license policy); releases, version bumps and tags; operator tests on a real desktop, HTPC or TV; repository settings, branch protection, secrets and history rewrites on `master`; the rules for agents themselves (`AGENTS.md`, `CONTRIBUTING.md`, `CODEOWNERS`, merge/release/CLA workflows). The owner merges those pull requests.

Fixing implementation to meet an existing requirement is delegated work; changing the intended product behaviour or acceptance criterion needs the owner. Update technical documentation and implementation evidence as needed. If the specification already describes the intended behaviour accurately, reference it rather than rewriting it for a bug fix. A correction inside a product-behaviour section of the specification still receives owner review. An explicit owner request authorizes preparing the requested owner-gated change, but does not waive owner-only merging or authorize unrelated decisions.

## Parallel work

Other agents work on this repository at the same time. Follow *Parallel work* in [CONTRIBUTING.md](CONTRIBUTING.md#parallel-work) ([ADR-0036](docs/adr/0036-parallel-development.md)). In short:

- **One claimed issue, one branch, one pull request.**
- **Change only paths in the issue's zone.** For a necessary dependency or CI fix outside it, check open issues and pull requests first. Widen the zone with a recorded rationale before editing if there is no active overlap and no contract/owner boundary is crossed. Otherwise create or reuse a separate task, integrate its fix after it lands, and record the dependency. An active overlapping claim or owner-gated decision is not bypassed by widening a zone.
- **Contracts** (`crates/arinn-protocol`, SDK, `docs/plugin-api/`) change only in a `contract-change` task; never alongside other work, and only one such pull request is open at a time.
- **ADR numbers** come from the issue. Follow the reservation and collision procedure in [CONTRIBUTING.md](CONTRIBUTING.md#parallel-work); `python scripts/adr.py next` suggests a number but does not reserve it. Run `python scripts/adr.py register` after adding or amending an ADR.
- **Do not bump the specification version, edit other tasks' gates in `ROADMAP.md`, or resolve `Cargo.lock` by hand.**
- **Never push to another agent's branch.** Take over an issue only through the stale-claim rule.
- **Labels** come from `.github/labels.yml`; add new ones there, never by hand.

## Working style

- Prefer small, reviewable commits that each build and pass tests.
- When a requirement is ambiguous or two documents disagree, report the conflict with file and line references and pause only the affected work. Continue independent work within the authorized task; do not choose unrelated tasks automatically.
- Reference requirement IDs (AC-xx, G-x.y, R-xx, OQ-xx) in commit bodies and pull request descriptions.
- Add a `Co-Authored-By:` trailer naming the agent to your commits.
