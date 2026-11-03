<!-- SPDX-FileCopyrightText: 2026 Arinn contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Arinn — Brand Guide v0.2

Accepted by the owner on 2026-11-02 ([ADR-0136](../../adr/0136-hearth-light-visual-direction.md), [#721](https://github.com/LimeBeck/arinn/issues/721)). It replaces [Brand Guide v0.1](brand-guide-v0.1.txt) as the brand reference. v0.1 is a historical source, kept unchanged and in Russian. This version is its English text with these changes:

- **§5 Interface** and the last sentence of **§15** are new: the *Hearth light* direction replaces the description of the generic TV template.
- **§3 Ember** adds the ignition and the short ignition (*kindle*).
- **§9** adds the generic TV template to what to avoid.
- **§10 Typography** adds a face for content titles.
- **§16 Sound** is new.

The interface copy is Russian. The examples in §6 and §11 are given in English here, as the source strings of the code.

## 1. Brand

- Name: Arinn, pronounced "Arin".
- Origin: Old Norse *arinn*, the hearth, the place of fire in a house.
- Metaphor: Arinn is the digital hearth of the home, the one place where films, the browser, games, applications and other content gather.

The Nordic origin must not become a visual theme. No runes, Vikings, ornaments or fantasy aesthetics.

Character: calm, domestic, modern, technological, minimal.

## 2. Logo

**Primary mark: Hearth A, A1 Classic.** A stylised letter A that suggests at once:

- the A of Arinn;
- a roof, a sheltered space;
- a hearth;
- a portal into the home entertainment environment.

A small orange dot, the Ember, sits in the middle of the lower part. It stands for a living fire, the active core of Arinn.

Main composition: `[mark] Arinn`. In the interface the logo is small and unobtrusive. It is never a large decorative object on the main screen.

## 3. Ember

The orange dot is not only part of the logo but Arinn's visual motif. It may mark:

- an active state;
- the application running now;
- progress indicators;
- small status indicators;
- loading and the startup animation.

**Ember is used rarely.** The interface must not become orange; the small amount of colour is what makes the accent visible. Ember is never a focus colour: focus is light.

**The ignition.** At startup the Ember dot appears where it sits in the mark, light rises from it to the floor, and the mark and the name follow. **Kindle**, a 700 ms version, marks Arinn coming back: the Home key, waking from Ambient, returning to a running application. The dot flares once and the floor light swells. These are the only animations that belong to the brand itself.

## 4. Colour system

| Role | Colour |
| --- | --- |
| Charcoal | `#0F1115` |
| Stone White | `#EDEDED` |
| Ember Orange | `#FF8A3D` |

This is the base palette, not the only colours allowed in the interface. Content brings its own colours, for example the artwork of a film or a game. The main interface stays neutral; its warmth comes from light, not from paint.

## 5. Interface

The interface is a dark, quiet room with one warm light low in it. Content screens are lit from below: a warm floor light, and a pool of light under the item in focus. Containers stay dark until focus reaches them; the focused row is lit, the rest of the room recedes. Content titles are set in a face with a hand, system text in a plain sans-serif. Colour comes from content and from the light, never from decoration. Large, readable elements, horizontal rows and an obvious focus remain.

The room gets darker in the evening and at night, as a room with a hearth does. Arinn should feel first of all like a living-room interface, not a desktop launcher.

The visual contract is the [Hearth light brief](../briefs/hearth-light/README.md) and its [prototype](../prototypes/hearth-light/README.md), on top of the [design handoff](../handoff/README.md).

## 6. The logo in the interface

The preferred form on the main screen is `[A] Arinn` in the top left corner, without a plate. The mark never competes with the content: the main visual object of a screen is the user's context, not the brand.

## 7. Mark variants

- **Primary: A1 Classic.** Used almost everywhere.
- **Secondary: A2 Rounded.** Allowed for softer visual treatments, never as a separate competing logo.
- **Small sizes: A4 Minimal.** Only where A1 technically loses legibility: favicon, tray icon, a very small status icon. Otherwise A1 is preferred.

## 8. Monochrome

The logo must work without Ember Orange: a white mark on dark backgrounds, a dark mark on light ones. The shape must stay recognisable without the central dot.

## 9. What to avoid

- a literal drawing of a flame;
- a realistic fireplace;
- a house with a roof and a chimney;
- Nordic or Viking styling;
- runes;
- a red-yellow fire palette;
- a strong glow;
- complex gradients inside the main logo;
- a gaming or RGB aesthetic;
- an overly futuristic sci-fi interface;
- an oversized logo on the home screen;
- **the generic TV template**: uniform rounded plates under every element, uppercase letter-spaced eyebrows, letter monograms on pastel colours, a colour glow behind content.

The image of the hearth is a metaphor, not an illustration.

## 10. Typography

A clean, modern, highly legible family for system text, and a warmer face with a hand for content titles.

- **System text: Onest.** High legibility on a TV, good Cyrillic, an expressive semibold for headings and a calm regular for secondary text, without a techno or futuristic character.
- **Content titles: Playfair Display** (OFL-1.1, Latin and Cyrillic), weight 600, lining figures: the hero, the detail title, the player title, episode title cards, the title card and plate without artwork, and the Ambient clock. A film's name in this display serif, next to plain system text, is the typographic trait of Arinn. Literata is not used for content titles. Owner decision on 2026-11-03 ([#751](https://github.com/LimeBeck/arinn/issues/751), [ADR-0137](../../adr/0137-playfair-display-for-content-titles.md)).
- No uppercase letter-spaced eyebrows. Hierarchy comes from size and family, not from bold against extra bold.

The wordmark Arinn uses the same visual language; a small customisation is allowed later.

## 11. Tone of voice

Arinn must not sound like an infrastructure product. "Multimedia orchestration platform with capability-based plugin architecture" suits developer documentation, not the interface.

The user's language is simple: *Continue*, *Open*, *Return*, *Running now*, *Games*, *Films*, *Applications*. Arinn should feel as if it simply helps to use the TV, although a fairly powerful system sits inside.

## 12. Architecture and brand

The plugin-first architecture stays an internal strength of the product and does not dictate the visual brand. The main logo shows home, hearth, Arinn, and not plugins, nodes, a graph or four modules around a core. Such graphics may appear in developer documentation and the Plugin SDK.

## 13. Component naming

Recommended namespace: `arinnd`, `arinn-shell`, `arinn-sdk`, `arinn-plugin-<name>`. The interface shows *Arinn*, never technical component names.

## 14. Short description

For the README and the website:

> Arinn is a living-room interface for your PC. Browser, games, movies and apps — together in one calm, TV-first experience.

A more emotional variant is "Arinn — a digital hearth for your home". *Digital hearth* is better used as a slogan or a metaphor than as the main explanation of the product.

## 15. Short prompt for a designer or an AI

> Design for Arinn, a modern living-room/HTPC interface. Arinn means "hearth" in Old Norse, but the visual identity must remain contemporary and minimal rather than Nordic or fantasy-inspired. The primary logo is a geometric letter A resembling a subtle hearth or shelter, with a small Ember Orange dot at its base. The interface is a dark, quiet room lit from below by a warm, low light; the item in focus stands in that light. Use warm white typography, a warm humanist face with calligraphic forms for content titles and a plain sans-serif for system text, and orange only as a restrained accent, never for focus. The product should feel calm, premium, domestic and effortless. Never redesign Arinn into a generic cinematic streaming service or gaming launcher, and do not fall back on the generic TV template: uniform rounded plates, uppercase eyebrows, pastel monograms or a colour glow behind content.

## 16. Sound

Arinn has a small palette of interface sounds. Their character is wood and warmth: soft, low and short. No beeps, chimes, sci-fi sweeps or crackling fire. The startup has its own warm swell, and kindle a short one. Interface sounds never play over video or music, are quiet by default and follow the system output and its mute. They are original works; no third-party samples.
