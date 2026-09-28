# ADR-0044: Documentation-only pull requests skip the build jobs

- **Status:** Accepted
- **Date:** 2026-09-28
- **Deciders:** project owner, explicit request to stop full application builds for documentation changes; mechanism under delegated authority
- **Amends / Supersedes:** amends [ADR-0033](0033-ci-cd-and-releases.md) (scope of the pull request pipeline only)
- **Amended by / Superseded by:** —
- **Specification:** [§17.1, §18](../specifications/arinn-specification.md#171-continuous-integration-and-delivery)

## Context

ADR-0033 runs the full pipeline on every pull request, including the Rust build,
the Qt build and integration suite on Arch Linux and the Arch package build with an
install test. Pull requests that change only documentation, such as ADRs,
specification text or changelog fragments, cannot affect these results but wait
for them. `master` is protected by required checks, so simply not starting the
workflow would leave the required checks pending forever.

## Decision

A **documentation-only pull request** changes only Markdown files (`*.md`, anywhere)
and changelog fragments under `changes/`. Any other file, including non-Markdown
files under `docs/` (interface XML, design assets, evidence JSON) that tests and
the build read, makes the pull request a code change.

For a documentation-only pull request the CI workflow skips the Rust, Qt/integration
and Arch package jobs. The licensing and documentation jobs still run. Skipped jobs
report their required checks as passed. The package workflow skips its own jobs
through an input, so its nested checks are reported instead of missing.

The classification is done by `scripts/ci-scope.py` on the pull request's merge
commit. The build jobs run unless the result is positively documentation-only;
an error in the classification falls back to the full pipeline.

Pushes to `master`, the nightly run, manual runs and the release pipeline always run
the full pipeline, so every commit on `master` and every release is still built,
installed and smoke-tested.

## Consequences

Documentation pull requests finish in minutes instead of waiting for the Arch
package build. A code defect can never merge without a full run, because a
documentation-only diff does not change code; the push to `master` still runs
everything. A Markdown file that becomes a build input in the future must be
excluded in `scripts/ci-scope.py` in the same change.

## Alternatives considered

- **Workflow-level `paths-ignore`**: rejected; the required checks would never
  report and the pull request could not be merged.
- **A third-party path-filter action**: not needed for one rule; a small script is
  testable locally and keeps the supply chain unchanged.
- **Skipping on a label**: rejected; it depends on people remembering to set it and
  could skip a code change.

## Verification

`tests/ci/test_ci_scope.py` (run by the documentation job) covers the
classification. A documentation-only pull request shows the build checks as skipped
and remains mergeable; a pull request with code runs every job.
