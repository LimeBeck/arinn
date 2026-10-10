# Arinn product website

The static product website combines a landing page, eight current Qt gallery
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

The builder reads an explicit input allowlist and outputs 17 public files into
`build/site` (or `--output /tmp/arinn-product-site`). It excludes README files,
technical documents, historical prototypes, app binaries, configuration and Git
metadata. The checker rejects extra files, symlinks, broken links and references
outside the output directory. Build outputs must not be committed.

The [Pages deployment workflow](../../.github/workflows/pages.yml) is owned by
[the handbook task](https://github.com/LimeBeck/arinn/issues/335). Product-site
integration must stage the output of `scripts/build-site.py` and validate it with
`scripts/check-site.py`, preserving master-only deployment and PR validation.
Merge the handbook dependency before this product-site change. Review deployment
integration in the owner-approved PR; never deploy competing workflows to the
same Pages site.

The complete replacement is prepared in [pages.yml](pages.yml). After #335 lands
and the owner authorizes changing its workflow, install it on this branch:

```sh
cp docs/site/pages.yml .github/workflows/pages.yml
```

It validates both the handbook and product site in PRs, uploads only the assembled
17-file artifact, and deploys only on `master`. Screenshot changes also rebuild
the website. The template is not an active workflow until it is installed; the
current handbook workflow continues to publish the handbook by itself.

GitHub Pages activation is an owner action: select **Settings → Pages → Source →
GitHub Actions**, confirm the intended visibility and the `github-pages`
environment's branch policy, then deploy the reviewed commit on `master`.
Use the URL returned by `actions/deploy-pages` as the live URL. A private
repository's Pages availability depends on its GitHub plan. No visibility change,
release or application deployment is part of this website.

## Assets and maintenance

The builder copies the approved mark unchanged from
`docs/design/handoff/assets/arinn-mark.svg`. It copies all eight PNGs directly from
[current-ui](../design/current-ui/README.md), including the handbook's Home image.
The handbook also includes its approved mark and app-icon favicon from
`docs/user/assets/`; their provenance is recorded in the handbook README.
Refreshing those originals refreshes the website on its next build; no separate
screenshot copies need maintenance. Captures use fictional fixtures and original
placeholder artwork, not personal media or proof of finished integrations.

The site uses system fonts and original CSS, JavaScript and content. All website
sources under `docs/site` are documentation covered by the existing CC-BY-SA-4.0
REUSE annotation. The Python build/check scripts use GPL-3.0-or-later headers.
No new license policy or historical design edit is required.
