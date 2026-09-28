# ADR-0033: CI/CD pipeline and release policy

- **Status:** Accepted
- **Date:** 2026-09-27
- **Deciders:** project owner (requirement: no regressions reaching users)
- **Amends / Supersedes:** amends [ADR-0018](0018-demo-testing-gates.md)
- **Amended by / Superseded by:** [ADR-0036](0036-parallel-development.md) — changelog fragments; up-to-date branches before merge
- **Specification:** [§17, §18](../specifications/arinn-specification.md)

## Context

Arinn targets a rolling-release distribution (Arch Linux / EndeavourOS), where Qt, KDE, Rust and system libraries change weekly. Users must not receive builds that regress, and a dependency update must not silently break an installed Arinn. Some checks need a real KDE session and a TV and cannot run in hosted CI.

## Decision

**Continuous integration** runs on every pull request, every push to `master` and manually:

| Job | What it proves |
| --- | --- |
| Rust | `fmt`, `clippy -D warnings`, `cargo test` on stable Rust |
| Integration (Arch Linux) | Qt shell builds against current Arch Qt; KWin adapter tests; private-bus integration suite (daemon, provider, CLI, shell) |
| Licensing | `reuse lint`, `cargo deny check licenses` |
| Documentation | `scripts/check-docs.py`, version consistency (`scripts/release.py check`) |
| Package | the Arch package builds with `makepkg` (including `check()`), installs into a clean container, passes `tests/package/smoke.py` and uninstalls cleanly |

**Nightly:** the same pipeline runs on `master` every night against the latest Arch Linux, Qt and Rust, to detect regressions caused by dependency updates before users install them. A red nightly is a release blocker and is fixed with priority.

**Merging:** `master` is protected; changes land only through pull requests with all CI checks and the CLA check green. There are no direct pushes to `master`.

**Releases (continuous delivery):**

- Versions follow Semantic Versioning; `0.x` releases are published as pre-releases. The single source of the version is the Cargo workspace version; `packaging/arch/PKGBUILD` must match it.
- A release is created by pushing a tag `vX.Y.Z` on a `master` commit. The release workflow checks that the tag matches the version and that `CHANGELOG.md` has a `## [X.Y.Z]` section, reruns the **full** CI pipeline on that commit, and only then publishes a GitHub Release with the Arch package, the source tarball, `SHA256SUMS` and the changelog section as notes.
- A release that changes the platform layer, input handling or session behaviour also requires the relevant operator tests on a real KDE session (see the runbook), with evidence attached to the release pull request.
- Distribution channel: GitHub Releases now; an AUR package and autostart integration are Phase 5 work (G-5.3).

**Regression rule:** every bug fix includes a test that fails without the fix. A regression found by users gets a test before the fix is released.

## Consequences

Every change is built and installed the way users install it. Nightly runs catch breakage from Arch updates early. Pull requests take longer to validate because of the package build. Hosted CI cannot verify compositor behaviour, real lock/suspend or TV readability; those remain operator and acceptance tests.

## Alternatives considered

- Release from any branch or without re-running CI: rejected, allows untested builds.
- CI only on pull requests without nightly: misses regressions caused by the rolling distribution.
- Testing only on Ubuntu: its Qt is older than the minimum (6.6) and does not represent the target.

## Verification

The workflows `.github/workflows/ci.yml`, `package.yml`, `release.yml` and branch protection settings on GitHub.
