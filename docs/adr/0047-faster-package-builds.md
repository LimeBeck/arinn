# ADR-0047: Cached package builds and no release-mode check() on pull requests

- **Status:** Accepted
- **Date:** 2026-09-28
- **Deciders:** project owner, explicit request to speed up the package job; mechanism under delegated authority ([ADR-0037](0037-agent-autonomy-and-owner-gated-decisions.md))
- **Amends / Supersedes:** amends [ADR-0033](0033-ci-cd-and-releases.md) (package job on pull requests and `master` only)
- **Amended by / Superseded by:** —
- **Specification:** [§17.1](../specifications/arinn-specification.md#171-continuous-integration-and-delivery)

## Context

The Arch package job took about 11 minutes of every pull request. The log of a
typical run showed where the time went:

- 5 minutes: compiling the whole Rust workspace in release mode from scratch;
- 3.5 minutes: `check()`, mostly recompiling the tests in release mode;
- the rest: the Qt build, dependencies and packaging.

The Rust job in the same pipeline already runs `cargo test` on every pull
request.

## Decision

- On pull requests and pushes to `master`, the package job restores a Cargo
  cache: `CARGO_HOME` and `CARGO_TARGET_DIR` in a directory outside the
  makepkg source tree. The cache key is the rustc version plus `Cargo.lock`.
  The PKGBUILD uses `CARGO_TARGET_DIR` from the environment when it is set,
  and `target` otherwise.
- On pull requests, makepkg runs with `--nocheck`. Pushes to `master`, the
  nightly and tag builds still run `check()`.
- Tag builds (releases) never restore the cache. They build from scratch and
  run `check()`, so the published package comes from a clean build.

## Consequences

- A pull request package build reuses compiled dependencies. Only the
  workspace crates, the Qt shell and packaging are rebuilt.
- A pull request no longer runs the tests in release mode. A failure that
  appears only in release mode is found on `master`, the nightly or the
  release build.
- A cache from a different rustc version is not reused: Arch updates rustc
  often, and a mismatch means a full build, which is still correct.

## Alternatives considered

- **A prebuilt container image with dependencies:** saves only about 30
  seconds and adds an image to maintain.
- **sccache for C++:** the Qt build takes under a minute.
- **Skipping the package job on pull requests:** rejected. The package job
  and its smoke test catch packaging and session-unit regressions that the
  other jobs cannot.

## Verification

- The package job duration on pull requests before and after this change.
- A tag build logs `check()` and shows no cache restore.
