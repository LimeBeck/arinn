# Arinn Specification

| | |
| --- | --- |
| **Version** | 0.6 |
| **Status** | Current — the single source of current requirements ([ADR-0028](../adr/0028-documentation-governance.md)) |
| **Date** | 2026-09-28 |
| **Supersedes** | [Developer Specification v0.3](developer-specification-v0.3.md) (Russian, kept unchanged as history) |
| **Target platform** | Linux HTPC, KDE Plasma 6 on Wayland |
| **Primary UX** | TV-first, D-pad-first, air mouse supported |
| **Architecture principle** | Plugin-first, capability-based API |

---

## 1. Document control

### 1.1. How this document is used

This specification states **what** Arinn must do and the architectural constraints it must respect. The reasons behind decisions are recorded in [Architecture Decision Records](../adr/README.md); detailed technical contracts live in [`docs/architecture`](../architecture) and [`docs/plugin-api`](../plugin-api); phase status and exit criteria live in the [roadmap](../../ROADMAP.md).

If this document conflicts with any other living document, this document wins and the conflict is a bug to be fixed. Historical sources (v0.3, the design handoff and its archive, the brand guide) are never edited; where they differ from this document, this document applies.

The key words **MUST**, **MUST NOT**, **SHOULD**, **SHOULD NOT** and **MAY** are used as in RFC 2119. Plain descriptive text is informative.

### 1.2. Changing this document

