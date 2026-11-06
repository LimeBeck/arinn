<!-- SPDX-FileCopyrightText: 2026 Arinn contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Design brief: music in Hearth light

How the Music section and the music player fit the Hearth light concept. Issue [#796](https://github.com/LimeBeck/arinn/issues/796), from the TV design review [#756](https://github.com/LimeBeck/arinn/issues/756).

**Status:** accepted by the owner on 2026-11-06 (decisions MH-1…MH-8 below; [ADR-0142](../../../adr/0142-music-in-hearth-light.md)).
- **Scope.** This brief is the visual contract for music. It amends the [Hearth light brief](../hearth-light/README.md), which only said that the music screens "adopt the same roles", and replaces the layout parts of the earlier [music design brief](../../music-design-brief.md).
- **What carries over.** The behaviour rules of the earlier brief still apply: sources, capabilities, queue replacement, honest states, the state sheets and privacy.
- **What changes.** Its layout B player row and its music prototype are replaced.

## Package

| File | Content |
| --- | --- |
| [`frames.html`](frames.html) | The eleven frames of this brief, drawn on the 1920 × 1080 canvas. Add `#only=<frame id>` to the address to render one frame at full size. Fonts load from the network. |
| [`screenshots/`](screenshots/) | 1920 × 1080 renders of the frames (`mh-*.png`; `mh-05` is 1920 × 700). `mh-01`, `mh-10` and `mh-11` show one room over two sources. |

The frames are illustrations. Where a frame and the text below differ, the text applies. All track, artist, album and station names and all artwork are synthetic. The UI strings are Russian, as on the TV.

## Problem

The owner reviewed the running Music section on the TV on 2026-11-06 ([#756](https://github.com/LimeBeck/arinn/issues/756), findings M1–M27). They found that it looks like a music service's client embedded in Arinn, not like part of Arinn. The causes:

1. **It was designed before Hearth light.** The music brief and its prototype were merged on 2026-11-01 and 2026-11-02, the same days Hearth light was accepted. They use the earlier language: uppercase eyebrows, plates under everything, Ember as a focus fill. The implementation also drifted from the prototype: a flat list instead of an artwork grid.
2. **Its hierarchy is a service's, not Arinn's.** The screen starts with the source and the facets, then a list. The housekeeping actions *Refresh*, *Disconnect* and the source picker sit on the main browsing surface. Home and Library show content first and keep housekeeping in Settings.
3. **It uses none of Arinn's own primitives.** There is no hero with a Playfair title, no shelves, no floor light and no pool under focus. Yet music artwork is the most colourful content Arinn has, and every cover is a ready pool hue.
4. **The player reads like a form.** Ten identical unlabelled icon buttons compete with the track and the artwork.
5. **Arinn cannot see music when you leave the section.** Music is the one activity that keeps sounding while the user moves around Arinn. Yet it is absent from Home, the Quick menu, the status area and Ambient (M2–M4).

## Principle: music is the fire in the hearth

Music is not a service inside Arinn. It is what keeps burning in the room while you do something else. It has three roles, and each is built from primitives that already exist. Music gets no visual language of its own.

| Role | What it is |
| --- | --- |
| **Room** | The Music section is laid out like Home and Library: a hero describes the focused item, shelves of artwork sit below it, and the light follows the artwork. The source and the housekeeping move to secondary places. |
| **Listening** | *Now playing* is a calm screen by the fire. It has a large cover with its hue on the floor and the title in Playfair. Transport is minimal; everything else is in *More*. |
| **Presence** | While music plays, it shows in the same form everywhere: in the status area, as the first Continue card, as the media card of the Quick menu and as a line on Ambient. |

## Owner decisions

Given on 2026-11-06 in the Claude session of [#756](https://github.com/LimeBeck/arinn/issues/756). The comparison of directions A, B and C was shown to the owner as a review page; the frames of B and C are kept as `mh-08` and `mh-09`. Later the same day, the owner found that a source switch still splits the room into modes. They chose one room over all sources (MH-1, MH-5, MH-9, MH-10) from a second review page.

| ID | Decision |
| --- | --- |
| MH-1 | **Direction A, shelves, one room for all sources.** The Music root is a Home-like room with a hero and shelves. The shelves gather entries from every connected source at once, and each entry shows where it comes from with a small source mark. Rejected:<br>• B, where the section opens on the player and the catalogue is a shelf below;<br>• C, the current list restyled to the roles;<br>• shelves labelled per source;<br>• music only as shelves on Home. |
| MH-2 | **Continue card, variant 1.** A centred cover over its own blurred backdrop, a playing equaliser in a neutral chip and an Ember progress line. Rejected: a mini-player (cover on the left, time on the right) and a full-bleed cover, which crops a square cover by 44 %. |
| MH-3 | **Player row.** Previous, play/pause and next, then the labelled pills *Like* and *Queue* and a *More* button. Dislike, *Radio from this track*, *Pin station* and *Stop* move to the *More* sheet. This replaces the layout B row selected on 2026-11-01. |
| MH-4 | **Status area.** While music plays, an Ember dot and "track · artist" appear left of the clock. |
| MH-5 | **No source switch.** Browsing never selects a source. A *Sources* pill in the header opens a sheet with each source's state and its *Connect*, *Refresh* and *Disconnect* actions, off the browsing surface. Settings is not used: it has no Music category, and its rows hold typed values, not actions (§5.6, [ADR-0065](../../../adr/0065-provider-settings-contract.md)). |
| MH-6 | **Stations on Favourites.** Stations can be pinned to the Favourites shelf on Home. |
| MH-7 | **Quick menu.** While media plays, the *Now playing* row becomes a media card with transport and *Open player*. |
| MH-9 | **Order on shared shelves.** Personal selections come first: entries a streaming source offers as made for the user, such as My Wave or a personal mix, ahead of ordinary entries. This holds for every source that offers them, current and future. The ordinary entries follow by date, newest first. |
| MH-10 | **Search and lists cover every source.** *Find* searches all connected sources at once. In the full *All* lists, the source is a filter ("All · This computer · <service>"), not a mode. It starts at *All* every time. Identical items from two sources are **not** merged in this version: each keeps its own card and mark. |
| MH-8 | **Ambient during music.** Automatic Ambient may start while music plays, and it shows the current track. It still does not start while video or other media plays. This changes [specification §5.7](../../../specifications/arinn-specification.md#57-system-ui-power-and-ambient). Until now the rule was violated by accident (M4). |

## Screens

Measurements are for the 1920 × 1080 canvas. Unlisted values come from the [Hearth light brief](../hearth-light/README.md): tokens, light, focus, typography and shape roles.

### Music root (`mh-01`)

- **Top bar** as on Home: the lockup on the left and the status area on the right (MH-4). Music gains the screen chrome it lacks today (M20).
- **Hero** (x 120, y 150, fixed height 240), as on Home. It describes the focused item and never repeats the row label.
  - The eyebrow names the kind: "Station", "Album · 2024", "Playlist · 42 tracks" or "Artist". An Ember dot plus "playing" is added when the item is the one that plays.
  - Below it come the title in Playfair Display 96 (68 on two lines) and a meta line of 28.
- **Header controls** on the right of the hero line: a *Sources* pill and a *Find* pill (MH-5).
  - *Sources* opens a right-hand sheet 720 wide that lists every music source: its name, its icon in the neutral chip and its state (connected, needs sign-in, scanning, unavailable with a reason) (M11).
  - Each row holds that source's housekeeping as pills, by capability: *Connect*, *Refresh* and *Disconnect*. *Disconnect* asks for confirmation, with Cancel as the initial focus.
  - The sheet never filters what the room shows. A provider's typed settings, such as local music folders, stay in their Settings category.
  - *Find* opens the music search screen (`mh-10`).
- **Shelves** gather entries from every connected source (MH-1), in this order. A shelf no source can supply is absent, not empty.
  1. *Stations*: personal stations and mixes, pinned stations, and a source's station over the user's own music (for local music, "All my music, shuffled"), as 16:9 cards 448 × 252. The playing station shows the playing chip (see *Continue card*).
  2. *For you*: personal playlists, square covers 240 × 240, r12.
  3. *Albums*: square covers 240 × 240.
  4. *Artists*: round portraits 240, with a neutral chip and initial when there is no portrait.
  5. *Recent tracks*: a short shelf of square covers. Selecting one plays it in the context it came from.
- **Source mark.** Every caption starts its subtitle with the source's line icon, 22 px in `text-3`: a cloud for a streaming service, a drive for this computer. The detail and *Now playing* name the source in words in their eyebrow. With only one connected source the mark is still shown, so the meaning does not change when a second source is added.
- **Order (MH-9).** On every shelf:
  - personal entries come first: what a provider returns in its `selections` facet, in that provider's own order. With several such providers, the providers follow the order of the *Sources* sheet;
  - ordinary entries follow by date, newest first.
  - The protocol's `Entity` has no date yet. Adding an optional date is a separate `contract-change` task. Until it lands, ordinary entries keep each provider's own order, and the shelf takes one entry from each provider in turn.
- **Late sources.** A source that answers later adds its entries without moving focus. Entries arrive before the focused one only while the shelf has no focus.
- **No merging (MH-10).** An album or artist present in two sources appears twice, each with its own mark. Playing always uses the source of the card: a queue never mixes sources.
- **Each shelf ends with an *All* tile.** It opens the full grid or list for that facet (`mh-11`).
  - The view has text tabs "All · This computer · <each connected service>". The selected tab carries the Ember dot. It starts at *All* each time.
  - It offers *By date* and *A–Z* order.
  - Lists use container rows; grids use the shelf covers.
- **Light.** Rows follow the Hearth light rules: the focused row at 0.82, other rows at 0.62, the floor light on, the pool keyed to the focused artwork.
- **First use.** With no connected source, the hero explains the next step ("Connect a source") and offers that one action, which opens the *Sources* sheet. The shelves stay absent. Arinn never shows an empty connected library.
- **A source drops out.** Its entries leave the shelves; focus moves to the nearest surviving entry. The *Sources* sheet says why. A playing session is not interrupted by browsing.

### Album, playlist and artist detail (`mh-02`)

Built like the movie detail:
- the backdrop is the blurred cover, with a left scrim;
- the cover is 420 × 420, r12;
- the eyebrow, then the title in Playfair Display 104 (80 on three lines), then the creator and length as meta.

Actions are pills: the primary *Listen*, then *Shuffle* where the source supports it and *Radio from album* with the matching capability. Opening the detail never starts playback.

Tracks are container rows r20 with the number, the title, the artist when it differs from the album's, and the duration.
- At rest the rows have no plate and use `text-2`. In focus they get the 8 % plate and the rim.
- The row that plays carries the playing equaliser in place of its number.
- An unavailable track stays readable and says why.

An artist detail uses the same frame: a round portrait, then shelves of releases and tracks. A shelf the source cannot supply is absent. The eyebrow names the source in words ("Album · 2024 · This computer").

### Search (`mh-10`)

- **One field for every connected source** (MH-10), laid out like *Site or search*: the rim in focus, Onest text and an Ember caret (M9). A line under the field says where it searches: "Searching everywhere: this computer and <service>".
- **Results** come as shelves *Artists*, *Albums* and *Tracks*, each entry with its source mark.
  - Results are not merged. Within a shelf, they are ordered by the source's own relevance, one source after another in turn.
  - A source that is still answering adds a quiet line, "<service> is still searching…", not a full-screen spinner.
  - Late results never move focus.
- **Back** closes the keyboard first, then returns to the root with the focused entry restored.

Back restores the opening entity on the root or in the list (M16).

### Now playing (`mh-03`)

- **Artwork** 560 × 560, r12, on the left. The floor and the pool take the artwork hue.
- **Copy:**
  - the eyebrow is the context: an Ember dot plus "My Wave · source" or "Album · source";
  - the title in Playfair Display 96, with the fit steps 96/80/64 from #790;
  - the artist in Onest 600 32 `text-1`;
  - album and year as 28 meta.
- **Seek line.** A 4 px track at 20 % white with an Ember fill. The knob shows only in focus. Elapsed and total time sit below it in tabular figures, with hours when a track is longer than an hour (M25). Left and Right on the line seek; there is no separate seek button.
- **Row**, in one line and vertically centred (MH-3):
  - previous: round, 96;
  - play/pause: round, 120. It is the default focus when the screen opens (M13);
  - next: round, 96;
  - *Like*: a pill with a heart and a label;
  - *Queue*: a pill with a label.
- **Focus and like states.**
  - A focused round button is a white circle with an ink glyph. A focused pill is a white fill with ink text.
  - *Like* shows its selected state as a filled heart plus "Liked", not only as a stronger plate (M18). It has pending and failure states.
- **D-pad.** Up from the row goes to the seek line, Down from the seek line returns to play/pause. Left and Right move along the row and stop at its ends (M15).
- **Labels.** Labels under the round buttons appear in focus at 26 (M14).
- **More.** A round *More* button (96, `more_horiz`) ends the row and opens the *More* sheet. It is always present. The Menu key keeps its §6.2 meaning (global navigation) and never opens the sheet.
- **No Stop button in the row.** Stopping is in *More*.

### More sheet

A right-hand sheet 720 wide, like the tile actions sheet. It lists only the actions the session supports, as container rows with a line icon:
- *Dislike*;
- *Radio from this track*;
- *Pin station to Home* (MH-6), or *Unpin* when it is pinned;
- *Go to album*;
- *Go to artist*;
- *Stop*.

Back closes the sheet and restores focus to the control that opened it.

### Queue

A right-hand sheet 720 wide.
- **Header.** The station name for a continuous queue, or the album or playlist name.
- **Rows.** Container rows r20 with the cover 72, title, artist and duration. The current entry carries the playing equaliser and is the initial focus.
- **Continuous queue.** It shows the loaded window and a closing line "More tracks will follow", never a total count or duration.
- **Empty state.** "The queue is empty".
- **Back** restores focus to *Queue* (M16, M17).

### Dialogs and states

- **Replace the current queue.** This uses the handoff confirm dialog: w900, r48, a scrim, Cancel as the initial focus (M6).
- **Loading.** The shelves use the Library skeletons: artwork-shaped plates at 4 %, not a text line (M19).
- **Errors.** An error names its cause and offers an action, for example "Yandex Music is not responding · Retry". It clears when the source recovers. No bare "Unavailable" caption remains on screen (M5, M8).
- **The state sheets of the earlier music brief still apply.**

### Presence

**Status area (`mh-01`, `mh-04`; MH-4).**
- While music plays, the status area shows an Ember dot and "track · artist" (Onest 500 24, `text-2`, at most 420 px, ellipsis) left of the network icon and the clock.
- On pause the dot and the line go.
- Over a video player and on Ambient the line is not shown.
- It appears on every screen that has the status area.

**Continue card (`mh-04`, `mh-05`; MH-2).** The music session is the first Continue entry while it plays or is paused.
- **Shape.** It keeps the Continue geometry: 448 × 252, r12, rim focus, caption below.
- **Inside:**
  - the backdrop is the cover itself, scaled to cover the card, blurred by 28 px, with a 30–55 % Charcoal shade;
  - the cover is centred at full height minus 22 px top and bottom (208 × 208 on the 448 × 252 card), r8, with a 1 px hairline and a cast shadow;
  - the playing chip sits top left at 16 px: a neutral chip 56 × 42 holding three Ember equaliser bars. The bars move while playing. On pause they stop and turn `text-3`;
  - the Ember progress line runs along the bottom edge of the image.
- **Caption.** The track as the title; "artist · station or album" as the subtitle.
- **Hero.** The eyebrow is "My Wave · playing" or "Paused". The meta line names the keys: "OK — open player · ⏯ — pause".
- **Keys.** OK opens *Now playing*; Back from there returns to Home with focus on the card. Play/Pause acts on the music session without moving focus (M3).
- **Missing cover.** The station cover is used, then the source icon in a neutral chip.
- **Reduced motion.** The bars stand still at mixed heights.

**Favourites (MH-6).** A pinned station is a Favourites shelf item: its name with the source icon in the neutral chip.
- Selecting it starts the station, replacing the current queue with the usual confirmation, and opens *Now playing*.
- The pin is made in the *More* sheet. It is removed there or in the shelf item's action sheet.

**Quick menu (`mh-06`; MH-7).** While a media session plays or is paused, the *Now playing* row becomes a media card.
- **Shape.** A container r20, 624 wide and 284 high, with a faint glow in the artwork hue at the left.
- **Contents:**
  - the cover 128, r8;
  - an eyebrow with the equaliser and "station · source";
  - the title in Onest 600 32 and the artist in 24 `text-2`;
  - the Ember progress line;
  - a row of round buttons 64 (previous, play/pause, next) and an *Open player* pill.
- **Focus.** It starts on play/pause. Left and Right move along the row; Down goes to *Sound*.
- **Open player** opens *Now playing* for music, or the video player for video. Back from there returns to the sheet.
- **Video.** The same card shows a video session: the poster in place of the cover, the episode in place of the artist.
- **Without a session** the existing single row stays. The card is fed by the music session as well as by media sessions (M2).
- **Two sessions at once.** A media session (a browser tab, a video player) and the music session can both exist.
  - Only sessions that are **playing or paused** take part. A stopped session, titled or not, is not shown in the Quick menu; Continue may still show it (§5.1). With no session playing or paused, the plain row stays.
  - **One precedence order** decides which session the card shows:
    1. a playing session ranks above a paused one;
    2. between two sessions in the same state, the one whose playback state changed last ranks higher.
  - The time of a change never overrides the state.
  - Example: both sessions play and the user pauses the one in the row. The card keeps its session, because playing still ranks above paused, and nothing moves.
  - Its transport and *Open player* act only on the session it shows.
  - The other session is a compact container row under the card, 624 × 88: its artwork 56, title, state and its own play/pause. OK on the row toggles that session. The card and the row change places only when the precedence order changes, for example when the session in the row starts playing while the card's session is paused.
  - No product or provider is preferred.
- **Fitting the sheet.** With the media card, and possibly the second-session row, the sheet's rows no longer fit the screen. The fixed parts are the header (clock and title, y 64–180) and the hint line (y 996).
  - In between, the rows from y 200 to y 960 form one column that **scrolls with focus**. It follows the handoff's one-item-from-the-edge rule: the focused row never sits at the edge, and the next row stays partly visible.
  - A 96 px fade over the bottom edge of the column shows that more rows follow, and a 48 px fade appears under the title once the column has scrolled.
  - There is no scroll bar and no other key: Down from *Power* reaches *Open Home* and *Close Arinn*, which the spec requires.
  - Opening the sheet always starts at the top, with focus on play/pause.
  - The row sizes stay as they are: compressing them would drop below the 96 px TV row and the 26 px action text.

**Ambient (`mh-07`; MH-8).** It follows the Ambient section of the Hearth light brief:
- black, the clock in Playfair Display 220 at the lower left, the date in Onest 30, the block drifting against burn-in.
- **While music plays,** a track line sits under the date, in the same drifting block:
  - the cover 96, r8, at 90 %;
  - the equaliser and the track in Onest 600 30;
  - "artist · station" in 26 `text-3`.
- **Light.** The hearth light at the lower left takes a faint second glow in the artwork hue.
- **Pause and stop.** On pause the line dims to 60 % and reads "Paused". When the music stops, the line goes and the Ambient status line returns.
- **Keys.** Entering or leaving Ambient never plays or pauses music. Play/Pause on the remote acts on music and does not wake the screen; every other key wakes it as before.

### Navigation icon

The Music item in global navigation uses a music note line icon (`music_note`), not `volume_up` (M21).

## Mapping to the #756 findings

| Finding | Covered by |
| --- | --- |
| M2, M3 | *Presence*: Quick menu and Continue card |
| M4 | MH-8: dropped as a bug; Ambient during music is intended |
| M5, M6, M8, M19 | *Dialogs and states* |
| M9 | *Search* |
| M10, M13, M14, M15, M18 | *Now playing* |
| M11 | The *Sources* sheet (MH-5) |
| M12, M17 | Container rows in the detail and the Queue |
| M16 | Back rules in the detail, Queue and *More* |
| M20 | Top bar and screen chrome on the Music root |
| M21 | *Navigation icon* |

M1, M7 and M22–M27 are implementation defects that this brief does not change.

## What is not changing

- **Behaviour.** Sources, capabilities, the provider-owned playback and credentials, the queue replacement confirmation and the privacy rules of the earlier [music brief](../../music-design-brief.md) and [§5.3.1](../../../specifications/arinn-specification.md#531-native-music-owner-requested-extension) stay.
- **Contract.** The music protocol ([`docs/plugin-api/music.md`](../../../plugin-api/music.md)) is not changed by this brief.
  - The shell gathers the shelves and search results by asking each provider with `GetPage`.
  - Personal entries come from the existing `selections` facet.
  - One optional field is needed later: a date on `Entity` for MH-9. It is a separate `contract-change` task, and so is any other field a screen needs that the protocol lacks.
- **Video.** The video player and its OSD are out of scope.

## Verification

Not yet verified. The frames are static illustrations: no remote-navigation prototype, pixel comparison or TV check has been made for this brief. Each implementation task carries its own tests and a TV check with the sound muted.
