# ADR-0136: Hearth light visual direction

- **Status:** Accepted
- **Date:** 2026-11-02
- **Deciders:** Owner, decisions D-1…D-10 of the Hearth light brief (2026-11-02, Claude session, [#721](https://github.com/LimeBeck/arinn/issues/721)). The setting ids and the shell implementation are delegated technical choices ([ADR-0037](0037-agent-autonomy-and-owner-gated-decisions.md)).
- **Amends / Supersedes:** amends [ADR-0019](0019-design-handoff-contract.md) (the visual contract: content-title face, focus treatment, light field) and [ADR-0112](0112-shell-preferences-as-core-settings.md) (two more core settings)
- **Amended by / Superseded by:** [ADR-0137](0137-playfair-display-for-content-titles.md) (D-2: Playfair Display 600 replaces Ysabeau; Literata is not a content-title face); [ADR-0140](0140-tv-review-visual-decisions.md) (floor over an Activity, near bands, Library tabs, keyboard context rows)
- **Specification:** §5, §5.1, §5.5, §5.6, §5.7, §15

## Context

The owner observed that the shell looks like most TV shells. A design review on 2026-11-02 traced the likeness to the generic TV template: uniform rounded plates under every element, uppercase eyebrows, a white-ring focus, letter monograms on hashed pastel colours and a centred colour glow behind content. Brand Guide v0.1 asks for most of that template in its sections 5 and 15. The owner approved every proposal and asked the agent to act as the designer. The answer is the [Hearth light brief](../design/briefs/hearth-light/README.md) and its [interactive prototype](../design/prototypes/hearth-light/README.md), refined in a second round after a review at sofa scale and against the owner's captures of the running shell.

The historical handoff ([ADR-0019](0019-design-handoff-contract.md)) and Brand Guide v0.1 are never edited, so the decision needs a contract that sits on top of them.

## Decision

Arinn adopts the **Hearth light** visual direction (variant A of the brief). The brief and the prototype amend the design handoff for everything they specify; the handoff applies to everything else. [Brand Guide v0.2](../design/brand/brand-guide-v0.2.md) replaces v0.1 as the brand reference. The owner's decisions:

| ID | Decision |
| --- | --- |
| D-1 | Variant A: light from below, the lit row, a content-title face with a hand, three shape roles, neutral monograms, rim-light focus, the ignition and the short ignition (*kindle*) |
| D-2 | Ysabeau (OFL-1.1) for content titles: hero, detail, player title, episode title cards, Ambient clock; Onest stays for system text. The owner first chose Literata and revised it the same day after the prototype's font lab: Literata looked bookish |
| D-3 | Home favourites are a shelf of names, not tiles |
| D-4 | Interface sounds are on, at *Quiet* by default |
| D-5 | The light follows the time of day automatically, with a setting to turn that off |
| D-6 | Brand Guide v0.2, with new sections 5 and 15 |
| D-7 | Focus is a rim light from below with a dark mat around artwork, not the white ring |
| D-8 | The caption of the focused Continue card stays visible |
| D-9 | Applications tiles hold their application's identity light |
| D-10 | The favourites shelf shows provider icons only, no letter monograms |

Fixed rules that the implementation must keep:

- **Ember is still never a focus colour.** Focus is light: the rim, the lift from the bottom edge and the lit row.
- **Interface sounds never play over media.** No interface sound plays while video or music plays, OSD navigation included; error sounds are the only exception. The level follows the system output and its mute. Sounds are original, synthesised works; no third-party samples.
- **The time-of-day light only changes presentation**: light, a warm veil and text brightness. It never changes behaviour, and contrast stays within the brief's ratios in every period.
- **Settings.** Two core settings join the reserved `arinn` source ([ADR-0112](0112-shell-preferences-as-core-settings.md)): *Settings › Sound › Interface sounds* (Off, Quiet, Normal; default Quiet) and *Settings › Appearance › Light follows the time of day* (on by default). *Light field* keeps turning the floor light and the pool off.

## Consequences

- The shell implementation is split into tasks listed in the brief (*After acceptance*): tokens and typography, the light system, shape roles and focus, Home and navigation, Applications, Ambient with the ignition and kindle, and interface sounds. Each keeps the existing UI comparison (`scripts/ui-compare.py`) and updates `docs/architecture/ui-components.md`.
- Ysabeau becomes a bundled font. Adding the font file and its `REUSE.toml` entry is a licensing change and needs the owner's review in its pull request.
- `Settings1` gains two entries in the daemon's own schema; the wire contract does not change.
- Interface sounds need pre-rendered WAV files (rendered from the prototype's synthesis), `QSoundEffect` and a media guard in the shell.
- The music screens ([#677](https://github.com/LimeBeck/arinn/issues/677)) follow the same roles.
- The uppercase *Continue* eyebrow, a part of the old handoff, goes away with this direction.

## Alternatives considered

- **Variant B** (Hearth light without a separate content-title face): less distinctive; the title face was the most visible single difference.
- **Variant C** (*Calm*: remove plates, pastel monograms and uppercase eyebrows only): a smaller step that would leave the shell recognisably generic.
- **Keep the current look**: rejected by the owner's observation that started the review.
- Rejected details: Literata (first choice; too bookish set large), Source Serif 4 and the other font-lab candidates (Playfair Display, Prata, Cormorant Garamond, Bona Nova, Brygada 1918, Piazzolla, Tenor Sans, Jost, Geologica); favourites as tiles; letter monograms on the shelf; hiding the focused caption (tried in the second round; the empty space looked odd); Applications as a shelf or as plain tiles.

## Verification

- The brief's sofa-scale review and the prototype's screenshots and recordings (`docs/design/prototypes/hearth-light/`) document the intended result.
- Each implementation task compares the shell with the prototype through `scripts/ui-compare.py` and its QML tests.
- D-7 needs a check on the owner's TV at 3 m with dark and bright artwork; until it is done, the rim light is accepted but not verified.
- Interface sounds need a listening check on the owner's setup; the media guard needs a test.
- Accepting this ADR does not mean any of these checks has passed.
