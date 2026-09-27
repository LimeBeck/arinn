# Arinn design

The current design handoff was delivered by the project owner and copied to [`handoff/`](handoff/README.md) without changes, including the hidden logo file. The previous package is kept in the [archive](archive/2026-09-27-before-brand/README.md). Added to the repository on 2026-09-27. The handoff has no version number; "v2" is part of the design system file name.

## Source materials (historical, not edited)

- [Arinn brand](brand/README.md): Brand Guide v0.1 (in Russian) and the logo direction sheet, delivered 2026-09-27.
- [A1 Classic mark SVG](handoff/assets/arinn-mark.svg) and the [logo source sheet](handoff/assets/logo-direction-A.png).
- [Design handoff](handoff/README.md): look, behaviour, tokens and integration points.
- [Interactive prototype](handoff/design/arinn.dc.html).
- [All screens](<handoff/design/arinn Screens.dc.html>).
- [Design System v2](<handoff/design/arinn Design System v2.dc.html>).
- [Direction study](<handoff/design/arinn Directions.dc.html>).
- [22 screenshots of screen states](handoff/screenshots/).
- [Logos in image slots](handoff/design/.image-slots.state.json); `support.js` and `image-slot.js` are kept next to the HTML.

To view the prototype, open `handoff/design/arinn.dc.html` in a browser and click it once to give it keyboard focus. The HTML is a design reference, not the production shell. External font and runtime dependencies may need network access; offline use is not confirmed.

The handoff declares colours, typography, sizes, radii, focus and motion timings final. Artwork is placeholder.

## Relation to development

The [specification](../specifications/arinn-specification.md) defines architecture and MVP; [ADR-0019](../adr/0019-design-handoff-contract.md) links the visual contract to the Qt/QML presentation layer. The [integration review](integration-review.md) records conflicts between the handoff and the specification, their resolutions and remaining runtime gates.

Source design files are never edited to record decisions. Requirement changes are made explicitly in the specification and ADRs, not hidden in a copy of the prototype.

## Licensing

Design materials are licensed CC-BY-SA-4.0 ([ADR-0029](../adr/0029-licensing.md)).

**One deliberate change to the delivered sources (2026-09-27):** before the repository was made public, the third-party service logos (browser, Kinopoisk, IVI, YouTube, Steam, EmulationStation) were removed from the prototype's image slots (`.image-slots.state.json`, handoff and archive) and replaced by neutral letter monograms, and all screenshots were re-rendered from the unchanged prototype with those placeholders (960×540, clock fixed at 22:57). The logos were also removed from Git history. Nothing else in the handoff was changed. Production builds must use service icons whose licenses allow it.
