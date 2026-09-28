# Contributing to Arinn

Thank you for helping build Arinn. These rules apply to everyone — human contributors and AI coding agents alike. Agents additionally follow [AGENTS.md](AGENTS.md).

## Before you start

1. Read the [specification](docs/specifications/arinn-specification.md) (what Arinn must do) and the [roadmap](ROADMAP.md) (what is being built now).
2. For anything larger than a small fix, open an issue first and agree on the approach. Changes to architecture, the Plugin API or product scope need an [ADR](docs/adr/README.md).
3. Check the [open questions](docs/specifications/arinn-specification.md#21-open-questions): work that depends on an undecided question waits for the decision.

## Development setup

Requirements: Rust (edition 2024) with `rustfmt` and `clippy`, CMake ≥ 3.24, Ninja, a C++20 compiler, Qt 6 ≥ 6.6 (Quick, Quick Controls, DBus, Test), Python 3, Node.js, `dbus-run-session`. Live platform tests additionally need KDE Plasma 6 on Wayland, a systemd user session and `qdbus6`.

```sh
cargo build --locked
cmake -S . -B build -G Ninja
cmake --build build
```

Running Arinn and the operator tests is described in the [Phase 0 runbook](docs/architecture/phase-0-runbook.md).

## Required checks

Run these before opening a pull request; CI runs the same checks.

```sh
cargo fmt --all --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cmake --build build && dbus-run-session -- python tests/integration/phase0.py
node --test tests/integration/kwin_adapter.test.cjs
python scripts/check-docs.py
python scripts/release.py check
reuse lint
cargo deny check licenses
```

To test packaging locally on Arch: `git archive --prefix=arinn-$(python scripts/release.py version)/ -o packaging/arch/arinn-$(python scripts/release.py version).tar.gz HEAD && (cd packaging/arch && makepkg -f)`, then install the package and run `dbus-run-session -- python tests/package/smoke.py`.

**Operator tests** (`tests/integration/kwin_*.py`, `scripts/phase0-session.py`) drive a real desktop session: they load KWin scripts, kill daemons or lock the screen. They never run in CI. Run them only on a disposable session and only when the change touches the platform layer; attach the result (evidence JSON or a short log) to the pull request.

## Continuous integration

Every pull request runs the full pipeline described in [ADR-0033](docs/adr/0033-ci-cd-and-releases.md): Rust, the Qt build and integration suite on Arch Linux, licensing, documentation, and an Arch package that is built, installed into a clean system and smoke-tested. The same pipeline runs nightly on `master` against the latest Arch packages. A pull request is merged only when every check is green; a failing nightly is fixed before anything else is released.

## Releases

Only the maintainer releases. To release version `X.Y.Z`:

1. In a pull request, set `version` in the root `Cargo.toml` and `pkgver` in `packaging/arch/PKGBUILD` to `X.Y.Z`; run `python scripts/release.py collect X.Y.Z` to move the changelog fragments into a `## [X.Y.Z]` section; bump the specification version and update its §22 if requirements changed since the last version. Run `python scripts/release.py check vX.Y.Z`.
2. If the release touches the platform layer, input or session behaviour, run the relevant operator tests on a real KDE session and attach the evidence to that pull request.
3. Merge, make sure the latest nightly is green, then tag the merge commit: `git tag -s vX.Y.Z -m "Arinn X.Y.Z" && git push origin vX.Y.Z`.
4. The release workflow reruns the full CI on the tag and publishes the GitHub Release with the Arch package, source tarball and checksums. `0.x` versions are marked as pre-releases.

## Parallel work

Several agents and people work on Arinn at the same time. These rules keep them out of each other's way ([ADR-0034](docs/adr/0034-parallel-development.md)):

1. **One issue per task.** Work starts from an issue created with the *Agent task* template: goal, requirement IDs, **zone** (paths the task may change), shared files, dependencies and a reserved ADR number if needed. Claim it by assigning yourself and adding the label `agent:<name>` (for example `agent:claude`, `agent:codex`); never take a claimed issue. One issue = one branch = one pull request.
2. **Stay in your zone.** A change outside the zone goes into a separate issue, or is agreed in the issue first.
3. **Contracts first, one at a time.** Changes to `crates/arinn-protocol`, the plugin SDK or `docs/plugin-api/` are separate pull requests labelled `contract-change`; only one is open at a time, and dependent tasks start after it merges.
4. **Shared files without conflicts:**
   - changelog: add a fragment under [`changes/`](changes/README.md), never edit `CHANGELOG.md`;
   - ADRs: use the number reserved in the issue; regenerate the register with `python scripts/adr.py register`;
   - specification: change text only; the version is bumped at release;
   - roadmap: change only the rows of your own gates;
   - `Cargo.lock`: on a conflict, take `master`'s version and let `cargo build` regenerate it; never merge it by hand.
5. **Stay current.** Merge `master` into your branch regularly and before asking for review; the branch must be up to date to merge. Never rewrite another branch's history.
6. **Isolate your workspace.** Use your own clone, worktree or cloud session with its own `target/` and `build/`. Automated tests use private D-Bus buses and can run in parallel; operator tests cannot and need the owner's permission.

Labels used by this process: `task`, `agent:<name>`, `blocked`, `contract-change`, `skip-changelog`.

## Branches and commits

- Branch from `master`. Names: `feature/<topic>`, `fix/<topic>`, `docs/<topic>`, `chore/<topic>`. Agent branches may use the agent's prefix (for example `claude/…`, `codex/…`).
- Keep commits focused. Subject line in the imperative mood, at most 72 characters, no trailing period: `Add paging to provider snapshots`.
- The body explains **why** and references requirement IDs where relevant: `Implements G-1.1, AC-11.`
- Never rewrite history that others have pulled. Rebase your own unpublished branch as you like.

## Pull requests

- One topic per pull request; small pull requests get reviewed faster.
- Fill in the [pull request template](.github/PULL_REQUEST_TEMPLATE.md): the requirement IDs it touches, how it was tested and which documents were updated.
- CI must be green and the CLA must be signed ([Licensing](#licensing-and-the-cla)).
- The maintainer reviews every pull request against the [architecture rules](docs/specifications/arinn-specification.md#20-architecture-rules) R-01–R-10.
- Pull requests are merged with **squash merge**; the pull request title becomes the commit subject, so write it like one.

## Requirement traceability

Requirements have stable IDs: AC-xx (acceptance criteria), R-xx (architecture rules), G-x.y (phase gates), OQ-xx (open questions), P-xx (product decisions), DR-xx (design reconciliation). Reference them in pull requests, commit bodies and test names where it helps (`test_ac11_provider_reload`). Never mark a criterion as passed without linked evidence.

## Documentation rules

The full rules are [ADR-0028](docs/adr/0028-documentation-governance.md). In practice:

| When you… | Update in the same pull request |
| --- | --- |
| change behaviour a user or plugin author can observe | the [specification](docs/specifications/arinn-specification.md) (text only; the version is bumped at release) and a [changelog fragment](changes/README.md) |
| make or change an architectural decision | a new ADR (never edit an accepted one) and the specification |
| complete a gate, move an item or change a phase status | [ROADMAP.md](ROADMAP.md) with evidence links |
| change a technical contract (IPC, runtime, persistence) | the matching document in `docs/architecture/` or `docs/plugin-api/` |
| change build steps or requirements | README, this file and the runbook |

- **Living documents** are English and kept current. **Historical sources** are never edited: `docs/specifications/developer-specification-v0.3.md`, `docs/design/handoff/`, `docs/design/archive/`, `docs/design/brand/brand-guide-v0.1.txt`.
- Link to the source of truth instead of copying requirements. Duplicated text drifts.
- State facts precisely: "not tested" is better than an optimistic guess. Accepting a decision never means it has been verified.

## Code conventions

- **Rust:** `rustfmt` defaults, `clippy -D warnings`, `anyhow` in binaries, typed errors at API boundaries. No blocking calls on async runtimes.
- **C++/QML:** C++20; the UI thread never makes blocking D-Bus calls; QML contains no product-specific business logic (AC-12) and never runs commands.
- **All code:** the core knows capabilities, not products (R-01). IDs from providers are opaque. Validate every IPC input. Never log URLs, titles, payloads or secrets.
- **Files:** UTF-8, LF, final newline ([.editorconfig](.editorconfig)). Every new source file starts with an SPDX header:

  ```text
  SPDX-FileCopyrightText: 2026 Arinn contributors
  SPDX-License-Identifier: GPL-3.0-or-later
  ```

  Use `MIT OR Apache-2.0` in `crates/arinn-protocol`, the plugin SDK and interface XML. Files that cannot carry a header are covered in [REUSE.toml](REUSE.toml).

## Tests

- Unit tests for daemon logic, parsing and routing; the shared contract suite for every provider; integration tests on a private D-Bus (`dbus-run-session`); Qt Quick Test for focus and navigation.
- A bug fix comes with a test that fails without the fix.
- Tests never depend on the developer's real desktop, browser profile or media files.

## Licensing and the CLA

Arinn is `GPL-3.0-or-later`; the protocol crate and SDK are `MIT OR Apache-2.0`; documentation is `CC-BY-SA-4.0` ([ADR-0029](docs/adr/0029-licensing.md)). Before your first pull request can be merged you sign the [Contributor License Agreement](CLA.md) once: the CLA Assistant bot comments on the pull request and you reply with the sentence it asks for.

Only add dependencies whose licenses pass `cargo deny check licenses`. Never add third-party code, fonts, icons or images without recording their source and license, and never commit third-party logos or trademarks.

## AI-assisted contributions

AI tools are welcome under the same rules as everyone else. The person or agent opening the pull request is responsible for the whole change: it must build, pass the checks and follow these rules. Credit the tool with a `Co-Authored-By:` trailer. Agents follow [AGENTS.md](AGENTS.md). If you run an agent yourself, its commits count as your contribution: sign the CLA with your own account. Agents operated by the maintainer are on the CLA allowlist ([ADR-0034](docs/adr/0034-parallel-development.md)).

## Language

Code, comments, commit messages, pull requests and documentation are written in English. Issues and discussions may be in English or Russian.

## Definition of Done

A change is done when:

- [ ] it implements what the issue or roadmap item asks, and nothing unrelated;
- [ ] all required checks pass locally and in CI;
- [ ] tests cover the new behaviour; a bug fix has a regression test that fails without the fix;
- [ ] the specification, ADRs, roadmap and technical documents are updated as the table above requires, and a changelog fragment is added (or the pull request is labelled `skip-changelog`);
- [ ] new files have SPDX headers;
- [ ] the CLA is signed.

## Repository settings (maintainer)

These GitHub settings make the rules above enforceable:

- **Branches → `master`:** require a pull request; require status checks *Rust*, *Qt shell and private-bus integration*, *Licensing*, *Documentation consistency*, *Changelog fragment*, *Package / Build Arch package*, *Package / Install and smoke-test package* and *CLAAssistant*; require branches to be up to date before merging; block force pushes and deletion. Do not require code owner review (the maintainer account also opens agent pull requests).
- **Actions → General:** workflow permissions "Read and write" (the CLA bot stores signatures in the `cla-signatures` branch).
- **Security:** private vulnerability reporting, Dependabot alerts and secret scanning on.
- **Labels:** create the labels listed under *Parallel work*.

## Code of conduct and security

Everyone follows the [Code of Conduct](CODE_OF_CONDUCT.md). Report vulnerabilities privately as described in [SECURITY.md](SECURITY.md), never in a public issue.
