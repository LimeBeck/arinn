<!-- SPDX-FileCopyrightText: 2026 Arinn contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Phase 3 runbook: browser operator test

The owner follows this runbook on a real desktop to qualify the
[Phase 3 exit criteria](../../ROADMAP.md#phase-3--browser): AC-03, AC-04, AC-05
and AC-06, with spot checks of AC-10 and G-3.1, which already pass in CI. It
also checks the window-per-activity rules of
[ADR-0050](../adr/0050-window-per-browser-activity.md), the window link of
[ADR-0052](../adr/0052-browser-window-association.md) and the closed-browser
launch of [ADR-0055](../adr/0055-launching-a-closed-browser.md). G-5.5 repeats
this acceptance on an X11/EWMH desktop, so every step runs on **KDE Plasma 6 on
Wayland** and on **XFCE on X11**; [Platform differences](#platform-differences)
lists where the two differ.

Requirements: [§5.1](../specifications/arinn-specification.md#51-home),
[§5.2](../specifications/arinn-specification.md#52-global-home-overlay),
[§6.3](../specifications/arinn-specification.md#63-home-and-back),
[§7.1](../specifications/arinn-specification.md#71-browser-chromechromium),
[§12](../specifications/arinn-specification.md#12-platform-integration-kde-plasma-6--wayland),
[§13](../specifications/arinn-specification.md#13-security) and
[§19](../specifications/arinn-specification.md#19-acceptance-criteria).
Implementation: [plugin runtime](plugin-runtime.md#chrome-implementation-2026-09-28-82),
[browser tab screen](ui-browser.md) and [X11/EWMH platform](x11-ewmh.md).

Results go into an evidence file made from the
[Phase 3 evidence template](evidence/phase-3-template.md). This runbook does not
mark any criterion passed; the owner records the result in `ROADMAP.md`.

| Criterion or decision | Steps |
| --- | --- |
| AC-03 Chrome or an online service opens from Home | AC03-1 to AC03-5 |
| AC-04 Existing tabs are shown and switchable | AC04-1 to AC04-7 |
| AC-05 Global Home over Chrome | AC05-1 to AC05-6 |
| AC-06 Return to the same browser activity | AC06-1 to AC06-6 |
| AC-10 Chrome provider crash (spot check) | AC10-1 to AC10-4 |
| G-3.1 Extension permissions, native host, reconnect (spot check) | G31-1 to G31-3 |
| ADR-0050 window per activity, fullscreen, DRM | AC03-2 to AC03-5, W-1 to W-4, AC04-3, AC04-6 |
| ADR-0052 window link | AC04-3, AC06-1, AC06-2, AC06-4 |
| ADR-0055 launching a closed browser | AC03-1 |
| G-5.5 repeat on XFCE/X11 | All steps, with [Platform differences](#platform-differences) |

## Safety

- **Only the owner runs this runbook.** Agents never run it
  ([AGENTS.md](../../AGENTS.md), hard rules). It controls the real desktop,
  starts and closes browser windows and kills the Chrome provider process.
- **Use a disposable session.** Prefer a separate test user account, or at least
  a separate Chrome profile that holds no personal tabs, logins or history.
  Close windows with unsaved work first. Closing a row in Open services closes
  that browser window without confirmation.
- **Privacy ([§13](../specifications/arinn-specification.md#13-security)).**
  Evidence never contains real URLs, tab titles, account names, screenshots of
  personal pages or personal paths. Use the synthetic fixture shortcuts below,
  which open public documentation pages. For a DRM service, record only the
  packaged shortcut name or "owner subscription", never the page address or
  the title of what was played. `arinnctl browser tabs` prints titles and
  addresses: count its rows, do not paste its output.
- **Nothing else is changed.** No step locks the screen, changes the monitor
  layout, resolution or scale, or loads a development KWin script. This runbook
  uses the installed package and none of the `tests/integration/kwin_*.py`
  scripts; those target the development launcher of the
  [Phase 0 runbook](phase-0-runbook.md).

## Prerequisites

### Packages

- The `arinn` Arch package built from the commit under test (a
  [pre-release](https://github.com/LimeBeck/arinn/releases) or a local
  `makepkg`, see [CONTRIBUTING.md](../../CONTRIBUTING.md#required-checks)). After
  an update, log out and in again, or restart the session target.
- Google Chrome or Chromium, the two browsers in the packaged
  `/usr/share/arinn/browser-applications.toml`.
- Python 3, for the counting one-liners below.
- **KDE:** Plasma 6 on Wayland with a systemd user session.
- **XFCE:** XFCE on X11 with xfwm4 compositing enabled and a running lock
  service (`xfce4-screensaver` or another `org.freedesktop.ScreenSaver`
  provider). The EWMH adapter fails closed without a lock API
  ([X11/EWMH platform](x11-ewmh.md#home-return-and-video)).

### Managed Chrome and the extension

1. Start the browser with the test profile. Open `chrome://extensions`, enable
   *Developer mode*, choose *Load unpacked* and select
   `/usr/share/arinn/chrome-extension`. After a package update press *Reload*
   on the extension card.
2. The card shows the ID `eajlncomibfmpmkhkbcdmajmkpfaaibf`. *Allow in
   Incognito* stays off.
3. Load the extension in one profile only, so one browser connection exists.
4. Quit the browser completely (all windows; on Chromium also check that no
   background process remains: `pgrep -x chrome` or `pgrep -x chromium` prints
   nothing).

### Configuration

`$XDG_CONFIG_HOME/arinn/config.toml` needs no change for this runbook:

- `browser_application` stays empty, so the daemon launches the first
  launchable packaged candidate (ADR-0055). Set it to a desktop-file ID only
  when the browser under test is not a packaged candidate, and record that.
- `home_shortcut` stays at the default Meta+F12; the remote's HomePage key
  works as well ([ADR-0094](../adr/0094-configurable-global-home-keys.md)).
  Record any other value.

### Fixture shortcuts

Add two synthetic web shortcuts on public documentation pages with distinct
origins, and note their IDs from `arinnctl shortcuts list`:

```sh
arinnctl shortcuts add "Fixture A" https://example.org/
arinnctl shortcuts add "Fixture B" https://www.iana.org/help/example-domains
arinnctl shortcuts list
```

Pin both to Home favorites if they do not appear there (Menu on the tile). The
DRM checks use one packaged online-service shortcut the owner has a
subscription for.

### Start the session

**KDE Plasma 6 / Wayland:**

```sh
systemctl --user start arinn-session.target    # or enable it and log in again
```

**XFCE / X11:** use the persistent autostart template or the manual recipe in
[X11/EWMH opt-in setup](x11-ewmh.md#opt-in-setup). The KDE target and the EWMH
target conflict; run only one.

### Pre-flight

```sh
arinnctl --version
arinnctl --json status
arinnctl providers
pacman -Q arinn
```

Expected: `platform_connected` is `true` and `home_visible` is `true` (Home
shows after start). `providers` lists `chrome` and `desktop` as `ready`; with
the browser closed the `chrome` backend is `unavailable`. Record the build id,
the package version, the browser version (`chrome://version`, the version line
only), the desktop and its version, the session type and the input devices
used.

Count tabs without printing them:

```sh
arinnctl --json browser tabs | python3 -c 'import json,sys; print(len(json.load(sys.stdin)))'
```

## Procedures

Each step names its ID in the evidence template. Use the remote, or a keyboard
that sends the same keys (arrows, Enter, Esc/Backspace, Meta+F12/HomePage,
Menu). `arinnctl home` and `arinnctl back` are fallbacks; a step that used
them instead of a key records that in its notes. A step fails when any
expected result does not hold; write what happened instead.

### AC-03: a service opens from Home

| ID | Action | Expected result |
| --- | --- | --- |
| AC03-1 | The browser is closed. On Home, select the Fixture A tile. | Within 30 s the browser starts and Fixture A shows in a fullscreen window with no tab strip, address bar or frame. Home hides. A launch overlay may show after 400 ms and adds "taking longer" after 8 s. Record the approximate time and whether the browser also opened its own start-up window (an accepted consequence of ADR-0055). |
| AC03-2 | The browser is running. Press Home, select the Fixture B tile. | A new fullscreen window shows Fixture B. Fixture A's window gets no extra tab. Home hides. |
| AC03-3 | Press Home. The Fixture A tile carries the "open" dot. Select it. | Fixture A's existing window comes to the front; no new window opens. The tab count is unchanged. |
| AC03-4 | Press Home, select the DRM online-service tile. Sign in with the test account if asked, start any title and let it play for 30 s. | The service opens in its own fullscreen window and protected playback starts (no DRM or "unsupported browser" error). |
| AC03-5 | Applications › Online (or a Home favorite): launch the browser application itself. | An ordinary browser window with the normal browser UI opens. It is not forced to fullscreen. |

**Window rules (ADR-0050).** Run these in the windows opened above.

| ID | Action | Expected result |
| --- | --- | --- |
| W-1 | In the Fixture A window, Ctrl+click (or middle-click) a link. | The new tab moves into its own fullscreen window; Open services lists it as its own row. |
| W-2 | Take a managed window out of fullscreen with the window manager (KDE: Alt+F3 › More Actions › Fullscreen; XFCE: Alt+F11). | The window returns to fullscreen within about a second. |
| W-3 | In the DRM service, enter the player's fullscreen and leave it with Esc. | The window stays fullscreen without browser UI. Esc reaches the page; Arinn does not intercept it ([§6.2](../specifications/arinn-specification.md#62-keys)). |
| W-4 | Optional: a page opens its own popup (`window.open`). | The popup is kept fullscreen. Record what happened; the isolated check did not cover it. |

### AC-04: existing tabs are shown and switchable

Start with Fixture A, Fixture B, the DRM service and the W-1 window open, plus
the ordinary window from AC03-5.

| ID | Action | Expected result |
| --- | --- | --- |
| AC04-1 | Press Home. | The service on screen is the first Continue card. With more than one browser activity, Continue has an "N more open" (or "Open: N") entry ([§5.1](../specifications/arinn-specification.md#51-home)). |
| AC04-2 | Menu › Open services. | Every open tab is one row; the one on screen is first and marked "On screen". The row count equals the tab count from the one-liner. |
| AC04-3 | Select a row that is not on screen. | That window comes to the front with the same page; Home hides. The window that was on screen before is not raised instead. |
| AC04-4 | Press Home, open Open services again. | The row from AC04-3 is now first and "On screen"; the others follow last use. |
| AC04-5 | Let the DRM service play sound; open Open services. | Its row carries "Sound". |
| AC04-6 | Close the W-1 row with its close control. | No confirmation; the window closes and the row collapses. The tab count drops by one. |
| AC04-7 | Close every managed window from Open services, then quit the browser. | Open services shows "No open services" with Home and Site or search; no "Browser unavailable" error. Home shows no browser Continue card. The fixture tiles lose the "open" dot. |

Reopen Fixture A, Fixture B and the DRM service from Home before the next
section.

### AC-05: global Home over Chrome

| ID | Action | Expected result |
| --- | --- | --- |
| AC05-1 | Focus the DRM service while it plays (click into the page). Press the Home key on the remote, then repeat with Meta+F12 on the keyboard. | Arinn Home appears over the browser on the same screen each time. The browser keeps running. Arinn does not pause playback ([§5.2](../specifications/arinn-specification.md#52-global-home-overlay)); record whether the page paused itself. |
| AC05-2 | Put the keyboard focus in a text field of a page (for example a search field). Press Home. | Home appears. Esc and Backspace typed into the page before that stayed with the page. |
| AC05-3 | With Home shown, press Home again. | Home stays, focus moves to Return; it does not toggle back to the browser ([§6.3](../specifications/arinn-specification.md#63-home-and-back)). |
| AC05-4 | In the DRM player's own fullscreen, press Home. | Home appears above the video. |
| AC05-5 | In the ordinary browser window, press Home. | Home appears. |
| AC05-6 | Ten cycles: Home, then Back, over the DRM service. | Ten of ten show Home and come back; no second shell window appears. Record the count. |

### AC-06: Return to the same browser activity

| ID | Action | Expected result |
| --- | --- | --- |
| AC06-1 | From AC05-1, run `arinnctl --json status` and note `return_window`. Press Back. | The same window and tab come back with the same page and scroll position and the same playback state. `active_window` now equals the noted `return_window`. |
| AC06-2 | Focus Fixture B, press Home, then Back. Repeat with Fixture A. | Each time exactly the window that was on screen comes back, not the most recently opened one. |
| AC06-3 | Focus Fixture A, press Home, select the current-service Continue card. | Fixture A comes back. |
| AC06-4 | Close Fixture B from Open services. On Home, select the Fixture B tile, which creates a new window. As soon as it shows, press Home, then Back. | The new window shows in front of Home, not behind it, and Back brings it back. A window gets its platform link only after 0.5 s of focus (ADR-0052); record any other window that comes back instead. |
| AC06-5 | Focus Fixture B, press Home, close the Fixture B row in Open services, go back to Home and press Back. | Home stays and briefly shows the activity as unavailable; the next Back opens global navigation ([§6.3](../specifications/arinn-specification.md#63-home-and-back)). |
| AC06-6 | Focus the ordinary browser window, press Home, then Back. | The ordinary window comes back with its browser UI. |

### Spot check: AC-10 (passes in CI)

With Fixture A and the DRM service open and Home shown, kill the Chrome
provider process only:

```sh
arinnctl --json status | python3 -c 'import json,sys; print(json.load(sys.stdin)["epoch"])'
arinnctl --json providers | python3 -c 'import json,sys; p=next(p for p in json.load(sys.stdin) if p["id"]=="chrome"); print(p["generation"], p["restarts_since_daemon_start"])'
unit=$(arinnctl --json providers | python3 -c 'import json,sys; p=next(p for p in json.load(sys.stdin) if p["id"]=="chrome"); print("arinn-provider-"+p["path"].rsplit("p_",1)[1]+".service")')
systemctl --user kill --signal=SIGKILL "$unit"
```

| ID | Action | Expected result |
| --- | --- | --- |
| AC10-1 | Watch Home for one minute after the kill, moving focus with the arrows; then run the first command again. | The shell stays up and focus moves without delay. Home may show "Browser unavailable" briefly. The daemon epoch is unchanged. |
| AC10-2 | Run the second command again, then `arinnctl providers`. | `chrome` is `ready` again with a new generation and a higher restart count. The browser windows were not closed. |
| AC10-3 | Open Open services. | Within about 30 s (the extension's capped backoff) the tabs are listed again. |
| AC10-4 | Select the DRM service row, press Home, then Back. | The window comes to the front and comes back after Home. |

### Spot check: G-3.1 (passes in CI)

| ID | Action | Expected result |
| --- | --- | --- |
| G31-1 | `python3 -c 'import json; m=json.load(open("/usr/share/arinn/chrome-extension/manifest.json")); print(sorted(m.get("permissions",[])), m.get("host_permissions"), m.get("content_scripts"))'` | `['nativeMessaging', 'tabs'] None None`. The extension card lists no site access. |
| G31-2 | Show `allowed_origins` of the native host manifest: `/etc/opt/chrome/native-messaging-hosts/org.arinn.chrome_host.json` (Chrome) or `/etc/chromium/native-messaging-hosts/org.arinn.chrome_host.json` (Chromium). | Exactly `chrome-extension://eajlncomibfmpmkhkbcdmajmkpfaaibf/`. |
| G31-3 | Reconnect, observed in AC10-3. | The extension reconnected after the provider restart; no command ran twice (no window opened or closed by itself). |

The native host frame limits (1 MiB to the browser, 8 MiB from it) are checked
only in CI.

## Platform differences

| Topic | KDE Plasma 6 / Wayland | XFCE / X11 (EWMH adapter) |
| --- | --- | --- |
| Session start | `arinn-session.target` | `arinn-ewmh-session.target` through the autostart template or the manual recipe ([setup](x11-ewmh.md#opt-in-setup)) |
| Global Home | KWin action through KGlobalAccel. Check System Settings › Shortcuts if Meta+F12 does nothing. | Passive X key grabs. A key another client already grabs is skipped and logged; check that XFCE's keyboard shortcuts do not use Super+F12 or HomePage. |
| Window placement | `active_output` and `home_output` name the screen; they must match. | Output names are empty (no RandR placement yet); check the screen by eye. |
| Focus stealing | KWin focus-stealing prevention decides whether a new browser window takes the focus while Home is shown (ADR-0052). | xfwm4's focus policy decides the same; record any window that opens behind Home. |
| Leaving fullscreen (W-2) | Alt+F3 window menu | Alt+F11 or the window menu |
| Receded background behind Home | Last frame of the browser window ([§12.3](../specifications/arinn-specification.md#123-activity-background)) | Not implemented yet; a plain background is expected and is not a failure here |
| Lock service | Plasma's | Required; the adapter fails closed without one |

KDE Plasma 6 on X11 ([ADR-0077](../adr/0077-kde-x11-session.md)) is optional
and is not part of G-5.5; run it only if wanted and record it as a separate
platform.

## Recording and cleanup

1. Fill one evidence file per platform from the
   [template](evidence/phase-3-template.md) and save it as
   `docs/architecture/evidence/phase-3-<platform>.json`.
2. The owner updates the Phase 3 rows of `ROADMAP.md` (and G-5.5 for the X11
   run) with the result and a link to the evidence, in a pull request of their
   own. A failure becomes an issue with the step ID; a failed ADR-0050 model on
   the real desktop triggers its fallback, which needs a new ADR.
3. Remove the fixtures: `arinnctl shortcuts remove <id>` for Fixture A and B.
   Close the test windows, stop or disable the session target as wanted, and
   delete the test profile or user if it was only for this run.
