# ADR-0091: Debian package CI artifact

- **Status:** Accepted
- **Date:** 2026-10-07
- **Deciders:** project owner (issue #377, architect binding 2026-10-06)
- **Amends / Supersedes:** amends [ADR-0033](0033-ci-cd-and-releases.md) (package CI and release artifacts)
- **Amended by / Superseded by:** [ADR-0102](0102-packages-built-for-master-only.md) — pull requests never build the packages
- **Specification:** [§17.1](../specifications/arinn-specification.md#171-continuous-integration-and-delivery), [§18](../specifications/arinn-specification.md#18-packaging-distribution-and-licensing)

## Context

Contributors and integrators asked for an apt-installable `.deb` built from every
qualified commit, parallel to the existing Arch package job. Arinn still targets
Arch Linux for nightly qualification and releases; Ubuntu was rejected as a
*product* stand in [ADR-0033](0033-ci-cd-and-releases.md) because LTS images ship
Qt older than the minimum (6.6). A CI image with a new enough Qt is acceptable
for build and install smoke only.

There is no public apt repository or PPA in scope; hosting and signing stay
owner-gated.

## Decision

- **Arch remains the target distribution** and the nightly/release package
  blocker. A red Arch package job or nightly still blocks releases.
- **A `.deb` is an additional CI artifact** built from `packaging/debian/**`,
  installed into a clean Ubuntu-based container, and smoke-tested with the same
  `tests/package/` scripts where paths match. It does not replace Arch packaging.
- **Version** for the Debian package comes from the Cargo workspace via
  `scripts/release.py`, the same source as `packaging/arch/PKGBUILD`.
- **CI suite:** Ubuntu **25.04** (`ubuntu:25.04`) is the Debian-format build and
  install smoke image (Qt ≥ 6.6). It is not a product target distribution.
- **Release workflow** attaches the `.deb` next to the Arch package on GitHub
  Releases when the full pipeline passes. No apt publishing.
- Pull requests follow the same package scope rules as Arch ([ADR-0049](0049-ci-minutes.md))
  and the same cached-build / `nocheck` policy on pull requests ([ADR-0047](0047-faster-package-builds.md)).

## Consequences

- CI runs two package build and smoke paths on qualified commits. Arch job names
  and behaviour stay unchanged for branch protection.
- `tests/package/uninstall.py` supports `apt-get remove` when the Debian package
  is installed.
- Documentation under `packaging/debian/README.md` describes local builds and the
  CI suite.

## Alternatives considered

- **Replace Arch with Debian in CI:** rejected; Arch is the rolling target.
- **Ubuntu 24.04 LTS as the CI image:** rejected; Qt is below 6.6.
- **Public apt repository in this task:** rejected; owner-gated.

## Verification

- `.github/workflows/package.yml` jobs *Build Debian package* and *Install and
  smoke-test Debian package* on pull requests that touch packaging inputs.
- `packaging/debian/build-deb.sh` produces `arinn_<version>-1_amd64.deb`.
- Release workflow downloads `arinn-deb-package` with `arinn-arch-package`.
