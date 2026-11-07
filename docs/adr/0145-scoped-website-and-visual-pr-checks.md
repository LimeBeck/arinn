# ADR-0145: Scoped website and visual-fixture pull request checks

- **Status:** Proposed
- **Date:** 2026-11-07
- **Deciders:** Owner requested a separate CI-process MR in [#874](https://github.com/LimeBeck/arinn/issues/874); prepared for owner review and merge
- **Amends / Supersedes:** amends [ADR-0044](0044-docs-only-ci-scope.md) for public website and visual-fixture PRs; preserves the non-PR policy of ADR-0044/ADR-0102
- **Amended by / Superseded by:** —
- **Specification:** §17.1; existing coverage and acceptance criteria unchanged

## Context

The existing classifier treats every non-Markdown change as a native application
change. [#869](https://github.com/LimeBeck/arinn/pull/869) therefore runs the full
Rust/Qt/private-bus suite for the public site and its capture fixtures. Website
HTML/CSS/JavaScript and exported screenshots are not native build inputs.
Capture fixtures do need Qt rendering, but do not require every application or
provider test.

Some files under `docs/` are native build inputs, including protocol XML and the
approved mark. An unrestricted `docs/**` exception would skip relevant checks.
The new classification must remain explicit and conservative.

## Decision

For pull requests, `scripts/ci-scope.py` selects one scope from the entire diff:

| Scope | Allowed changes | Checks |
| --- | --- | --- |
| `docs` | Markdown/changelog Markdown | Existing documentation and licensing checks |
| `website` | Docs plus allowlisted public website sources/assets, site build/check scripts and site tests | Common checks plus handbook/site build, links, assets and publication boundary |
| `visual` | Docs/site plus QML/JSON under `tests/ui/visual/`, the comparison helper and its tests | Website/common checks plus build only `arinn-ui-test` and render all manifest states at 1080p and 4K |
| `full` | Any native/build/workflow input, unknown path, unsafe path, mixed native change, empty diff or failed path discovery | Existing full pipeline plus changed site/render checks |

The public-site allowlist covers HTML/CSS/JS/SVG/PNG under `docs/site/` and
`docs/user/`, exported PNGs under `docs/design/showcase/`, and explicitly named
site tools/tests. It does not cover protocol XML, handoff assets, evidence JSON,
native test harnesses, production QML or arbitrary scripts under those roots.

The existing required Arch check is retained. For a visual-only PR its native
steps are skipped and its render steps execute in that same check: a failed
render cannot hide behind a skipped required native job and a new optional job.
It builds only the capture executable, clears any inherited capture subset and
checks the complete rendered-state list. Render differences remain review
information; they do not become a pixel-error acceptance threshold or TV proof.

All non-PR runs remain full, including pushes to `master`, tags/releases, nightly
and manual runs. PR package policy, snapshots, symbol upload, CLA, permissions,
runner selection, cancellation and required-check names are preserved. A failed
scope job or missing outputs causes native checks to run and leaves the failed scope check blocking. An empty/undiscoverable PR diff explicitly selects native, website and render checks. Pure docs/site PRs
report the native check as skipped rather than omit it.

Pages watches the actual showcase PNG source paths as well as its current site
inputs, so a screenshot-only update is validated and publishes on `master`.

## Consequences

Public-site PRs avoid native builds and tests. Visual fixture changes validate
the renderer without running provider/application suites. Native changes keep
all existing coverage. The render report and synthetic images are retained as a
seven-day CI artifact. No operator or account access is needed.

New native inputs inside a lightweight root must be excluded from that scope in
the same change. New website file types/tools require an explicit allowlist and
regression-test update. The canonical local check list and agent reference use
the same scopes. This process change remains owner-reviewed; it does not make
any product gate pass or alter the full `master`/release policy.

## Alternatives considered

- **All `docs/**` changes are lightweight:** rejected because that includes
  native inputs such as interface XML and the embedded mark.
- **Skip the entire workflow with paths:** rejected because required checks
  would never report.
- **Run the full application suite for every capture-fixture edit:** rejected
  because the renderer directly validates the changed fixture behavior.
- **Skip native checks on docs-only master pushes too:** deferred; that would
  change the package/snapshot policy, beyond this PR-scoping request.

## Verification

Classifier tests cover each allowlist, mixed/unknown inputs, discovery failure,
non-PR events and a real synthetic merge diff. Workflow tests evaluate actual
job/step guards, including missing/failed scope outputs, and verify the stable
required-check name. Site validation and isolated 1080p/4K rendering are run with
synthetic fixtures. Accepting this ADR does not claim those checks or real TV
qualification have passed; attach the actual evidence to the implementation MR.
