# ADR-0097: Build identity with the commit hash

- **Status:** Accepted
- **Date:** 2026-10-08
- **Deciders:** Owner (goal and Home behaviour in [#357](https://github.com/LimeBeck/arinn/issues/357)); delegated technical choice for the build id and the contract
- **Amends / Supersedes:** —
- **Amended by / Superseded by:** —
- **Specification:** §8.2, §8.6; G-5.3

## Context

Every master snapshot is built from the same workspace version, so two
snapshots of `0.1.0` cannot be told apart by the binaries, the package or
the user. After an upgrade the shell and the daemon once ran different
builds, and Home only said that it could not load ([#357](https://github.com/LimeBeck/arinn/issues/357)).
The owner asked for the commit hash in the version. The shell talks only to
the daemon (spec §8.2), so the daemon must tell it which build it is.

Rust binaries and the Qt shell are built by different tools (cargo and
CMake). They must agree on one id without a shared build step. Packages are
built from a `git archive` tarball, which has no `.git` directory
([ADR-0033](0033-ci-cd-and-releases.md), [ADR-0086](0086-automatic-master-prereleases.md)).

## Decision

- **Build id.** Every binary of one build carries the same build id:
  `<workspace version>+g<first 12 hex of the commit>`, with `-dirty` when
  tracked files differ from the commit, for example `0.1.0+g1bd20d647afe`.
  When the commit is unknown, the build id is the bare version. The format
  is checked by `arinn_protocol::build::validate`.
- **One source.** `scripts/build_id.py` computes it. The first source that
  applies wins:
  1. `ARINN_BUILD_ID`, when a build sets it (it must match the format and
     the version);
  2. `scripts/build-commit`, which `git archive` fills with the archived
     commit through `export-subst` in `.gitattributes`;
  3. `git` in a checkout.

  Cargo build scripts call it through `scripts/cargo_build_id.rs` and embed
  the result as `ARINN_BUILD_ID`. CMake calls the same script. Without
  Python a cargo build falls back to the bare version with a warning.
- **Contract.** `org.arinn.Daemon1.BuildInfo() -> (s version, s build_id)` is
  additive; no existing signature changes. A daemon older than this method
  answers `UnknownMethod`.
- **Comparison.** Two build ids name the same build when they are equal
  after removing `-dirty` (`arinn_protocol::build::same_build`). A packaged
  build is never dirty; ignoring the flag keeps local builds of one commit
  from reporting a mismatch.
- **Shell.** The shell compares the daemon's build id with its own on connect
  and on every daemon owner change. A different build, or `UnknownMethod`,
  gives the Home state described in #357. A matching build behaves as
  before.
- **Clients and telemetry.** `arinnctl --version` and `arinnctl status`
  print the build id, and the Sentry `release` uses it.
- **Package version.** The snapshot package version that pacman compares is
  part of the owner-gated release workflow (AGENTS.md). It is changed in a
  separate pull request that the owner merges.

## Consequences

- Every binary and package can name its exact build, and a support report
  carries it.
- A tarball made with `git archive` carries its commit without any change to
  the package build. A tarball made otherwise reports the bare version, so
  two such builds of one version cannot be told apart, but they never report
  a false mismatch either.
- Build scripts rerun when `HEAD`, the index or the current branch ref
  changes. `-dirty` reflects tracked changes at the last build-script run, so
  a local build may report a stale flag; the comparison ignores it.
- The Qt build needs Python at configure time, as it already does for
  `scripts/release.py`.

## Alternatives considered

- **`git describe` in every build tool separately.** Rejected: two
  implementations can disagree, and a tarball has no `.git`.
- **Only an environment variable set by the package build.** Rejected as the
  only source: local builds would carry no hash, and the owner-gated package
  build would have to change before any binary can report its commit. It
  stays as the first source for builds that want to set the id.
- **Comparing versions only.** Rejected: every snapshot has the same version,
  which is the problem in #357.
- **A new interface instead of a `Daemon1` method.** Rejected: build identity
  belongs to the daemon itself, and an additive method keeps older clients
  working.

## Verification

- `arinn_protocol::build` unit tests for the format and the comparison.
- `tests/ci/test_build_id.py` for the sources and their order.
- The `arinnd` private-bus test: `BuildInfo` reports the embedded build, and
  the live `Daemon1` export matches `docs/plugin-api/daemon1.xml`. The CI
  `phase1.py` wire check covers the same XML.
- The implementation follow-up adds a daemon fixture with another build id,
  and one without `BuildInfo`; both give the mismatch state in the shell.
