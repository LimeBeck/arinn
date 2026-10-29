# ADR-0114: One package target directory per self-hosted runner instance

- **Status:** Accepted
- **Date:** 2026-10-24
- **Deciders:** delegated technical choice ([ADR-0037](0037-agent-autonomy-and-owner-gated-decisions.md)), follow-up that ADR-0113 left to [#489](https://github.com/LimeBeck/arinn/issues/489)
- **Amends / Supersedes:** amends [ADR-0113](0113-per-runner-ci-target-directories.md) (the package jobs were left unchanged)
- **Amended by / Superseded by:** —
- **Specification:** [§17.1](../specifications/arinn-specification.md#171-continuous-integration-and-delivery)

## Context

[ADR-0113](0113-per-runner-ci-target-directories.md) gave the Rust job of
`ci.yml` one Cargo target directory per self-hosted runner instance, because
the two instances on one host (`alma10` and `alma10-2`) mount the same cache
directory and Cargo's lock covers compiling, not the steps that use the
artifacts afterwards.

It left the package jobs of `package.yml` unchanged. They build only for
`master`, the nightly and tags ([ADR-0102](0102-packages-built-for-master-only.md)),
but both instances still share `/var/cache/arinn-ci/arch-pkg` and
`/var/cache/arinn-ci/deb`, and each job keeps using its target directory after
Cargo has released the lock:

- the PKGBUILD's `check()` and `debian/rules` run release-mode tests from it
  (nightly and manual runs);
- `package()` and the Debian install step copy binaries from `target/release`;
- the job copies `target/release/examples/private-systemd` for the smoke test.

Two overlapping master or nightly runs on the two instances can therefore
replace each other's artifacts mid-job, the race of
[#485](https://github.com/LimeBeck/arinn/issues/485).

The Arch job also ran `chown -R builder /ci-cache`, which walks the whole
shared directory. With a target directory per instance that walk would cross
the other instance's directory while its build creates and removes files, and
`chown` fails on a file that disappears under it.

## Decision

- When `ARINN_RUNNER` is set, the Arch package job (`build`) and the Debian
  package job (`build-deb`) use `/ci-cache/target-<runner name>` as their
  Cargo target directory, with the naming rule of ADR-0113: characters outside
  `A-Za-z0-9_.-` in `runner.name` become `_`. The Cargo home, and the Debian
  job's rustup home, stay shared, as in ADR-0113.
- On GitHub-hosted runners (`ARINN_RUNNER` unset) both jobs keep the single
  `/ci-cache/target` that `actions/cache` restores and saves.
- Tag builds keep their clean target directory in the job's workspace
  (`cargo-cache/target` for Arch, `cargo-target` for Debian), on any runner
  ([ADR-0047](0047-faster-package-builds.md), ADR-0101).
- The 12 GB limit of [ADR-0101](0101-host-local-ci-caches.md) applies to each
  target directory.
- The Arch job gives the `builder` user only its Cargo home and its own target
  directory, never all of `/ci-cache`.

## Consequences

- Overlapping package builds on two runner instances no longer test or package
  each other's artifacts.
- The runner host keeps up to about 12 GB of target directory per instance and
  package job, instead of one shared directory per package job.
- The first package build on each instance after this change builds the
  workspace in release mode from scratch once.
- The old shared `/var/cache/arinn-ci/arch-pkg/target` and
  `/var/cache/arinn-ci/deb/target` are no longer used on the self-hosted
  runner. They are deleted by hand once, when no package job from before this
  change is running.
- The Arch job still runs `chown -R` over the shared Cargo home. Only the
  `builder` user writes there, and only while it fetches new dependencies, so
  the remaining window is small; it is not changed here.

## Alternatives considered

- **A concurrency group for the package jobs.** Safe, but master and nightly
  package builds would wait for each other even though the host has two
  runner instances.
- **One target directory per job run.** Safe, but every master build would
  compile the workspace and its dependencies in release mode from scratch,
  which ADR-0101 avoided.

## Verification

- `tests/ci/test_ci_build_scope.py` `PackageTargetDirectoryTest` runs each
  package job's target directory selection: a runner name gives its own
  sanitized directory, a hosted run keeps `/ci-cache/target`, and a tag build
  keeps its workspace directory. It also checks that `check()`, `package()`,
  the smoke-test copy and the size limit use the selected directory, and that
  `chown` does not walk all of `/ci-cache`.
- Master and nightly package builds on the self-hosted runner pass while both
  instances are busy.
