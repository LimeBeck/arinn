# ADR-0086: Automatic master pre-releases and ordinary version-tag releases

- **Status:** Proposed
- **Date:** 2026-10-04
- **Deciders:** project owner requested this process; acceptance and merge remain owner-gated
- **Amends / Supersedes:** ADR-0033, ADR-0049
- **Amended by / Superseded by:** [ADR-0088](0088-single-master-snapshot-retention.md); [ADR-0092](0092-less-repeated-ci-work.md) — a second push run of the same commit cancels the first; [ADR-0099](0099-snapshot-package-version.md) — snapshot packages carry the commit in `pkgver`
- **Specification:** §17.1, §18

## Context

The owner requested a pre-release for every push to `master`, and an ordinary
release on a version tag. The existing full CI already builds and smoke-tests
packages on master, but only `vX.Y.Z` tags publish releases. Cancelling a master
run when another commit arrives can prevent that commit from being published.

## Decision

After all Rust, Qt/private-bus, licensing, documentation and Arch package build
and installation jobs succeed, the master push CI publishes an automatic GitHub
Pre-release. This includes documentation-only master commits: master always runs
the full pipeline. Pull requests, nightlies and manual CI runs never publish.

Each commit has its own master concurrency group. Superseded pull-request runs
still cancel; master runs do not cancel one another. A failed or explicitly
cancelled pipeline publishes nothing. This costs one full package pipeline per
master commit, as requested, without a second build for publication.

The immutable tag is `snapshot-<full SHA>`, pointing to the exact checked-out and
tested commit. Snapshot tags do not match the version-release trigger. Assets
come from that same run: distributable packages, source archive and checksums.
Internal smoke-test helpers stay in CI artifacts and are excluded from public
assets, as requested by the owner in [#596](https://github.com/LimeBeck/arinn/issues/596)
and approved for merge in [#597](https://github.com/LimeBeck/arinn/pull/597).
Notes record the SHA and unreleased fragments; workspace/package versions and
changelog fragments are not changed. Tags are created only in the gated publish
job. The job uses the owner-configured `SNAPSHOT_RELEASE_TOKEN` repository
secret with Contents and Workflows write permissions; the built-in token stays
read-only. GitHub requires workflow authorization when an older commit differs
from the current default-branch workflows, so the built-in `GITHUB_TOKEN` cannot
cover every concurrent master build ([GitHub API](https://docs.github.com/en/rest/releases/releases#create-a-release)).
The secret must be configured before enabling publication. A draft stays private until uploaded asset names, sizes and SHA-256 digests
verify. The final lookup uses the action-provided release ID, since GitHub
returns only published releases through tag lookup. A rerun never changes a published snapshot and rejects a tag collision
or different rebuilt assets. Pre-releases never become the latest stable release.

A `vX.Y.Z` tag retains the existing matching-version, changelog and full CI gates,
but publishes an ordinary release, including `0.x`. Version preparation and
required real-desktop operator evidence still belong to the maintainer. The
automatic snapshot channel does not require a fresh real-desktop run for every
push and never qualifies the operator gates; this distinction is part of the
owner-gated policy proposal. Agents
prepare workflow changes; only the owner merges this process change. No agent
publication or release-policy exception is requested by this ADR.

## Consequences

Users get verified master packages without manual publication or version bumps.
Concurrent master builds use more runner resources. Interrupted uploads leave a
draft that can be repaired by rerunning CI; a checksum mismatch prevents public
publication. This does not claim compositor/TV acceptance or change phase status.

## Alternatives considered

A manual workflow was rejected by the owner. Publishing from a docs-only commit
without a full package check was not selected: every master push retains full CI.

## Verification

`tests/ci/test_snapshot_release.py` covers event/branch/SHA fencing, required
assets, checksum verification, wrong tags, incomplete drafts and immutable reruns.
CI must prove the complete dependency chain and skip publishing on PRs. A real
master snapshot run after owner merge is the publication qualification; local
unit tests do not claim live GitHub publication.
