# ADR-0049: Spend fewer CI minutes

- **Status:** Accepted
- **Date:** 2026-09-29
- **Deciders:** project owner, after the private repository ran out of GitHub Actions minutes on 2026-09-28 (explicit approval of each point below); mechanism under delegated authority
- **Amends / Supersedes:** amends [ADR-0033](0033-ci-cd-and-releases.md) and [ADR-0044](0044-docs-only-ci-scope.md) (which jobs a pull request runs)
- **Amended by / Superseded by:** —
- **Specification:** [§17.1](../specifications/arinn-specification.md#171-continuous-integration-and-delivery)

## Context

The repository is private, so GitHub-hosted Actions minutes are limited. On
2026-09-28 they ran out after a day of parallel agent work:

- every pull request built and smoke-tested the Arch package, the most
  expensive job;
- every push to `master` ran the full pipeline to the end, even when the next
  merge followed within minutes.

## Decision

- **Superseded runs are cancelled.** A newer push cancels the running pipeline
  of the same pull request (as before) and of `master`. The nightly and release
  runs always finish.
- **Pull requests build the package only when it can change.** That means
  changes to:
  - `packaging/`, `tests/package/`, `.github/workflows/package.yml`;
  - any `Cargo.toml` or `Cargo.lock`, `CMakeLists.txt`;
  - provider manifests (`plugins/*/*.toml`) and `plugins/chrome/extension/`;
  - the KWin adapter loader and script.

  `scripts/ci-scope.py` decides and fails open: unknown paths mean a build.
  Pushes to `master`, the nightly, releases and manual runs always build the
  package. Running CI manually on a branch forces a package build for a pull
  request.
- **The runner is switchable.** Every job in `ci.yml`, `package.yml` and
  `changelog.yml` runs on the repository variable `ARINN_RUNNER` when it is set
  (for example `self-hosted`), and on `ubuntu-latest` otherwise. A self-hosted
  runner uses no GitHub minutes. Jobs that ran directly on the Ubuntu image now
  run in containers (`python:3.12`, `rust:1`), so a runner on any Linux
  distribution needs only Docker and git. No job runs on the host itself:
  container jobs leave root-owned files in the runner's shared workspace,
  which a host-level job could not clean. At the owner's request
  (2026-09-29) the CLA workflow uses the same switch; it runs no pull request
  code. The release and label workflows keep `ubuntu-latest`: they run rarely.
  Before the repository becomes public, self-hosted runners must be revisited,
  because pull requests from forks could then run on them (#74).

## Consequences

- An ordinary code pull request runs the Rust, Qt and integration, licensing,
  documentation, changelog and CLA jobs. The package job reports as skipped, and
  packaging regressions from ordinary code changes are found on `master`.
- `master` artifacts exist only for the last commit of a burst of merges.
- **A self-hosted runner** must provide Linux x64 with Docker and git, and
  enough disk for images and Cargo caches, about 30 GB.

  It runs pull request code with its own user's rights. While the repository is
  private and all authors are trusted this is acceptable. Before the repository
  becomes public, a self-hosted runner must not run pull requests from forks
  (checklist [#74](https://github.com/LimeBeck/arinn/issues/74)).

## Alternatives considered

- **Making the repository public** (free minutes): not yet, publication is a
  separate owner decision (#74).
- **Building the package only on `master`:** rejected. Packaging changes should
  still fail in their own pull request.

## Verification

`tests/ci/test_ci_scope.py` covers the package decision: package inputs build,
ordinary code skips, unknown paths build. The workflows parse, and the next
pull request and `master` runs show the skipped or cancelled jobs.
