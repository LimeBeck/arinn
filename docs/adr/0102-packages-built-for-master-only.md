# ADR-0102: Packages are built for master only, never for pull requests

- **Status:** Accepted
- **Date:** 2026-10-08
- **Deciders:** project owner (request on 2026-10-08: "the .deb and the Arch package can be built on master only"); implementation under delegated authority ([ADR-0037](0037-agent-autonomy-and-owner-gated-decisions.md))
- **Amends / Supersedes:** amends [ADR-0049](0049-ci-minutes.md) (a pull request built the package when it touched packaging inputs) and [ADR-0091](0091-debian-package-ci-artifact.md) (the `.deb` followed the same pull request scope)
- **Amended by / Superseded by:** —
- **Specification:** [§17.1](../specifications/arinn-specification.md#171-continuous-integration-and-delivery)

## Context

CI runs on one self-hosted runner host with 5 vCPUs ([ADR-0049](0049-ci-minutes.md)).
The Arch and Debian package builds are the longest jobs; the Debian release
build alone takes tens of minutes there. A pull request that touched packaging
inputs (manifests, `Cargo.lock`, workspace `Cargo.toml` files, the KWin adapter,
the browser extension) built and smoke-tested both packages, which happened on
most code changes.

## Decision

- **Pull requests never build or smoke-test the Arch and Debian packages.**
  `scripts/ci-scope.py` reports `package=false` for every pull request. The
  package jobs report their required checks as skipped, as before.
- **These runs still build, install and smoke-test both packages:**
  - pushes to `master`;
  - the nightly;
  - release tags;
  - manual runs (`workflow_dispatch`).

  A failing package job blocks the master snapshot and releases as before.
- **Checking a packaging change before merge** is done with a manual CI run on the
  branch.

## Consequences

- Pull request pipelines no longer spend time on the two release builds.
- A packaging regression can reach `master` and is caught there or in the
  nightly, unless the author ran CI manually on the branch. The master snapshot
  is published only after the package jobs pass ([ADR-0086](0086-automatic-master-prereleases.md)),
  so a broken package is never published.
- The path list that selected package builds for pull requests is removed.

## Alternatives considered

- **Keep path-based package builds on pull requests.** Most code changes touch
  `Cargo.lock` or a manifest, so the long builds still ran on most pull requests.
- **Build only the Arch package on pull requests.** The owner asked for both
  packages to move to master.

## Verification

- `tests/ci/test_ci_scope.py` checks that:
  - a pull request, including one that changes packaging, reports `package=false`;
  - a push reports `package=true`.
- The *Package* checks on a pull request show as skipped; a push to `master`
  runs them.
