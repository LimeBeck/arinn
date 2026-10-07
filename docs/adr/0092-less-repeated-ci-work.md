# ADR-0092: Less repeated work in CI

- **Status:** Proposed
- **Date:** 2026-10-07
- **Deciders:** project owner requested the optimization on 2026-10-07; mechanism under delegated authority ([ADR-0037](0037-agent-autonomy-and-owner-gated-decisions.md)); the release-mode test policy and the master pipeline remain owner-gated
- **Amends / Supersedes:** amends [ADR-0047](0047-faster-package-builds.md) (where `check()` runs) and [ADR-0086](0086-automatic-master-prereleases.md) (duplicate master runs of one commit)
- **Amended by / Superseded by:** —
- **Specification:** [§17.1](../specifications/arinn-specification.md#171-continuous-integration-and-delivery)

## Context

The self-hosted runners (two of them, [ADR-0049](0049-ci-minutes.md)) are the
bottleneck: jobs queue for up to an hour, so every runner-minute a pipeline
spends delays every other pipeline. Master run
[37670700422](https://github.com/LimeBeck/arinn/actions/runs/37670700422)
used about 78 runner-minutes and took 1 h 40 min. Much of it repeated work
done elsewhere:

| Step | Time | Repeats |
| --- | --- | --- |
| Package: `cmake --build build` (Release, 275 targets) | 15.5 min | about 11.5 min build Qt test executables, the UI gallery, the performance harness and fixtures, which the package neither installs nor runs |
| Package: `check()` (`cargo test --release`) | 17 min | the Rust job's `cargo test` on the same commit, in another profile |
| Integration: `cargo build` | 6 min | all dependencies from scratch: the job had no cache |
| Integration: Qt build | 8 min | every translation unit from scratch |

Run 678 and run 679 were two push runs for the same master commit; the
second waited for the first and then repeated the whole pipeline.

## Decision

- **The package builds only what it installs.** The PKGBUILD builds the
  `arinn-shell` CMake target instead of all targets. The Qt test executables
  are still built and run by the integration job.
- **Release-mode `check()` runs on the nightly, manual runs and tag builds.**
  Pull requests and pushes to `master` pass `--nocheck`; the Rust job runs
  `cargo test` on the same commit. Releases still build from scratch and run
  `check()` ([ADR-0047](0047-faster-package-builds.md)), and the nightly runs
  it on `master` every day.
- **The integration job caches its builds.** Cargo dependencies are cached
  with `Swatinem/rust-cache`, like the Rust job. C++ objects are cached with
  `ccache` (1 GB); every run restores the newest cache, and only master pushes
  and the nightly save one, so pull requests do not fill the repository cache.
  A cache miss only means a full build.
- **One run per master commit.** A second push run for the same commit
  cancels the first. Runs of different commits still never cancel each other,
  so every commit can publish its snapshot ([ADR-0086](0086-automatic-master-prereleases.md)).

## Consequences

- A master push needs about 30–40 fewer runner-minutes (no `check()`, no test
  targets in the package, cached integration builds); a code pull request
  saves the integration rebuild, and a pull request that builds the package
  also skips the test targets.
- A defect that appears only in release-mode tests is found by the nightly or
  the release build instead of the master push. The snapshot of that commit
  may already be published; the nightly turns red and blocks the next release
  as before.
- The package's `cmake --build` no longer proves that the test executables
  compile in Release; the integration job compiles them in the default
  configuration.
- The integration job depends on two caches. A stale `ccache` entry cannot
  produce a wrong object: `ccache` keys include the compiler, its flags and
  the preprocessed source.

## Alternatives considered

- **Skipping the master pipeline when the pull request already tested the
  same tree.** It removes the largest repetition, since a squash merge of an
  up-to-date pull request produces the tree its CI tested. Not done here: it
  contradicts the owner's rule that `master` always runs the full pipeline
  ([ADR-0044](0044-docs-only-ci-scope.md), ADR-0086). It is an owner decision.
- **A merge queue** instead of merging `master` into every pull request
  before merge: a repository setting, owner-gated.
- **Merging the Rust and integration jobs** to compile the workspace once:
  renames required checks in branch protection, owner-gated, and serializes
  the two longest jobs of a pull request.
- **Guarding the test targets in CMake with `BUILD_TESTING`:** needs a
  restructuring of `apps/arinn-shell/CMakeLists.txt`, whose resource loops
  mix shell and test targets. Building one target gives the same saving.

## Verification

- `tests/ci/test_ci_build_scope.py` pins the PKGBUILD target, the
  `--nocheck` condition and the integration caches.
- On the first master run after merge the package log shows no test targets
  and no `Starting check()`, and the integration log shows `ccache` hits from
  the second master run on.
- The next nightly runs `check()`.
