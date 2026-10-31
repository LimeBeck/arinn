# ADR-0121: A red master or nightly opens an issue and blocks merges and releases

- **Status:** Proposed
- **Date:** 2026-10-31
- **Deciders:** project owner chose options 2, 3 and 4 of [#614](https://github.com/LimeBeck/arinn/issues/614) in chat on 2026-10-31; workflow details proposed under delegated authority ([ADR-0037](0037-agent-autonomy-and-owner-gated-decisions.md)); owner review and merge required (merge, release and agent rules are owner-gated)
- **Amends / Supersedes:** amends [ADR-0033](0033-ci-cd-and-releases.md) (how a red nightly blocks releases) and [ADR-0104](0104-qt-floor-and-prebuilt-ci-images.md) (what the package install test adds to its clean image)
- **Amended by / Superseded by:** —
- **Specification:** [§17.1](../specifications/arinn-specification.md#171-continuous-integration-and-delivery)

## Context

No master CI run passed from 2026-10-08 to 2026-10-30, and the nightly failed on every run from at least 2026-10-19. Pull requests stayed green because they never build the packages ([ADR-0102](0102-packages-built-for-master-only.md)), so nothing told anyone that master's package jobs were broken; the install smoke, session and uninstall tests did not run on master for weeks. The specification says a red nightly blocks releases, but only a manual step of the release procedure checked it, and v0.2.0 was tagged without a green nightly after the fixes. The package session test also skipped desktop entry validation in CI because the clean install images do not contain `desktop-file-validate`.

## Decision

- **Alert.** `.github/workflows/ci-alert.yml` runs after every finished CI run on `master` and calls `scripts/ci-status.py alert`. A failed (`failure`, `timed_out`, `startup_failure`) push run opens the issue "CI is red on master" labelled `ci-red`; a failed nightly opens "Nightly CI is red". Later failures of the same kind comment on the open issue. The next green run of the same kind comments and closes it. A green push never closes the nightly issue, because only the nightly upgrades the rolling dependencies. Cancelled and skipped runs, pull requests and manual runs change nothing. Each master commit has its own CI concurrency group, so runs finish out of order; a run older than the newest passed or failed run of its kind, or an earlier attempt of a re-run, changes nothing. Reports run one at a time, in order, with only `issues: write`, from the master copy of the script.
- **Merges.** While a `ci-red` issue is open, only pull requests that fix it or change only documentation and changelog fragments are merged. This is a merge rule for agents and contributors, not branch protection; the owner may add a required check later.
- **Releases.** The release workflow's first job runs `scripts/ci-status.py nightly` and fails unless the latest finished scheduled CI run on `master` succeeded. A missing, failed or cancelled latest nightly blocks the release; after a red nightly, the tag waits for the next green one. The check does not require that nightly to include the tagged commit: the release reruns the full CI on the tag itself.
- **Desktop entries.** The Arch and Debian install test jobs install `desktop-file-utils` after the package and before `tests/package/session.py`, which then requires the validator (`ARINN_REQUIRE_DESKTOP_VALIDATE=1`) instead of skipping. The clean smoke images still hold only Python and D-Bus, so the validator cannot supply a dependency the package forgot to declare.

## Consequences

- A red master or nightly is visible as an open issue within one CI run, with a link per failure.
- Contributors must check for an open `ci-red` issue before merging; merges of unrelated work wait until it closes.
- A release is blocked while the latest nightly is red; once a nightly has gone red, the earliest tag is after the next green nightly.
- Each install test downloads one small package; desktop entry errors now fail CI.

## Alternatives considered

- **A cheap package subset on pull requests that touch packaging, build or workflow files** (option 1 of #614): not chosen now because of CI minutes on the single runner host ([ADR-0049](0049-ci-minutes.md), [ADR-0102](0102-packages-built-for-master-only.md)).
- **Branch protection requiring a green master:** a repository setting the owner controls; a required check would also block the pull request that fixes master.
- **Installing the validator in the smoke images:** would preinstall GLib, a declared dependency of both packages, and hide its loss from the install test.
- **One issue for both kinds:** a green push would close it while the nightly is still red.
- **Requiring the green nightly to include the tagged commit:** stricter, since every release would wait for a nightly after its last merge; it goes beyond option 3 of #614 (check the latest nightly status) and is left for the owner to decide.

## Verification

- `python -m unittest discover -s tests/ci`: `test_ci_status.py` covers the release check, opening, commenting and closing per kind, and the ignored runs; `test_ci_images.py` covers the validator install order and requirement.
- The first failed master or nightly run after merge opens a `ci-red` issue, and the next green run of the same kind closes it (not yet observed).
- The next release's first job reports the nightly state (not yet observed).
