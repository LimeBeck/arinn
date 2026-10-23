# ADR-0101: Host-local build caches on the self-hosted runner

- **Status:** Accepted
- **Date:** 2026-10-08
- **Deciders:** project owner asked to speed up CI on the self-hosted runner (2026-10-08); mechanism under delegated authority ([ADR-0037](0037-agent-autonomy-and-owner-gated-decisions.md))
- **Amends / Supersedes:** amends [ADR-0047](0047-faster-package-builds.md) and [ADR-0092](0092-less-repeated-ci-work.md) (where the build caches are kept)
- **Amended by / Superseded by:** [ADR-0113](0113-per-runner-ci-target-directories.md) (one target directory per self-hosted runner instance)
- **Specification:** [§17.1](../specifications/arinn-specification.md#171-continuous-integration-and-delivery)

## Context

CI runs on a self-hosted runner when the repository variable `ARINN_RUNNER` is
set ([ADR-0049](0049-ci-minutes.md)). The runner host has 5 vCPUs and 11 GB of
memory. Each job still kept its caches in the GitHub cache service:

- every Rust, Arch package and Debian package job downloaded and uploaded them, up to a minute per job;
- the Debian package build with tests took 25 minutes.

The runner also maps one `/github/home` into every container job. The Debian
job's rustup installed its proxies into `/github/home/.cargo/bin`, and
`Swatinem/rust-cache` saved that directory. The Arch job's `cargo fmt` then
resolved `cargo-fmt` to a rustup proxy without rustfmt and failed (#411).

## Decision

- **One cache directory per job.** Each compiling job mounts a host directory
  `/var/cache/arinn-ci/<job>` as `/ci-cache`. The jobs are `arch-ci`, `arch-pkg`
  and `deb`; each Linux distribution and job type keeps its own directory.
- **What `/ci-cache` holds:**
  - the Cargo home;
  - the Cargo target directory;
  - ccache;
  - the rustup home, for the Debian job.

  Cargo's own locks serialize concurrent jobs that share a directory.
- **Never `/github/home` for Rust.** `CARGO_HOME` (and `RUSTUP_HOME` in the Debian
  job) point into `/ci-cache`. `Swatinem/rust-cache` is no longer used.
- **Cheaper debug builds.** The Rust job builds with `CARGO_INCREMENTAL=0` and
  line-table debug information (`CARGO_PROFILE_DEV_DEBUG=line-tables-only`).
  Package builds keep their release profile unchanged.
- **Bounded disk use.** A target directory larger than 12 GB is deleted before
  the build, which then starts from scratch once.
- **GitHub-hosted fallback.** Without `ARINN_RUNNER`, the same `/ci-cache` is
  restored and saved through `actions/cache`, with the existing save rules:
  master pushes and the nightly for the Rust job, every non-tag build for the
  packages.
- **Tag builds start clean.** Release packages use a fresh target directory, as
  before ([ADR-0047](0047-faster-package-builds.md)).

## Consequences

- Unchanged crates are not rebuilt between runs on the self-hosted runner, and
  no job spends time transferring caches.
- The runner's disk holds up to about 12 GB of target directory per job type,
  plus the Cargo registry, ccache (1 GB) and one rustup toolchain.
- A corrupted cache is cleared by deleting `/var/cache/arinn-ci` on the runner
  host.
- On GitHub-hosted runners the cached Rust job's target directory is larger than
  the dependency-only cache it replaces.

## Alternatives considered

- **sccache.** It caches compilation units with a bounded size, but it misses
  every workspace crate whose path or flags change, and it adds a tool to every
  image. A persistent target directory gives the same reuse with no new tool.
- **Keep the GitHub cache on the self-hosted runner.** It still transfers
  gigabytes per job over the network to a cache stored on the same host's disk.
- **A target directory in the workspace.** `actions/checkout` cleans the
  workspace, so it would not survive between runs.

## Verification

- `tests/ci/test_ci_build_scope.py` checks:
  - the `/ci-cache` layout;
  - that no compiling job uses `rust-cache` or `/github/home` for Cargo;
  - the save conditions for hosted and self-hosted runners.
- A CI run on the self-hosted runner passes. Per-step durations before and after
  are recorded on #411.
