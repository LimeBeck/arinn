# Arinn design

The current design handoff was delivered by the project owner on 2026-09-30 and copied to [`handoff/`](handoff/README.md) with the one deliberate change described under *Licensing*. It answers:
- the [ADR-0050](../adr/0050-window-per-browser-activity.md) design brief with the *Open services* screen;
- the [#106](https://github.com/LimeBeck/arinn/issues/106) brief with the *address and search* screen;
- the [#172](https://github.com/LimeBeck/arinn/issues/172) brief with the Settings value editors, *Library folders* and the folder picker.

Earlier packages are kept in the archive: [before settings editors](archive/2026-09-30-before-settings-editors/README.md) (2026-09-29 to 2026-09-30), [before address and search](archive/2026-09-29-before-address-search/README.md) (earlier on 2026-09-29), [before Open services](archive/2026-09-29-before-open-services/README.md) (2026-09-27 to 2026-09-29) and [before the brand](archive/2026-09-27-before-brand/README.md). The handoff has no version number; "v2" is part of the design system file name.

## Source materials (historical, not edited)

- [Arinn brand](brand/README.md): Brand Guide v0.1 (in Russian) and the logo direction sheet, delivered 2026-09-27.
- [A1 Classic mark SVG](handoff/assets/arinn-mark.svg) and the [logo source sheet](handoff/assets/logo-direction-A.png).
- [Design handoff](handoff/README.md): look, behaviour, tokens and integration points.
- [Interactive prototype](handoff/design/arinn.dc.html).
- [All screens](<handoff/design/arinn Screens.dc.html>).
- [Design System v2](<handoff/design/arinn Design System v2.dc.html>).
- [Open services](<handoff/design/arinn Open Services.dc.html>) (ADR-0050): design answers, screens OS-1…7, Home and navigation states H-1…6 and the transition spec.
- [Address and search](<handoff/design/arinn Browser Input.dc.html>) ([ADR-0051](../adr/0051-address-and-search-entry.md)): design answers, input states IN-1…9, tile T-1, OS-8, owner-decision variants V-1…3 and the input rules.
- [Settings editors](<handoff/design/arinn Settings Editors.dc.html>) ([ADR-0065](../adr/0065-provider-settings-contract.md)):
    - design answers;
    - editors ED, EN, IN and STR;
    - saving SV-1…6;
    - Library folders LF-1…8;
    - folder picker PK-1…10;
    - the contract-to-UI map.
- [Direction study](<handoff/design/arinn Directions.dc.html>).
- [Screenshots of screen states](handoff/screenshots/): 22 main screens, 13 Open services states (`os-*.png`), 18 address and search states (`in-*.png`) and 22 settings editor states (`se-*.png`; the folder picker has frames in the HTML only).
- [Logos in image slots](handoff/design/.image-slots.state.json); `support.js` and `image-slot.js` are kept next to the HTML.

To view the prototype, open `handoff/design/arinn.dc.html` in a browser and click it once to give it keyboard focus. The HTML is a design reference, not the production shell. External font and runtime dependencies may need network access; offline use is not confirmed.

The handoff declares colours, typography, sizes, radii, focus and motion timings final. Artwork is placeholder.

## Relation to development

The [specification](../specifications/arinn-specification.md) defines architecture and MVP; [ADR-0019](../adr/0019-design-handoff-contract.md) links the visual contract to the Qt/QML presentation layer. The [integration review](integration-review.md) records conflicts between the handoff and the specification, their resolutions and remaining runtime gates.

Source design files are never edited to record decisions. Requirement changes are made explicitly in the specification and ADRs, not hidden in a copy of the prototype.

## Licensing

Design materials are licensed CC-BY-SA-4.0 ([ADR-0029](../adr/0029-licensing.md)).

**One deliberate change to the delivered sources (2026-09-27):** before the repository was made public, the third-party service logos (browser, Kinopoisk, IVI, YouTube, Steam, EmulationStation) were removed from the prototype's image slots (`.image-slots.state.json`, handoff and archive) and replaced by neutral letter monograms, and all screenshots were re-rendered from the unchanged prototype with those placeholders (960×540, clock fixed at 22:57). The logos were also removed from Git history. Nothing else in the handoff was changed. Production builds must use service icons whose licenses allow it.

**The same change for the 2026-09-29 delivery:** the package again carried the service logos in `.image-slots.state.json` and in all screenshots. Before it was added, the logo file was replaced by the neutral monogram file already in the repository. The 22 main screenshots were taken from the previous sanitized render, because `arinn.dc.html`, the screens, the design system and the direction study are byte-identical to the previous package. The 13 `os-*.png` screenshots were re-rendered at 1920×1080 from the unchanged `arinn Open Services.dc.html` with the monograms, using headless Chrome. The README and the Open services HTML are unchanged.

**The same change for the second 2026-09-29 delivery** (address and search): the logo file was again replaced by the monogram file. The package's 22 main screenshots are byte-identical to the previous delivery, so the sanitized versions were kept; they predate this delivery's prototype change to the navigation panel (now y72–1008), so `05-global-nav.jpg` does not show it. The 13 `os-*.png` and 18 `in-*.png` screenshots were re-rendered at 1920×1080 from the unchanged HTML files with the monograms. Nothing else was changed.

**The same change for the 2026-09-30 delivery** (settings editors):
- The logo file was again replaced by the monogram file.
- Every design file except `arinn Settings Editors.dc.html` is byte-identical to the previous package. The sanitized main, `os-*` and `in-*` screenshots were therefore kept.
- The 22 new `se-*.png` screenshots show no third-party logos and are added as delivered. The package README is unchanged.
- Designer working files that the package README does not list were not added: the prototype and design-system drafts, the shortcut editor draft, uploads and capture helpers.
