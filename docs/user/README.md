# User documentation website

The user guide is a static, dependency-free website in `index.html` with local
CSS and one current-interface screenshot. It documents the shipped prototype,
not unmerged work or all intended MVP behavior. The specification remains the
requirements source; the roadmap remains the phase-status source.

## Preview and check

From the repository root:

```sh
python scripts/check-user-site.py
python -m http.server 8000 --directory docs/user
```

Open `http://localhost:8000`. Stop the server with Ctrl+C. No application,
provider or real-desktop integration starts. The page also works without
JavaScript and under a project prefix such as `/arinn/`.

## GitHub Pages deployment

The handbook is published as part of the [product website](../site/README.md),
under `guide/`. `.github/workflows/pages.yml` checks it in pull requests and, on
relevant pushes to `master` or a manual run on `master`, deploys the site's
explicit 17-file allowlist. PRs and manual runs on other branches never deploy.
Existing CI, package and release workflows are unchanged.

The owner must first select **Settings → Pages → Build and deployment → Source →
GitHub Actions** and confirm the `github-pages` environment permits `master`.
The repository is private, so Pages availability depends on the owner's plan.
The expected project URL is `https://limebeck.github.io/arinn/`; use the actual
URL returned by the deployment job before advertising it as live. Repository
visibility does not imply that the Pages site is private: confirm the intended
site visibility before enabling deployment.

This deployment workflow and repository settings need owner review and merging.
It does not create release tags, publish releases or change repository visibility.
Only the explicit site allowlist is uploaded; technical documentation, historical
designs and repository files are not bundled.

## Maintenance and provenance

Keep the guide consistent with the current implementation and link to technical
contracts for detailed setup. The first-use walkthrough introduces Home and Back;
application, browser, library and download chapters explain the connected flows.
Distinguish implemented features from real-TV qualification and from older
package versions. In particular, do not describe planned pin/unpin controls as
available while Home still shows the automatic application/shortcut list. Revisit the limits when their linked issues land;
do not call an acceptance criterion passed without evidence. The HTML and CSS
are original documentation under CC-BY-SA-4.0, covered by existing `docs/**` REUSE
annotations. No third-party fonts, scripts, logos or analytics are used.

`assets/home.png` is copied from `docs/design/current-ui/home.png`: the
`tests/ui/visual` fixture render of the current Qt interface, fictional
fixtures, not a screenshot of a user's desktop. Its
[provenance](../design/current-ui/README.md) and existing documentation license
apply. Refresh this copy when that screenshot changes.

## Visual identity

The guide follows the [Arinn brand](../design/brand/README.md): Charcoal
`#0F1115`, Stone White `#EDEDED`, quiet neutral panels and restrained Ember
Orange `#FF8A3D` accents. Keyboard focus uses a light outline, following the
current design handoff. Typography uses the system sans-serif stack without
remote font requests.

`assets/arinn-mark.svg` is an unchanged copy of the approved A1 Classic mark
from `docs/design/handoff/assets/arinn-mark.svg`; `assets/arinn-icon.svg` is an
unchanged copy of `packaging/icons/org.arinn.Shell.svg`, used as the favicon.
Both are Arinn assets under CC-BY-SA-4.0. Refresh these copies if their source
assets change, and keep the Pages staging allowlist in sync.
