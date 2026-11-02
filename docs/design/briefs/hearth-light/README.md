<!-- SPDX-FileCopyrightText: 2026 Arinn contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Design brief: Hearth light

A visual identity pass that makes Arinn recognisable without giving up anything a living-room interface needs. Issue [#721](https://github.com/LimeBeck/arinn/issues/721).

**Status:** accepted by the owner on 2026-11-02 (decisions D-1…D-10 below; [ADR-0136](../../../adr/0136-hearth-light-visual-direction.md)). Proposed earlier the same day. On 2026-11-02 the owner reviewed the design review below, approved every proposal and asked the agent to act as the designer. This brief and the [Hearth light prototype](../../prototypes/hearth-light/README.md) are the designer's answer. The owner's answers are recorded in [*Owner decisions*](#owner-decisions). A second round on 2026-11-02 followed the designer's own review of the first prototype (see [*Second round*](#second-round)); the owner's first reaction was that the startup animation works. The historical handoff and Brand Guide v0.1 stay unchanged. With the owner's acceptance this brief and the prototype amend the handoff as the visual contract ([ADR-0136](../../../adr/0136-hearth-light-visual-direction.md), amending [ADR-0019](../../../adr/0019-design-handoff-contract.md)); [Brand Guide v0.2](../../brand/brand-guide-v0.2.md) and [specification §15](../../../specifications/arinn-specification.md#15-design-contract) carry the decisions. The shell is changed by separate implementation tasks.

## Problem

The owner observed that Arinn looks like most TV shells. The review traced the likeness to a shared template, not to any one mistake. Every item below is in the current shell ([current captures](../../current-ui/README.md)), and almost every item is also in other TV launchers, media-centre skins and generated "TV UI" mock-ups:

| Template element | In Arinn |
| --- | --- |
| Near-black background | `#131416` |
| Top-left hero: uppercase, letter-spaced eyebrow, large bold title, meta line | Home hero (`eyebrow: [24, 600, 1.92]`) |
| Horizontal rows of cards with one radius everywhere | `cardRadius: 26`; radii 26–30 on cards, tiles, rows, menus and buttons |
| A grey translucent plate under every element | `plate: rgba(255,255,255,0.06)` on cards, navigation items, settings rows, favourites and buttons |
| Focus as scale + white ring + drop shadow | scale 1.06, ring 45 % white |
| A floating rounded side drawer for navigation | global navigation |
| Letter monograms on hashed pastel colours | `identityColors`, six colours chosen by a string hash |
| A dominant-colour radial glow behind content | `AmbientGlow`, centred upper right |
| A neutral geometric grotesque for everything | Onest |

The brand guide itself asks for this template. Section 5 lists "almost-black background, large white typography, horizontal content rows, soft cards". The designer prompt in section 15 repeats it. The result is accurate to the brief and average by construction.

Part of the template is not style but function, and stays: large type, a safe area, horizontal rows for the D-pad, an obvious focus, one action per OK and calm motion. The identity has to come from layers that do not cost usability: light, type, shape, motion, the idle screen and sound.

### What is already Arinn's own

- **Focus is light, not colour.** The handoff direction is "2a Depth with Light", and Ember is never a focus colour ([handoff](../../handoff/README.md), *Colours*). For a brand named "hearth" this is the right seed. The implementation reduced it to the same centred colour glow other shells use.
- **The running application recedes behind Arinn** (scale, blur, brightness). Few TV shells sit over real desktop applications this way.
- **The words.** "Return", "Running now", "… is paused" (Russian UI strings) are simple and warm.
- **Ambient** already knows what is paused, but visually it is a clock on black.

## Field check against the running shell

On 2026-11-02 the owner shared captures of the shell on their own machine with a real library: Home, a film detail, a series detail with its episode rail, and Settings. The captures are not stored in the repository: they show real titles, third-party artwork and service logos, and a personal path. Only the findings are recorded here.

They confirm the diagnosis and add constraints that synthetic fixtures hide:

| Finding in the real shell | Consequence for this direction |
| --- | --- |
| Real artwork is bright, saturated and busy (animation, illustrated posters). Captions overlaid on a gradient become hard to read on light artwork. | Captions go **below** the image (shape roles). The Ember line gets a 2 px Charcoal band above it, so it stays visible on light artwork. The row light (other rows at 0.62) keeps bright artwork from competing with the focused item. |
| The colour light field turns the whole screen one hue (a green cast across Home). | The floor light stays low and capped in chroma; the room keeps its warm floor under any artwork. |
| Long titles are common. The Home hero cuts them to one line with an ellipsis, and the detail title is cut after two lines. | The hero has a fixed height and fits the title in steps: 96 on one line, otherwise 68 on two. The detail title steps from 104 (two lines) to 80 (three). The prototype measures the text, not its length (screenshots 12 and 14). |
| A poster without artwork shows a letter monogram on a plate. | A missing poster becomes a **title card**: the title in Ysabeau on a Charcoal card (screenshot 13). It reads as a deliberate book cover, not a broken image. |
| Episodes without their own stills repeat the series image on every card. | When episode stills are missing, the rail uses title cards with the episode number, not the same image repeated. |
| Favourites mix provider icons, letter monograms and a search glyph; long names wrap to two lines inside tiles. | Every icon sits in the same neutral chip. On the shelf, a name is one line, at most 360 px, with an ellipsis (the prototype has one such name). |
| Metadata is often sparse: a year and a synopsis, no runtime, rating or genre. | Layouts must not depend on the meta line being full. The content title and the light carry the screen when the meta line is short. |
| Settings values that are long (paths, mappings) wrap into three lines in the value column. | Long values move under the label and truncate in the middle; the value column is for short values. |

The uppercase "Continue" eyebrow above the hero repeats the row label, but the current handoff asks for exactly that ("continue media" gets the uppercase Continue eyebrow). It is part of the design this brief replaces, so it changes only with D-1.

Deviations from the existing handoff and specification seen in the same captures are implementation bugs, independent of the owner decisions below. Each needs its own task (some may already be covered by open work, such as the translation pull requests):

- a series detail eyebrow uses the plural category name instead of the item kind (the source string `Series` is shared with the Library tab and translated in the plural);
- detail progress is a percentage instead of the remaining time, and the primary action has no position ("Continue" instead of "Continue from …"); "Start over" is missing for a started item;
- the episode duration and several provider setting texts appear in English in the Russian interface;
- the whole interface looks uniformly dimmed in the captures (titles render mid-grey). This may come from the capture method; if it is the real output, it is a contrast defect to check on the device.

## Direction: Hearth light

> Arinn is a dark, quiet room with one warm light low in it. What you are about to choose stands in that light.

In the A1 mark the Ember dot sits at the base of the letter. The light of a hearth comes from below, warm and low, and leaves the rest of the room in shadow. Arinn's light currently does the opposite: a cool 1 px highlight on the *top* edge of every plate and a colour glow in the *upper right*. The direction turns it over:

1. **Light from below.** A low, warm floor light is the atmosphere of every content screen. The focused item stands in a pool of that light, which follows focus along the floor.
2. **The lit row, the dark room.** The row in focus is in the light, the other rows recede. The rest of the screen is not covered with plates. Containers are dark until focus reaches them.
3. **Content titles have a hand, system text does not.** Film and series titles are set in Ysabeau, a sans-serif with calligraphic forms, so a name reads like it was written for that film. Menus, settings and labels stay in Onest. (The first round used the book serif Literata; see *Font lab*.)
4. **Colour belongs to content and light.** Artwork brings colour. An application without artwork gives its identity hue to the light, not to a pastel square.
5. **One Ember.** Orange stays rare: running, progress, current section, done, toggle on, and the startup ignition. It is never a fill and never a focus colour.

The guide's prohibitions still hold: no literal flame, fireplace or house, no Nordic or Viking styling, no red-yellow fire palette, no strong glow, no RGB gaming look. The light is a matter of luminance and temperature, not of orange.

## Variants for the owner

The [prototype](../../prototypes/hearth-light/README.md) switches between these live; `?variant=` selects one.

| Variant | Light | Content titles | Shapes and plates | Monograms | Focus | Favourites |
| --- | --- | --- | --- | --- | --- | --- |
| **A. Hearth light** (recommended) | Floor light + pool | Ysabeau (first round: Literata) | Three roles, no rest plates on containers | Neutral | Rim light from below | Text shelf |
| A′. Hearth light, Source Serif | as A | Source Serif 4 | as A | as A | as A | as A |
| B. Hearth light, sans | as A | Onest 600 | as A | as A | as A | as A |
| C. Calm (conservative) | Current glow | Onest 700 | Three roles, no rest plates | Neutral | White ring | Tiles |
| Current | Current glow | Onest 700 | Uniform r26 plates | Hashed colours | White ring | Tiles |

C is the minimum de-templating: it removes the plates, the pastel monograms and the uppercase eyebrows, and keeps everything else. A adds the parts that make Arinn recognisable at a glance.

## Design system changes

Tokens below are the proposal for variant A. Unlisted handoff tokens (safe area, grid, spacing, focus timings, input modes, spatial navigation) stay as they are.

### Colour

| Token | Current | Proposed | Use |
| --- | --- | --- | --- |
| `bg` | `#131416` | `#0F1115` (brand Charcoal) | Every screen |
| `text-1` | `#F4F5F7` (cool) | `oklch(0.95 0.008 75)` ≈ `#F2EEE8` (warm white) | Titles; fill of focused pills |
| `text-2` | `#C9CBCF` | `oklch(0.80 0.008 75)` ≈ `#C6C1BB` | Secondary text |
| `text-3` | `#9A9DA3` | `oklch(0.66 0.008 75)` ≈ `#9A958F` | Hints, years, disabled |
| `ember` | `#FF8A3D` | unchanged | Running, progress, current, done, toggle on, ignition |
| `hearth` | — | `oklch(0.44 0.07 55)` at its centre, below the screen edge | Base floor light |
| `pool` | — | `oklch(0.50 c′ h)` at its centre, below the screen edge | Light under the focused item |
| `ceiling` | — | black at 50 % at the top edge → 0 at 44 % of the height | The dark upper part of the room |
| `mat` | — | `rgb(11 12 15 / .95)`, 5 px | Dark gap between artwork and rim |
| `rim` | — | `oklch(0.97 0.03 70)`, 95 % at the bottom edge → 10 % at the top | Focus edge |
| `chip` | six hashed colours | `#1C1E23` + hairline `rgba(255,255,255,.06)` | Neutral monogram |

`h` is the hue of the focused item's artwork, or its identity hue. Its chroma `c′` is 0.10, reduced towards 0.065 for cool hues (full reduction at 240°, none beyond ±70° of it), so a blue film tints the floor without making the room cold. The warm `hearth` floor always stays underneath. (Turning the hue towards orange was tried and rejected: blue hues pass through green on the way.) Both centres sit below the bottom edge, so the light on screen is the falloff. The existing handoff rule that light under text stays at or below L 0.48 still applies and has to be measured in the shell; the prototype does not measure it. `text-3` on `bg` stays above 4.5 : 1.

### Light

| Layer | Geometry | Behaviour |
| --- | --- | --- |
| Ceiling | Linear, top 44 % of the screen | Always on content screens; it makes the floor read as light by contrast |
| Floor | Ellipse 100 % × 62 % of the screen, centred 22 % below the bottom edge | Always on content screens; absent on system screens (Settings, Downloads), as the current light field is |
| Pool | Ellipse 30 % × 42 %, centred 8 % below the bottom edge, x = centre of the focused item | Follows focus horizontally and changes hue with a 900 ms cross-fade after the current settle delay, so fast scrolling does not flicker (spec §6.5) |
| Cast | Blurred ellipse under the focused object's image (not under its caption), 84 % of its width, 34 px high, 20 px blur, pool hue at 45 % | Appears and disappears with focus (220 ms) |
| Rows | Focused row: other items at brightness 0.82. Other rows: 0.62 | 220 ms |

*Settings › Appearance › Light field* keeps turning the floor and the pool off. Focus then keeps the rim and the lift, and the screen is flat Charcoal.

The floor and the pool are two gradients painted into a small texture and scaled up. They cost less than the current per-hue three-layer field, and no full-resolution artwork is blurred.

### Time of day

The room gets darker in the evening, as a room with a hearth does.

| Period | Default hours | Light | Warm veil over everything | `text-1` |
| --- | --- | --- | --- | --- |
| Day | 06:00–18:59 | 100 % | none | 100 % |
| Evening | 19:00–22:59 | 85 %, floor hue 5° warmer | 6 % | 100 % |
| Night | 23:00–05:59 | 60 %, floor hue 8° warmer | 22 % | raised to `oklch(0.97 …)`, `text-2` 0.84, `text-3` 0.71, so that under the veil `text-3` stays at 4.8 : 1 (calculated) |

Because the veil also darkens text, the night text tokens are raised to compensate; without that, `text-3` would fall to about 3.3 : 1. The first round only dimmed the light at night, and nobody would have noticed. The veil darkens and warms the whole picture, artwork included, so night reads as night.

Contrast stays within the ratios above in every period. The shell uses its own clock; no location or sunrise data is needed. A setting turns the effect off (owner decision D-5).

### Typography

- **Content titles** (hero, detail, player title, episode title cards, Ambient clock): **Ysabeau**, weight 600, no tracking, sentence case, lining figures. Ysabeau is a humanist sans-serif with calligraphic forms, full Cyrillic, licensed OFL-1.1, and a variable `wght` axis; the shell needs the same `wght`-axis handling as Onest (#546). It keeps the warmth the serif was chosen for without the look of a printed document.
- **System text** stays **Onest**. Page titles of system screens (Library, Applications, Settings) are Onest 600 at 56 with no negative tracking, so they stop looking heavier than the hero.
- **No uppercase eyebrows.** The eyebrow becomes sentence case, Onest 500 26, `text-2`, and carries an Ember dot only when something is running.
- Hierarchy comes from size and family, not from bold versus extra bold.

| Role | Family | Size / weight / tracking |
| --- | --- | --- |
| display (Ambient clock) | Ysabeau | 220 / 600 / 0, lining figures |
| hero | Ysabeau | 96 / 600 / 0 |
| detail title | Ysabeau | 104 / 600 / 0 |
| page title | Onest | 56 / 600 / 0 |
| section | Onest | 30 / 600 / 0 |
| body, control | Onest | 30 / 400 or 600 / 0 |
| meta | Onest | 26–28 / 500 / 0 |
| eyebrow | Onest | 26 / 500 / 0 |
| caption | Onest | 24 / 500 / 0 |

### Shape roles

One radius for everything is replaced by three roles:

| Role | Members | Shape | At rest | In focus |
| --- | --- | --- | --- | --- |
| Content | Posters, stills, backdrops, Continue cards | r10 posters, r12 stills | Artwork, no plate, caption *below* the image | Lift, rim, cast, row light |
| Action | Buttons, chips | Pill | Plate 7 %, primary 14 %. A button must look like a button from the sofa | White fill + ink (unchanged) |
| Container | Navigation items, settings rows, list rows, favourites, Library tabs | r20 | **No plate**, `text-2` | Plate 8 % + rim |

Application tiles are the one *object* role: r18, plate 3.5 % at rest, so a grid of names still reads as tiles. Each tile holds its application's identity light: a radial glow from below the tile in the identity hue, 50 % at rest and full in focus. Colour lives in light, not in a pastel square (D-9).

Rows that scroll horizontally fade out over the left 24–112 px of the screen, so the previous card, kept visible by the handoff's one-item-from-the-edge rule, is not cut hard at x 0 (screenshot 34).

Captions move from an overlay on a darkened gradient to below the image. Progress becomes an **Ember line**: 4 px, flush with the bottom edge of the image, no grey track. It is the Ember motif doing its documented job.

### Focus

| | Current | Hearth light |
| --- | --- | --- |
| Lift | scale 1.06, −6 px, from the centre | scale 1.04, −8 px, **from the bottom edge**: the object rises out of its light |
| Edge | ring 3 px, 45 % white, all round | a 5 px Charcoal mat around the image, then a 2 px rim outside it, bright at the bottom and fading towards the top, plus a faint warm bounce inside the bottom edge of the image |
| Shadow / glow | black drop shadow + colour glow around | cast light under the object, none around it |
| Context | — | the rest of the row at 0.82, other rows at 0.62 |

The rim is a full outline, brightest where the light comes from. The mat separates it from the artwork: without it a light rim disappears on light artwork, which the owner's real library has plenty of. Focus must stay obvious at 3 m on dark and bright artwork alike; this needs a TV check with the owner (D-7).

The caption of the focused Continue card stays visible (D-8: the owner found the empty space under the card odd). The prototype keeps the alternative behind a switch.

### Monograms

An application or service without an icon gets a neutral chip: Charcoal `#1C1E23`, hairline, letter in Onest 600 `text-2` (`text-1` in focus). On the favourites shelf letter chips are left out altogether: a row of single letters reads like a keyboard and says nothing the name does not. Provider icons stay, in the same neutral chip (D-10). Its identity hue colours the floor pool when it has focus, so applications stay distinguishable without six pastel squares in a row. Provider icons are still shown as they are (ADR-0059, ADR-0075).

## Screens

The prototype covers these. Measurements are for the 1920 × 1080 canvas.

### Home

- **Top bar** (y 56): mark 36 + "Arinn" Onest 600 30, **without a pill plate**. Status on the right without plates: Ember ↓ + "82 %", Wi-Fi, clock 30/500.
- **Hero** (x 120, y 150, fixed height 240): eyebrow "Film · 2024" or "Series · S1 E4", or Ember dot + "Running now"; title Ysabeau 96 on one line or 68 on two; meta 28 right under the title. Nothing below the hero moves when the title changes. It describes the focused item, never repeating the row label above it. The current duplicate "CONTINUE" / "Continue" disappears.
- **Continue** (label y 404; one activity plus at most four resumable entries, spec §5.1): 16:9 cards 448 × 252, r12, gap 32, caption below (title 28/600, subtitle 24/500), Ember line along the bottom of the image.
- **Favourites as a shelf** (label y 836): names in a line, Onest 600 32, items 88 high with 28 px side padding and a 28 px gap; provider icons in a neutral chip 56, no letter chips (D-10). At rest `text-2` and no plates. In focus a container plate with the rim, and the pool moves under it. A running application has an Ember dot after its name. It replaces the row of identical 264 × 132 tiles, reads like the names on a shelf and takes half the height (D-3).
- **New**: posters 180 × 270, r10. Its label peeks at the bottom edge. Focusing it scrolls the page and hides the hero, as now.
- **Home over a running application**: the receded application layer and its scale, blur and brightness stay as specified. The floor light takes the application's identity hue.

### Global navigation

An **edge panel**, not a floating card: full height, 660 wide, Charcoal from 97 % to transparent at the right edge, with the screen behind at brightness 0.45 and blur 6. Items are text with line icons 36, no plates at rest, height 84. The current section has an Ember dot before its label. The items follow specification §5: the five sections, a hairline, then **Open services** and **Running** with their counts; whichever holds the activity on screen carries an Ember dot, so there is no separate activity row. **Power** and **Close Arinn** sit at the bottom. This also fixes the current deviations from the handoff (plates and outlines at rest, no icons, Power styled like a section).

### Library

Title Onest 600 56. Text tabs: selected = `text-1` + Ember dot under the label; focused = white pill + ink. Poster grid as in the handoff with r10 posters and captions in `text-2` (`text-1` in focus). Floor light at 60 %.

### Detail

Full-bleed backdrop with the current scrims; floor light at 50 %. Eyebrow, Ysabeau 104 title, meta, a synopsis of three lines at most. Actions: primary pill "Continue from 1:13:42", secondary pills with a 7 % plate. Ember fill on a faint 360 × 4 track + "1 h 32 min left".

**Series** (handoff 07): the copy sits at y 180 with the primary action "Continue · S1 E4" and the season rail below it. Rules for the rail:

- Down from the action lands on the *selected* season tab, then on the *current* episode the first time, not on the first one.
- Focusing the rail moves the page up so the rail sits at y 180, dims the copy to 0.18 and the backdrop to brightness 0.45, so episode captions stay readable over bright artwork.
- Episode stills are 420 × 236, r12, with a check badge when watched and the Ember line on the current one.
- **An episode without its own still is a numbered title card**: the number in Ysabeau 76 at the top, the title in Ysabeau 30 at the bottom, on Charcoal. The caption under it then shows only the duration, because the card already names the episode. The series image is never repeated on every card (screenshot 33).
- Back from the rail returns to the primary action; the next Back leaves the detail.

### Applications

Groups as rows of object tiles 300 × 168 (r18, chip 64, name 30/600) with the identity light inside each tile. The alternative is the shelf language of Home: each group a line of names (D-9; screenshots 07 and 15). "Add application" is an action pill with a plus, not a dashed card. The hint line at the bottom stays.

### Player OSD

Ysabeau title top left. Timeline: 4 px track at 20 % white, Ember fill, knob only in focus. **One play/pause control**, previous and next as round icon buttons, then the subtitle and audio chips. The current capability-dump row (Play, Pause, Play/Pause, Seek, …) is an implementation deviation to fix separately. While media plays, navigating the OSD is silent (see *Sound*).

### Ambient

The signature screen. Black (`#07080A`), a very low hearth light at the lower left that breathes over 14 s (opacity 0.85 ↔ 1, under the threshold of noticing). Ysabeau 220 clock at the lower left, the date in Onest 30, Ember dot + "<application> is paused". The block drifts by up to 24 px every minute against burn-in. At night everything sits at 55 %.

### Startup: ignition

The brand guide reserves Ember for the startup animation. The sequence takes 1.9 s and any key skips it:

| Time | Event |
| --- | --- |
| 0 ms | Charcoal |
| 150 ms | The Ember dot appears where it sits in the mark (scale 0 → 1, 300 ms, slight overshoot); *ignite* sound |
| 450 ms | Light rises from the dot to the floor (1000 ms) |
| 600 ms | The A strokes and "Arinn" fade in (400 ms) |
| 1300 ms | The lockup moves to its place at the top left (500 ms) while Home rises in (24 px, 60 ms stagger per row) |

With reduced motion it is a 300 ms cross-fade.

### Kindle: the short ignition

The startup becomes a motif. A 700 ms version marks the moments when Arinn comes back: the Home key, waking from Ambient and returning to a running application. The Ember dot of the lockup flares (scale 0.2 → 1.8 → 1, a soft Ember halo that fades out) and the floor light swells once (brightness 1.8 → 1 over 900 ms), with the *kindle* sound. It never delays the action: Home is already on screen when it plays. With reduced motion there is no animation, only the sound.

## Motion signature: kindle

Focus does not just scale. The rim and the cast light rise from the bottom edge (220 ms, the existing ease-out `cubic-bezier(.2,.8,.2,1)`), the object lifts from its bottom edge, and the pool follows along the floor 900 ms later. Leaving focus reverses it in 200 ms. Durations are the existing ones; only the origin and the order are new. Screen changes stay cross-fades with a short rise, never lateral slides.

## Sound

Arinn has no interface sounds today. A small palette gives the shell a physical feel and is one of the cheapest ways to make it recognisable.

**Character:** wood and warmth. Soft, low, short; no beeps, no chimes, no sci-fi sweeps, and no crackling fire (it would be a literal flame).

| Sound | Event | Design (synthesised) | Length |
| --- | --- | --- | --- |
| tap | Focus moves | Sine 560 Hz falling to 420 Hz + a 2.2 kHz band-passed noise tick; ±3 % random pitch so long runs do not sound mechanical | 60 ms |
| edge | Focus cannot move further | Duller tap: sine 190 → 150 Hz + low-passed noise | 90 ms |
| select | OK | Warm pluck: G4 + D5, triangle and sine, the second partial 18 ms later | 220 ms |
| back | Back | The same pluck reversed and quieter | 160 ms |
| open | A panel, sheet or navigation opens | Air: noise through a band-pass sweeping 500 → 1600 Hz | 180 ms |
| close | It closes | The sweep reversed | 180 ms |
| ignite | Startup | A slow warm A major swell (A2 E3 A3 C♯4, slight detune) with a soft spark at the start | 1.8 s |
| kindle | Home key, waking from Ambient, returning to a running application | A short fifth (A3 + E4) with the same spark | 0.5 s |
| error | An action failed | Two low notes, E4 → C4 | 320 ms |

Rules:

- **Never over media.** No interface sound plays while video or music plays, including OSD navigation. Error sounds are the only exception and stay quiet.
- Levels: *Off*, *Quiet* (default), *Normal* in Settings › Sound › Interface sounds (D-4). The level follows the system output and its mute.
- Held keys: repeated taps at least 40 ms apart and at half gain.
- Interface sounds never use the remote's microphone and never delay an action: the action happens first, the sound accompanies it.
- In the shell the sounds are pre-rendered 48 kHz WAV files played with `QSoundEffect`, rendered from the same synthesis as the prototype. They are original works under the repository's documentation or asset licence; no third-party samples.

## Copy

Unchanged; it is already right. The only change is the case of the eyebrows.

## What is not changing

- The information architecture, screens, navigation rules, input modes and spatial navigation.
- Safe area, grid, spacing, D-pad behaviour and focus timings.
- The A1 mark and the brand palette.
- The rule that Ember is never a focus colour.

## Owner decisions

The agent does not decide these ([ADR-0037](../../../adr/0037-agent-autonomy-and-owner-gated-decisions.md)). The owner answered all ten on 2026-11-02 ([#721](https://github.com/LimeBeck/arinn/issues/721)); the last column is the decision.

| ID | Decision | Options | Recommendation | Owner decision |
| --- | --- | --- | --- | --- |
| D-1 | Visual direction | A Hearth light · B Hearth light, sans · C Calm · keep current | **A**. C if a smaller step is preferred first | **A** |
| D-2 | Face for content titles | Literata · Source Serif 4 · none; after the font lab twelve candidates | **Literata**; after the font lab **Ysabeau** | **Ysabeau** (first Literata; revised the same day after the font lab: Literata looked bookish, "like my coursework") |
| D-3 | Favourites | Text shelf · tiles | **Shelf** | **Shelf** |
| D-4 | Interface sounds | On, Quiet by default · off by default | **On, Quiet** | **On, Quiet** |
| D-5 | Time-of-day light | Automatic by clock, with a setting to turn it off · off | **Automatic** | **Automatic** |
| D-6 | Brand guide | Amend §5 and §15 as below as Brand Guide v0.2 · keep v0.1 | **Amend** | **Brand Guide v0.2** |
| D-7 | Rim light instead of the white ring | Accept after a TV check · keep the ring | **Accept after the check** on the owner's TV at 3 m, with dark and bright artwork | **Rim light** (the TV check stays a verification step) |
| D-8 | Caption of the focused Continue card | Gives way to the hero · always shown | **Gives way**; keep shown if the empty space under the focused card feels like something is missing | **Always shown**: the empty space looks odd |
| D-9 | Applications | Tiles with identity light · shelf · plain tiles | **Tiles with identity light** | **Tiles with identity light** |
| D-10 | Letter monograms on the favourites shelf | Provider icons only · letters too | **Icons only** | **Icons only** |

### Proposed brand guide text (D-6)

Brand Guide v0.1 is a historical source and is not edited. An accepted D-6 adds a v0.2 next to it, replacing section 5 with:

> The interface is a dark, quiet room with one warm light low in it. Content screens are lit from below: a warm floor light, and a pool of light under the item in focus. Containers stay dark until focus reaches them; the focused row is lit, the rest of the room recedes. Content titles are set in a face with a hand (Ysabeau), system text in a plain sans-serif (Onest). Colour comes from content and from the light, never from decoration. Large, readable elements, horizontal rows and an obvious focus remain.

and the last sentence of section 15 with:

> Never redesign Arinn into a generic cinematic streaming service or gaming launcher, and do not fall back on the generic TV template: uniform rounded plates, uppercase eyebrows, pastel monograms or a colour glow behind content.

## Second round

After the first prototype the designer reviewed it at sofa scale (screenshots reduced to the size they appear from 3 m) and found that the direction's namesake did not read: A differed from C mostly by the font and the missing plates. The second round changed the prototype and this brief:

| Finding | Change |
| --- | --- |
| The floor light and the pool vanished at a distance | A dark ceiling, a brighter floor and a tighter pool; the focused row now visibly stands on a lit floor |
| All placeholder artwork was dark, unlike the owner's real library | Bright, busy placeholder artwork (`art=bright`, mixed by default) |
| A light rim disappears on light artwork | The 5 px Charcoal mat between artwork and rim |
| The focused title appeared twice, in two families | Tried: the focused card's caption gives way to the hero. Rejected by the owner (D-8): the caption stays |
| Applications was the most generic screen | Identity light inside each tile, and the shelf as an alternative (D-9) |
| Secondary actions read as text from the sofa | Actions keep a faint plate |
| A row of letters on the shelf | Letter chips left out on the shelf (D-10) |
| Long titles pushed the hero into the rows | Fixed hero height with fitted title steps |
| Night was indistinguishable from evening | The warm veil |
| A scrolled card was cut hard at the screen edge | Rows fade out at the left edge |
| Series rail: entry on the wrong tab and episode; captions unreadable over bright backdrops; repeated series image | Entry on the selected tab and the current episode; the backdrop steps back; numbered title cards |
| The owner liked the startup | Kindle, the short ignition, as a motif |

### Font lab

After accepting the direction the owner found Literata on Home odd, "like my coursework": a book serif set large reads like a printed document. The prototype gained a switch for the content-title face (review panel, `font=` parameter) with twelve OFL-1.1 faces with Cyrillic: Literata, Playfair Display, Prata, Cormorant Garamond, Bona Nova, Brygada 1918, Piazzolla, Ysabeau, Tenor Sans, Jost, Geologica and Onest. They were compared on the Home hero, a long title, the detail screen and the Ambient clock. The book serifs (Literata, Brygada 1918, Cormorant Garamond) share the problem; Playfair Display and Prata read as cinema posters but are common in glossy print; Jost and Geologica lose most of the difference from Onest; Tenor Sans is too thin from the sofa. **Ysabeau** keeps the warmth and the hand without the document look, and pairs with Onest as two sans-serifs of different temperament. The owner chose it (D-2).

## After acceptance

1. Done in [#721](https://github.com/LimeBeck/arinn/issues/721): the owner's decisions above, [ADR-0136](../../../adr/0136-hearth-light-visual-direction.md), [Brand Guide v0.2](../../brand/brand-guide-v0.2.md) and the specification (§5, §5.6, §15).
2. This brief and the prototype are the visual contract for the areas they cover; the historical handoff applies to everything else.
3. Implementation tasks, each with its own issue and the existing UI comparison (`scripts/ui-compare.py`):
    - tokens and typography (`Theme.qml`, `UiText.qml`, Ysabeau as a resource, `REUSE.toml` entry, owner-gated);
    - the light system (`AmbientGlow.qml` → floor + pool + cast);
    - shape roles, captions below the image, the Ember line;
    - focus: rim, lift from the bottom edge, row light;
    - Home shelf, navigation edge panel, Ambient, ignition;
    - interface sounds: the WAV render script, `QSoundEffect`, the setting and the media guard.
4. The music screens ([#677](https://github.com/LimeBeck/arinn/issues/677), [#710](https://github.com/LimeBeck/arinn/pull/710)) adopt the same roles.

The deviations from the current handoff found in the review (navigation plates at rest, the duplicated Applications subtitle, the capability-dump transport buttons, the duplicated Continue eyebrow, page title weight) do not depend on these decisions and are fixed in separate tasks.
