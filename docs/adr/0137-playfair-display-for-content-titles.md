# ADR-0137: Playfair Display for content titles

- **Status:** Accepted
- **Date:** 2026-11-03
- **Deciders:** Owner (Anatoly), issue [#751](https://github.com/LimeBeck/arinn/issues/751) and its comments on 2026-11-03. Bundling the OFL file stays owner-reviewed in the pull request.
- **Amends / Supersedes:** amends [ADR-0136](0136-hearth-light-visual-direction.md) (D-2, the content-title face)
- **Amended by / Superseded by:** —
- **Specification:** [§15](../specifications/arinn-specification.md#15-design-contract)

## Context

[ADR-0136](0136-hearth-light-visual-direction.md) D-2 set content titles in Ysabeau and kept Onest for system text. The owner first chose Literata, then rejected it the same day as too bookish. On 2026-11-03 Cyrillic in Ysabeau read flat from the sofa. The owner chose **Playfair Display 600** (OFL-1.1, Latin and Cyrillic) for every content title, and then confirmed that a title card or plate without artwork uses the same face. Literata is not kept for that plate. UI text stays Onest. The [brief](../design/briefs/playfair-content-titles/README.md) packages that decision. The historical handoff is not edited ([ADR-0019](0019-design-handoff-contract.md)).

## Decision

Arinn sets content titles in **Playfair Display**, weight **600**, tracking 0, sentence case, lining figures. The roles are the hero, the detail title, the player title, episode title cards, the Ambient clock (`display`), and the title card or plate without artwork (the Home New row and a library poster with no artwork). Sizes and fit steps stay as in the Hearth light typography table.

System text, page titles, eyebrows, captions under artwork and every other UI role stay **Onest**.

The shell bundles the variable face `PlayfairDisplay[wght].ttf` from Google Fonts `ofl/playfairdisplay`, unchanged, as `apps/arinn-shell/ui/fonts/PlayfairDisplay.ttf`. Weight uses the existing `wght` axis (`Theme.applyWeight`, #546). Ysabeau is no longer a content-title face and is not bundled. Literata is not bundled.

## Consequences

- `Theme.contentFontFamily` is Playfair Display. Content roles and the no-artwork plates above use it at weight 600.
- The file is OFL-1.1. `REUSE.toml` names The Playfair Display Project Authors, the same annotation pattern as Onest. `LICENSES/OFL-1.1.txt` is unchanged. Debian `copyright` and the PKGBUILD licence line still omit the bundled faces; that gap stays with [#736](https://github.com/LimeBeck/arinn/issues/736) and now applies to Onest and Playfair Display.
- Playfair Display has lining figures (`lnum`) and no tabular-figures feature (`tnum`). Content roles still request both; `tnum` does not change this face. System text in Onest keeps tabular figures. A clock in this face uses proportional lining figures.
- Adding the font file is a licensing change. This pull request carries `needs-owner`.
- A new handoff package is not part of this decision. Brand Guide v0.2 states the current face. The Hearth light prototype keeps Playfair as a lab switch.

## Alternatives considered

- Keep Ysabeau: rejected. Cyrillic reads flat from the sofa.
- Keep Literata on the title card without artwork: rejected by the owner on 2026-11-03. Literata stays out of every content-title surface.
- Onest for content titles as well: the Hearth review already rejected a single face (ADR-0136, variant B).

## Verification

- `tests/ui/tst_typography.qml` loads Playfair Display, checks weight 600 on the `wght` axis, lining figures, Onest for system roles, and the fit steps.
- Home, library and episode tests check the hero, the detail title, the New title card and the no-artwork plates.
- Frames at 1920×1080 of the Home hero, a title card without artwork and detail, in English and Russian, belong with the implementation pull request.
- A check on the owner's TV is still the owner's. Accepting this ADR does not mean that check has passed.
