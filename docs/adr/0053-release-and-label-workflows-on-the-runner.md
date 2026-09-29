# ADR-0053: Release and label workflows use the switchable runner

- **Status:** Accepted
- **Date:** 2026-09-29
- **Deciders:** project owner, explicit request in the session of 2026-09-29 after the label sync failed on GitHub-hosted runners; mechanism under delegated authority ([ADR-0037](0037-agent-autonomy-and-owner-gated-decisions.md))
- **Amends / Supersedes:** amends [ADR-0049](0049-ci-minutes.md) (the release and label workflows no longer keep `ubuntu-latest`)
- **Amended by / Superseded by:** —
- **Specification:** [§17.1](../specifications/arinn-specification.md#171-continuous-integration-and-delivery)

## Context

[ADR-0049](0049-ci-minutes.md) put every CI, package, changelog and CLA job
on the repository variable `ARINN_RUNNER`. It kept the release and label
workflows on `ubuntu-latest`, because they run rarely. On 2026-09-29 GitHub
refused to start the label sync on a hosted runner: "recent account payments
have failed or your spending limit needs to be increased". A release would
fail the same way.

## Decision

- The `check` and `publish` jobs of `release.yml` and the `sync` job of
  `labels.yml` run on `${{ vars.ARINN_RUNNER || 'ubuntu-latest' }}`, in a
  `python:3.12` container, like the other jobs. A self-hosted runner still
  needs only Docker and git.
- `actions/setup-python` is dropped, because the container provides Python.
  The release `check` job marks the checkout as a safe git directory, as the
  other container jobs do.
- The label sync writes labels through the GitHub REST API with the job token,
  because the container has no `gh` CLI. Existing labels are updated, missing
  ones created, and none are deleted, as before.
- The rest of ADR-0049 stands, including removing `ARINN_RUNNER` when the
  repository becomes public (#74).

## Consequences

- No workflow depends on GitHub-hosted minutes while `ARINN_RUNNER` is set.
- The release still runs the full CI through `ci.yml` before publishing.
- The label sync no longer depends on the `gh` CLI being installed.

## Alternatives considered

- **Keep `ubuntu-latest` and restore the GitHub billing.** Not chosen by the
  owner.
- **Run on the runner host without a container.** Rejected by ADR-0049: host
  jobs cannot clean the root-owned files that container jobs leave behind.

## Verification

- The label sync runs after this change merges, because it edits a workflow
  path. The next push to `.github/labels.yml` also runs it, and a manual run is
  possible. Its log shows `synced <name>` for every label.
- The release workflow runs only for a maintainer's tag, and has not been run
  with this change.
