# ADR-0099: Snapshot package version with the commit

- **Status:** Accepted
- **Date:** 2026-10-08
- **Deciders:** Owner (snapshot packages carry the hash, [#357](https://github.com/LimeBeck/arinn/issues/357)); delegated technical details
- **Amends / Supersedes:** [ADR-0086](0086-automatic-master-prereleases.md) (snapshot packages no longer keep the workspace version)
- **Amended by / Superseded by:** —
- **Specification:** §17 (CI and releases)

## Context

Every master snapshot was packaged as `arinn-0.1.0-1` ([ADR-0086](0086-automatic-master-prereleases.md)).
pacman saw no newer version between snapshots, and neither pacman nor a user
could tell two of them apart ([#357](https://github.com/LimeBeck/arinn/issues/357)).
The binaries already carry the commit ([ADR-0097](0097-build-identity.md)).

## Decision

- **Snapshot `pkgver`.** A package built from anything but a version tag has
  `pkgver=<workspace version>.r<commit count>.g<first 12 hex of the commit>`,
  for example `0.1.0.r394.g1bd20d647afe`.
  `scripts/build_id.py --pkgver` computes it and refuses a shallow clone, whose
  commit count would be wrong. The package workflow fetches the full history.
- **Ordering.** pacman orders `0.1.0 < 0.1.0.r394.g… < 0.1.0.r395.g… < 0.1.1`:
  a release is newer than every snapshot of the version before it, and each
  master snapshot upgrades the one before it. Commits on master only grow, so
  the count orders snapshots.
- **Release builds.** A `vX.Y.Z` tag builds `pkgver=X.Y.Z`, as before.
- **Where it is set.** The repository `PKGBUILD` keeps the workspace version
  (`release.py check` still requires it). The package workflow writes the
  snapshot `pkgver` into its build copy and names the source tarball after it.
  The same tarball carries the commit through `export-subst`, so the binaries'
  build id and the package version name the same commit.
- **Workspace version and fragments.** Unchanged, as in ADR-0086.

## Consequences

- `pacman -U` of a newer snapshot is an upgrade, not a reinstall of the same
  version, and `pacman -Q arinn` names the commit.
- A pull-request package uses the commit count of its merge commit. It is
  never published.
- The package job fetches the full history, which this repository keeps small.
- The Debian `.deb` is a CI artifact and keeps the workspace version.

## Alternatives considered

- **A commit date instead of the count.** Rejected: commit dates need not
  grow, and the count is the usual form for VCS package versions.
- **Bumping the workspace version for each snapshot.** Rejected: ADR-0086 keeps
  snapshots from changing versions or fragments.
- **A `pkgver()` function in the `PKGBUILD`.** Rejected: makepkg runs it only
  for VCS sources, and the package is built from a tarball.

## Verification

- `tests/ci/test_build_id.py`: the snapshot format, refusal of a shallow or
  missing checkout, and the real checkout when it is a full clone.
- The package workflow checks that the build copy of the `PKGBUILD` carries the
  computed `pkgver`; the package build and smoke tests then install it.
