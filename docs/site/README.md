# Arinn product website

The static product website combines a landing page, four current fixture-driven Qt
screenshots and the [user handbook](../user/README.md). It presents implemented
features with explicit pre-alpha and screenshot-fixture context. The specification
and roadmap remain the source of requirements and acceptance status.

## Build and preview

From the repository root, with Python 3.12 or later:

```sh
python scripts/build-site.py
python scripts/check-site.py
python -m http.server 8000 --directory build/site
```

Open `http://localhost:8000/`. The handbook is at `guide/index.html`. All internal
references are relative, including the handbook's Home links, so the site works
under GitHub Pages project prefixes such as `/arinn/` as well as a custom domain.
The site has no package-manager dependencies, analytics or remote runtime assets.
Screenshot links open a native modal dialog with JavaScript; without JavaScript
they open the original image. Escape, the Close button and the backdrop close the
dialog and restore focus. Modified clicks preserve normal browser behavior.

## Publication boundary

The builder reads an explicit input allowlist and outputs 14 public files into
`build/site` (or `--output /tmp/arinn-product-site`). It excludes README files,
technical documents, historical prototypes, app binaries, configuration and Git
metadata. The checker rejects extra files, symlinks, broken links and references
outside the output directory. Build outputs must not be committed.

The [Pages deployment workflow](../../.github/workflows/pages.yml) publishes this
site (#339). In pull requests it validates the handbook and the product site. On
`master` (a push or a manual run) it builds the site with `scripts/build-site.py`,
checks it with `scripts/check-site.py`, uploads only the assembled 14-file artifact
and deploys it. Mark changes rebuild the website. Showcase-only updates must also touch a
listed site source or use a maintainer-triggered Pages run: the current workflow
path filter still watches `docs/design/current-ui/*.png`, not `showcase/*.png`. Pull
requests and runs on other branches never deploy. No other workflow deploys to
the same Pages site.

GitHub Pages activation is an owner action: select **Settings → Pages → Source →
GitHub Actions**, confirm the intended visibility and the `github-pages`
environment's branch policy, then deploy the reviewed commit on `master`.
Use the URL returned by `actions/deploy-pages` as the live URL. A private
repository's Pages availability depends on its GitHub plan. No visibility change,
release or application deployment is part of this website.

## Assets and maintenance

The builder copies the approved mark unchanged from
`docs/design/handoff/assets/arinn-mark.svg`. It copies all four PNGs directly from
[showcase](../design/showcase/README.md), including the handbook's Home image.
The handbook also includes its approved mark and app-icon favicon from
`docs/user/assets/`; their provenance is recorded in the handbook README.
Refreshing those originals refreshes the website on its next build; no separate
screenshot copies need maintenance. Captures use fictional fixtures and original
placeholder artwork, not personal media or proof of finished integrations.

The site uses system fonts and original CSS, JavaScript and content. All website
sources under `docs/site` are documentation covered by the existing CC-BY-SA-4.0
REUSE annotation. The Python build/check scripts use GPL-3.0-or-later headers.
No new license policy or historical design edit is required.
