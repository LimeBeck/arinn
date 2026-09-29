# Arinn design

The current design handoff was delivered by the project owner on 2026-09-29 and copied to [`handoff/`](handoff/README.md) with the one deliberate change described under *Licensing*. It answers the [ADR-0050](../adr/0050-window-per-browser-activity.md) design brief with the *Open services* screen. Earlier packages are kept in the archive: [before Open services](archive/2026-09-29-before-open-services/README.md) (2026-09-27 to 2026-09-29) and [before the brand](archive/2026-09-27-before-brand/README.md). The handoff has no version number; "v2" is part of the design system file name.

## Source materials (historical, not edited)

- [Arinn brand](brand/README.md): Brand Guide v0.1 (in Russian) and the logo direction sheet, delivered 2026-09-27.
- [A1 Classic mark SVG](handoff/assets/arinn-mark.svg) and the [logo source sheet](handoff/assets/logo-direction-A.png).
- [Design handoff](handoff/README.md): look, behaviour, tokens and integration points.
- [Interactive prototype](handoff/design/arinn.dc.html).
- [All screens](<handoff/design/arinn Screens.dc.html>).
- [Design System v2](<handoff/design/arinn Design System v2.dc.html>).
- [Open services](<handoff/design/arinn Open Services.dc.html>) (ADR-0050): design answers, screens OS-1…7, Home and navigation states H-1…6 and the transition spec.
- [Direction study](<handoff/design/arinn Directions.dc.html>).
- [Screenshots of screen states](handoff/screenshots/): 22 main screens and 13 Open services states (`os-*.png`).
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