- Every change goes through a pull request.
- A change to an architectural decision (see [ADR-0028](../adr/0028-documentation-governance.md) for the list) needs an ADR in the same pull request.
- Pull requests change the text but not the version, so parallel pull requests do not conflict ([ADR-0036](../adr/0036-parallel-development.md)). Each change is recorded in a changelog fragment (`changes/`).
- The maintainer bumps the version when preparing a release: **minor** (0.4 → 0.5) if requirements were added or changed since the last version, **patch** (0.4 → 0.4.1) for clarifications and errata only, and summarises the changes in [§22](#22-changes-from-v03) or its successor change log. Version 1.0 is declared together with a stable Plugin API v1.

Implementation fixes that restore an existing requirement reference that requirement;
they do not rewrite an already accurate specification. Changes to intended product
behaviour and corrections within product-behaviour sections receive owner review.
Agent task scope, verification and coordination are defined in
[AGENTS.md](../../AGENTS.md) and [CONTRIBUTING.md](../../CONTRIBUTING.md);
[ADR-0068](../adr/0068-agent-instruction-clarifications.md) records the proposed
process clarification. This does not change product requirements or phase status.

### 1.3. Requirement identifiers

| Prefix | Meaning | Defined in |
| --- | --- | --- |
| AC-xx | Acceptance criterion of the MVP | [§19](#19-acceptance-criteria) |
| R-xx | Mandatory architecture rule | [§20](#20-architecture-rules) |
| OQ-xx | Open question awaiting a decision | [§21](#21-open-questions) |
| P-xx | Product decision by the project owner | [§4.4](#44-product-decisions) |
| DR-xx | Design reconciliation item | [integration review](../design/integration-review.md) |
| G-x.y | Phase gate check | [roadmap](../../ROADMAP.md) |

---

## 2. Product overview

**Arinn** is a TV-first shell for a Linux home theatre PC. One interface brings together:

- web services and the browser;
- local and network media;
- Steam;
- EmulationStation / ES-DE;
- other external applications;
- system actions;
- the state of the current activity.

Arinn does not replace the browser, Steam, the media player or emulators. It provides one home interface on top of the existing Linux system and hides the regular desktop UX where it is not needed.

The main usage scenario is a **Balanced HTPC**:

- the browser and online video are the main scenario — about **70% of usage time**, so browser integration is part of the MVP;
- local media is a full scenario;
- Steam and ES-DE are full entry points;
- ordinary Linux applications can be connected without changing Arinn itself.

Architecturally Arinn is **TV UX + Activity orchestrator + capability aggregator + plugin platform** over an existing Linux HTPC. It is not a launcher, a media centre or a desktop shell in the traditional sense.

---

## 3. Product principles

### 3.1. TV-first

The interface MUST be usable from about 3 metres (the design targets 2–4 m on a 55–75″ 4K TV). The primary controls are the D-pad/arrows, Enter/Select, Back, Home and media buttons. An air mouse is supported, especially in the browser, but MUST NOT be required to navigate Arinn. Every primary shell action MUST be available without a mouse.

### 3.2. A shell, not an ecosystem

Arinn MUST NOT build its own browser, Steam client, media player, EmulationStation or desktop environment. It orchestrates existing applications.

### 3.3. External applications are first-class citizens

A running Chrome, Steam, ES-DE or media player is not something the user must "exit back to the launcher" from. Arinn lives **on top of activities**: global Home opens Arinn over the current application, and closing Home returns to the same place.

### 3.4. Plugin-first

The core MUST NOT know what Chrome, Steam, ES-DE, Jellyfin, mpv, Transmission, webOS or a particular streaming service is. The core knows only **capabilities** ([§9.1](#91-capabilities)). Integrations are external plugin processes.

---

## 4. Users and target environment

### 4.1. User model

Arinn is a **shared household device** used by two people. Version 1 has no login screen, no user selection and no per-user favorites, history or settings; one shared space is used. Support for separate profiles MUST NOT be built into the UX in a way that complicates the MVP.

### 4.2. Target environment

- **Operating system:** Arch Linux / EndeavourOS (first official target); the architecture MUST NOT depend on pacman.
- **Desktop:** KDE Plasma 6 on Wayland.
- **Display:** the target HTPC has **one screen** ([ADR-0031](../adr/0031-single-screen-target.md)). Multi-monitor behaviour ([§12.2](#122-overlay-and-output-selection)) is implementation robustness verified on the development machine, not a product requirement.
- **Input:** keyboard-compatible remote and air mouse. The Air Mouse G60S Pro (BT/2.4G, Russian layout) emits standard keyboard and mouse events (P-05, P-07); no dedicated remote integration is needed ([ADR-0030](../adr/0030-phase0-machine-and-standard-input.md)).
- **Development and Phase 0 verification** run on the development machine, which is technically equivalent to the HTPC (P-06). Final acceptance is repeated on the target HTPC at deployment.

### 4.3. Distribution targets

Binary components: `arinnd`, `arinn-shell`, `arinnctl`, `arinn-plugin-*` and `arinn-chrome-host`. The packaging layer installs the Chrome Native Messaging manifest. See [§18](#18-packaging-distribution-and-licensing).

### 4.4. Product decisions

Explicit answers of the project owner on 2026-09-27. They were recorded as ADR-0023, ADR-0024, ADR-0025, ADR-0026, ADR-0030 and ADR-0031 and are reflected throughout this document.

| ID | Decision |
| --- | --- |
| P-01 | Back on Home returns to the active application; without one it opens navigation ([§6.3](#63-home-and-back)). |
| P-02 | The MVP includes a catalogue of local and mounted network folders, metadata and an external player with audio and subtitle selection ([§7.4](#74-local-library-and-mpv)). |
| P-03 | Transmission is part of the MVP ([§7.5](#75-downloads-and-transmission)). |
| P-04 | "Sleep" sends the computer to OS suspend; the screensaver (ambient) is a separate feature ([§5.7](#57-system-ui-power-and-ambient)). |
| P-05 | Acceptance on a KDE/Wayland HTPC with the Air Mouse G60S Pro, which emulates a mouse and keyboard. |
| P-06 | The development machine is technically equivalent to the HTPC and is used for the technical spike; it has two monitors. |
| P-07 | The remote maps to standard keys, including Backspace and Esc. |
| P-08 | The target HTPC has one screen; two monitors belong to the development machine. |

---

## 5. Information architecture

Top-level sections:

```text
Home
Library
Downloads
Applications
Settings
```

Global navigation also lists **Open services** directly below the top-level sections: the screen of open browser activities ([§7.1](#71-browser-chromechromium)). The item stays when nothing is open; its count is then hidden. Owner decision on 2026-09-29 ([#104](https://github.com/LimeBeck/arinn/issues/104)).

Home is both the main screen and the main overlay. Layout, sizes and visuals of every screen are defined by the [design handoff](../design/handoff/README.md) ([§15](#15-design-contract)).

### 5.1. Home

Home answers "What do I want to do right now?", not "Which application do I need to start?". It fits one screen without a long vertical feed.

**Continue.** One merged section that may contain an unfinished film, a series, a local video, the current browser activity, a running game or other resumable content. If an Activity exists right now, it is shown here too. There MUST NOT be competing concepts such as "Now running", "Continue watching" and "Recent apps" as separate sections. The initial projection is one foreground or last running Activity plus up to four resumable content entries; this UI limit MUST NOT limit the registry ([§10](#10-activity-model)). When more browser activities are open than the one shown, one of the four further entries is an **"N more open"** summary that opens the Open services screen; it is part of Continue, not a separate section, and is hidden when only one browser activity is open. Owner decision on 2026-09-29 ([#104](https://github.com/LimeBeck/arinn/issues/104)).

**Favorites / Online.** Quick entry points such as *Site or search* ([§7.1](#71-browser-chromechromium)), Kinopoisk, IVI, YouTube, Steam and EmulationStation. Streaming services may technically open in Chrome, but for the user they MUST exist as separate tiles; the user never goes Browser → Bookmark → Kinopoisk. The user pins and unpins favorites. Online-service tiles are Arinn-owned **web shortcuts** (title, URL, optional icon) that open in the default BrowserProvider. URLs are `https`, or plain `http` for a local-network host such as a media server or NAS ([ADR-0045](../adr/0045-local-http-web-shortcuts.md)); a default set ships as packaging data, and the user adds, edits and removes shortcuts from the TV. In the MVP, text is entered with a hardware keyboard (for example the keyboard of a keyboard-compatible remote); an on-screen keyboard is post-MVP. Without a keyboard, shortcuts are managed through the CLI (§14.3). Owner decision on 2026-09-28 ([#68](https://github.com/LimeBeck/arinn/issues/68)). This resolves OQ-02 by owner decision on 2026-09-28 ([ADR-0043](../adr/0043-web-shortcuts.md)).

**New.** Not part of the MVP: Home has no New section in version 1, and no provider contract defines "new" content. This resolves OQ-05 by owner decision on 2026-09-28. A later version may add New as a generic projection of LibraryProvider data through a separate ADR; Arinn has no recommendation engine.

### 5.2. Global Home overlay

Home MUST be available whatever application is active — Chrome, Steam, a game, ES-DE or a media player:

```text
Chrome  ──Home──▶  Arinn overlay  ──Back──▶  Chrome
```

Opening Home MUST NOT terminate the application, MUST NOT pause it unless the provider requires that, and MUST NOT destroy its state. The shell appears visually over the Activity; the Activity is shown as a receded background ([§12.3](#123-activity-background)).

### 5.3. Library

A catalogue of films and series from LibraryProviders, with tabs, a poster grid, detail screens, missing-file badges, loading skeletons and an empty state ([§7.4](#74-local-library-and-mpv)).

### 5.4. Downloads

A working MVP section backed by DownloadProvider ([§7.5](#75-downloads-and-transmission)): queue, progress, pause/resume/start now/retry, error and service-unavailable states, completion toast, and a "Watch" action once the media is imported into the library.

### 5.5. Applications

Online services, games and other applications as tiles; pin/unpin to Home ([§7.2](#72-applications-steam-and-es-de)).

Tiles are grouped, in this order: **Online** (browsers and web shortcuts), **Games**, **Media** and **Other**; empty groups are hidden. This extends the handoff's Online · Games · Other layout with Media, by owner decision on 2026-09-28 ([#62](https://github.com/LimeBeck/arinn/issues/62)). The provider sets each application's group; the core and the shell never infer it from titles, IDs or product names, and an unknown group is shown as Other.

An application record may include an icon reference ([ADR-0059](../adr/0059-application-icon-references.md)): empty, a theme icon name, or an absolute local file path. It is not image bytes and not a remote address. An empty reference, or one the shell cannot load as a local image, keeps the existing neutral placeholder. The shell does not call the provider to obtain an icon (R-03). Web-shortcut icons and service defaults are a separate field and are unchanged.

### 5.6. Settings

Categories as in the design: Media, Playback, Sound, Network, Applications, Appearance, System. UI preferences belong to the daemon configuration. Provider settings use a typed schema (id, title, description, type, current value, constraints, enabled; types boolean, enum, string, integer, path reference); plugins MUST NOT supply QML widgets or raw RPC. The wire contract is [ADR-0065](../adr/0065-provider-settings-contract.md); values are stored in `config.toml` ([ADR-0064](../adr/0064-provider-settings-in-config.md)). The core settings UI shows only registered capabilities; a missing playback, download or network function never pretends to succeed. Details: [Activity and interaction](../architecture/activity-and-interaction.md).

### 5.7. System UI, power and ambient

Arinn provides one system overlay for Home, Back, Play/Pause, volume, mute, the power menu and Settings. Each action is executed by the platform layer or the matching provider. CEC is not part of the architecture; TV control (for example webOS) is added later through the same plugin mechanism, never in the core.

**Media and volume keys.** While an external application has focus, Arinn does not register media or volume keys globally: Plasma handles them with its own OSD. Arinn handles them, and shows its own volume HUD, only while the shell is visible. This resolves OQ-04 by owner decision on 2026-09-28.

**Power.**

- **Sleep** calls the platform's system suspend and respects inhibitors and permissions (P-04). On refusal an error is shown; the UI never reports a successful sleep based on a timer. After wake, the Home/Activity context is restored and providers reconnect.
- **Restart** and **Shut down** require confirmation, with Cancel as the default focus. The list of applications that will be closed comes from the live registry. Confirmation never hides inhibitor or permission errors.

**Ambient (screensaver).** Started by an explicit action or an idle timeout (10 minutes by default). Any key or pointer movement wakes the UI; the first wake event is consumed and triggers no action. Automatic ambient MUST NOT start while an external Activity is in the foreground **or** media is playing. The idle timer never triggers OS suspend. Ambient never stops applications.

**Session lifecycle.** On lock or before sleep the shell hides regardless of daemon state and keeps the Activity context; after wake it re-checks the lock state before showing anything. If the lock state is unknown, the shell treats the session as locked ([session lifecycle](../architecture/session-lifecycle.md)). Lock/sleep observation MUST remain independent of daemon availability through an asynchronous, fail-closed platform lifecycle guard ([ADR-0041](../adr/0041-independent-session-lifecycle-guard.md)). The guard observes session state; it does not execute power or application commands.

---

## 6. Navigation and input

### 6.1. Focus

Focus is first-class state. Every focusable component MUST have distinguishable states: default, focused, pressed, disabled and loading. From about three metres the user MUST see the current focus unambiguously; focus MUST NOT rely only on a small colour change, mouse hover or a thin border. Focus uses light, scale and a ring over artwork, never the Ember accent colour.

Focus scopes trap focus in the topmost scope: dialog › power › sheet › nav › screen. Each screen remembers its last focus. After an item disappears, focus moves to the nearest previous index; closing a modal or sheet returns focus to its opener, or to the nearest focusable in the parent scope if the opener is gone.

Input modes: **D-pad mode** (cursor hidden, full focus treatment) and **pointer mode** (air mouse; hover moves focus, lift is halved; hover is ignored for 500 ms after any scroll). The first arrow key returns to D-pad mode. Spatial navigation rules are in the design handoff.

### 6.2. Keys

| Input | Inside the shell |
| --- | --- |
| Arrows | Move focus |
| Enter / OK | Select |
| Esc, Backspace | Back ([§6.3](#63-home-and-back)). In an editable text field Backspace deletes a character; Esc closes the topmost scope. |
| Global Home | Configurable global shortcut owned by the platform layer (Meta+F12 in the Phase 0 spike) |
| Menu | Toggle global navigation; long press in Applications pins/unpins |
| Media and volume keys | Media and volume actions; outside the shell Plasma handles them ([§5.7](#57-system-ui-power-and-ambient)) |

Outside the shell, keys belong to the active application. Arinn MUST NOT intercept Esc, Backspace or plain letters globally. The prototype's letter shortcuts (H, M, P, A, O) are development bindings, not product keys. The production mapping uses semantic actions and is configurable per device.

### 6.3. Home and Back

Back MUST behave predictably and MUST NOT close the active application just because the user pressed Back inside Home.

| State | Back | Home |
| --- | --- | --- |
| Modal / power confirmation / sheet / navigation | Close the topmost scope or go one step back; focus returns to the opener. | Show Home, closing transient UI without confirming a destructive action. |
| Nested screen | Go to the parent screen; at the top level of a section, go to Home. | Home. |
| Home with a return Activity | Close Home and activate the same Activity. | Stay on Home, focus on Return; no toggle. |
| Home without a return Activity | Open global navigation. | Stay on Home. |
| External application | Back belongs to the external application. | The platform opens the Arinn overlay and remembers the return Activity. |

If the saved Activity disappears, Home stays open, briefly shows an unavailable state and moves focus by the rules of [§6.1](#61-focus). The next Back behaves as "Home without a return Activity". Opening Home by itself never pauses or stops anything.

### 6.4. Player

- **OSD hidden:** Left/Right seek ±10 s (only if seek is supported) and show the OSD with focus on the timeline; OK/Up/Down only show the OSD.
- **OSD visible:** OK on the timeline toggles play/pause. The OSD hides 4 s after the last input while playing and never while paused.
- **Back** first hides the OSD; the next Back leaves the playback view of a managed mpv session, stops playback, saves resume and returns to the Home or Detail screen where playback started. It never terminates an arbitrary external application.
- **Input during managed playback:** the shell keeps keyboard focus as a transparent fullscreen input layer over the mpv window, including while the OSD is hidden. All keys are handled locally in the shell; only commands (seek, play/pause, stop) go to the provider. mpv never receives keys and never forwards them. This resolves OQ-01 by owner decision on 2026-09-28 ([ADR-0046](../adr/0046-shell-input-layer-during-playback.md)); the media integration gate verifies that the layer does not degrade playback.

### 6.5. Performance principles

UI interaction MUST NOT wait for provider IPC. Focus navigation MUST be fully local to the shell; provider data loads asynchronously. The chain *keypress → D-Bus request → plugin → response → focus change* is forbidden.

Initial engineering budgets on the target device: p95 input-to-focus ≤ 50 ms with a warm UI at 60 Hz; p95 frame time ≤ 16.7 ms outside heavy initial loads; a slow provider does not affect focus ([implementation baseline](../architecture/implementation-baseline.md)).

Visual animations are not results: the 1100 ms launch animation does not mean the launch succeeded; the UI switches when the target Activity actually exists and shows an error immediately on failure. The 1.9 s startup animation never delays a degraded Home.

---

## 7. MVP integrations

### 7.1. Browser (Chrome/Chromium)

Arinn MUST NOT implement its own WebView or browser engine; a regular Chrome/Chromium is used. The MVP browser integration MUST be able to:

- launch Chrome and open a URL;
- list windows and tabs and find the active tab;
- switch to, create and close a tab;
- show title, URL and favicon when available;
- return to an existing browser activity.

Chain ([ADR-0011](../adr/0011-chrome-integration.md), [ADR-0021](../adr/0021-plugin-runtime-ownership.md)):

```text
Chrome ──Extension API──▶ Arinn Chrome extension (Manifest V3)
       ──Native Messaging──▶ arinn-chrome-host (short-lived, started by Chrome)
       ──D-Bus──▶ arinn-plugin-chrome (systemd-supervised provider)
       ──Plugin API──▶ arinnd
```

The extension requests only `tabs` and `nativeMessaging`; no content scripts and no host permissions. Version 1 needs no page scraping, DOM automation, arbitrary JavaScript or content analysis. The plugin works only with windows, tabs, URL, title and active/audible state; version 1 transfers no favicon, because reading one would need a further extension permission ([ADR-0048](../adr/0048-browser-wire-contract.md)). Incognito is not connected by default. Several browser profiles may produce several connections without creating Arinn profiles.

Generic `open` accepts only `http` and `https`. The desktop ApplicationProvider launches the browser application; `arinn-plugin-chrome` declares only `browser`, not `application`. The application and browser entities are linked through a validated platform window token, never through titles or product-specific core logic. This resolves OQ-03 by owner decision on 2026-09-28 ([ADR-0038](../adr/0038-browser-launch-ownership.md)). Details: [plugin runtime](../architecture/plugin-runtime.md).

**TV presentation.** Every browser activity is its own fullscreen browser window without browser UI; Arinn, not the browser's tab strip, is the switcher between them. Opening a URL focuses an existing managed window whose active tab has the same origin, otherwise it creates a new fullscreen window, so a web shortcut never duplicates an open service. A tab that appears in an occupied managed window moves into its own window, and a managed window that leaves fullscreen returns to it. A window the user opens as an ordinary browser window keeps the normal browser UI; it is reached as a secondary action of the address and search screen. The **Open services** screen lists the managed windows as a flat list, the one on screen first, with actions to switch to and to close each one; closing needs no confirmation. A web-shortcut tile whose origin is open is marked as open. If this model does not hold on a real desktop, Arinn falls back to launching the browser as an ordinary application. Owner decision on 2026-09-29 ([ADR-0050](../adr/0050-window-per-browser-activity.md)).

**Address and search.** The Home tile *Site or search* and a button on the Open services screen open a screen where the user types an address or a search query on a hardware keyboard; Enter opens the result as a managed window under the rules above. `http` and `https` are used as typed; an IPv4 address, `localhost`, a `*.local` host or a host with a port gets `http://`; a dotted host with a TLD of two or more letters and no spaces gets `https://`; any other scheme is rejected; everything else is a search with the configured engine (a URL template, selectable in Settings, default Google). While typing, up to three matching web shortcuts are suggested; with an empty field, up to five recent entries are shown with a *Clear* action. An open page can be saved as a web shortcut from the Open services screen. Opening a typed address needs a daemon method defined in a separate contract task; this screen is not part of the MVP acceptance criteria until the [roadmap](../../ROADMAP.md) assigns it. Owner decision on 2026-09-29 ([ADR-0051](../adr/0051-address-and-search-entry.md)).

### 7.2. Applications, Steam and ES-DE

Arinn works with ordinary Linux applications: discover, launch, activate, terminate and running state. `arinn-plugin-desktop` provides ApplicationProvider from `.desktop` entries; the core MUST NOT scan desktop files. Applications are launched by the session application launcher in their own application scope, never in the plugin's cgroup. Terminate requests a graceful close; force kill is not a regular MVP terminate. Each record may carry the optional icon reference in [§5.5](#55-applications) ([ADR-0059](../adr/0059-application-icon-references.md)). Reading a desktop file's `Icon` key is not part of that contract. Implementation and qualification status: [desktop provider](../architecture/desktop-provider.md).

Steam and EmulationStation MUST be available in the MVP as dedicated Home tiles, launched through the generic ApplicationProvider, without an own Steam library client. Game libraries, artwork, recent games, continue playing and metadata are post-MVP providers.

### 7.3. Media sessions (MPRIS)

`arinn-plugin-mpris` discovers current media sessions on Linux. A session reports title, subtitle/artist, artwork, playback state, duration, position and available actions, and supports play, pause, playPause, seek, next, previous and stop. The shell MUST NOT talk to MPRIS directly. Session selection, units and position anchors: [plugin runtime](../architecture/plugin-runtime.md). The wire contract is [ADR-0054](../adr/0054-media-and-library-wire-contract.md): sessions are a paged snapshot, and `MediaOpen1`, `MediaTracks1` and `MediaTimeline1` are optional. An MPRIS provider does not implement tracks or thumbnails.

### 7.4. Local library and mpv

([ADR-0024](../adr/0024-local-library-mpv.md), [media MVP](../architecture/media-mvp.md))

- `arinn-plugin-library` (LibraryProvider) scans configured local folders and already-mounted network folders. No SMB/NFS client, mount management or Jellyfin server. Scanning is asynchronous: initial, explicit rescan, filesystem hints and a periodic check every 10 minutes with autoscan. File-name parsing, the scan schedule, artwork ids and the provider's own configuration file are [ADR-0056](../adr/0056-local-library-scan-metadata-and-resume.md).
- Metadata comes from file names, NFO sidecars, ffprobe and local artwork, with an honest fallback (file name, generic artwork, unknown fields). Online enrichment is optional.
- An unavailable mount marks the root unavailable and never deletes entries or resume. "Remove from library" hides the entry and never deletes the file.
- `arinn-plugin-mpv` (MediaProvider plus `MediaOpen1`, `MediaTracks1`, `MediaTimeline1`) controls an external mpv process over a private JSON IPC socket. The shell draws the OSD and track sheets; mpv draws video.
- The library provider is the only persistent owner of resume and watched state (written every 5 s and on pause, seek, end and stop). A film is finished 90 s before its end when its duration exceeds 180 s; otherwise only on confirmed EOF.
- MPRIS remains the integration for other players and does not have to implement tracks or thumbnails.
- The wire contract is [ADR-0054](../adr/0054-media-and-library-wire-contract.md). `LibraryResume1` and `LibraryImport1` are optional. Launch resolves a local file for the daemon, which opens it through `MediaOpen1`. The shell never sees the file URI.

### 7.5. Downloads and Transmission

([ADR-0025](../adr/0025-downloads-mvp.md), [Transmission MVP](../architecture/transmission-mvp.md), [API review](../plugin-api/mvp-extensions.md))

- A generic DownloadProvider capability describes download jobs, not torrents. `arinn-plugin-transmission` adapts Transmission RPC (default loopback endpoint).
- The MVP manages jobs added with the regular Transmission client: pause, resume, start now, retry. Adding jobs from Arinn is not required.
- Completed media flows through `LibraryImport1` into the library exactly once; completed and imported are separate states. Non-media downloads show "Done" without "Watch".
- Torrent terminology (peers, seeds, ratio, trackers) MUST NOT appear in the default UI. The core never talks to Transmission directly.

---

## 8. Architecture

### 8.1. Components

```text
┌──────────────────────────────┐
│          arinn-shell         │  Qt 6 / QML
└──────────────┬───────────────┘
               │ session D-Bus (org.arinn.Daemon1)
┌──────────────▼───────────────┐      ┌───────────────────────┐
│            arinnd            │◀────▶│ Platform adapter       │
│             Rust             │      │ (KWin script, KDE)     │
│ Registry · Routing · State   │      └───────────────────────┘
│ Operations · Persistence     │
└──────────────┬───────────────┘
               │ session D-Bus (Plugin API v1)
   ┌───────────┼───────────┬───────────┬───────────┐
   ▼           ▼           ▼           ▼           ▼
 demo       desktop      chrome   mpris/mpv   library/transmission …
```

`arinnctl` uses the same public daemon API as the shell.

### 8.2. arinn-shell

Qt 6 Quick/QML with a small C++ layer for QtDBus and typed models. The shell is only the presentation layer: navigation, focus management, animations, rendering, overlays, screens, the design system and interaction state (focus scopes, input mode, OSD visibility, timeline preview).

The shell MUST NOT launch Chrome, read `.desktop` files, talk to MPRIS, control Steam, run shell commands or know the IPC of third-party applications. All application/provider integration data and commands go through `arinnd`. The daemon-independent lifecycle guard in §5.7 is limited to session-state observation and shell hiding; it does not authorize direct application integration. The UI thread never makes blocking D-Bus calls.

### 8.3. arinnd

The minimal core and orchestrator, written in Rust (Tokio, zbus). Responsibilities: plugin and capability registries, plugin lifecycle orchestration, state aggregation, command routing, subscriptions, snapshot/revision management, error normalization, the platform abstraction, configuration and a small amount of own persistent state.

> If functionality can be implemented as a provider without changing the core, it MUST be implemented as a provider.

### 8.4. Plugins

A plugin is a separate OS process. Loading integrations as `.so` or other native libraries into `arinnd` is forbidden: crash isolation, independent dependencies, any implementation language, independent updates, a stable IPC boundary and no ABI coupling with Rust. A plugin crash MUST NOT crash the shell, the daemon or other plugins.

Plugins supply data, metadata, actions and capabilities. They MUST NOT supply QML, HTML or any other UI; Arinn draws all UI (visual consistency, TV navigation, accessibility, security, API stability).

### 8.5. Platform layer

Platform-specific functions are behind `PlatformService`; the first implementation is `KdePlatformService` for KDE Plasma 6 on Wayland: global shortcuts, application/window activation, overlay and window integration, session state and system actions (volume, network, power). KDE-specific code MUST NOT be spread across the daemon, and window management MUST NOT be spread across plugins. Details: [§12](#12-platform-integration-kde-plasma-6--wayland).

The shell lifecycle guard uses platform-layer session observation independently of the daemon. Platform-specific transport stays in that layer; QML only consumes the resulting visibility guard. The existing shell-local transport is scheduled for extraction in [task #29](https://github.com/LimeBeck/arinn/issues/29).

### 8.6. IPC

Session D-Bus on both boundaries ([ADR-0020](../adr/0020-dbus-contract-baseline.md)): shell/CLI ↔ daemon under `org.arinn.Daemon1` at `/org/arinn/Daemon1`, and daemon ↔ plugins, each provider exporting `/org/arinn/Provider1` under its unique bus name. Interfaces carry a major version suffix (`…1`). Clients never call provider endpoints, MPRIS or KWin directly. The Chrome host ↔ Chrome provider link is a separate typed D-Bus interface that never reaches the core. The narrow session-observation exception in §5.7 is defined by [ADR-0041](../adr/0041-independent-session-lifecycle-guard.md).

---

## 9. Plugin API v1

Phase 1 fixes the application/daemon wire bindings in [ADR-0035](../adr/0035-phase1-application-wire-contract.md)
and the [versioned XML](../plugin-api/README.md). Installed and running application
entries share one paged snapshot. Public commands return daemon-owned operations;
private provider replies complete those operations. Media and library wire
bindings are fixed by [ADR-0054](../adr/0054-media-and-library-wire-contract.md);
the daemon routes `media` and `library` ([ADR-0058](../adr/0058-media-routing.md),
[ADR-0060](../adr/0060-library-routing.md)). Action and download still
receive concrete XML before their first provider. Plugin API major 1 remains pre-stable.

Plugin API v1 is not yet stable; wire XML, SDK and the contract suite are produced per capability before its first provider ([plugin API](../plugin-api/README.md)).

### 9.1. Capabilities

A plugin declares capabilities, never a product type. One plugin may implement several.

| Capability | Manifest name | Conceptual operations |
| --- | --- | --- |
| ApplicationProvider | `application` | listApplications, getApplication, launch, activate, terminate, listRunning |
| BrowserProvider | `browser` | getTabs (windows and tabs in one snapshot), open (a new tab), activateTab, closeTab; Window = id, window token, focused; Tab = id, windowId, title, url, active, audible ([ADR-0048](../adr/0048-browser-wire-contract.md)) |
| MediaProvider | `media` | listSessions, getSession, play, pause, playPause, seek, next, previous, stop; extensions `MediaOpen1`, `MediaTracks1`, `MediaTimeline1`, `AssetProvider1` ([ADR-0054](../adr/0054-media-and-library-wire-contract.md), [ADR-0062](../adr/0062-asset-contract.md)) |
| LibraryProvider | `library` | listCollections, listItems, getItem, launch; extensions `LibraryResume1`, `LibraryImport1`, `AssetProvider1` ([ADR-0054](../adr/0054-media-and-library-wire-contract.md), [ADR-0062](../adr/0062-asset-contract.md)) |
| ActionProvider | `action` | generic actions with a machine-readable id, title, icon and enabled flag (for example TV power, TV input, custom system actions) |
| DownloadProvider | `download` | GetSnapshot, GetJob, Pause, Resume, StartNow, Retry ([API review](../plugin-api/mvp-extensions.md)) |

Media and library operation lists are fixed by [ADR-0054](../adr/0054-media-and-library-wire-contract.md). Action and download stay conceptual until their wire XML is fixed. Extension interfaces are optional feature flags; a missing feature is hidden or disabled in the UI, never a no-op.

### 9.2. Identity and routing

- `plugin_id`: stable package ID from the manifest. `provider_id`: stable ID of a configured plugin instance (defaults to `plugin_id`).
- Entities are addressed by `EntityRef = (provider_id, capability, local_id)`. All IDs are opaque; the core MUST NOT parse `local_id`. Ephemeral entities also carry a `generation`; a stale generation gives `Conflict`.
- A command on an entity goes to its provider. A generic command (such as opening a URL) goes to the default provider of the capability; with exactly one provider it is chosen automatically, with several the user selects a default in Settings. There is no hidden product-specific priority.

### 9.3. State: snapshots and revisions

State is transferred as **snapshot + revision**, not as many imperative events. A revision is scoped to `(provider_id, capability)`, is a `uint64` starting at 1 and grows strictly within one `generation`; a generation is a UUID issued by the daemon at each registration. A consumer subscribes before reading, buffers notifications and re-reads when it sees a higher revision. Large lists are paged (≤ 200 entries or 256 KiB per page), bound to one generation/revision. A new generation invalidates the old snapshot and all pending operations.

### 9.4. Events

Subscriptions deliver `StateChanged(provider_id, capability, generation, revision)`, provider availability changes and Activity changes. Events do not carry full state; after `StateChanged` the consumer fetches the snapshot. Events may be coalesced; stale replies are dropped.

### 9.5. Operations

Every operation that may touch an external application is asynchronous. Typed methods take a caller `request_id` (UUID) and return an operation object immediately after validation. Operation states: queued, running, succeeded, failed, cancelled. Default timeouts: IPC admission/read 3 s; controls, activate, seek 5 s; launch, open, createTab 30 s; handshake 10 s. After a timeout the outcome is unknown until reconciled with a snapshot. Cancellation is best effort and never rolls back side effects. The daemon never retries mutating commands; duplicate `(caller, request_id)` returns the same operation. Details: [implementation baseline](../architecture/implementation-baseline.md).

### 9.6. Errors

Errors are normalized by the core in the namespace `org.arinn.Error1.*`: `NotFound`, `Unavailable`, `Unsupported`, `PermissionDenied`, `Timeout`, `InvalidArgument`, `Conflict`, `Internal`, `Cancelled`. Error texts are safe for the UI; plugin stack traces MUST NOT reach the QML API.

### 9.7. Limits and validation

Message body ≤ 256 KiB; ID ≤ 256 bytes; text ≤ 8 KiB; URL ≤ 16 KiB; an application icon reference ≤ 1024 bytes; optional metadata ≤ 32 keys; ≤ 32 concurrent operations per provider. Artwork and application icons are passed as typed references, never inline. Every message is validated for type, length, provider ownership and command permission before use.

### 9.8. Extensibility and versioning

Core fields stay typed. Optional metadata MAY use a bounded `a{sv}` with scalar values and string arrays; the API MUST NOT become an untyped dictionary. Plugin API version is `1`; minor additions never change required fields or signatures. Unknown optional keys are ignored; unknown enum values are shown as unsupported. An incompatible plugin never becomes a working provider and is shown in diagnostics.

### 9.9. Manifest and discovery

```toml
id = "chrome"
name = "Chrome Integration"
version = "0.1.0"
api_version = 1
executable = "/usr/lib/arinn/plugins/arinn-plugin-chrome"
args = []
capabilities = ["browser"]
```

Required fields: `id`, `name`, `version`, `api_version`, `executable` (absolute path), `capabilities`; optional `args`. Neither `executable` nor `args` is a shell expression. Plugin IDs are stable. Discovery reads `$XDG_DATA_HOME/arinn/plugins`, then `arinn/plugins` of each `$XDG_DATA_DIRS` entry; a user manifest overrides a system manifest with the same ID; duplicates at one level are `Conflict`. The registry is re-read at startup and on `arinnctl providers reload`. System and user plugins use the same format.

### 9.10. Lifecycle and registration

`arinnd` orchestrates discover, start, handshake, monitor, restart and stop, delegating process supervision to systemd: each provider is a **transient user service** (`Restart=on-failure`, 2 s delay, at most 5 starts per 60 s, then failed until a manual restart). After start the plugin calls `Register` with a one-time token; the daemon verifies the D-Bus sender, UID, token and cgroup, accepts only manifest capabilities and returns a generation. A provider is available only after registration and its first valid snapshot. Runtime states: discovered, starting, ready, restarting, failed, stopped, incompatible; backend states: available, unavailable, degraded. Details: [plugin runtime](../architecture/plugin-runtime.md).

---

## 10. Activity model

An **Activity** is what the user is doing right now: a Chrome tab, a Steam game, ES-DE, movie playback. An Activity is not necessarily an Application (Chrome is an application; a YouTube tab is an activity). Activities let Home show a meaningful Continue instead of a process list.

- Each provider owns its external entities and describes them with Activity descriptors (`id`, `owner EntityRef`, `generation`, `kind`, `title`, `artwork_ref`, `status`, `available_actions`, optional `parent_ref`, `related_refs`, `platform_window_token`, `last_interaction_at`).
- The daemon aggregates descriptors. Entities are grouped only by explicit ID links or a validated platform window token, never by matching titles or URLs. If the link is unknown, entities stay separate.
- The foreground Activity is the platform window plus the active provider entity in it. Display precedence inside a group: content playback → browser tab → generic application.
- Details: [Activity and interaction](../architecture/activity-and-interaction.md).

---

## 11. State ownership and persistence

Each provider is the source of truth for its own state (browser tabs, playback, installed applications, library and resume, download jobs). The core aggregates state and MUST NOT duplicate it in its own database; Chrome history, a Steam library database or a media database are never copied when a provider can supply them.

Arinn persists only its own state:

| Location | Content |
| --- | --- |
| `$XDG_CONFIG_HOME/arinn/config.toml` | UI preferences, provider configuration and defaults, Home settings; atomic write, schema version and migration |
| `$XDG_STATE_HOME/arinn/state.sqlite3` | favorites and order, web shortcuts ([ADR-0043](../adr/0043-web-shortcuts.md)), provider-instance mapping, recent launch targets (≤ 100, 30 days), diagnostic counters |
| `$XDG_CACHE_HOME/arinn/` | artwork cache (LRU, 512 MiB); clearing it loses nothing else |
| `$XDG_DATA_HOME/arinn/` | plugin manifests; provider-owned data in `providers/<digest>/` directories ([ADR-0034](../adr/0034-provider-data-directory-identity.md)) |

Provider data and state directories MUST be named by the full lowercase hexadecimal
SHA-256 digest of the exact UTF-8 bytes of the validated `provider_id`, without
normalization. They live under `$XDG_DATA_HOME/arinn/providers/<digest>/` and
`$XDG_STATE_HOME/arinn/providers/<digest>/`. Their location MUST remain independent
of `state.sqlite3`, registry UUIDs, plugin versions and registration generations.
Recreating Arinn state therefore preserves the directory location for an unchanged
provider ID; it does not restore lost instance configuration. This resolves OQ-06
by the project owner's confirmation on 2026-09-28 ([ADR-0034](../adr/0034-provider-data-directory-identity.md)).

Favorites survive removal of their provider and are marked unavailable. Browser tab titles, URLs and history are not archived. The only URLs Arinn stores are user-configured web shortcuts and up to five recent address and search entries, which the user can clear ([ADR-0051](../adr/0051-address-and-search-entry.md)); neither may appear in logs or diagnostics. Secrets are never stored in TOML; credential references go to the system secret storage when an integration needs one.

---

## 12. Platform integration (KDE Plasma 6 / Wayland)

Wayland constraints: applications cannot control foreign windows, focus stealing is restricted, global hotkeys and overlay semantics depend on the compositor. Window management therefore stays in the platform layer.

The pre-stable provider-facing window enumeration and control bindings are defined
by [ADR-0039](../adr/0039-platform-window-contract.md) and the
[platform window contract](../architecture/platform-windows.md). Protocol/SDK
bindings and isolated fixtures exist; the production daemon/KDE implementation
and Phase 2 qualification remain pending. The internal trusted-adapter transport
and SDK registration context are specified by [ADR-0040](../adr/0040-kwin-inventory-transport.md)
and the [KWin transport contract](../architecture/kwin-transport.md).

### 12.1. Global Home

The platform layer registers the global Home shortcut, which MUST work while focus is in Chrome, Steam, a game or ES-DE. The shell MUST NOT rely on receiving ordinary key events while unfocused. The first mechanism is a packaged KWin script with a typed bridge to the daemon ([ADR-0032](../adr/0032-kwin-script-platform-adapter.md)); plugins can never submit scripts.

### 12.2. Overlay and output selection

When Home opens, the platform remembers the Activity window and its output; the single shell window is shown fullscreen on that output (the only screen on the target HTPC). Without an Activity the active KWin screen is used. Return activates the same window without moving it. Arinn MUST NOT change resolution, scale, the primary screen or layout ([ADR-0026](../adr/0026-multiple-monitors.md), [ADR-0031](../adr/0031-single-screen-target.md)).

### 12.3. Activity background

The receded Activity behind Home uses the last frame of the Activity window, captured by the trusted KWin component with `WindowThumbnail`, exported as a bounded frame to a private runtime directory and rendered by the shell (near: scale .95, blur 8, brightness .6; far: scale .90, blur 36, brightness .32). The snapshot is bound to generation + Activity + Home request and is invalidated on Return, Activity change, lock/sleep or provider/daemon loss. The UI never waits for the frame before local focus ([ADR-0027](../adr/0027-activity-background-snapshot.md)).

### 12.4. Session lifecycle

See [§5.7](#57-system-ui-power-and-ambient) and [session lifecycle](../architecture/session-lifecycle.md).

---

## 13. Security

The MVP is **not** a hostile-plugin sandbox: plugins are locally installed trusted software. The architecture still enforces:

- process isolation of plugins;
- IPC and command validation with bounded message sizes and timeouts;
- no arbitrary shell execution from QML;
- no arbitrary code injection through the Plugin API;
- plugin registration bound to the systemd unit, UID and one-time token;
- minimal Chrome extension permissions and no page content access;
- private runtime directories (0700) and files (0600) for tokens, sockets and snapshots.

Regular logs MUST NOT contain cookies, authorization headers, passwords, session tokens, browser page content, URLs, tab titles or payloads — only IDs, operation, outcome and a safe error code. Debug logging does not lift this rule. Vulnerability reporting: [SECURITY.md](../../SECURITY.md).

---

## 14. Observability

### 14.1. Logging

All components write structured JSON logs to stderr, collected by journald, with at least `component`, `provider_id`, `request_id`, `operation`, `duration_ms` and `error` (missing values are `null`). Levels: error, warn, info (default), debug.

### 14.2. Diagnostics

Diagnostics show the daemon status and, per provider: runtime and backend state, generation, plugin and API version, registered capabilities, manifest source path, last safe error with code and time (and recovery time), restarts since daemon start and cumulative restarts. Integration errors MUST NOT turn into "the tile just does nothing": an unavailable provider, a degraded daemon connection and a timeout each have an explicit UI state.

### 14.3. CLI

`arinnctl` uses the same public daemon API as the shell and provides at least `status`, `providers`, `capabilities`, `activities`, `diagnostics`, `provider restart <id>` and `providers reload`. Human-readable output by default and `--json` for read commands. Exit codes: 0 success, 1 runtime failure, 2 invalid usage, 3 daemon unavailable. Web shortcuts are managed with `shortcuts [list]`, `shortcuts add <title> <url> [icon]`, `shortcuts edit <id> <title> <url> [icon]`, `shortcuts remove <id>` and `shortcuts open <id>` (the TV fallback without a keyboard, §5.1); `browser tabs`, `browser activate <provider> <tab>` and `browser close <provider> <tab>` work on browser providers. A failed call prints the Arinn error name (for example `Conflict`) on standard error; URLs appear only in the command's own output.

---

## 15. Design contract

The visual direction is **Quiet Spatial OS** — "Calm OS → Cinematic Content": the shell is calm and lets content be the main visual object. It does not imitate Android TV, Apple TV, Steam Big Picture or a desktop launcher. The [design handoff](../design/handoff/README.md) refines this as direction **2a "Depth with Light"** and is the source of all visual parameters ([ADR-0019](../adr/0019-design-handoff-contract.md)):

- logical canvas 1920×1080, uniform scale `min(vw/1920, vh/1080)`, safe area from the handoff;
- Onest and Material Symbols Rounded; text ≥ 24, action-critical text ≥ 26; tabular numerals; room for Russian labels ~30% longer;
- Ember `#FF8A3D` marks status, progress and brand only, never focus;
- light field keyed to artwork hue, crossfaded over 900 ms, can be disabled, absent on system screens;
- all tokens, radii, elevation and motion timings as in the handoff and Design System v2;
- loading, empty, error, disabled, pressed and focus states per the design system and the 22 screenshots.

The handoff's JavaScript state model demonstrates behaviour only; it never becomes a shell backend. Conflicts between the handoff and this document are resolved in the [integration review](../design/integration-review.md); where they differ, this document applies. Brand: [brand](../design/brand/README.md).

---

## 16. Non-goals for version 1

- own browser engine or media engine;
- a full desktop environment;
- a plugin marketplace or remote plugin repository;
- loading arbitrary QML or other plugin UI;
- an automation engine or recommendation engine;
- multi-user accounts, profiles or cloud sync;
- Windows or macOS;
- own Steam library or Jellyfin implementation;
- SMB/NFS clients or mount management;
- adding torrents from Arinn, torrent statistics and tracker settings;
- a multi-monitor product experience beyond [§12.2](#122-overlay-and-output-selection).

---

## 17. Quality and testing

| Level | Scope |
| --- | --- |
| Unit | daemon, state aggregation, parsing, routing, plugin SDK |
| Plugin contract | one suite run against every provider for its declared capabilities: registration, valid IDs, revision growth, NotFound for unknown IDs, reconnect after restart |
| Integration | shell/core, core/plugin, plugin crash and restart, provider disappear/reappear, daemon restart |
| UI | D-pad navigation and the focus graph (Qt Quick Test) |
| Platform (operator) | Home/Return cycles, lock/suspend, background capture on a real KDE session |

`arinn-plugin-demo` is mandatory: UI development without external applications, contract tests, bug reproduction and Plugin API documentation. Demo fixtures cover empty/populated, delayed, invalid ID, crash, reconnect, revision race and unavailable backend.

Failure behaviour: if the Chrome plugin crashes, Home keeps working, Steam still launches, media control keeps working and the Browser section shows as unavailable. If `arinnd` is unavailable, the shell does not crash, shows a degraded state and reconnects. If a provider is slow, the UI thread is not blocked and the operation ends in a timeout or error state.

Performance budgets: [§6.5](#65-performance-principles). Contribution workflow and required local checks: [CONTRIBUTING.md](../../CONTRIBUTING.md).

An offline development check, `arinnd --check-plugins`, validates discovered manifests
and reports JSON diagnostics without launching plugins or requiring a session bus.
Its availability does not imply that production runtime discovery or Plugin API v1
is complete. The [Phase 1 implementation record](../architecture/phase-1-progress.md)
distinguishes implemented primitives from the remaining runtime work.

The standalone UI gallery also provides a Home/navigation development preview
(`arinn-ui-gallery --home`) with explicit populated, empty, loading, error and
unavailable presentation fixtures. It emits observable action intentions only;
it does not launch applications, retry a provider or perform system actions.
This preview does not replace the production shell or implement web shortcuts.
Its scope and verification are recorded in the [component contract](../architecture/ui-components.md).

The production shell uses the same Home/navigation components with asynchronous
daemon data: an opaque Return target, and a Home favorites row that lists daemon
web shortcuts and then application entries until the user can pin favorites.
Steam and ES-DE stay on that row as application tiles ([§5.1](#51-home),
[§7.2](#72-applications-steam-and-es-de)). A shortcut shows its title and a
monogram or neutral placeholder, and selecting it calls `WebShortcuts1.Open`.
Selecting an application launches it through the generic application provider.
The Applications route lists those same application targets and does not define
New. A failed application catalog is a separate error from an empty shortcut
list. Unimplemented sections show an explicit unavailable presentation; launch
and shortcut feedback follow the daemon operation. The
[shell binding](../architecture/ui-web-shortcuts.md) records this Home row. The
[component contract](../architecture/ui-components.md#production-home-and-applications)
records the shared components.

### 17.1. Continuous integration and delivery

([ADR-0033](../adr/0033-ci-cd-and-releases.md))

- Every pull request and every push to `master` MUST pass the full pipeline: Rust lints and tests, the Qt build and private-bus integration suite on current Arch Linux, KWin adapter tests, licensing checks, documentation and version checks, and an Arch package build that is installed into a clean system, smoke-tested and uninstalled.
- A pull request that changes only Markdown files and changelog fragments runs only the licensing and documentation checks; the build jobs are skipped. Pushes to `master`, nightly and release runs always run the full pipeline ([ADR-0044](../adr/0044-docs-only-ci-scope.md)). On pull requests the package build reuses a Cargo cache and skips the release-mode `check()`, which the Rust job covers; release builds are clean and run `check()` ([ADR-0047](../adr/0047-faster-package-builds.md)). A pull request builds and smoke-tests the package only when it changes packaging inputs, and a newer push cancels the running pipeline of the same pull request or of `master`; jobs run on a self-hosted runner when the repository variable `ARINN_RUNNER` names one ([ADR-0049](../adr/0049-ci-minutes.md)).
- The same pipeline runs nightly against the latest Arch Linux, Qt and Rust. A red nightly blocks releases.
- `master` is protected: changes land only through pull requests with green CI and a signed CLA.
- Releases are cut from `master` by a `vX.Y.Z` tag that matches the workspace version and a `CHANGELOG.md` section. The release pipeline reruns the full CI on the tagged commit and publishes only if it passes. `0.x` versions are pre-releases.
- Pull requests must be up to date with `master` before merging, so CI runs on the merged result; parallel work follows [ADR-0036](../adr/0036-parallel-development.md).
- Every bug fix includes a regression test. Changes to the platform layer, input or session behaviour also need operator tests on a real KDE session before release, because hosted CI cannot run a compositor.

---

## 18. Packaging, distribution and licensing

- The first official target is Arch Linux / EndeavourOS with KDE Plasma 6 on Wayland; the architecture does not depend on pacman.
- `arinnd` runs as a user service bound to the graphical session; the shell starts from the session autostart after Plasma login; no duplicate instances. After login Arinn starts automatically and shows Home (AC-01).
- Packaging installs binaries, plugin manifests, the KWin script, the Chrome extension and the Native Messaging manifest. The Arch package recipe is `packaging/arch/PKGBUILD`; every full CI run builds, installs and smoke-tests it ([§17.1](#171-continuous-integration-and-delivery)). Releases are published on GitHub Releases; an AUR package follows in Phase 5.
- Licensing ([ADR-0029](../adr/0029-licensing.md)): core, shell, CLI, platform adapter and Arinn's plugins are `GPL-3.0-or-later`; `arinn-protocol`, the plugin SDK and interface XML are `MIT OR Apache-2.0` so third-party plugins may use any license; documentation is `CC-BY-SA-4.0`. Contributions require the [Contributor License Agreement](../../CLA.md). The repository contains no third-party logos; production builds must use service icons whose licenses allow it, or neutral placeholders.

Repository layout (may evolve; the separation shell / core / protocol / plugins / platform MUST stay):

```text
apps/arinn-shell/        Qt/QML shell
crates/arinnd/           daemon
crates/arinn-protocol/   shared protocol types (permissive license)
crates/arinn-plugin-sdk/ plugin SDK primitives (permissive license; transport pending)
plugins/<name>/          Arinn's own plugins
platform/kde/            KDE platform adapter (KWin script)
tools/arinnctl/          CLI
chrome-extension/        Chrome extension and native host (planned)
packaging/               distribution packaging (Arch PKGBUILD)
docs/                    specification, ADRs, architecture, plugin API, design
tests/                   contract, integration and UI tests
```

---

## 19. Acceptance criteria

The MVP is accepted when all of the following hold on the target HTPC. Verification status is tracked in the [roadmap](../../ROADMAP.md).

| ID | Criterion |
| --- | --- |
| AC-01 | **Startup.** After logging into the HTPC session Arinn starts automatically and shows Home. |
| AC-02 | **Remote navigation.** The main interface is fully controlled with arrows/D-pad, Select and Back. |
| AC-03 | **Browser.** The user opens Chrome or an online service from Home. |
| AC-04 | **Browser tabs.** Arinn shows existing Chrome tabs and switches between them. |
| AC-05 | **Global Home.** From Chrome the user invokes Home and gets Arinn over the browser. |
| AC-06 | **Resume.** After closing Home the user returns to the same browser activity. |
| AC-07 | **Steam.** Steam launches from its own Home tile. |
| AC-08 | **ES-DE.** EmulationStation launches from its own Home tile. |
| AC-09 | **Media.** An active MPRIS media session is shown in Arinn and controlled through it. |
| AC-10 | **Plugin isolation.** A Chrome provider crash does not crash the shell or the daemon. |
| AC-11 | **Dynamic provider.** A provider can be installed and removed without provider-specific logic in `arinnd`. |
| AC-12 | **Capability abstraction.** The shell has no conditions such as `if provider == "chrome"` or `if application == "steam"` for capability business logic. |
| AC-13 | **Shared household.** The whole main UX works without profile selection or an Arinn login. |
| AC-14 | **3-metre UX.** Current focus and primary actions are unambiguous on a TV. |
| AC-15 | **Library.** A local folder and a mounted network folder are scanned; metadata or fallback is shown; entries survive an unavailable mount; a film and an episode can be started. |
| AC-16 | **Playback.** Play/pause/seek, audio and subtitle selection, resume after restart and a correct Continue in the mpv workflow. |
| AC-17 | **Downloads.** Transmission queue; pause/resume/start now/retry; error and unavailable states; completed media appears in Library exactly once and can be started. |
| AC-18 | **Home/Back input.** Home/Back follow [§6.3](#63-home-and-back) with standard keyboard and mouse events; Esc and Backspace work with the text-input exception. The G60S Pro uses the same input path; a physical remote check is an extra smoke test. |
| AC-19 | **Suspend and ambient.** Sleep really suspends the HTPC; wake and provider reconnect work; the shell is never shown over the lock screen. Ambient is checked separately and never triggers suspend. |

---

## 20. Architecture rules

These rules are mandatory. Architecture review checks every pull request that touches boundaries against them.

| ID | Rule |
| --- | --- |
| R-01 | The core knows capabilities, not products. |
| R-02 | Integrations are external processes. |
| R-03 | The shell never talks to third-party applications directly. |
| R-04 | Plugins never supply arbitrary UI. |
| R-05 | Platform-specific code is isolated. |
| R-06 | An external Activity is a normal state of Arinn, not an exit from it. |
| R-07 | Browser integration is MVP functionality. |
| R-08 | Steam and ES-DE are first-class entry points even before dedicated providers exist. |
| R-09 | A new integration does not change `arinnd` if existing capabilities suffice. |
| R-10 | If existing capabilities are insufficient, the Plugin API change is reviewed separately and must apply to more than one product. |

Extension criterion: a developer adds a new service by writing `arinn-plugin-foo`, declaring capabilities and installing a manifest; `arinnd` discovers the provider and Arinn starts using it — without changes to `arinnd` or `arinn-shell`. If every new integration needs core changes or product-specific QML, the architectural goal is not met.

---

## 21. Open questions

Each question needs a decision by the project owner. The recommendation is the proposed default; when a question is decided, it moves into the relevant section (with an ADR where required) and is removed from this list.

| ID | Question | Conflict | Recommendation |
| --- | --- | --- | --- |
| — | No open questions. | — | — |

---

## 22. Changes from v0.3

Owner decisions after the `v0.1.0` pre-release (2026-09-28, no version change): OQ-01 (the shell is the input layer during managed playback, ADR-0046), OQ-04 (Plasma handles media and volume keys outside the shell) and OQ-05 (New is not part of the MVP) are resolved; web shortcut text entry uses a hardware keyboard in the MVP.

Version 0.6 accompanies the first `v0.1.0` pre-release. It includes the production
application API (ADR-0035), browser launch ownership (ADR-0038), platform window
integration (ADR-0039/0040), and independent session lifecycle guard (ADR-0041).
It also includes production Home and Applications binding to asynchronous daemon
models. Phase 1 completion is recorded separately in the roadmap after release verification;
this version does not declare Plugin API 1.0 stable.

Version 0.5 resolves OQ-06: provider data-directory identity is derived from the
provider ID, independently of the registry database (ADR-0034). It also includes
the Phase 1 status clarification and offline manifest check documented in the
0.4.1 working draft. No implementation gate is closed by this decision.

| Area | v0.3 | v0.4 | Source |
| --- | --- | --- | --- |
| Status and language | Russian draft, architecture review candidate | English, current and living; ADR-0001–0019 accepted | ADR-0028 |
| Capabilities | Five; DownloadProvider deferred | Six, with DownloadProvider; media and library extensions | ADR-0024, ADR-0025 |
| MVP plugins | demo, desktop, chrome, mpris | + library, mpv, transmission | ADR-0023–0025 |
| Downloads | Reserved route | Working section with Transmission | P-03, ADR-0025 |
| Media | MPRIS only | MPRIS plus local library and external mpv | P-02, ADR-0024 |
| IPC | D-Bus shell ↔ daemon; plugin transport open | Session D-Bus on both boundaries; limits, operations, generations | ADR-0020 |
| Supervision | systemd "desirable" | Transient user services; separate Chrome native host | ADR-0021 |
| Errors | Eight normalized errors | + `Cancelled` | ADR-0020 |
| Back on Home without an Activity | Undefined | Opens navigation | P-01, ADR-0023 |
| Sleep | Undefined | OS suspend; ambient separate | P-04, ADR-0023 |
| Remote and verification machine | — | Standard keyboard input; development machine for Phase 0 | P-06, P-07, ADR-0030 |
| Screens | — | One screen on the target; Home on the Activity's output | P-08, ADR-0026, ADR-0031 |
| Platform mechanism | Not chosen | KWin script adapter; last-frame Activity background | ADR-0027, ADR-0032 |
| Persistence | Formats not chosen | TOML config, SQLite state, cache budget | ADR-0022 |
| CLI | `provider restart` desirable | `provider restart` and `providers reload` required; exit codes | ADR-0020 |
| Acceptance | AC-01–AC-14 | AC-01–AC-19 | ADR-0023 |
| Licensing | — | GPL-3.0-or-later core, permissive protocol, CLA | ADR-0029 |
| CI/CD | — | Full pipeline on every change and nightly; package install tests; gated releases | ADR-0033 |
| Development phases | §57 | Moved to the [roadmap](../../ROADMAP.md) | ADR-0028 |
| Repository layout | `crates/arinn-platform` | `platform/kde` for the KWin adapter | Phase 0 |

Phase 1 adds the concrete application wire contract and live introspection checks (ADR-0035).

---

## Appendix A. Glossary

| Term | Meaning |
| --- | --- |
| Activity | What the user is doing right now (a tab, a game, playback); may differ from an application |
| Capability | A typed contract a provider implements (application, browser, media, library, action, download) |
| Plugin / provider | A plugin is an installed package with a manifest; a provider is a configured running instance of it |
| EntityRef | `(provider_id, capability, local_id)` address of an entity |
| Generation | UUID of one provider registration; invalidates everything from earlier registrations |
| Revision | Monotonic version of a provider's snapshot within a generation |
| Operation | Asynchronous command object with state, error and result |
| Home overlay | Arinn shown over a running external application |
| Return Activity | The Activity that Back or Return goes back to from Home |
| OSD | On-screen display of the player (timeline, tracks) |
| Ambient | Arinn's screensaver, separate from OS suspend |
| Ember | The brand accent colour `#FF8A3D` |
