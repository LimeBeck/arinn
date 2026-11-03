<!-- SPDX-FileCopyrightText: 2026 Arinn contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Design brief: Playfair Display for content titles

Replace the content-title face with **Playfair Display 600**. Issue [#751](https://github.com/LimeBeck/arinn/issues/751).

**Status:** proposed 2026-11-03. Owner decisions already recorded in the issue (Playfair Display 600 for all content titles, including the title-card / plate without artwork; UI stays Onest). This brief is the designer's packaging of that decision for implementation and a later handoff amendment. It does **not** edit the historical handoff in place ([ADR-0019](../../../adr/0019-design-handoff-contract.md)).

## Goal

Cyrillic in Ysabeau reads flat on the sofa. Content titles need one warmer face that works for Latin and Cyrillic, on the hero and on a poster plate with no artwork. System chrome stays plain.

## Decision (owner, 2026-11-03)

| Item | Decision |
| --- | --- |
| Content-title face | **Playfair Display**, weight **600** default |
| Script coverage | Latin **and** Cyrillic |
| Licence | **OFL-1.1** (SIL Open Font License 1.1) |
| System / UI text | **Onest** unchanged |
| Title-card / New plate without artwork | **Also Playfair Display 600** — not Literata |
| Scope vs Hearth | Separate from Hearth PRs (#725 / #747); land after green master |

Playfair Display revises D-2 of [ADR-0136](../../../adr/0136-hearth-light-visual-direction.md) (Ysabeau). Literata was the first Hearth pick and was already rejected for the hero as too bookish; it is **not** kept for the no-artwork plate either.

## Roles

Every role that today uses the content-title face switches to Playfair Display 600. Sizes, fit steps, tracking (0), sentence case and lining figures stay as in the [Hearth light brief](../hearth-light/README.md) typography table unless a later TV check says otherwise.

| Role | Where it appears | Face / weight |
| --- | --- | --- |
| Hero | Home hero title | Playfair Display 600 |
| Detail | Film / series detail title | Playfair Display 600 |
| Focused poster caption | Caption under a focused content card when it is a content title | Playfair Display 600 |
| New plate without art | Home New row: Charcoal (or grey) plate when `noArt` — title on the plate | Playfair Display 600 |
| Detail without art | Detail / title-card when there is no poster | Playfair Display 600 |
| Episode title cards | Numbered cards when episode stills are missing | Playfair Display 600 |
| Player title | Player OSD title | Playfair Display 600 |
| Ambient clock (`display`) | Ambient large clock | Playfair Display 600 |

Page titles, section labels, menus, settings, eyebrows and all other system text stay **Onest**.

## Shipping

Ship the face the same way as Onest and Ysabeau:

- Binary under `apps/arinn-shell/ui/fonts/` (variable `PlayfairDisplay[wght].ttf` from Google Fonts `ofl/playfairdisplay`, or an equivalent static set that includes 600).
- `FontLoader` + `contentFontFamily` in `Theme.qml`; weight through the existing `wght`-axis path (#546 / `Theme.applyWeight`).
- `REUSE.toml` annotation and copyright line for The Playfair Display Project Authors; licence text already in `LICENSES/OFL-1.1.txt`.
- Document the source URL and SHA-256 in `docs/architecture/ui-components.md` when the file lands.
- Remove **Ysabeau** from the content-title role (drop the bundled file once nothing references it). **Literata** is not bundled in the shell today; do not add it for this role.

Default title weight remains **600**. A variable face may keep 400–900 available; named content roles use 600 unless a fit or Ambient exception is recorded later.

## Specimens

Owner comparison (11 OFL faces, Cyrillic titles) lives on the design stand, not yet in the repository:

- Directory: `/workspace/design-review/cyrillic-title-fonts/`
- Contact sheet: `contact-sheet.png`
- Playfair frames: `playfair-home.png`, `playfair-detail.png`
- Side-by-side: `compare-top-vs-ysabeau.png`

Attach key frames to the implementation PR or copy a short specimen set next to this brief when packaging the font. Do not treat the stand path as a runtime dependency.

## Handoff and docs (later)

Per ADR-0019 / ADR-0136:

1. **Do not** edit the historical handoff package in place.
2. Implementation PR: font binary, Theme/UiText swap, tests (`tst_typography.qml` and friends), REUSE, architecture note, changelog fragment.
3. A **new** handoff / brief amendment package (and Brand Guide / ADR follow-ups naming Playfair instead of Ysabeau) follows after the shell change is accepted — same pattern as Hearth amending the handoff without rewriting old frames.
4. Prototype `?font=playfair` already exists in the Hearth light prototype for review; keep it as a lab switch until the default is Playfair.

## Non-goals

- No change to Onest, radii, light, focus, Ember rules or information architecture.
- No merge into open Hearth implementation PRs.
- No force-keep of Literata on any content-title surface.
