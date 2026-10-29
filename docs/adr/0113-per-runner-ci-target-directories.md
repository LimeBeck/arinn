# ADR-0113: One Cargo target directory per self-hosted runner instance

- **Status:** Accepted
- **Date:** 2026-10-22
- **Deciders:** delegated technical choice ([ADR-0037](0037-agent-autonomy-and-owner-gated-decisions.md)), after a CI failure analysis ([#485](https://github.com/LimeBeck/arinn/issues/485))
- **Amends / Supersedes:** amends [ADR-0101](0101-host-local-ci-caches.md) (sharing of the target directory)
- **Amended by / Superseded by:** [ADR-0114](0114-per-runner-package-target-directories.md) (one package target directory per self-hosted runner instance)
- **Specification:** [§17.1](../specifications/arinn-specification.md#171-continuous-integration-and-delivery)

## Context

[ADR-0101](0101-host-local-ci-caches.md) keeps the Rust job's Cargo target
directory in `/ci-cache`, a host directory, and assumes that "Cargo's own
locks serialize concurrent jobs that share a directory".

The repository now has two self-hosted runner instances on one host (`alma10`
and `alma10-2`). Both mount the same `/var/cache/arinn-ci/arch-ci`, so their
jobs share one `target/` with identical container paths. Artifact file names of
the workspace's own crates do not depend on their sources.

Cargo's lock covers compiling, not the later steps that use the artifacts:
doctests, `cargo build`, the Qt build and `tests/integration/phase1.py`, which
runs `./target/debug` binaries. So a job on one instance rebuilt
`arinn_protocol` from `master` while a pull request's job on the other instance
was running its doctests and `phase1.py`. Three pull requests failed that way
on 2026-10-22 (#470, #476 and #478; job timelines in
[#485](https://github.com/LimeBeck/arinn/issues/485)). `master` runs passed in
the same period only because no overlapping job replaced their artifacts at
the wrong moment.

## Decision

- When `ARINN_RUNNER` is set, the Rust job of `ci.yml` uses
  `/ci-cache/target-<runner name>` as its target directory. Characters outside
  `A-Za-z0-9_.-` in the name become `_`. The Cargo home and ccache stay shared:
  the registry and ccache handle concurrent use, and both are keyed by content.
- On GitHub-hosted runners (`ARINN_RUNNER` unset) the job keeps the single
  `/ci-cache/target` that `actions/cache` restores and saves, because every
  hosted job starts on its own machine.
- The 12 GB limit of ADR-0101 applies to each target directory.
- The package jobs are not changed here. They build only for `master`, the
  nightly and tags ([ADR-0102](0102-packages-built-for-master-only.md)), but
  they also run tests from their target directories (`check()` in the
  PKGBUILD) and install from `target/release`, and both runner instances share
  those directories too. Two overlapping master or nightly runs can therefore
  race the same way; that is tracked separately
  ([#489](https://github.com/LimeBeck/arinn/issues/489)).

## Consequences

- Concurrent jobs on two runner instances no longer replace each other's
  artifacts mid-job.
- The runner host keeps up to about 12 GB of target directory per instance for
  the Rust job, instead of one shared directory.
- The first run on each instance after this change builds the workspace from
  scratch once; dependencies from the shared Cargo home are not downloaded
  again.
- The old shared `/var/cache/arinn-ci/arch-ci/target` is no longer used and
  keeps up to 12 GB on the runner host. It is deleted by hand once, when no job
  from before this change is running.

## Alternatives considered

- **Delete the workspace crates' artifacts at the start of every job**
  (`cargo clean -p` for each member). This was rejected in review of #486: it
  deletes the artifacts of a job running on the other instance, which makes the
  race worse, and it rebuilds every workspace crate on every run.
- **A concurrency group that runs one Rust job at a time.** It is safe but
  halves the throughput of two runner instances.
- **One target directory per job run.** It is safe but gives up the incremental
  reuse that ADR-0101 introduced.

## Verification

- `tests/ci/test_ci_build_scope.py`
  `test_each_self_hosted_runner_instance_has_its_own_target_directory`: the Rust
  job derives its target directory from the runner name when `ARINN_RUNNER` is
  set, and keeps `/ci-cache/target` otherwise.
- CI on pull requests no longer fails in doctests or `phase1.py` with artifacts
  from another branch while both runner instances are busy (#485).
