<!-- SPDX-FileCopyrightText: 2026 Arinn contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# CI scope selection

The owner-reviewed process proposal is [ADR-0145](../adr/0145-scoped-website-and-visual-pr-checks.md),
implemented under [#874](https://github.com/LimeBeck/arinn/issues/874).
[CONTRIBUTING](../../CONTRIBUTING.md#required-checks) is the canonical local check list.

| PR diff | Selected scope | Native check |
| --- | --- | --- |
| Only Markdown/changelog Markdown | `docs` | Reported as skipped |
| Markdown plus public site sources/assets or named site tools/tests | `website` | Reported as skipped; site validation runs |
| The above plus capture QML/JSON or the named comparison tool/tests | `visual` | Builds `arinn-ui-test` and renders the full manifest; application/Rust steps skip |
| Runtime code, native tests/build inputs, workflows, unknown/mixed paths or discovery failure | `full` | Existing full Rust/Qt/integration suite |

The allowlist lives in `scripts/ci-scope.py`; it is not a blanket exception for
`docs/` or all images. In particular, `docs/plugin-api/*.xml`, handoff assets,
production QML, `tests/ui/main.cpp`, CMake files, manifests and workflow changes
retain full checks. A native path selects full native checks even among lightweight paths; changed website/capture inputs still receive their corresponding checks.

The classifier reads the PR merge commit's first-parent diff with rename
tracking disabled, so both the old and new paths of a rename are considered.
Unknown paths select full checks. An empty/undiscoverable PR diff selects native, website and render checks, since their inputs cannot be ruled out. Non-PR events always
select full, preserving packages and snapshots on `master` and qualification
on nightly, manual and release runs.

The existing required Arch check contains both native and visual paths. This
keeps render failures visible to the same required check; a new optional job
cannot accidentally bypass the gate. Scope-job failure/missing outputs use the
native path. Workflow condition tests exercise these fallback cases.

The visual path uses the prebuilt Arch image and builds only the isolated Qt
capture target. It renders every enabled manifest state at 1080p and 4K with
synthetic models, no daemon, desktop, account or audio. `ARINN_CAPTURE_ONLY` is
cleared in CI to prevent an inherited subset from reducing coverage. The report
and images are retained for seven days. Successful capture creation is a test;
the image-difference metric remains review evidence, with no pass threshold.
No visual job grants TV or design acceptance.

Website changes run the handbook checker, site builder, publication/link checker
and common Python CI tests. The Pages workflow also watches showcase PNGs, so
an exported-image-only update triggers its normal PR validation and master
publication. No repository settings or publishing destination are changed.

To inspect a proposed PR diff locally:

```sh
python scripts/ci-scope.py README.md docs/site/index.html
python scripts/ci-scope.py tests/ui/visual/capture.qml docs/design/showcase/home.png
python -m unittest discover -s tests/ci -p 'test*scope*.py'
python -m unittest discover -s tests/ci -p test_scoped_ci.py
```

CLI paths simulate a PR. Without paths the actual CI event is used. Add a
classifier and workflow regression test whenever a path begins to affect a
native build or a new lightweight input is introduced. Full runtime check
coverage, package policy and test scheduling remain separate from scope selection.
