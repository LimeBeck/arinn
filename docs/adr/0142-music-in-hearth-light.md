# ADR-0142: Music in Hearth light

- **Status:** Accepted
- **Date:** 2026-11-06
- **Deciders:** Owner, decisions MH-1…MH-8 of the music Hearth light brief, given on 2026-11-06 in the Claude session of the TV design review ([#756](https://github.com/LimeBeck/arinn/issues/756), [#796](https://github.com/LimeBeck/arinn/issues/796)). Implementation details are delegated technical choices ([ADR-0037](0037-agent-autonomy-and-owner-gated-decisions.md)).
- **Amends / Supersedes:** amends [ADR-0136](0136-hearth-light-visual-direction.md) (adds the music screens to the Hearth light contract) and replaces the layout parts of the [music design brief](../design/music-design-brief.md), including the layout B player row the owner selected on 2026-11-01
- **Amended by / Superseded by:** —
- **Specification:** §5.1, §5.3.1, §5.7, §15

## Context

The Hearth light brief ([ADR-0136](0136-hearth-light-visual-direction.md)) said only that the music screens "adopt the same roles". The music design brief and its prototype were written just before Hearth light, in the earlier visual language. The implementation then moved further away from both.

When the owner reviewed the running Music section on the TV on 2026-11-06, they found that it looks like a service's client embedded in Arinn ([#756](https://github.com/LimeBeck/arinn/issues/756), findings M1–M27).
- Its hierarchy starts with the source, and housekeeping actions sit on the browsing surface.
- It uses none of Arinn's primitives: hero, shelves, light.
- The player row has ten unlabelled icons.
- Music is invisible outside the section, although it is the one activity that keeps playing while the user moves around Arinn.

The owner compared three directions on a review page: A, shelves; B, the player first; C, a restyle of the current list. They chose A, then settled the remaining points. A source switch in the header still made sources into modes. A second review page compared four variants: one room with source marks; shelves labelled per source; one card per item, with duplicates merged across sources; and music only as shelves on Home. The owner chose the first, without merging, as the simplest step.

One finding, M4, turned out to be a product question rather than a bug. Automatic Ambient starts while music plays. Specification §5.7 forbids that, but it is how music is usually listened to on a TV.

## Decision

The [music Hearth light brief](../design/briefs/music-hearth-light/README.md) and its frames are the visual contract for music. They amend the Hearth light brief, and the music design brief stays the source for behaviour. The owner's decisions:

| ID | Decision |
| --- | --- |
| MH-1 | The Music root is one Home-like room for all connected sources: a hero describing the focused item, then shelves of Stations, For you, Albums, Artists and Recent tracks. Every entry carries a small source mark. Album, playlist and artist detail is built like the movie detail. |
| MH-2 | The music Continue card shows a centred cover over its own blurred backdrop, a playing equaliser in a neutral chip and an Ember progress line. |
| MH-3 | The player row is previous, play/pause (the default focus) and next, then the labelled pills *Like* and *Queue* and a *More* button that opens a sheet with the other actions. The Menu key keeps its meaning (§6.2). This replaces layout B. |
| MH-4 | While music plays, the status area shows an Ember dot and "track · artist". |
| MH-5 | Browsing never selects a source; there is no source switch. A *Sources* pill opens a sheet with each source's state and its *Connect*, *Refresh* and *Disconnect* actions. Settings has no Music category and holds no actions (§5.6, [ADR-0065](0065-provider-settings-contract.md)). |
| MH-6 | Stations can be pinned to the Favourites shelf on Home. |
| MH-7 | While media plays or is paused, the Quick menu's *Now playing* row is a media card with previous, play/pause, next and *Open player*. It is fed by the music session as well as by media sessions. Only playing or paused sessions take part; a stopped one is not shown there. When both take part, one precedence order applies: playing ranks above paused; between sessions in the same state, the latest state change the shell observed ranks higher; with no observed change for either, the native music session ranks higher. The card shows the higher-ranked session, and its controls act only on that session. The other session is a compact row under it with its own play/pause, and the two change places only when the order changes. No product is preferred. The sheet's rows scroll with focus when they do not fit. |
| MH-9 | On shared shelves, personal selections come first, from every streaming source that offers them (what a provider returns in its `selections` facet, such as My Wave). Ordinary entries follow by date, newest first. The date needs an optional `Entity` field: a separate `contract-change` task. Until then, each provider's own order is kept and providers alternate. |
| MH-10 | *Find* searches every connected source at once. Full lists filter by source ("All · This computer · <service>") without making it a mode. Identical items from two sources are not merged in this version, and a queue never mixes sources. |
| MH-8 | Automatic Ambient may start while music plays, and Ambient shows the current track. It still must not start while video or other media plays, or while an external Activity is in the foreground. |

Music uses the existing Hearth light tokens, light, focus and shape roles, and introduces no visual language of its own. The playing equaliser is the one new mark: three Ember bars, still under reduced motion. It is a status mark like the Ember dot, never a focus mark.

## Consequences

- **Specification.**
  - §5.1 gains the music Continue entry, pinned stations and the status line.
  - §5.3.1 points to the new brief and drops layout B.
  - §5.7 changes the Quick menu row and the Ambient rule.
  - §15 lists the brief in the design contract.
- **Shell.**
  - The shell gathers shelves and search results from every connected provider (`GetPage` per provider), orders them by MH-9 and never moves focus when a late source answers.
  - The shell needs a music Continue projection, a status-area binding, a Quick menu media card that reads both catalogues, an Ambient track line and pinned station items on the Favourites shelf.
  - The Music section is rebuilt around shelves.
- **Owner-gated pieces.** Pins need persistent storage. Whether that is a shell preference or a provider capability is a technical choice for its task. If it needs a protocol field, it is a separate `contract-change` task.
- **Tasks in flight.** The visual items M9–M21 of #756 were put on hold on 2026-11-06 and are now implemented through this brief. M4 is closed as intended behaviour, not fixed.
- **Superseded material.** The music prototype under `docs/design/prototypes/music/` and the layout comparison stay as the record of the earlier brief. They no longer define the target.

## Alternatives considered

- **B, the player first.** The section opens on *Now playing*, with the catalogue as a shelf below. It suits station listening, but it starts from an empty player when nothing plays and handles large catalogues poorly.
- **C, restyle the current structure.** It is cheap and compatible with the queued fixes, but the section keeps reading as a service's catalogue.
- **Continue card variants.** A mini-player with the cover on the left and the time on the right was rejected because its right half stays half empty. A full-bleed cover was rejected because it crops a square cover by 44 %.
- **A source switch (the first version of MH-5).** It turns each source into a mode the user has to choose before browsing.
- **Shelves labelled per source.** The room is still split by source, only vertically, and grows with every source.
- **Merging identical items across sources.** Matching by metadata fails on remasters and editions, and an album drawn from two sources would need a mixed queue, which the protocol does not support. It stays a possible later step.
- **Music only as shelves on Home.** Home would no longer fit one screen (§5.1).
- **Keeping the §5.7 ban on Ambient during music.** It would make the owner's usual listening setup impossible without a manual start every time.

## Verification

- Accepting this ADR does not mean anything is implemented or verified.
- Each implementation task carries UI tests for its screens and states, and a TV check with the sound muted.
- MH-8 needs a test that automatic Ambient may start while only music plays, and that it still does not start while a media session plays video.
