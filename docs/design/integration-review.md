# Reconciling the specification and the design handoff

Sources: [developer specification v0.3](../specifications/developer-specification-v0.3.md) (historical), the current [specification](../specifications/arinn-specification.md) and the [design handoff](handoff/README.md). No source was edited except the logo removal described in the [design README](README.md#licensing). The original conflicts are kept below as history; decisions come from explicit answers of the project owner and delegated technical choices on 2026-09-27.

## Handoff update of 2026-09-30 (settings editors)

The package answers the [#172](https://github.com/LimeBeck/arinn/issues/172) brief:
- it adds `design/arinn Settings Editors.dc.html`, the *Settings editors* section of the README and 22 `se-*.png` screenshots;
- every other listed design file is unchanged;
- the previous package is in the [archive](archive/2026-09-30-before-settings-editors/README.md).

| ID | Point | Resolution / remaining check |
| --- | --- | --- |
| DR-24 | The handoff leaves four questions to the owner: a reason for `enabled = false`, a unit for integers, a *Restart now* action, and how folder availability is reported. | Closed by the owner on 2026-09-30 ([specification §5.6](../specifications/arinn-specification.md#56-settings)): a generic text without a reason; an optional unit in the integer constraints; a toast only, with no restart action; the library provider reports folder availability itself. |
| DR-25 | An integer unit is not in the settings contract: `Constraints` rejects unknown keys (ADR-0065). | Open: a `contract-change` task adds an optional `unit`. |
| DR-26 | *Library folders* shows a folder as unavailable (LF-4) and the library still lists its items as unavailable. No contract carries root availability. | Open: a `contract-change` task, in which the library provider publishes root availability. Until then LF-4 is not shown. |
| DR-27 | The folder picker (PK) lists local directories and mount points. The shell talks only to the daemon; listing directories is new daemon behaviour. | Open, technical: the contract task defines a bounded daemon method that lists folders only. It returns no files, no hidden entries and no contents. |
| DR-28 | *Library folders* is a full screen with library wording, and OK on a row opens the library filtered to that folder. Providers supply no UI (R-04). | Technical: it is the generic editor for a `path` setting with `directory` and `multiple`, with labels taken from the setting. The filter by folder needs a library filter that does not exist yet; it stays out of the first shell task. |
| DR-29 | "Applies at next start" stays on the row until the restart (SV-3). | Technical: the shell keeps the mark until the provider's generation changes. It is lost when the shell restarts. |
| DR-30 | The package carried third-party service logos again. | Replaced with the existing monograms before adding. The `se-*` screenshots contain none; see the [design README](README.md#licensing). |

## Handoff update of 2026-09-29 (address and search)

The package answers the [#106](https://github.com/LimeBeck/arinn/issues/106) brief ([issue #107](https://github.com/LimeBeck/arinn/issues/107)). It adds `design/arinn Browser Input.dc.html` and 18 `in-*.png` screenshots. It also updates the prototype and the Open services file: the Open services screen joins the prototype, the navigation panel spans y72–1008, the Open services item has no divider, and the opening overlay reuses the launch overlay. The previous package is in the [archive](archive/2026-09-29-before-address-search/README.md).

| ID | Point | Resolution / remaining check |
| --- | --- | --- |
| DR-18 | The handoff leaves V-1 (ordinary Chrome window), V-2 (recent entries), V-3 (save as shortcut), T-1 (tile name) and the search engine to the owner. | Closed by the owner on 2026-09-29: V-1b, V-2b, V-3b, T-1b, and a selectable engine with default Google ([ADR-0051](../adr/0051-address-and-search-entry.md)). |
| DR-19 | Opening a typed address or search needs a shell-facing method; ADR-0048 allows only web shortcuts. | Closed by `Browse1` ([ADR-0066](../adr/0066-address-and-search-contract.md)), served by the daemon since [#177](https://github.com/LimeBeck/arinn/issues/177). |
| DR-20 | Recent entries contradict "web shortcuts are the only stored URLs" (ADR-0043, §11). | Closed by ADR-0051 and [specification §11](../specifications/arinn-specification.md#11-state-ownership-and-persistence). |
| DR-21 | The Open services navigation item lost its divider; §5 said "after a divider". | Specification §5 updated. |
| DR-22 | The input rules and the search engine name must not become QML business logic (R-01, R-05). | Technical: the rules live in the daemon or the shell's C++ layer; engine names come from configuration. |
| DR-23 | The delivered main screenshots predate the navigation panel change. | `05-global-nav.jpg` is stale for the panel height; the prototype and README are authoritative. |

## Handoff update of 2026-09-29 (Open services)

The package answers the [ADR-0050](../adr/0050-window-per-browser-activity.md) design brief ([issue #104](https://github.com/LimeBeck/arinn/issues/104)). It adds `design/arinn Open Services.dc.html`, the *Open services* section of the README and 13 `os-*.png` screenshots; every other design file is unchanged. The previous package is in the [archive](archive/2026-09-29-before-open-services/README.md). The designer answered the six brief questions: the screen is called "Open services" (Russian UI label in the handoff); the list is a separate screen reached from navigation and from Continue; a row shows the web-shortcut icon when the address matches a shortcut, otherwise the domain's first letter; closing needs no confirmation, even with sound; a web-shortcut tile shows an Ember dot when its service is open; only Arinn animates, because the browser window cannot be animated ([#80](https://github.com/LimeBeck/arinn/issues/80)).

| ID | Point | Resolution / remaining check |
| --- | --- | --- |
| DR-11 | Continue shows an "N more open" summary entry (H-1). §5.1 allowed only the Activity and content entries. | Closed by the owner on 2026-09-29: accepted in [specification §5.1](../specifications/arinn-specification.md#51-home). |
| DR-12 | Global navigation gets an "Open services" item below the top-level sections (H-2). | Closed by the owner on 2026-09-29: accepted in [specification §5](../specifications/arinn-specification.md#5-information-architecture). |
| DR-13 | The list is ordered by last use. `Browser1` carries only `focused` and `active`, no time of use. | Technical, no contract change: the order is kept from observed focus changes and is lost when the daemon or shell restarts. Decided in the implementation task. |
| DR-14 | The "already open" dot and the shortcut icon on a row need the open windows' addresses matched to web shortcuts. | Technical, no contract change: match by origin as in ADR-0050, in the daemon or the shell's C++ models, never in QML (R-01, R-05). |
| DR-15 | The *Retry* button on *Browser unavailable* (OS-7) "reconnects the plugin". Restarting the provider does not reconnect a closed Chrome. | Open for the implementation task: with no browser connected, retry launches the browser through the ApplicationProvider (ADR-0038); with a failed provider, it restarts the provider. |
| DR-16 | The opening transition fades to "a dark tint of the service hue". No service hue is in the data. | Open for the implementation task: take it from a packaged shortcut icon when there is one, otherwise use the neutral tint. |
| DR-17 | The package carried third-party service logos again. | Replaced with the existing monograms before adding; see [design README](README.md#licensing). |

## Handoff update of 2026-09-27

The current package comes from `design_handoff_arinn (1)/design_handoff_arinn`; the previous one is in the [archive](archive/2026-09-27-before-brand/README.md). The README, three HTML references (prototype, screens, design system) and all 22 screenshots were updated; the A1 Classic SVG and the logo sheet were added in `assets/`.

Main changes: A1 Classic instead of the amber dot mark; Ember instead of amber; a new error token. Mark size: 36 on Home, 38 in navigation, 120 at startup. Light-based focus stays; Ember is not a focus colour. Background and text tokens are unchanged.

The new handoff README still describes Back on Home as opening navigation and Sleep as the screensaver. The accepted decisions for DR-01 and DR-06 below still apply; updated visuals do not cancel them.

## Current status

| ID | Resolution / remaining check |
| --- | --- |
| DR-01 | Closed by the owner (P-01): Back on Home → Activity; without an Activity → navigation. [Specification §6.3](../specifications/arinn-specification.md). |
| DR-02 | Closed by the owner (P-03): Transmission in the MVP; [DownloadProvider API review](../plugin-api/mvp-extensions.md). |
| DR-03 | Folder library + external mpv chosen; tracks/timeline API defined. [Media MVP](../architecture/media-mvp.md). The OSD over external video needs a runtime gate and has an open input question ([OQ-01](../specifications/arinn-specification.md#21-open-questions)). |
| DR-04 | Closed: the library provider stores resume, the media provider playback, the shell interaction state. [Media MVP](../architecture/media-mvp.md). |
| DR-05 | `WindowThumbnail` and near/far verified; the last frame can be read by a separate Qt renderer. Capture lifecycle and performance remain implementation work. [Experiment](../architecture/background-experiment.md), [ADR-0027](../adr/0027-activity-background-snapshot.md). |
| DR-06 | Closed by the owner (P-04): Sleep = OS suspend; ambient is separate. [Specification §5.7](../specifications/arinn-specification.md). |
| DR-07 | Closed: browser-first order stays; the 1+4 UI limit does not limit the registry; New is a generic library projection (API gap [OQ-05](../specifications/arinn-specification.md#21-open-questions)). [Interaction](../architecture/activity-and-interaction.md). |
| DR-08 | Closed: an animation duration never means completion; pending/error comes from the operation. [Interaction](../architecture/activity-and-interaction.md). |
| DR-09 | Closed at the architecture level: typed settings schema, platform actions, live registry. The wire schema comes before the settings implementation. [Interaction](../architecture/activity-and-interaction.md). |
| DR-10 | Player keys defined; the G60S Pro emits standard keys. Esc/Backspace → Back in the shell, except Backspace while editing text; no dedicated integration. [ADR-0030](../adr/0030-phase0-machine-and-standard-input.md). |

Design tokens stay as specified. No product question blocks development; unverified platform capabilities are not presented as confirmed.

## Where the sources agree

TV/D-pad-first, air mouse, one shared household space, Home/Library/Downloads/Applications/Settings, one Continue, dedicated online/Steam/ES-DE entry points and returning to the Activity appear in both. Direction 2a "Depth with Light" refines Quiet Spatial OS. In the handoff "Now running" is a label of the Activity inside Continue and the hero, not a separate competing section.

The handoff allows Qt/QML and asks to follow the established stack. Shipping the HTML prototype or changing the stack is not required. Source files stay reference assets regardless of their JavaScript demo model.

## Original conflicts (history)

| ID | Developer specification v0.3 | Design handoff | What had to be decided |
| --- | --- | --- | --- |
| DR-01 · Back/Home | §6, §48, AC-06: closing Home returns to the Activity; Back ends with Home → active Activity. | Input model / global navigation: Back on the Home root opens navigation; Back in navigation closes it. Return to the Activity via the Return card, navigation or a favorite. | Back on Home with and without an Activity, without a Home ↔ nav loop or lost resume. |
| DR-02 · Downloads | §13: only a reserved route until a separate API review; no DownloadProvider in v1. | Downloads / State Management: real states, pause/resume/retry, queue, progress badge, Transmission adapter, automatic Library/New. | When to implement it, and a download capability that works with the library. |
| DR-03 · Player/library | §12, §22, §23: MPRIS transport controls and a conceptual LibraryProvider; no own media engine. | Library / Detail / Player: scanner, metadata, missing files, audio/subtitle tracks, chapters, buffered range, seek thumbnails; settings for autoscan, autonext, skip intro. | External providers/player and the missing operations and metadata. |
| DR-04 · Ownership/resume | §38–39: the provider is the source of truth; the core does not copy media databases. | State Management: `prog`, `lib`, `newIds`, `dls`, `play`, Continue order; a film is finished 90 s before the end; episodes advance. | Split demo state into interaction state and provider data; the owner of watched/resume and completion policy. |
| DR-05 · Activity texture | §34–36: platform boundary; Home/overlay/focus gate on Wayland. | Activity / depth: live compositor texture or last-frame snapshot; near/far scale, blur, dimming and app hue; animated return. | Access to a texture or snapshot and the fallback without capture. |
| DR-06 · Sleep/ambient | §7, §34: system actions through platform/provider; Sleep undefined. | Power: "Sleep" goes straight to the screensaver; ambient also starts on idle timeout (10 min). | Whether Sleep is suspend, display off or ambient; idle inhibition rules and wake. |
| DR-07 · Scope and Continue | §1: browser is the main scenario (~70%); §5, §37: several resumable activities, no limit. | Overview: three equal modes; Continue is at most 1 Activity + 4 media; New described through local media. | Whether "equal" changes development priority; several activities; New for other LibraryProviders. |
| DR-08 · Async launch | §27, §54–55: asynchronous completion, timeout/error, the UI never waits for the backend. | Motion: launch 1100 ms, startup 1.9 s; the prototype switches demo states. | Separate animation duration from launch result. |
| DR-09 · Settings/actions | §24, §34: generic actions and platform boundary, no full settings schema. | Settings: network, sound, apps, library folders, playback, system; the power confirmation lists apps to close. | Typed settings/actions, capability availability, source of running apps. |
| DR-10 · Keys/player | §2, §36: real remote/Home/media buttons and a global shortcut. | Keys use H/M/P/A/O and ± as prototype bindings; the player describes ←/→ as seek + OSD and arrows as only showing the OSD. | Player key priority, real remote mapping, global/local scope, long press. |

## Visual references

The handoff declares colours, typography, spacing, radii, focus and motion timings final. They remain visual requirements; the DR items concern behaviour, platform capabilities and API. Exact values stay in the handoff and Design System v2 to avoid diverging copies of the tokens.

Screen numbers inside the handoff's Screens / Views section sometimes differ from screenshot file names. Use the actual file name and image, for example `04-home-over-activity.jpg`, `10-player-osd.jpg`, `16-power.jpg`, `22-ambient.jpg`. The original numbering is not corrected.

The whole bundle is kept with the hidden image-slot state (logos replaced by neutral placeholders, see the [design overview](README.md#licensing)), `support.js` and `image-slot.js`. The prototype runtime is not a production dependency. Artwork stays placeholder, and the handoff's advice on production fonts and logos stands.
