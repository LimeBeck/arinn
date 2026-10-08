# ADR-0092: Less repeated work in CI

- **Status:** Proposed
- **Date:** 2026-10-07
- **Deciders:** project owner requested the optimization on 2026-10-07; mechanism under delegated authority ([ADR-0037](0037-agent-autonomy-and-owner-gated-decisions.md)); the release-mode test policy and the master pipeline remain owner-gated
- **Amends / Supersedes:** amends [ADR-0033](0033-ci-cd-and-releases.md) (the Rust and integration jobs become one), [ADR-0047](0047-faster-package-builds.md) (where `check()` runs) and [ADR-0086](0086-automatic-master-prereleases.md) (duplicate master runs of one commit)
- **Amended by / Superseded by:** [ADR-0101](0101-host-local-ci-caches.md) — build caches in a host directory on the self-hosted runner
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
| Rust job (Debian container) | 6.5 min | a second debug compile of the workspace and its dependencies next to the integration job |

Run 678 and run 679 were two push runs for the same master commit; the
second waited for the first and then repeated the whole pipeline.

## Decision

- **The packages build only what they install.** The PKGBUILD and the
  Debian rules ([ADR-0091](0091-debian-package-ci-artifact.md)) build the
  `arinn-shell` CMake target instead of all targets. The Qt test executables
  are still built and run by the build job.
- **Release-mode `check()` runs on the nightly, manual runs and tag builds.**
  Pull requests and pushes to `master` pass `--nocheck` (`nocheck` in
  `DEB_BUILD_OPTIONS` for the Debian package); the build job runs
  `cargo test` on the same commit. Releases still build from scratch and run
  `check()` ([ADR-0047](0047-faster-package-builds.md)), and the nightly runs
  it on `master` every day.
- **One job compiles the workspace.** The Rust job (`fmt`, `clippy`,
  `cargo test`) and the Qt/private-bus integration job become one job,
  *Rust, Qt shell and integration (Arch Linux)*, in the Arch Linux container.
  It runs `fmt`, `clippy` and `cargo test` first, then the Xvfb-only X11
  tests (`--ignored`, the others already ran), the Qt build, the QML tests
  and the private-bus suite. Arch's `rust` package replaces the `rust:1`
  image and `dtolnay/rust-toolchain`; both follow stable Rust.
- **The build job caches its builds.** Cargo dependencies are cached
  with `Swatinem/rust-cache`, like the Rust job. C++ objects are cached with
  `ccache` (1 GB); every run restores the newest cache, and only master pushes
  and the nightly save one, so pull requests do not fill the repository cache.
  A cache miss only means a full build.
- **One run per master commit.** A second push run for the same commit
  cancels the first. Runs of different commits still never cancel each other,
  so every commit can publish its snapshot ([ADR-0086](0086-automatic-master-prereleases.md)).

## Consequences

- A master push needs about 35–45 fewer runner-minutes (no `check()`, no test
  targets in the package, one cached workspace build); a code pull request
  saves a second workspace compile and the uncached rebuilds, and a pull request that builds the package
  also skips the test targets.
- The two required checks *Rust (fmt, clippy, test)* and *Qt shell and
  private-bus integration (Arch Linux)* are replaced by *Rust, Qt shell and
  integration (Arch Linux)*. The owner changes branch protection when merging
  this; until then pull requests wait for the old check names.
- A code pull request uses one runner for the build instead of two. Its
  wall-clock time can grow by the length of `clippy` and `cargo test`, while
  the runner time it takes away from other pipelines falls.
- Rust is tested with Arch's stable `rust` package, which can trail a new
  stable release by a few days.
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
- **Guarding the test targets in CMake with `BUILD_TESTING`:** needs a
  restructuring of `apps/arinn-shell/CMakeLists.txt`, whose resource loops
  mix shell and test targets. Building one target gives the same saving.

## Verification

- `tests/ci/test_ci_build_scope.py` pins the PKGBUILD target, the
  `--nocheck` condition, the single build job with its step order, the
  caches and that every `needs` names an existing job.
- On the first master run after merge the package log shows no test targets
  and no `Starting check()`, and the integration log shows `ccache` hits from
  the second master run on.
- The next nightly runs `check()`.
