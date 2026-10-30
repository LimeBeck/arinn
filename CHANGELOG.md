# Changelog

All notable changes to this project are documented here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). Arinn has no releases yet; entries are grouped by roadmap phase until the first release, after which [Semantic Versioning](https://semver.org/) applies.

Unreleased changes are not written here: every pull request adds a fragment under [`changes/`](changes/README.md), and `python scripts/release.py collect VERSION` moves them into a release section ([ADR-0036](docs/adr/0036-parallel-development.md)).

## [Unreleased]

## [0.2.0] - 2026-10-30

### Added

- Add an in-process provider supervisor when the session has no systemd user manager (ADR-0089).
- Kill supervised provider process groups on daemon exit; failed spawns count toward the same start limit as systemd.
- Document the `ARINN_PRIVATE_TEST_BUS` test-harness exception in ADR-0089 (production bootstrap unchanged).
- The Chrome provider links its windows to compositor windows, so switching to an open service raises exactly its window ([ADR-0052](docs/adr/0052-browser-window-association.md)).
- The plugin API defines the address and search contract `Browse1`: the input rules, recent entries and packaged search engines ([ADR-0066](docs/adr/0066-address-and-search-contract.md)).
- Opening a web shortcut with the browser closed launches the browser first ([ADR-0055](docs/adr/0055-launching-a-closed-browser.md)); `browser_application` in `config.toml` chooses the application.
- Add a local comparison of fixture-driven QML screens with the design handoff at 1920×1080 and at 4K scale ([scripts/ui-compare.py](scripts/ui-compare.py)). The report is layout evidence for the UI increment. It does not pass G-5.1.
- Open services can save the page on a row as a web shortcut. OK on the bookmark control calls `WebShortcuts1.Add` with that page's title and address; an origin that already matches a shortcut shows the saved mark, a rejected address shows a short message instead of the raw error code, and the new shortcut is listed under Applications › Online ([ADR-0051](docs/adr/0051-address-and-search-entry.md)).
- `arinn-plugin-mpris` publishes session-bus players as `media` sessions. `Features` stays empty, artwork URLs are not forwarded, and a command the player does not offer returns `Unsupported`. A player that does not answer is retried and does not stall the others.
- `arinn-plugin-library` scans configured folders into a `library` catalogue with resume and idempotent import ([ADR-0056](docs/adr/0056-local-library-scan-metadata-and-resume.md)). Roots come from `ARINN_PROVIDER_SETTINGS` at provider start. File names and NFO supply metadata; duration comes from ffprobe when it is installed, and stays unknown when the binary is missing. An unreadable directory does not stop the scan. An unavailable folder keeps entries and does not log paths or titles.
- `arinn-plugin-mpv` plays local files in an external mpv process and exposes tracks, chapters and seek thumbnails (`MediaOpen1`, `MediaTracks1`, `MediaTimeline1`). The player runs in its own user unit ([ADR-0057](docs/adr/0057-mpv-ipc-and-process-scope.md)). mpv is optional; without it the backend is `unavailable`. A missing or unreadable file does not open successfully, a slow open stops the player, track changes are confirmed by mpv, and a confirmed end of file reports `ended` with seek and stop only.
- The daemon routes `library` providers and starts a library item in a media provider with `MediaOpen1`; the file URI never reaches the shell ([ADR-0060](docs/adr/0060-library-routing.md)).
- The daemon routes `media` providers: it exports `org.arinn.Media1` with sessions, playback commands, tracks and timeline ([ADR-0058](docs/adr/0058-media-routing.md)).
- The daemon forwards playback progress of sessions opened from the library to the library provider, so resume positions are saved ([ADR-0061](docs/adr/0061-playback-progress-forwarding.md)).
- The plugin API defines how artwork, posters, fanart and thumbnails are fetched by id: the optional `AssetProvider1` interface and the daemon's `Assets1` ([ADR-0062](docs/adr/0062-asset-contract.md)).
- Add the [Phase 3 operator runbook](docs/architecture/phase-3-runbook.md) and its [evidence template](docs/architecture/evidence/phase-3-template.md) for AC-03–AC-06 with AC-10 and G-3.1 spot checks, on KDE Plasma 6/Wayland and XFCE/X11.
- Continue and the now-playing screen read media sessions from `org.arinn.Media1`. Position follows the anchor clock only while `playing`, and a command is an operation ([ADR-0054](docs/adr/0054-media-and-library-wire-contract.md), [ADR-0058](docs/adr/0058-media-routing.md)).
- The now-playing screen shows a waiting state while a command is in progress, and an error banner when the command is unsupported, times out, or fails.
- A missing or future position anchor no longer jumps playback to the end of the session.
- The player OSD binds to `org.arinn.Media1`: transport, audio and subtitle sheets, and the timeline when the provider lists `MediaTimeline1`. A missing `MediaTracks1` or `MediaTimeline1` flag hides that control. A seek is one shell command; the thumbnail request does not seek again. Playback still requests a thumbnail, and focusing the timeline again after a seek does too ([ADR-0046](docs/adr/0046-shell-input-layer-during-playback.md), [ADR-0054](docs/adr/0054-media-and-library-wire-contract.md)).
- The Library screen reads collections, films, series and episodes from `org.arinn.Library1`, keeps a missing file in the catalogue, and shows Continue from resume. Launch is an operation whose result is the new media session ([ADR-0054](docs/adr/0054-media-and-library-wire-contract.md), [ADR-0060](docs/adr/0060-library-routing.md)).
- Play on a series resumes the in-progress episode, or starts the first available episode, and Continue follows that episode. A launch that is already waiting does not start another, and the episode still shows busy.
- Connect managed playback controls to the native Wayland video window, keep shell input over hidden OSD, and preserve playback across Home/Return (AC-16, G-4.1, ADR-0074).
- Retain failed Release cleanup scope for explicit retry until acknowledgement succeeds, fenced against new presentation and owner replacement.
- Isolate obsolete Present cleanup from another session's Release retry state.
- Expose explicit Home Back retry after failed Release while preserving modal and navigation Back handling.
- The plugin API defines the download contract: `DownloadProvider1` for providers, `Downloads1` for the shell, and the SDK `DownloadService` ([ADR-0067](docs/adr/0067-download-wire-contract.md)).
- `arinn-plugin-transmission` adapts a Transmission RPC service to the download provider: queue, pause, resume, start now and retry, with folder mapping and credentials in the system secret storage ([ADR-0025](docs/adr/0025-downloads-mvp.md), [ADR-0067](docs/adr/0067-download-wire-contract.md)).
- Route download snapshots and controls through the daemon, and checkpoint completed-output imports into the library across retries and restarts. Shell-facing download rows omit file URIs and distinguish download completion from library import.
- The plugin API defines provider settings: providers declare a typed schema through `ProviderSettings1`, and the daemon will serve and change them through `Settings1` ([ADR-0065](docs/adr/0065-provider-settings-contract.md)).
- Home closes an open player track sheet and the launch dialog without confirming them, then shows Home. Playback is not paused or stopped.
- From an external application, Back stays with that application. Home opens the shell on the activity that was in front.
- Shell: global navigation › Power now opens the power menu. Sleep, Restart and Shut down go to the system through the daemon; the menu shows which actions the system allows, names the application that blocks one, lists the running applications that will close, and shows the system's refusal instead of a guess. An accepted request shows a waiting state, never success by itself. A request that has been sent cannot be cancelled; Home still leaves, and a later refusal is shown when it arrives ([#148](https://github.com/LimeBeck/arinn/issues/148), [ADR-0108](docs/adr/0108-system-control.md)).
- Shell: the volume and mute keys change the system volume while Arinn is on screen (Volume Up also turns muted sound back on) and show the volume HUD from the design handoff ([#148](https://github.com/LimeBeck/arinn/issues/148)).
- The daemon exports `Background1` and `BackgroundCapture1` for the Activity background ([ADR-0087](docs/adr/0087-bounded-activity-background-frames.md)). Only an explicit Home request arms a capture, and it gets a 1500 ms deadline. Only the selected platform adapter can publish a sealed frame, and only the shell can read it. Return, a target or window change, adapter or shell loss, lock or sleep invalidate the frame. No producer or shell consumer is connected yet ([#149](https://github.com/LimeBeck/arinn/issues/149)).
- Add independently testable Activity-background foundations: sealed bounded RGBA descriptors, Qt grab colour/alpha conversion and shell frame lifetime admission. Production capture, daemon transport, Home integration and target-device qualification remain pending (#149).
- On KDE Plasma, a packaged KWin script captures the exact Activity window when Home opens over it and hands the frame to the daemon for Home's receded background. The script uses the new `org.arinn.background` QML module, and `arinn-kwin.service` loads it next to the adapter. If KWin refuses it, Home keeps its normal background. No screenshot file is written ([#149](https://github.com/LimeBeck/arinn/issues/149)).
- Home can show the running application's last frame as its receded background: near while the application's card has focus, far otherwise. The shell reads the frame from the daemon's `Background1` without delaying Home or focus, and clears it as soon as the session locks. Until a platform producer publishes frames, Home keeps its normal background ([#149](https://github.com/LimeBeck/arinn/issues/149)).
- On X11/EWMH desktops with a compositing manager, the adapter captures the exact Activity window through XComposite when Home opens over it and hands the frame to the daemon for Home's receded background. Without a compositor, or while the session is locked, Home keeps its normal background. The adapter now reports the Home output as `x11-screen` ([#149](https://github.com/LimeBeck/arinn/issues/149)).
<!-- SPDX-FileCopyrightText: 2026 Arinn contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

- Add an isolated Home performance harness with raw samples and p95 reports for focus latency, frame cadence and delayed D-Bus reads; target-device qualification remains owner-run.
- Media sessions can report the state `ended` for a confirmed end of file ([ADR-0063](docs/adr/0063-ended-session-state.md)).
- The daemon serves artwork, posters and thumbnails to the shell through `org.arinn.Assets1` and validates every image again ([ADR-0062](docs/adr/0062-asset-contract.md)).
- The shell loads artwork, posters and fanart through `Assets1`, and keeps the generic plate when an image is missing ([ADR-0062](docs/adr/0062-asset-contract.md)). The player timeline thumbnail stays with the player binding.
- Providers can have settings in `config.toml` (`[providers.<id>.settings]`), which the daemon passes to them as `ARINN_PROVIDER_SETTINGS` at start ([ADR-0064](docs/adr/0064-provider-settings-in-config.md)).
- Resume positions record a confirmed end of file: the daemon forwards `ended` when a media session reaches the `ended` state ([ADR-0063](docs/adr/0063-ended-session-state.md)).
- The daemon serves provider settings through `org.arinn.Settings1`: it lists providers with settings, returns current values, stores changes in `config.toml` and applies them to the running provider; the daemon's own `browser_application` is available as provider `arinn` ([ADR-0065](docs/adr/0065-provider-settings-contract.md)).
- The Settings screen reads `org.arinn.Settings1` and groups provider settings by category. Each kind has a typed control: a toggle, a stepper and number field, a text field, an option list, or a path list when `multiple` is set. `SetValue` is an operation with a pending state and an error state, disabled settings stay visible but cannot be edited, a pending setting cannot be changed again until that operation settles, D-pad focus stays on the same control when the list refreshes, and `SettingsChanged` refreshes the screen ([ADR-0065](docs/adr/0065-provider-settings-contract.md)).
- The daemon serves `Browse1`: typed addresses and searches open in the browser, the last five entries are kept, and the search engine is a setting over the packaged `search-engines.toml` (Google by default).
- Add the keyboard-driven Site or search screen from Home and Open services, with daemon-resolved input, shortcut suggestions, an already-open hint, clearable recent entries and operation feedback.
- Define optional managed playback window identity and acknowledged presentation
controls for the transparent shell input layer (AC-16, G-4.1; ADR-0074).
- Plugin API: integer settings may declare a short display `unit`, a new daemon interface `org.arinn.Folders1` lists folders for the folder picker, and library providers may report which configured roots are available through the optional `org.arinn.LibraryRoots1` (ADR-0071). The daemon export of `Folders1` and `Library1.GetRoots` follow separately.
- Settings value editors open as sheets: an enum list with paging, an integer stepper with bounds and hold acceleration, and a string field that asks for a keyboard when none is attached. A remote’s arrow keys do not count as a keyboard. A disabled row stays visible with a lock and “Cannot be changed now”. Saving shows a pending spinner, a toast when the value is applied, a row note when the operation error is `Unsupported` (succeeded or failed), and the daemon’s error text when it is rejected. Paths stay plain text ([ADR-0065](docs/adr/0065-provider-settings-contract.md)).
- Add the design brief for closing running applications from the tile menu, the Continue card and a running list, with draft frames in the handoff `.dc.html` format and open questions for the owner ([brief](docs/design/briefs/closing-running-applications/README.md)).
- Define an optional typed Return-to-platform-window association and trusted
  adapter report contract (ADR-0073, #212). Existing opaque Return/Home remains
  compatible; production producer and consumer work is still needed before
  Home/service deduplication can use it.
- Export the optional Return/window association from the production daemon and
  report exact saved-window evidence from the KDE adapter (ADR-0073, #217).
  Serialize Home/inventory invalidation and reject stale browser links. Shell
  consumption and real KDE qualification remain separate follow-ups.
- Recover from transient owner lookups and unacknowledged association reports
  without exposing unverified identity, and keep bus round-trips outside the
  provider/inventory locks (PR #218 review follow-up).
- Fence delayed Home authorization so an obsolete owner cannot overwrite a newer
  Home commit, including one with identical public association content.
- Add managed player launch identity, daemon media-window association and acknowledged KWin presentation routing (AC-16, G-4.1, ADR-0074).

- Clean up failed presentation dispatches by exact scope and wake media invalidation on provider or session loss.
- Recheck provider readiness and negotiated support at the final association and Present admission fence.
- Bound automatic cleanup to each exact presentation, retain admitted cleanup on provider loss, and preserve the report barrier after skipped controls.
- Retain coalesced media-window invalidations during signal emission and suppress unchanged provider polling signals.
- Release pending and active presentation when the provider removes negotiated media-window support.
- Advance the Present cancellation fence only for an admitted Release.
- Define optional provider-owned web shortcut icon APIs and bounded public-HTTPS discovery/cache policy, with monogram fallback and an exact-match packaged preset migration plan. Production download, Kinopoisk correction and tile rendering follow after this contract merges.
- Add bounded provider-owned site icon discovery/cache and asynchronous web tile image loading under [ADR-0075](docs/adr/0075-web-shortcut-site-icons.md). Route saved shortcut IDs through the daemon with revision and provider-lifetime checks.
- Correct the packaged Kinopoisk URL to `https://hd.kinopoisk.ru` and migrate exact unmodified old presets once without recreating removed entries.
- Open Downloads from the Home menu. The screen shows the live download queue from the daemon with progress, size, speed and time left, and offers Pause, Resume, Start now and Retry. A completed download says when it was added to the library; Watch opens it there, and a toast announces each completed import once per session ([downloads screen](docs/architecture/ui-downloads.md)).
- The daemon exports `org.arinn.Folders1` for the folder picker (folders only, hidden entries skipped, mounts as start points) and serves library root availability as `Library1.GetRoots`, announcing changes with its `StateChanged` (ADR-0071).
- Add optional KDE Plasma/X11 session selection and managed-player instance identity alongside Wayland; real X11/TV qualification remains pending.

Declare Python for no-check package builds, reject incompatible display-only
session selection, and record the ADR-0042 shell transport amendment.
- Add an opt-in generic X11/EWMH adapter with window inventory and control, Meta+F12 Home, exact-target Return and composited managed-video layering. Keep KWin/Wayland as the default; real desktop qualification, XComposite background and output placement remain pending.

Preserve exact desktop-file IDs, support XFCE lock endpoints in adapter and shell,
run native Xvfb coverage in CI, and document manual XFCE session startup.

Reject unproven suffixless application metadata and release video presentation
on unrelated focus or shell lifetime loss without stealing focus.

Resolve the Home Meta/Super modifier from the keyboard mapping, including remaps.

Publish media identity transitions independently of unchanged window inventory.

Restore player presentation before retiring a disappeared inventory row.

Preserve restoration evidence during shutdown and reconnect cleanup as well.
- Add an opt-in XFCE/X11 autostart template and session helper that imports the graphical environment, selects the EWMH backend before startup and stops Arinn when the XFCE session ends. See [XFCE startup setup](docs/architecture/x11-ewmh.md#persistent-xfcex11-startup); package installation enables nothing and real-desktop qualification remains pending.
- Add a Close Arinn action to global navigation, available with a pointer or
  D-pad even when the daemon is unavailable. Restore focus to Open services after
  switching to a browser tab and returning to the shell.
- Arinn has an application menu entry with its icon: it starts the session when none runs, otherwise it brings up Home, and the shell window shows the Arinn icon in the taskbar.
- The Now playing and Continue cards on Home show a frame of the film's current position instead of a letter (mpv provider).
- Add optional Sentry diagnostics, startup/operation traces and numeric metrics to the shell and daemon, disabled by default through a shared telemetry configuration flag, with filtered technical events and breadcrumbs; track public-release consent requirements separately.
- Embed the owner-approved public Sentry DSN as the shell and daemon default, preserving disabled-by-default telemetry and explicit endpoint overrides; enabling diagnostics now needs only the configuration flag.
- Shell: after ten minutes without input on an Arinn screen the ambient screen appears: a dim clock and date that drift across a black background. It never starts while media plays, during managed playback or while another application is in front, and never suspends the computer or stops anything. The first key or pointer movement only wakes it and returns to the same screen and focus. The power menu's Ambient action shows the same screen ([#307](https://github.com/LimeBeck/arinn/issues/307)).
- Recognise anime seasons marked `TV1`, `TV-2` … in file names, in `<Title> TVn` folders and in `TVn` sub-folders, with episode numbers such as ` - 05` or `[07]`; such files now appear as episodes of one series instead of as films. Inside a season folder a file named only by its number (`01.mkv`, `Show - 03.mkv`) also gets that episode number ([ADR-0085](docs/adr/0085-anime-tv-season-markers.md)).
- Define the daemon-owned Favorites1 contract for persistent, ordered Home favorites and revision-guarded pin/unpin clients; daemon and shell implementation follow separately.
- Export persistent Favorites1 from the daemon with ordered stable targets, transactional revision conflicts, current-catalog admission and post-commit notifications.
- Add a [user guide website](docs/user/index.html) with installation, controls, browser setup, local media, privacy and troubleshooting instructions, plus a GitHub Pages deployment workflow and local site validation. Pages activation and deployment-workflow merging require the owner.
- Add a source-backed throughput proposal including reproduced Settings identity/i64 correctness gaps, shared-file/build repetition evidence, updated scenario dependencies and owner-controlled measurement pilots; no process or runtime change is adopted.
- Add an Arinn product website with a responsive landing page, seven current Qt screenshots, accessible image enlargement and the user handbook. Assemble an explicit public-file allowlist for GitHub Pages and validate project-prefix links before publishing.
- Define optional authenticated Activity background metadata and sealed-frame transport, with bounded physical pixels, capture deadline and exact Home/window lifetime validation; production capture and device qualification remain separate.
- `org.arinn.Daemon1.BuildInfo()` reports the daemon's version and build id, `<version>+g<12 hex>` with `-dirty` for a modified checkout. Every build computes the id with `scripts/build_id.py`, and source archives carry their commit ([#357](https://github.com/LimeBeck/arinn/issues/357), [ADR-0097](docs/adr/0097-build-identity.md)).
- When the shell meets a daemon of another build, Home says "Arinn was updated" and offers Restart instead of "Home could not be loaded". Restart restarts the daemon and the shell through `arinn-shell.service` (exit status 75). `arinnctl --version` prints the build id, `arinnctl status` shows the CLI's and the daemon's build ids, and Sentry releases name the build ([#357](https://github.com/LimeBeck/arinn/issues/357), [ADR-0098](docs/adr/0098-restart-after-an-update.md)).
- Plugin API: optional `MediaProcess1`, `BrowserProcess1` and `MediaAssociation1` interfaces let the daemon link a browser's media session to the one audible tab that plays it, by process identity only ([ADR-0096](docs/adr/0096-browser-media-session-association.md), [#359](https://github.com/LimeBeck/arinn/issues/359)). Contract only; producers, the daemon join and Home's single card follow.
- Home shows one Continue card for a video playing in a browser tab instead of two: the tab card takes the playing or paused state of the browser's own media session, opens that exact tab, and sends play/pause media keys to the session. When the daemon cannot verify the link, for example with several audible tabs before a link exists, another browser or a sandboxed browser, both cards stay ([#359](https://github.com/LimeBeck/arinn/issues/359), [ADR-0096](docs/adr/0096-browser-media-session-association.md)).
- Downloads: the summary line says "N downloading" while something downloads and "N done" when every download is finished, and is hidden otherwise. An imported download shows its library item's poster; other rows show a neutral striped plate instead of a coloured chip ([#360](https://github.com/LimeBeck/arinn/issues/360)).
- Recognise a standalone season marker `S01` in library folder and file names, by the same rules as `TV1`: a release folder such as `Show Name [WEB-DL 1080p] S01/`, a bare `S02/` folder and `Show S01 - 05.mkv` now give seasons and episodes of one series instead of films. `S01E02` and `1x02` keep precedence ([ADR-0109](docs/adr/0109-season-only-snn-markers.md)).
- Show the shell in Russian when the system locale is Russian: every screen, status and error message is translated through Qt translations, with English kept for other locales ([shell translations](docs/architecture/ui-translations.md)).
- Home's hero describes the focused item and what OK does: Now running · Return for an activity, the time played and left for media, the group and Opens full screen or Running · Return for a favorite ([#364](https://github.com/LimeBeck/arinn/issues/364)).
- Continue media cards show a backdrop (session artwork, else the library item's fanart or poster), the time left and an ember progress bar ([#365](https://github.com/LimeBeck/arinn/issues/365)).
- The open-services card reads "N more open" or "Open: N" with "All open services" and stacked chips of up to three services ([#366](https://github.com/LimeBeck/arinn/issues/366)).
- Home's status area shows a download chip with the aggregate progress while something downloads ([#367](https://github.com/LimeBeck/arinn/issues/367)).
- Home has a warm ambient light field tinted by the focused item's artwork or identity colour, cross-fading in 900 ms ([#368](https://github.com/LimeBeck/arinn/issues/368)).
- Prepare package-matched native and Rust debug files for Sentry: verify Arch and Debian build IDs, and upload only from successful trusted publication builds. Runtime telemetry consent is unchanged; owner review and merge are required.
- Sentry diagnostics (when enabled) now time the shell's user-visible flows, with fixed names only: Home load, Downloads read, address and web shortcut opens, Return to the activity, and presenting a managed player until its input layer is confirmed ([#370](https://github.com/LimeBeck/arinn/issues/370)).
- Record shell player route changes and fixed reasons for abandoning managed playback, so Stop/relaunch failures can be distinguished from focus and window-association failures ([#376](https://github.com/LimeBeck/arinn/issues/376)).
- CI builds a Debian `.deb` from `packaging/debian/**`, smoke-tests it on Ubuntu 25.04, and attaches it to version releases alongside the Arch package. Arch remains the nightly and release package blocker ([#377](https://github.com/LimeBeck/arinn/issues/377), [ADR-0091](docs/adr/0091-debian-package-ci-artifact.md)).
- Plugin API: a read-only daemon-root `SystemStatus1` interface publishes mute state and network connectivity for Home's status area, with `unknown` for anything a platform cannot report ([ADR-0100](docs/adr/0100-system-status.md), [#387](https://github.com/LimeBeck/arinn/issues/387)). Contract only; the producer and the shell icons follow.
- Daemon: `SystemStatus1` now reports mute state from the sound server (`pactl`) and network connectivity, link kind and Wi-Fi level from NetworkManager. Either field stays `unknown` when its source is missing ([ADR-0100](docs/adr/0100-system-status.md), [#387](https://github.com/LimeBeck/arinn/issues/387)).
- Home's status area shows a mute icon while the sound is muted and a Wi-Fi icon with its signal level while connected over Wi-Fi, between the download chip and the clock. Unknown state shows nothing ([ADR-0100](docs/adr/0100-system-status.md), [#387](https://github.com/LimeBeck/arinn/issues/387)).
- Settings › Appearance: **Light field** turns the soft light behind Home on or off, and **Ambient after** chooses how long an Arinn screen waits before the ambient screen (5, 10, 15, 30 or 60 minutes, 10 by default). Both are stored in `config.toml` with the daemon's other settings ([ADR-0112](docs/adr/0112-shell-preferences-as-core-settings.md), [#389](https://github.com/LimeBeck/arinn/issues/389)).
- Design handoff of 2026-10-09: the on-screen keyboard for remote-only text input (KB-1…8), the folder picker screenshots and the settings editor decisions; open points are in the [integration review](docs/design/integration-review.md) ([#417](https://github.com/LimeBeck/arinn/issues/417)).
- On-screen keyboard: Site or search, the web shortcut editor and string settings can be typed with the remote alone. Its suggestions on Site or search are recent entries and open services; web shortcuts are no longer suggested there. An empty shortcut name now saves the address's host ([#418](https://github.com/LimeBeck/arinn/issues/418), [ADR-0103](docs/adr/0103-on-screen-keyboard.md)).
- README (English and Russian), the website gallery and the handbook show and describe the on-screen keyboard; the gallery captures it with `arinn-ui-gallery --keyboard`.
- Type Settings paths with the remote alone: a single path, a path list entry and *Add path* now have a keyboard button that opens the on-screen keyboard with the path row (`/`, `~/`, `/media/`, `/run/media/`, `/mnt/` …) and Save as its main key. Without a hardware keyboard the editor opens on that button. Subfolder suggestions come with the folder picker ([ui-keyboard.md](docs/architecture/ui-keyboard.md)).
- Contract for the global on-screen keyboard: the shell-facing `org.arinn.ExternalKeyboard1` and the adapter-facing `org.arinn.KeyboardBridge1`, with bounded text, fixed key names and the trust rules of [ADR-0105](docs/adr/0105-global-on-screen-keyboard-platform-text-delivery.md) ([contract](docs/architecture/external-keyboard.md), [#434](https://github.com/LimeBeck/arinn/issues/434)). The daemon routing and the platform deliverers follow separately.
- The on-screen keyboard also types into the focused application: `keyboard_shortcut` (default Meta+F10) or the remote's microphone opens it over the application, which keeps focus. The remote's arrows, OK and Back drive the panel. Every other key still reaches the application, and the text goes only into the window that had focus. A lock, focus loss, Back, Home or the quick menu closes the panel; nothing typed is logged or remembered. A key that is also a Home or quick-menu key stays with Home or the quick menu ([ADR-0105](docs/adr/0105-global-on-screen-keyboard-platform-text-delivery.md), [#434](https://github.com/LimeBeck/arinn/issues/434)).
- X11/EWMH: the adapter delivers the text with XTest, independent of the keyboard layout. KDE Plasma 6/Wayland: the new `arinn-input-method` is KWin's input method. The daemon selects it only when no other virtual keyboard is chosen, and keeps a KWin window rule so the panel never takes focus.
- The package installs a udev hwdb entry that remaps the Geniatron G60S PRO BT microphone button (`KEY_VOICECOMMAND`, which Qt and X11 cannot use) to `KEY_PROG1` (Launch1).
- When `arinnd` or the shell stops after repeated failures (the systemd start limit), a desktop notification says "Arinn stopped after repeated failures" and names the commands to collect diagnostics and to start Arinn again. Exit status 2 and the restart after an update (status 75) behave as before ([#437](https://github.com/LimeBeck/arinn/issues/437)).
- Arinn login sessions: **Arinn** (KDE Plasma on Wayland) and **Arinn (XFCE)** (X11) start the desktop with Arinn on top. `arinn-session-switch` and the **Return to Arinn** menu launcher switch the next login between Arinn and the plain desktop and log out through the desktop's own logout, without privileges ([ADR-0107](docs/adr/0107-dedicated-arinn-session.md), [#438](https://github.com/LimeBeck/arinn/issues/438)).
- `org.arinn.SystemControl1` gains `GetSessionSwitch` and `SwitchToDesktop`. Inside the dedicated Arinn login session the daemon ends the session through the unprivileged switch helper, and the next login starts the plain desktop. Provider connections are refused. The helper bounds its logout call to 5 s and restores the previous choice when the desktop does not answer; on its own timeout the daemon stops the helper and everything it started ([system control](docs/architecture/system-control.md), [#438](https://github.com/LimeBeck/arinn/issues/438)).
- Shell: in the dedicated Arinn login session the power menu offers **Switch to desktop**, also from the quick menu's Power row. It asks first, with Cancel focused and the live list of applications that will close. A refused or timed-out switch is shown as an error and Arinn keeps running. A request in flight cannot be cancelled; Home still leaves, and a late refusal is shown. Inside an ordinary desktop session the entry is not shown ([#438](https://github.com/LimeBeck/arinn/issues/438), [ADR-0107](docs/adr/0107-dedicated-arinn-session.md)).
- Quick menu: `quick_menu_shortcut` (default Meta+F11) in `config.toml` and the remote's Menu key (≡; Qt Menu and MenuKB, only while another window has focus, so Menu keeps toggling navigation inside Arinn) open a side sheet over the running application with Now playing and play/pause, the sound level with mute and volume steps, the network state, Sleep and Power (through the power menu), Open Home and Close Arinn. Back returns to the same application. On KDE the daemon writes the keys to the new KWin action "Arinn: Quick menu"; the X11/EWMH adapter grabs them next to the Home keys and reports its output, so the Activity background works on both ([ADR-0106](docs/adr/0106-quick-menu-presentation.md), [#439](https://github.com/LimeBeck/arinn/issues/439)).
- Daemon: a new `SystemControl1` interface lets the shell suspend, restart or power off the computer and change the volume. It asks logind for suspend, restart and power off without interactive authorization, refuses an action a block inhibitor holds and a second power request while one runs, changes the default sink's volume and mute through `pactl`, and refuses calls from plugins ([ADR-0108](docs/adr/0108-system-control.md), [#464](https://github.com/LimeBeck/arinn/issues/464)). The shell's power menu and volume HUD follow.
- `Daemon1.ReportQuickMenuRequest` and the `QuickMenuRequested` signal: the platform adapter reports the quick-menu key with the Home state of an explicit Home request, and the shell receives its own presentation intent ([ADR-0106](docs/adr/0106-quick-menu-presentation.md), [#467](https://github.com/LimeBeck/arinn/issues/467)).
- Plugin API: the private `Provider1.CancelLaunch(generation, id)` stops the unfinished launches of an application, and application launch operations become cancellable through `Operation1.Cancel`, which stops only that launch and answers `Conflict` while another launch of the same application is running. A provider that observes windows now answers `Launch` when a window appears, which may be after the 30 s dispatch budget. The SDK's default handler refuses cancellation, so existing providers keep working ([ADR-0110](docs/adr/0110-application-launch-lifetime-and-cancellation.md), [#475](https://github.com/LimeBeck/arinn/issues/475)).
- `Daemon1.ReportPowerMenuRequest` and the `PowerMenuRequested` signal: the platform adapter reports the remote's power key with the Home state of an explicit Home request, and the shell receives its own presentation intent to open the power menu ([ADR-0111](docs/adr/0111-power-key.md), [#481](https://github.com/LimeBeck/arinn/issues/481)).
- The remote's power button (⏻) opens Arinn's power menu instead of the desktop's power dialog while Arinn runs: over an application the shell comes up as for Home with the menu open, Back leaves Home on screen, the key ends ambient and does nothing on the lock screen. On KDE Plasma 6.7 or later, PowerDevil's power-button shortcut is set aside only for the running session (older Plasma keeps its own dialog and nothing of it is changed), nothing in the user's settings is written, and it is given back when Arinn stops; logind's own power-button handling is inhibited while Arinn has the key. In the dedicated Arinn XFCE session, xfce4-power-manager's power-button action is "do nothing" while Arinn runs and the user's value comes back when it stops; the ordinary XFCE desktop is not changed. Arinn holds the key only while its shell is running; when the shell closes or fails, the desktop handles the button again. A system autostart entry restores the XFCE setting at the next XFCE login after an unclean end of the dedicated session, and a per-user autostart entry that needs only XFCE's own tools does so even after Arinn is removed ([ADR-0111](docs/adr/0111-power-key.md), [#481](https://github.com/LimeBeck/arinn/issues/481)).
- The Arch package installs opt-in systemd user units to start Arinn after login on KDE Plasma: enable once with `systemctl --user enable arinn-session.target`. The target starts the daemon, the shell and the KWin adapter and stops them with the session ([ADR-0042](docs/adr/0042-opt-in-session-autostart.md)).
- Settings › Media marks a library folder as unavailable when the library provider reports it through `Library1.GetRoots` ([ADR-0071](docs/adr/0071-settings-editor-contract.md)). The folder keeps its place with a note, can still be edited or removed, and returns to normal when the provider reports it again; removing it never deletes files ([#540](https://github.com/LimeBeck/arinn/issues/540)).
- The pre-stable `org.arinn.WebShortcuts1` contract for Arinn-owned web shortcuts: list, add, update, remove and open, with URL and size validation in `arinn-protocol::web`; plain `http` is accepted only for local-network hosts such as a NAS or media server ([ADR-0045](docs/adr/0045-local-http-web-shortcuts.md)) ([web-shortcuts1.xml](docs/plugin-api/web-shortcuts1.xml), [ADR-0043](docs/adr/0043-web-shortcuts.md)). The daemon does not export it yet.
- The local library provider offers its folders in Settings › Media as a folder list, with *Rescan regularly* and *Read video details*; a change applies at the provider's next start. It also reports each folder's availability, so a disconnected drive or share is shown as unavailable ([#551](https://github.com/LimeBeck/arinn/issues/551), [ADR-0071](docs/adr/0071-settings-editor-contract.md)).
- The daemon stores web shortcuts in its state database and exports `org.arinn.WebShortcuts1`. The package ships a default set (YouTube, Kinopoisk, IVI) that is added once per user; opening a shortcut reports "unavailable" until browser integration arrives in Phase 3.
- Add an optional, snapshot-guarded library additions API and SDK publication method for provider-owned discovery times, with validated daemon routing; existing library providers remain compatible.
- Show recently added library films and series on Home, with persisted discovery dates, newest-episode series promotion and a return to the selected card after closing library detail.
- The Applications screen groups tiles into Online, Games, Media and Other; empty groups are hidden and the D-pad moves between groups. Desktop applications are grouped by their `Categories=` entry (web browsers, games, audio/video).
- Extra keys for shell actions can be set in `config.toml` under `[input.bindings]`, for example `menu = ["evdev:582"]` makes a remote's voice button open the menu. Keys Qt cannot name are bound by their evdev code (ADR-0079).
- Home now displays daemon-owned saved favorites in order. Hold Select on an application or favorite to pin/unpin it; missing favorites remain removable, and conflicts offer refresh without automatically retrying a write.
- Add a hardware-keyboard web-shortcut editor in Applications with revision-guarded updates, removal confirmation and private-bus/UI coverage. Favorites pinning remains pending its public contract.
- The browser wire contract (`Browser1`, `BrowserProvider1`) and SDK support for browser providers are defined (ADR-0048).
- Chrome and Chromium integration: an MV3 extension (tabs and nativeMessaging only), the `arinn-chrome-host` native host and the `arinn-plugin-chrome` browser provider. The extension is loaded unpacked from `/usr/share/arinn/chrome-extension`.
- The daemon routes `browser` providers: `Browser1` tab lists, tab activation that brings the browser window forward, and web shortcuts that open in the connected browser.
- `arinnctl shortcuts` manages web shortcuts (list, add, edit, remove, open) and `arinnctl browser` lists, activates and closes browser tabs.
- Integration test: killing the Chrome provider process leaves the daemon running and marks the browser provider unavailable (AC-10).
- Add an explicit, authenticated Home presentation request to the daemon contract
  and KDE adapter, including repeated Home while already visible. Reconnect and
  window remap remain state-only; shell consumption follows separately (#45,
  ADR-0069; AC-18 prerequisite).
- Media and library providers have a wire contract (`Media1`, `MediaProvider1`, `Library1`, `LibraryProvider1`) with optional `MediaOpen1`, `MediaTracks1`, `MediaTimeline1`, `LibraryResume1` and `LibraryImport1` ([ADR-0054](docs/adr/0054-media-and-library-wire-contract.md)).
- The shell browser screen lists daemon tabs from `org.arinn.Browser1` and activates or closes them through observed operations. A new tab is still a known web shortcut, not an arbitrary URL. See [browser tab screen](docs/architecture/ui-browser.md).
- The Downloads screen shows a fixture queue with progress, pause, resume, start now, retry and watch intents, and empty, loading, error and unavailable states, including a completion toast after import. It is not connected to Home or a download service.
- Add a fixture-driven Library screen with catalogue tabs, a poster grid, detail, missing-file badges, loading skeletons, and empty, loading, error and unavailable states. Focus stays in the screen and actions are intents only ([architecture](docs/architecture/ui-library.md)).
- Match the Library and detail frames to the handoff: a 56px title, ember-underlined pills, a seven-column poster grid with stand-in artwork, and a full-bleed detail with season pills and episode stills ([architecture](docs/architecture/ui-library.md)).
- The player screen shows a fixture OSD on a 1920×1080 surface in the handoff layout: title, clock, timeline, and subtitle, audio and more pills. A track pill opens a sheet. Hidden and visible keys follow the player section of the specification; play, pause, seek, track choice, stop and leave are signals only.
- Power and ambient overlay fixture with Sleep, Restart and Shut down. Restart and Shut down ask for confirmation, Sleep and ambient emit intents only, and a refusal stays an error.
- Power menu uses a full-screen dim scrim and a centered glass panel, with tall rows and side-by-side confirmation pills.
- Opening ambient does not also wake it. The next key or pointer move does.
- The shell can show one media session from local fixtures: title, artist, playback, position and play, pause, seek, next, previous and stop. Empty or unavailable shows no session. It does not talk to MPRIS ([session view](docs/architecture/ui-session.md)).
- Add a fixture-driven Settings screen. Categories and typed rows come from the caller; a missing playback, download or network capability is shown as unavailable; changing a value emits an intent and does not write daemon configuration ([settings view](docs/architecture/ui-settings.md)).
- Match the Settings frame to the handoff: light focus plates, an ember selection dot, and boolean switches without On/Off labels.

### Changed

- The Chrome extension opens every service in its own fullscreen window, focuses an already open service instead of opening it again, moves tabs opened from a service into their own window and keeps these windows fullscreen ([ADR-0050](docs/adr/0050-window-per-browser-activity.md)).
- The design handoff of 2026-09-29 adds the Open services screen for browser activities; global navigation gains an Open services item and Continue an "N more open" summary entry ([design](docs/design/README.md), [specification §5.1](docs/specifications/arinn-specification.md#51-home)).
- The owner decided how free browsing works on the TV: a *Site or search* screen replaces the Browser tile's ordinary Chrome window, which stays available as a secondary action. The screen keeps up to five clearable recent entries and uses a selectable search engine (default Google), and open pages can be saved as web shortcuts ([ADR-0051](docs/adr/0051-address-and-search-entry.md)). The design handoff adds the screen's states.
- Home and navigation open the Open services screen: the current service is the first Continue card, "N more open" appears when more than one service is open, and an Open services item sits under the main sections. A web-shortcut tile shows an ember dot when its origin is already open, and dims when the browser is unavailable. See [browser tab screen](docs/architecture/ui-browser.md).
- CI removes the Arch package and fails if a systemd unit, manifest or any other packaged file is left behind (G-5.3).
- The design handoff adds the Settings value editors, *Library folders* and the folder picker (#172). The owner decisions on its open questions are recorded in specification §5.6.
- Clarify agent scope for reviews, directed changes and authorized roadmap work; keep CI fixes within coordinated task zones and preserve owner review for process and product requirements.
- Consolidate verification commands in CONTRIBUTING.md, align SPDX instructions with existing REUSE coverage, and document ADR reservation collision handling.
- The design handoff of 2026-10-01 adds *Closing applications*: a tile action sheet, a Close button next to the Continue card and a separate Running applications screen, with one confirmation and a graceful close only. The owner decisions are recorded in specification §5, §5.1, §5.5 and §6.2: global navigation gains a Running item below Open services, and long-press Menu on an app tile opens the sheet instead of pinning directly ([design](docs/design/README.md), [specification §5.5](docs/specifications/arinn-specification.md#55-applications)).
- Show bounded local application icons with colored monogram fallbacks, full-chip icons on neutral backing, and light focus plates with soft glow/shadows. Keep fallbacks until Canvas pixels are ready and release previous icon sources.
- Preserve the opaque Return card: cross-source deduplication remains deferred to the explicit Return/platform-window association in [#212](https://github.com/LimeBeck/arinn/issues/212).
- `arinnctl providers` shows a table with one row per provider instead of an undivided list of fields; other list output separates records with a blank line. `--json` is unchanged.
- `arinnctl` also separates records inside nested lists, for example in `shortcuts`, `snapshot` and `diagnostics`, and marks any recovered provider error as recovered.
- The shell has one owner of keyboard focus: `Main.qml` gives focus to the screen on display, and screens move focus through `Focus.take()`, which refuses hidden or disabled items, so a screen that is not shown can never take focus from the remote (ADR-0076).
- The MVP is accepted on two platforms: KDE Plasma 6 on Wayland and an X11 desktop with an EWMH window manager, XFCE first ([ADR-0080](docs/adr/0080-mvp-platforms.md)).
- Hide the cursor on any key press in the shell and ignore the pointer for two seconds, so a resting air mouse cannot take focus from the remote. The cursor reappears only when the mouse moves; scrolling, refreshes or a screen opening under a resting pointer no longer switch to pointer mode ([ADR-0095](docs/adr/0095-shell-input-mode-ownership.md), [#308](https://github.com/LimeBeck/arinn/issues/308)).
- Update the Favorites1 API index to reflect the daemon export and the remaining shell pin/unpin work.
- Publish an immutable pre-release after every successful master CI/package run; keep master builds independent and publish ordinary releases on version tags.
- GitHub Pages publishes the whole product website (landing page, screenshot gallery and the user handbook under `guide/`) instead of the handbook alone ([#339](https://github.com/LimeBeck/arinn/issues/339)).
- Restyle the user documentation with Arinn’s charcoal palette, light typography, soft cards and restrained Ember accents; use the approved A1 Classic logo and app-icon favicon.
- Explain Arinn as an interface for a TV-connected Linux PC, add a first-use walkthrough and application, Home and download guidance, and update the current Downloads and media-key status.
- Retain one verified automatic master snapshot pre-release, skip superseded late publications and remove older snapshot release assets after checksum/ancestry checks; version releases and Git tags remain intact.
- Master snapshot packages are versioned `<version>.r<commits>.g<hash>` (for example `0.1.0.r394.g1bd20d647afe`), so pacman upgrades from one snapshot to the next and `pacman -Q arinn` names the commit; version tags still build the plain version ([#357](https://github.com/LimeBeck/arinn/issues/357), [ADR-0099](docs/adr/0099-snapshot-package-version.md)).
- Make Sentry diagnostics diagnosable without user data ([ADR-0093](docs/adr/0093-structured-sentry-diagnostics.md)):
  - Daemon operations report a fixed kind, capability, plugin id, outcome, error code and normalized reason.
  - Each failure is grouped as one issue.
  - Each operation is traced with provider-call and platform-wait spans.
  - Shell Qt/QML warnings arrive as rate-limited templates with their QML location.
  - Missing-icon noise is only counted.
- The daemon's operation completion log line now names the operation `kind` and failure `reason`.
- CI repeats less work: the Arch and Debian packages build only the shell target they install, release-mode package tests run on the nightly, manual and release builds instead of every `master` push, one Arch Linux job compiles the workspace once for the Rust lints and tests, the Qt build and the integration suite and caches Cargo dependencies and C++ objects, and a duplicate `master` run of one commit cancels the first ([#384](https://github.com/LimeBeck/arinn/issues/384), [ADR-0092](docs/adr/0092-less-repeated-ci-work.md)).
- Refresh the README, website and handbook screenshots of the current shell ([#398](https://github.com/LimeBeck/arinn/issues/398)).
- Keep CI build caches (Cargo home, target directory, ccache) on the self-hosted runner's disk instead of the GitHub cache. This also fixes the Rust job's `cargo fmt`, which picked up rustup proxies left by the Debian job in the shared runner home ([#411](https://github.com/LimeBeck/arinn/issues/411)).
- Pull requests no longer build the Arch and Debian packages; pushes to `master`, the nightly, releases and manual CI runs still build and smoke-test both ([#414](https://github.com/LimeBeck/arinn/issues/414)).
- Building Arinn requires Qt 6.8 or newer (was 6.6). CI jobs run in prebuilt Arch Linux and Ubuntu images from `ci/images/**` published on ghcr.io ([#416](https://github.com/LimeBeck/arinn/issues/416), [#413](https://github.com/LimeBeck/arinn/issues/413), [ADR-0104](docs/adr/0104-qt-floor-and-prebuilt-ci-images.md)).
- The on-screen keyboard is part of the MVP: every text field can be filled with the remote alone. On the address and search screen, the keyboard's suggestion row (recent entries and open services) replaces the web shortcut suggestions ([ADR-0103](docs/adr/0103-on-screen-keyboard.md), [#420](https://github.com/LimeBeck/arinn/issues/420)).
- The on-screen keyboard is global and part of the MVP: a separate key (`keyboard_shortcut`, default Meta+F10, and the remote's microphone button) opens it over the focused external application, which keeps focus. Text goes only into that window, never while the session is locked or its lock state is unknown, and is never logged; the suggestion row stays empty outside the shell (spec §6.2, §12.5, AC-02, AC-18, G-5.9, [#434](https://github.com/LimeBeck/arinn/issues/434)).
- The specification's on-screen keyboard delivery section ([§12.5](docs/specifications/arinn-specification.md#125-on-screen-keyboard-delivery)) now states the owner's rule of §6.2: text goes only into the window that had focus when the panel opened, not the window focused when the key is pressed. The X11 adapter's per-character focus and lock checks are named there too. No product behaviour changes.
- Specification: an application launch now succeeds when the application shows a window. While its application scope still runs with no window, the launch stays pending with no time limit and shows "Starting <app>…" with a confirmed Cancel; a scope that ends with no window fails the launch at once (spec §6.5, §7.2, §9.5, [#436](https://github.com/LimeBeck/arinn/issues/436)).
- Specification: Arinn can run as a dedicated **Arinn** login session on KDE Plasma/Wayland and XFCE/X11, with **Switch to desktop** in the power menu and a **Return to Arinn** desktop launcher (§4.2, §5.7, §18; Phase 5 gate G-5.8). Owner decision on 2026-10-10 ([#438](https://github.com/LimeBeck/arinn/issues/438)).
- The quick menu is part of the MVP: a second global key (`quick_menu_shortcut`, default Meta+F11, and the remote's Menu key) opens a side sheet over the running application with Now playing, volume and mute, network status, Sleep and the power menu, Open Home and Close Arinn. Inside the shell Menu still toggles navigation. Back returns to the same application; media and volume keys outside the shell stay with Plasma (spec §5.7, §6.2, §6.3, [#439](https://github.com/LimeBeck/arinn/issues/439)).
- Application records carry an optional icon reference: empty, a theme icon name, or an absolute local file path. The pre-stable `Application1` and `Provider1` entry signature changes from `(ssbs)` to `(ssbss)`. An empty or unusable reference keeps the neutral placeholder. No icon is fetched, and no third-party logo is bundled.
- Specification: while Arinn runs, the remote's power button opens Arinn's power menu instead of the desktop's power dialog. The desktop's own power handling is held off only while Arinn runs, never by a permanent settings change ([#481](https://github.com/LimeBeck/arinn/issues/481)).
<!-- SPDX-FileCopyrightText: 2026 Arinn contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

- Mark Phase 1 complete after verified v0.1.0 publication, full release CI, branch protection and release-asset checksums.
- Online-service tiles such as YouTube or Kinopoisk will be Arinn-owned web shortcuts that open in the browser, resolving OQ-02 ([ADR-0043](docs/adr/0043-web-shortcuts.md)). This is a specification decision; web shortcuts are not implemented yet.
- The development gallery `arinn-ui-gallery` is now a component catalogue only: the Home, Applications, navigation, Open services, Now playing, Library and Settings screen previews and their flags (`--home`, `--applications`, `--nav`, `--state`, `--browser`, `--now-playing`, `--library`, `--settings`) are removed; the gallery now rejects them, and any unknown option, with an error and a non-zero exit status. Screens and their states are rendered only from the [`tests/ui/visual`](tests/ui/visual/) fixtures; [`docs/design/current-ui`](docs/design/current-ui/README.md) is regenerated from that render on Qt 6.8.2 (#567). The README, website and handbook keep the earlier captures, moved to [`docs/design/showcase`](docs/design/showcase/README.md), until the re-shoot tracked in [#569](https://github.com/LimeBeck/arinn/issues/569).
- Refresh the public EN/RU README, website and handbook for the connected source features and current installation flow, with four fictional showcase captures from tests/ui/visual and updated capture provenance.
- Pull requests that change only documentation skip the Rust, Qt and package build jobs in CI; pushes to `master`, nightly and release runs still run the full pipeline ([ADR-0044](docs/adr/0044-docs-only-ci-scope.md)).
- Update the reviewed Cargo dependencies (libc 0.2.191, rustls 0.23.46, rustls-native-certs 0.8.5, serde_json 1.0.154, Tokio 1.54.1 and UUID 1.29.0) and GitHub Pages deploy/upload actions (v5).
- Application records carry a provider-chosen group (`online`, `games`, `media` or `other`) for grouping the Applications screen; the wire signature of an application entry changes from `(ssb)` to `(ssbs)` in the pre-stable `Application1`/`Provider1` interfaces. Unknown groups are shown as Other.
- Record the owner-confirmed design publication rights and 90-day diagnostic retention requirement for publication checklist #74; clarify that pre-releases and development snapshots already exist.
- Correct the claim that historical service-logo payloads were removed; the publication audit found them still reachable.
- Recorded owner decisions: the shell is the input layer during managed playback (ADR-0046), Plasma handles media and volume keys outside the shell, the New section is not part of the MVP, and web shortcut text entry uses a hardware keyboard in the MVP.
- The package job in CI is faster: pull requests reuse a Cargo cache and skip the release-mode `check()`; release builds stay clean.
- Phase 2 (applications) is Done: the owner qualified desktop, Steam and ES-DE launch and return on a real KDE Plasma session.
- CI spends fewer minutes: pull requests build the package only when they change packaging inputs, superseded master runs are cancelled, and jobs can run on a self-hosted runner.
- CI jobs run in containers and can be switched to a self-hosted runner that needs only Docker and git.
- Every browser activity is shown as its own fullscreen browser window, and opening an already open service focuses its window instead of adding a tab ([ADR-0050](docs/adr/0050-window-per-browser-activity.md)).
- The release and label workflows run on the switchable runner (`ARINN_RUNNER`) in containers, like the other jobs ([ADR-0053](docs/adr/0053-release-and-label-workflows-on-the-runner.md)).
<!-- SPDX-FileCopyrightText: 2026 Arinn contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

- Update the English and Russian READMEs to the screens the shell mounts: Library on `Library1`, Continue and Now playing on `Media1`, Settings on `Settings1`, and artwork through `Assets1`. Downloads and Power stay navigation entries without a mounted screen; the player on-screen display is not mounted. `Browse1` has no shell screen.
<!-- SPDX-FileCopyrightText: 2026 Arinn contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

- Refresh the English and Russian README previews with the shell screens that are wired today: Home, navigation, Library, Applications, Open services and Now playing.
- The shell browser screen shows open services as one list. The service on screen is first, OK switches to it, and its close control closes it without confirmation. See [browser tab screen](docs/architecture/ui-browser.md).
- Home favorites show daemon web shortcuts first, then application entries, until pinned favorites exist. Shortcuts open with `WebShortcuts1.Open`, including an Unavailable result when no browser provider is connected. A failed application catalog is a separate error from an empty shortcut list ([ui-web-shortcuts.md](docs/architecture/ui-web-shortcuts.md)).

### Fixed

- Opening a web shortcut no longer asks the platform to raise the browser window that had the focus before; it raises the window that shows the URL, or only hides Home for a new window.
- A stalled shell still shows "The browser is taking longer than usual" before the launch timeout dialog, instead of skipping that message.
- Arrow keys stay in D-pad mode when a hover event repeats the same pointer position.
- A Home operation result such as `Timeout` stays on screen when status is refreshed. A connection banner no longer replaces it.
- Home during a slow open hides the opening overlay and leaves the operation running. Cancel and Back still cancel it.
- Removing a media or library provider now also removes its `Media1`, `Library1` and `Assets1` objects from the daemon; before, they stayed exported at the old path.
- The mpv provider test no longer leaves files in `plugins/mpv/target/tmp`, which made the REUSE check fail on CI runners, and its socket path fits in a long checkout path.
- Settings `SetValue` operations now finish as failed with `Unsupported` when the provider cannot apply live, while still persisting the value for the next start (ADR-0065, specification section 5.6).
- The Open services screen no longer freezes with many tabs: snapshot updates change rows in place instead of rebuilding the whole list, and the focused row keeps its focus.
- The desktop provider lists `.desktop` files that repeat a key, keeping the last value as GLib does. Google Chrome's packaged entry is no longer missing, so the packaged browser fallback works.
- Retain Home's opaque Return activity without comparing unrelated window-token namespaces, and keep application-icon fallbacks visible until current Canvas pixels load; match rounded chip corners and release replaced image sources.
- Use explicit daemon Return/window associations to show one current browser service on Home after focus moves to the shell. Preserve Return on missing, ambiguous, stale or incomplete evidence, and retain independent media sessions.
- The Return card on Home no longer flickers between the open service and the generic Return while Arinn re-reads browser and window evidence.
- Restarting a provider (`arinnctl provider restart`, Settings) now starts it with the current `config.toml` settings. Before, a restart reused the settings the provider was first started with, so for example new library folders were ignored.
- The shell window requests an alpha channel, so the transparent playback layer no longer hides the video on Wayland.
- CI packages always rebuild the project's own binaries. A cached build could make a package ship outdated binaries, such as an mpv provider without managed playback.
- A daemon restart or provider reload replaces a provider that is still running a binary removed by a package upgrade, instead of leaving that process in place (#243).
- The player OSD hides 4 s after a film starts even if no key is pressed.
- Leaving the player with Back returns to where playback started: the shell stays on screen until Home is shown again, and the repeats of a held Back key no longer step on into the navigation menu.
- Continue on a stopped film resumes it from the library instead of opening a player that hangs.
- Opening the Library from the navigation menu focuses the library instead of the hidden *Back to Home* button, so arrows move through the library and Select no longer returns to Home.
- Home keeps keyboard focus when the hidden Library refreshes, and focus that falls to the window's root item returns to the screen on display, so the remote never stops responding until a mouse hovers the shell.
- A successful launch no longer leaves a permanent "Done" strip on Home: action results disappear after 3 seconds, and the strip is never shown over managed playback.
- Home → Applications: Down from a focused web shortcut now moves focus to **Edit shortcut**; Up on that button returns to the same entry (#262).
- Keep the focused row of *Site or search* in view when its rows change in the same moment as a key press; focus moves no longer scroll by positions from before the list was laid out ([#275](https://github.com/LimeBeck/arinn/issues/275)).
- Seek thumbnails in the player show the actual frame: the mpv provider now serves its extracted frames through `AssetProvider1` instead of leaving the placeholder.
- Fix X11 EWMH adapter failing on xfwm4 4.20 root `_NET_ACTIVE_WINDOW` payloads that use two CARD32 values (active window id with an ignored second word, or `0` plus the active window id).
- X11/EWMH: global Home works on xfwm4 while the shell is hidden. The shell is also found among root-window children when the window manager leaves it out of `_NET_CLIENT_LIST`; managed playback still requires a listed shell ([#283](https://github.com/LimeBeck/arinn/issues/283)).
- Make arinnctl help readable, with command descriptions, argument guidance,
  examples and exit codes. Root and subcommand help succeeds without a daemon;
  invalid usage prints a concise hint instead of a one-line command dump.
- Apply `home_shortcut` from `config.toml` to the global Home key on KDE and in the X11/EWMH adapter; editing it previously had no effect. The remote's Home (HomePage) key now also opens Home ([ADR-0094](docs/adr/0094-configurable-global-home-keys.md), [#286](https://github.com/LimeBeck/arinn/issues/286)).
- Request Home when the shell starts in an unlocked session, waiting for daemon
  and platform readiness so a successful autostart does not leave Arinn hidden.
  The startup request is not replayed after later daemon reconnects or unlocks.
- Show “No open services” when a healthy browser provider has no browser
  connected. Keep failed or missing providers and failed tab snapshots distinct
  from this normal empty presentation.
- Make the remote's play/pause and other media keys control Arinn playback. Plasma sends media keys to the current MPRIS player, so the mpv provider now exposes its session as an MPRIS player while playback runs; the MPRIS provider ignores Arinn's own players so a session is not listed twice ([ADR-0081](docs/adr/0081-mpris-export-of-managed-playback.md)).
- Fix Escape and Backspace on Open services after a Home data refresh by preventing the covered Home screen from taking keyboard focus.
- Wake the ambient screen on the first pointer movement; the first movement could previously become the reference point and be ignored ([#308](https://github.com/LimeBeck/arinn/issues/308)).
- Terminate managed playback on Stop, retain the final frame for Continue, and wait for confirmed exit before leaving playback controls. Keep unmanaged IPC peers alive on Stop, revalidate ownership on reconnect, retain failed-exit feedback, and wait for a pending library session before stopping it.
- Stop-only shutdown recovery releases the managed MPRIS name so unavailable playback does not intercept media keys.
- Stop the library scan from crashing on video files whose names contain non-ASCII letters (for example Cyrillic titles); the `1x02` episode search sliced names inside a multi-byte character.
- Synchronize the offscreen Downloads test fixture with rendered row layout before pointer input and navigation, avoiding intermittent hover, Watch-click and scroll assertions during CI.
- A key bound in `[input.bindings]` both by its name and by its `evdev:` code to two different actions is now ambiguous and keeps its normal meaning, instead of silently serving only one of the actions ([ADR-0079](docs/adr/0079-shell-input-bindings.md)).
- Open the side navigation with Menu from Library, Settings, Downloads, Open services, Browse and a media session; it now appears above them instead of staying hidden, and picking a section closes Open services, Browse or the session. The Home key now returns Home from Settings (closing an open editor without saving it), Downloads, Open services, Browse (also while typing an address) and a media session too.
- The desktop entry starts the shell again after menu exit even while the KDE or EWMH session target remains active, and reports a failed shell start instead of falsely claiming Home success.
- Repeated clean close/reopen cycles reset the shell start counter while failed shell units retain crash-loop protection.
- Leaving the library right after pressing Watch on a download no longer opens that film's Detail behind your back when you come back.
- Package upgrades restart the running Arinn user services (`arinnd`, the KWin and EWMH adapters, then the shell) of every logged-in user, so no Arinn process keeps running an old binary. The Arch package does this in a libalpm hook after the transaction, the `.deb` in `postinst`; install and removal start nothing, and playback in progress is interrupted ([#355](https://github.com/LimeBeck/arinn/issues/355)).
- The Chrome native host now exits when the Chrome provider restarts, including when the restarted provider takes over the bridge name directly, so the browser backend no longer stays unavailable until the old `arinn-chrome-host` process is killed by hand ([#356](https://github.com/LimeBeck/arinn/issues/356)).
- Home's open-services summary no longer counts a service that already has its own Continue card; it is matched by service identity, never by title, and disappears when nothing is left over ([#358](https://github.com/LimeBeck/arinn/issues/358)).
- Continue shows at most one activity and four further cards even when no service card is on screen ([#365](https://github.com/LimeBeck/arinn/issues/365)).
- Fix managed playback that sometimes stayed a decorated window after a five-second Present timeout ([#369](https://github.com/LimeBeck/arinn/issues/369)). The KWin adapter now counts command freshness from receipt, not from the start of its long poll, and always answers a command it received.
- Log why managed playback was not presented, with fixed reason names:
  - the KWin adapter's media channel attachment and presentation failures (rate-limited);
  - the daemon's Present/Release rejection and failure reasons, such as a stale media channel or a missing window;
  - the shell's choice of the session card over the player.
- Return to Arinn after leaving playback with Back on KDE: closing the player window no longer leaves the desktop on screen with Arinn hidden behind it.
- Resume a film or episode you are watching again: starting an item that was already watched to the end now clears its "watched" state, so stopping in the middle and pressing Continue resumes there instead of starting over ([ADR-0090](docs/adr/0090-rewatch-clears-completion.md)).
- Fence old Release failures when selecting a new media session, ignore stale Home snapshots during presentation, and bound the wait for Qt activation on initial playback and explicit Return without requesting focus ([#376](https://github.com/LimeBeck/arinn/issues/376)).

- Retain the playback overlay when authenticated native shell focus corroborates a missing Qt activation event, and retry unresolved window reports within the existing deadline without accepting changed window identities.

- Wait for the exact new media session when resuming a stopped title from Home, including Back before its catalog row arrives; keep Home as the return destination.

- Keep the native shell alpha channel across Home and playback cycles, with opaque shell backgrounds drawn separately from the transparent video overlay.

- Start the owned player in fullscreen so its first video window does not flash in windowed mode before the presentation handoff.

- Keep completed operation history out of live caller admission limits so repeated seeks and previews do not block Stop; suspend seek previews while the OSD is hidden, covered or stopping.
- Never list Arinn's managed player window as a running application or a Continue Activity on Home ([#376](https://github.com/LimeBeck/arinn/issues/376)). The KWin adapter keeps the window for presentation but gives it no desktop ID.
- X11/EWMH: the adapter connects when xfwm4 reports no focused window as a two-word zero `_NET_ACTIVE_WINDOW`, and the first Home reaches a shell that has never been shown, because the shell now creates its hidden native window at startup ([#379](https://github.com/LimeBeck/arinn/issues/379)).
- Without a systemd user manager, providers can register again: every `arinnd` handle now shares one supervisor, so `Register` finds the provider process the daemon started instead of failing with `NotFound`.
- Downloads: a row under the pointer takes focus when pointer mode turns on after the row's hover was checked, as other Home and Library cards already do. This also removes a flaky `Downloads` UI test ([#396](https://github.com/LimeBeck/arinn/issues/396)).
- Fix the nightly Debian package build, whose tests could not start a private D-Bus daemon, `gio` or `python` ([#405](https://github.com/LimeBeck/arinn/issues/405)).
- Home's mute and Wi-Fi icons come back after a failed status read: the shell reads `SystemStatus1` again after 1 s, 5 s and 30 s instead of waiting for the next change ([#409](https://github.com/LimeBeck/arinn/issues/409)).
- The Arch package no longer needs the newest Qt: CI builds its shell against Qt 6.8, so it starts on every system with Qt 6.8 or newer, and pacman refuses it on an older Qt instead of the shell failing at start ([#416](https://github.com/LimeBeck/arinn/issues/416), [ADR-0104](docs/adr/0104-qt-floor-and-prebuilt-ci-images.md)).
- Make Backspace work as Back on the keyboard button of a Settings text sheet, so a remote whose Back key sends Backspace can leave it ([ui-settings.md](docs/architecture/ui-settings.md)).
- Translate the Library tabs (Films, Series, Other): the shell now names them from the collection kind instead of showing the provider's English title ([#431](https://github.com/LimeBeck/arinn/issues/431)).
- An application launch now succeeds when the application shows a window, not when its launcher exits. A slow start (a game, Proton setup, shader compilation) stays pending with no time limit and shows "Starting <app>…" with Cancel after 8 s; Cancel asks for confirmation and stops only that launch's processes. An application that exits before it shows a window fails the launch at once ([#436](https://github.com/LimeBeck/arinn/issues/436)).
- Home no longer flickers while a provider publishes updates: a row updates its cards in place instead of recreating them, and the library, downloads and settings catalogs notify only when their content changed.
- Providers built on the plugin SDK no longer emit `StateChanged` when `replace` is called with unchanged entries and backend state, so a media provider polling every 250 ms stops making the daemon re-read its sessions several times a second. The daemon also emits `ProvidersChanged` for a download provider only when its runtime state, backend state, revision or error state changed, not on every one-second refresh.
- Removing a card from a Home row no longer logs an "Unable to assign [undefined]" warning.
- After an `arinnd` restart the KWin adapter resumes its `TakeCommand` polling on the new daemon within seconds, so the shell no longer reports "KWin adapter is not connected" until `arinn-kwin.service` is restarted. The adapter now waits until the new daemon serves its KWin interfaces and asks again if a call went unanswered during startup; the media channel and Return reporting no longer stay off when the first introspection came too early.
- Keep the library filesystem watcher building with Tokio 1.54.1 by registering its owned inotify descriptor through the current AsyncFd API.
- `arinnd` owns `org.arinn.Daemon1` only once it serves all of its objects, so a call made right after a daemon restart is answered instead of waiting for the 25 s D-Bus timeout, and systemd counts the daemon as ready only then. A second `arinnd` now exits on the name before it changes any state instead of taking the name over from the running daemon.
<!-- SPDX-FileCopyrightText: 2026 Arinn contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

- Show the approved Arinn brand mark in the Home menu button and navigation panel, retaining its contrast while focused.
- Refresh the English and Russian README previews with reproducible current Qt Home and navigation screenshots.
- Settings path editors check the path while you type. A relative path, a `.` or `..` folder, `//`, a control character or more than 4096 bytes shows why, and Save does nothing until the path is fixed, instead of the daemon rejecting it after Save.
- A Settings path can start with `~/` (or be `~`) for the home folder: the shell replaces `~/` with the home folder as you type and saves the full path.
- "Arinn (XFCE)" session and the opt-in XFCE autostart: after a fast re-login, Arinn now starts even while the previous login is still stopping it. The XFCE launcher (`arinn-xfce-session`) waits up to 65 s for the previous login's launcher to release `org.arinn.XfceSession` instead of exiting as a duplicate. The session entry counts Arinn as started only once its launcher confirms a started target. It logs `arinn-session: Arinn did not start; the desktop continues without it` when the launcher gives up or fails. A session-mode file with a NUL byte is no longer read as `desktop` ([#483](https://github.com/LimeBeck/arinn/issues/483)).
- Shell: the ambient screen matches handoff 20 more closely: the clock and date use the exact colours of the prototype (`#7F8083`, `#545557`), the date is 500 weight and left-aligned under the clock, and the clock has −3 % tracking on a 160 px line ([#487](https://github.com/LimeBeck/arinn/issues/487)).
- Shell: the volume HUD, including "Volume control is not available", is drawn above a web service's or an application's launch overlay instead of behind it; the power menu keeps its place ([#487](https://github.com/LimeBeck/arinn/issues/487)).
- Shell: idle ambient no longer starts while a power request waits for its answer, so a late refusal never takes focus behind ambient ([#487](https://github.com/LimeBeck/arinn/issues/487)).
- CI: the Arch and Debian package builds keep one Cargo target directory per self-hosted runner instance, so two overlapping master or nightly builds no longer test or package each other's artifacts ([ADR-0114](docs/adr/0114-per-runner-package-target-directories.md), [#489](https://github.com/LimeBeck/arinn/issues/489)).
- The daemon refuses `Background1.ReadFrame` after any `org.arinn.Shell1` owner change it has received since the frame was published, such as the reader losing the name while its read waited or releasing and retaking it, instead of handing out the frame ([ADR-0087](docs/adr/0087-bounded-activity-background-frames.md)).
- The daemon's session guard treats a `PrepareForSleep(true)` whose sender cannot be verified as logind as a sleep start; only logind's own resume ends it.
- The KWin background stage is bounded by its own screen's scale factor when the window's output matches no screen, so a HiDPI default screen no longer exceeds the 1920×1080 frame bound; the grab and the stage size round down alike.
- The KWin and X11 Activity background producers skip a `Background1.Get` reply that a newer `Changed` already overtook, so a stale attempt no longer starts a wasted capture or takes the X11 worker slot from the current one ([ADR-0087](docs/adr/0087-bounded-activity-background-frames.md)).
- The X11 producer counts its session-lock probe against the 1500 ms capture guard, so its work never runs past the daemon's unrenewed deadline.
- The Restart and Shut down confirmation no longer offers an action that the system reports as unsupported after the confirmation opened (the power menu opens before the availability is read): the action button is disabled, focus moves to Cancel, and the confirmation says "Not supported on this computer", as the menu row does ([#497](https://github.com/LimeBeck/arinn/issues/497)).
- Quick menu: Back over managed playback returns to the player also when it is pressed before the shell's first state refresh after the quick-menu key ([#507](https://github.com/LimeBeck/arinn/issues/507)).
- KDE: when the daemon stops, the remote's Menu keys are given back to applications, so the ordinary context-menu key works again while Arinn is not running ([#507](https://github.com/LimeBeck/arinn/issues/507)).
- KDE: a focus change while a new adapter connection writes the global keys no longer writes the previous quick-menu keys back in between, so Home keeps its key ([#507](https://github.com/LimeBeck/arinn/issues/507)).
- On X11, the adapter grabs no global key while the daemon is not running, so the remote's Menu keys no longer swallow the ordinary context-menu key in every application while arinnd is down. It waits for the daemon's bus name and grabs the keys again as soon as the daemon is back.
- On X11, the remote's Menu keys now follow the shell's focus within milliseconds instead of up to 0.5 s later: a change of the active window wakes the adapter at once.
- The packaged X11 adapter stays running across a daemon outage, so the keys recover when the daemon restarts; its session bus name remains released while it waits.
- The handoff visual comparison (`python scripts/ui-compare.py`) runs again: every handoff screenshot, including the closing-applications, on-screen keyboard and Settings editor frames, is mapped to a capture scene or skipped with a reason. The ambient scene draws the ambient screen instead of an empty power panel, and new capture-only scenes cover the quick menu, the volume HUD, Switch to desktop and the application launch states. The `docs` CI job now fails when a handoff screenshot has no manifest entry.
- The X11 keyboard lane's periodic lock check now starts one period after the lane starts, not at once. The Xvfb test `each_character_waits_for_its_own_lock_check` no longer fails intermittently: that first tick could land after the session opened and spend the lock check meant for the first character.
- X11/EWMH: when Arinn lets go of the power key and its repair of xfce4-power-manager's grab cannot be confirmed (the grab check fails, the restart fails or the new instance does not appear in time), the running adapter now tries again at most every 30 s, so xfce4-power-manager gets the key back within about 30 s instead of only at the adapter's next start. A slowly starting xfce4-power-manager is waited for, and if it does not come back after three restarts in a row, Arinn stops restarting it and leaves the key to the desktop. An xfce4-power-manager the user stopped is never restarted. The XFCE login restore of the power button setting holds a temporary logind power-key inhibitor when logind grants one (without one it still restores the setting, which xfce4-power-manager's own inhibitor covers), and retries of a stale setting open the system bus again when it was not available at the adapter's start ([#519](https://github.com/LimeBeck/arinn/issues/519)).
- On X11, `arinn-platform-x11 --release-power-key` (run when the adapter stops) no longer restarts xfce4-power-manager without logind's power-key inhibitor. As in the running adapter, no inhibitor means no restart: the XFCE setting still goes back, and the grab marker stays, so the next adapter start within the same login repairs xfce4-power-manager's grab.
- On X11, the on-screen keyboard key (`keyboard_shortcut`, the remote's microphone) no longer reaches the focused application now and then. The X server reports a keyboard mapping change whenever key events switch between devices (a remote, a keyboard, XTest), and the keyboard worker then released its key grabs before grabbing them again; a key pressed in between went to the application. The worker now grabs first and releases only combinations the new mapping no longer names; a new keycode another client already holds is skipped without dropping the key on its old keycode. The Xvfb test `the_lane_requests_opens_types_and_ends_on_lock` no longer fails intermittently with a stray `k`.
- The `arinnd` system-control and system-status tests no longer hang under load. They called the daemon's unique bus name right after the export, and zbus starts dispatching calls to a newly exported object in a task of its own, so a call that arrived before that task ran was dropped without a reply. The tests now wait until the daemon answers a ping, and every test call fails at its own line after 30 s instead of waiting forever. The daemon itself is not affected: clients find it by its well-known name, which it requests after every export ([#524](https://github.com/LimeBeck/arinn/issues/524)).
- On X11, the global Home, quick-menu and power keys no longer reach the focused application, or another client, now and then. The X server reports a keyboard mapping change whenever key events switch between devices (a remote, a keyboard, XTest), and the adapter then released all its key grabs before grabbing them again; a key pressed in between went to the application, and a client grabbing at that moment (a restarting xfce4-power-manager) could take it, and a stolen Home key stopped the adapter. The adapter now grabs first and releases only combinations no key names any more. A new Home or quick-menu keycode another client holds no longer drops the key on its old keycode; a contested power key is still given up as a whole.
- More `arinnd` private-bus tests now wait until the object they call is served. These are the media association, browse, settings, download and Activity background tests. Like the system-control tests in #524, they called a connection right after its first export with no round trip in between, so the first call could be dropped. The test could then hang, or a media-association pass could miss a provider's answer and fail. The `served` and `.bounded()` helpers now live in one shared test-support module ([#528](https://github.com/LimeBeck/arinn/issues/528)).
- The Xvfb tests `a_power_manager_starting_later_gets_its_grab_in_the_dedicated_session` and `the_power_grab_bookkeeping_matches_the_server_across_modes` no longer fail intermittently under load. They let another client's grab go by closing its connection and then expected the adapter's grab to succeed at once, but the X server frees such a grab only when it reads the close. They now wait for the release, or ungrab explicitly. The adapter was not affected: it retries a take that finds the power key still held.
- A late power refusal that arrives while a web service or an application is starting, before its launch screen appears, now waits for that screen to go instead of opening underneath it with focus.
- A long Settings choice sheet now opens scrolled to the current option, centred, instead of at the top of the list with the focused option out of sight ([#537](https://github.com/LimeBeck/arinn/issues/537)).
- In a Settings folder list, moving right onto the Remove button of the second or a later folder focused the first folder's Remove instead, so Select removed the wrong folder ([#540](https://github.com/LimeBeck/arinn/issues/540)).
- Shell: text in the bundled Onest font renders at its weight again. Qt matched every `font.weight` of the variable font to the 400 instance, so headings, labels and buttons looked lighter than the design; the shell now applies each weight to the font's `wght` axis ([#546](https://github.com/LimeBeck/arinn/issues/546)).
- The QML test `tst_launch.qml` no longer fails intermittently under load. Three cases sent keys or read the title too early: Home before Cancel had taken the focus, a key on the browser's slow stage after the timeout dialog had already replaced it, and the "Opening…" title after the slow stage had already begun. They now wait for Cancel's focus, leave the timeout to the case that tests it, and record the title at the moment the overlay is revealed.
- A library folder on a network share that `/etc/fstab` lists but that is not mounted now (its mount point left as an empty folder) is reported as unavailable, and its films stay in the library as unavailable, instead of going missing one by one. A folder with files, or one no fstab entry covers, is read as before ([#556](https://github.com/LimeBeck/arinn/issues/556), [ADR-0115](docs/adr/0115-unmounted-share-root-is-unavailable.md)).
- Settings polish ([#558](https://github.com/LimeBeck/arinn/issues/558)):
  - In Russian, removing a folder or path from a Settings list uses the design's verb for taking an entry out, not the verb for deleting; nothing is deleted.
  - The settings screen reads a library provider's folder availability with coalesced reads, keeping its features fresh when they change without a provider restart.
  - Only a folder list is marked with unavailable folders, not a list of files.
- Favorites (and other AppTile) names stay on one line and elide with an ellipsis instead of wrapping to two lines, matching the Home handoff.
- The bookmark control in Open services draws the handoff's `bookmark_add` and `bookmark_added` line icons (a ribbon with a plus or a check) instead of an outlined box. The shell ships no icon font; the icons are painted by `ui/LineIcon.qml`, so they do not depend on fonts installed on the device or on the Qt version.
- The `tests/ui/visual` fixtures in `capture.qml` use fictional services, titles and `.example` addresses instead of placeholders such as "Label 1" and "loc-0", with monogram chips and no third-party brands. Settings rows carry `providerName`, and Settings › Media holds more rows than fit on the screen, so the capture shows the provider line and how the list ends at the bottom edge.
- The keyboard's line-icon component is now the shared `ui/LineIcon.qml`, the one place for code-drawn icons. Open services takes an optional fixed `clockText` like Home and the player, and the captures set it, and the capture runner moves animation time by a fixed amount per state instead of the wall clock (transitions finished and loops at their start; closing, opening, stopping and saving states at the moment their handoff frame shows), so repeated captures are pixel-identical; the product keeps the live clock and real animation time.
- Correct the Open services Right-key hint to say Close instead of Save, matching the design handoff.
- Restore the launch overlay's glowing identity monogram and dark translucent backdrop over Home, including slow application starts.
- Prefer a browser's native MPRIS source over its verified Plasma proxy, restoring the proxy when the native source disappears. Let retained mpv sessions report process identity through the existing optional interface so they no longer block browser media association merely because that interface was absent. A new tab association still requires unambiguous audible evidence.
- Opening an already open web shortcut raises that service's exact window instead of the most recently focused browser window; incomplete window associations report an error instead of selecting another service.
- D-Bus activatable applications no longer sometimes fail to start from their tile: the desktop provider sends the activation itself instead of relying on `gio launch`, which could exit before the request was sent ([#59](https://github.com/LimeBeck/arinn/issues/59)).
- Collect Debian Sentry debug files after `dh_strip`/`debugedit` finalizes the packaged ELF build IDs. Keep normal Debian debug-package generation and require each collected file to match the final binary, so package verification no longer rejects a correctly built Debian package after its build ID is rewritten.
- A provider could stay in "starting" forever after a reload: two concurrent token rotations could leave the token file and the daemon's expected token different, so every registration was rejected. Rotations are now serialized ([#66](https://github.com/LimeBeck/arinn/issues/66)).
- Launching an application from Home now closes Home, so the new window is no longer opened underneath the Arinn shell.
- Switching to an open window from Arinn failed on real KDE sessions: the KWin adapter treated an idle control poll as a protocol error and reconnected every second or two, invalidating every window token. It now accepts the list types KWin delivers.
- `arinn-shell` exited silently when another shell already owned its bus name, and systemd restarted it until the start limit was hit. It now logs which process owns the name, and the unit no longer restarts it in that case.
- Returning to an Activity, or activating an open window, focuses that window before the shell stops covering the screen, so the panel does not flicker through during the transition.

## [0.1.0] - 2026-09-28

### Added

- Reusable QML UI foundation: Onest typography, design tokens, logical canvas, buttons, content cards, app tiles, progress and status components; isolated fixture gallery and offscreen Qt Quick component tests. Production screen integration remains pending.
<!-- SPDX-FileCopyrightText: 2026 Arinn contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

- Add a standalone Home preview with Continue, Favorites and New rows, global
navigation, local Home/Back and focus restoration, explicit failure/loading
fixtures, and Qt Quick navigation tests. Production provider binding is pending.
- Add the desktop application provider with bounded XDG discovery, system GIO launch in independent application units, exact window association, activation and graceful close through the production KWin platform backend. Private-bus, adapter and package fixtures cover integration and lifetime handling; real KDE/cgroup qualification remains outstanding.
<!-- SPDX-FileCopyrightText: 2026 Arinn contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

- Define the pre-stable platform window snapshot, activation and graceful-close contract, with typed protocol and SDK bindings and private-bus fixtures. Production daemon/KDE implementation remains pending (ADR-0039).
- Define the internal KWin inventory/control wire contract and SDK registration contexts for desktop-provider integration, with isolated contract and lifecycle tests. Production backend and KDE qualification remain pending (#24, G-1.5, G-2.1–G-2.3).
<!-- SPDX-FileCopyrightText: 2026 Arinn contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

- Add Home, Library and player design screenshots to the English and Russian READMEs, with captions distinguishing the target design from the implemented Qt shell.
<!-- SPDX-FileCopyrightText: 2026 Arinn contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

- The production shell now uses the shared Home and navigation design, with a Return
card, a paged Applications catalogue and asynchronous launch feedback. Navigation
remains local during operations, unchanged snapshots preserve focus, and missing
services show explicit empty, error or unavailable states.
- English [specification v0.4](docs/specifications/arinn-specification.md), consolidating developer specification v0.3 with all accepted decisions, including acceptance criteria AC-01–AC-19 and open questions OQ-01–OQ-06.
- [ROADMAP.md](ROADMAP.md) with phase status, exit criteria and evidence links.
- ADR-0028 (documentation governance), ADR-0029 (licensing), ADR-0030 and ADR-0031 (clarifications previously edited into ADR-0023 and ADR-0026), ADR-0032 (KWin script adapter), and an ADR template.
- Licensing: `GPL-3.0-or-later` for the core, `MIT OR Apache-2.0` for `arinn-protocol` and interface XML, `CC-BY-SA-4.0` for documentation; SPDX headers, `REUSE.toml`, `LICENSES/`, `deny.toml`.
- Contributor documentation: README (English and Russian), CONTRIBUTING, CLA, AGENTS/CLAUDE, SECURITY, Code of Conduct, issue and pull request templates.
- CI/CD (ADR-0033): Rust build, tests and lints; Qt build and private-bus integration suite on Arch Linux; KWin adapter tests; REUSE, cargo-deny, documentation and version checks; Arch package build with a clean-install smoke test; nightly runs against the latest Arch packages; tag-gated releases to GitHub Releases; CLA check; Dependabot.
- Arch Linux `PKGBUILD`, package smoke test and `scripts/release.py`.
- Rules and tooling for parallel development by several agents ([ADR-0036](docs/adr/0036-parallel-development.md)): changelog fragments, reserved ADR numbers with a generated register, work zones and an agent task issue template.
- Production `org.arinn.Daemon1`, application proxies and asynchronous operation objects with versioned XML, caller-bound paging, request deduplication and deadlines (ADR-0035).
- Systemd transient provider supervision, registration authentication, generation invalidation, daemon recovery and persisted restart diagnostics.
- Versioned TOML/SQLite storage, SHA-256 provider directories, ordered favorites and bounded launch recents.
- Production SDK demo fixtures and a private-bus contract suite covering recovery, limits, reload and Qt clients; package manifest and smoke-test integration.
- Phase 1 foundation: XDG plugin manifest discovery, deterministic overrides and duplicate diagnostics, bounded file parsing, executable/capability validation and a declarative reload plan.
- `arinnd --check-plugins`: offline JSON diagnostics without D-Bus or plugin execution; launch arguments are omitted from output.
- Permissive `arinn-plugin-sdk` snapshot primitives: immutable revision paging, actual D-Bus body-size checks, caller-bound expiring tokens and bounded leases. The production transport and SDK demo are integrated.

### Changed

- Agents work autonomously ([ADR-0037](docs/adr/0037-agent-autonomy-and-owner-gated-decisions.md)): they create and claim their own tasks from the roadmap, take over stale claims after 48 hours, and merge their own pull requests when all checks are green; open questions, product scope, licensing, releases, operator tests, repository administration and agent rules stay with the owner and are marked `needs-owner`. Repository labels are declared in `.github/labels.yml` and synced by a workflow.
<!-- SPDX-FileCopyrightText: 2026 Arinn contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

- Resolve OQ-03: the desktop ApplicationProvider owns browser launch; the Chrome provider declares only `browser`, with entities linked through validated platform window tokens ([ADR-0038](docs/adr/0038-browser-launch-ownership.md)). Implementation and Phase 2/3 acceptance remain pending.
<!-- SPDX-FileCopyrightText: 2026 Arinn contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

- Record the Phase 1 shell capability-boundary review and the unresolved Daemon1-only gate conflict with the direct session lifecycle guard; Phase 1 remains in progress.
<!-- SPDX-FileCopyrightText: 2026 Arinn contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

- Clarify the owner-approved shell boundary: application/provider integration uses Daemon1, while fail-closed lock/sleep observation remains daemon-independent; track platform transport isolation separately (ADR-0041).
<!-- SPDX-FileCopyrightText: 2026 Arinn contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

- Isolate asynchronous lock/sleep observation in the platform session layer while preserving daemon-independent shell hiding; add private-bus coverage for stale lock replies and absent or unresponsive daemon services (R-05, ADR-0041).
<!-- SPDX-FileCopyrightText: 2026 Arinn contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

- Update GitHub Actions for checkout, Python setup, artifact upload/download, REUSE checks and release publishing to current stable major versions.
- Make the private-bus desktop activation test wait for the application to handle dispatched activation, with a gated fixture that covers completion after the provider disconnects.
<!-- SPDX-FileCopyrightText: 2026 Arinn contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

- Record verified Phase 1 branch protection and completed platform lifecycle extraction; identify the remaining v0.1.0 release preparation and publication gate.
- Record successful Phase 1 CI and clean Arch package checks, with source links and the remaining branch-protection and maintainer release requirements.
- All living documentation translated to English. ADR-0001–ADR-0019 accepted.
- `docs/architecture/review-map.md` and `docs/architecture/product-decisions.md` folded into the specification and the roadmap.
- Shell, KDE adapter and CLI now use the production daemon API. CLI provides human/JSON output, paged diagnostics and capability commands; active Spike1 interfaces are removed.
- Documented the owner-approved parallel UI workstream alongside Phase 1: QML design system, Home, global navigation, local focus/input and fixture-based checks, with explicit API/open-question dependencies. Full UI acceptance remains in Phase 5; no implementation gate is closed by this scheduling decision.
- Specification v0.5 resolves OQ-06 with owner approval: provider data/state directories use the SHA-256 of `provider_id`, independent of `state.sqlite3` (ADR-0034). Provisioning and registry-reset recovery tests are implemented.

### Removed

- Third-party service logos from the design prototype and screenshots; replaced by neutral monograms, screenshots re-rendered from the prototype.

### Fixed

- Declare Qt SVG as a required build and Arch package dependency, and install it in clean CI environments so the UI artwork loading and pixel regression test can decode SVG fixtures. Missing Qt SVG now fails at configuration instead of during UI tests.
<!-- SPDX-FileCopyrightText: 2026 Arinn contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

- Isolate the Home navigation tests from pointer positions left by previous fixtures, preventing intermittent focus failures in CI.
<!-- SPDX-FileCopyrightText: 2026 Arinn contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

- Prevent unrelated D-Bus name changes from blocking provider registration and recovery after daemon restart.
- Ambient auto-start condition made consistent ("external Activity **or** playing media").

## Phase 0 — architecture spike (2026-09-27)

### Added

- Rust daemon `arinnd`, demo provider, `arinnctl` and a Qt Quick shell connected over session D-Bus (`org.arinn.Spike1`, experimental).
- Temporary KWin script adapter: global Home (Meta+F12), Home on the Activity's output, focus return, recovery after daemon crashes.
- Session guard hiding the shell on lock and sleep.
- Activity background experiment with `WindowThumbnail` and near/far rendering in a separate Qt process.
- Phase 0 report, gate matrix, runbook and evidence.
