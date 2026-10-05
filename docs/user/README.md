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

`.github/workflows/pages.yml` checks the site in pull requests and publishes only
`index.html`, `assets/site.css` and `assets/home.png` on relevant pushes to
`master`, or a manual run on `master`. PRs and manual runs on other branches
never deploy. Existing CI, package and release workflows are unchanged.

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
contracts for detailed setup. Revisit the limits when their linked issues land;
do not call an acceptance criterion passed without evidence. The HTML and CSS
are original documentation under CC-BY-SA-4.0, covered by existing `docs/**` REUSE
annotations. No third-party fonts, scripts, logos or analytics are used.

`assets/home.png` is copied from `docs/design/current-ui/home.png`: the current
Qt gallery, fictional fixtures, not a screenshot of a user's desktop. Its
[provenance](../design/current-ui/README.md) and existing documentation license
apply. Refresh this copy when the current-gallery screenshot changes.
