# Activity and interaction state

Part of [ADR-0022](../adr/0022-state-and-interaction-baseline.md), 2026-09-27. Product answers P-01–P-08 are in [specification §4–§6](../specifications/arinn-specification.md).

## Activity and Continue

Activity descriptor: opaque `id`, `owner EntityRef`, `generation`, `kind` (application, browser, media, content), `title`, `artwork_ref`, `status` (active, paused, resumable, unavailable), `available_actions`, optional `parent_ref`, `related_refs`, `platform_window_token`, `last_interaction_at`. The core never analyses URLs or titles to link entities.

A provider owns the external entity; the daemon builds the aggregated Activity from descriptors. The desktop provider gives a fallback application activity, the browser a tab activity, media a playback activity. Only an explicit ID link or a validated platform window token merges them into one group. Two windows are never merged because their titles match, nor two media sessions because of the film name. If the link is unknown, separate entities are better than resuming the wrong one.

The foreground Activity is the platform window plus the active provider entity inside it. Display precedence within a proven group: explicit content playback → browser tab → generic application. Media controls attach to the card through a related media ref. Initial UI projection: one foreground or last running Activity plus up to four resumable content entries, as in the design. The card limit does not limit the registry. Other windows and tabs are reachable through Applications or Browser.

Arinn stores the order of its recent launch targets; providers store media progress, watched state and library state. The design rule "finished 90 s before the end" is applied by the provider if the media integration supports such a policy; it never marks arbitrary browser video as watched. See [media MVP](media-mvp.md).

If the Activity disappears while Home is open: remove the live card and move focus to the nearest available item in the same row, then Favorites, then navigation. Returning to a vanished entity returns `Unavailable` and keeps Home open. Without an Activity, Home never tries to activate the desktop in some random way.

## Local UI and pending operations

Focus scopes, input modes, per-screen focus memory, OSD visibility and timeline preview belong to the shell. Provider state is never a precondition for a focus change. After an item is removed, focus goes to the nearest previous index; closing a modal or sheet returns focus to its opener, or, if the opener is gone, to the nearest available focusable in the parent scope.

The 1100 ms launch animation is a visual cycle, not guaranteed completion. If the operation is still pending, a calm loading state continues; if the target Activity is already available, the UI switches to it. An error opens the error state immediately. The 1.9 s startup animation never holds back a degraded Home when the backend is unavailable. No fake success after a timer.

With the player OSD hidden, Left/Right seek ±10 s only if seek is supported and open the OSD on the timeline; OK/Up/Down only open the OSD. With the timeline visible, OK is playPause. Back first hides the OSD; the next Back leaves the playback view of a managed mpv session with stop and resume save, and returns to the Home or Detail it came from. It never terminates an arbitrary external application. Global Home stays a platform action. During managed playback the shell keeps focus as a transparent input layer over the mpv window and sends only commands to the provider ([ADR-0046](../adr/0046-shell-input-layer-during-playback.md)).

H/M/P/A/O are prototype development shortcuts, not required global keys. The production mapping uses semantic Home/Back/Menu/Select/media actions and is configurable per device. The letter H is never grabbed globally in the browser. Any key press switches from pointer mode and blocks pointer takeover for 2 s; the cursor then stays hidden until the pointer moves ([ADR-0095](../adr/0095-shell-input-mode-ownership.md)). Hover suppression of 500 ms after scrolling stays.

Remote input ([ADR-0030](../adr/0030-phase0-machine-and-standard-input.md)): the G60S Pro emits standard keyboard and mouse events, including Backspace and Esc; no special driver, SDK or protocol is needed. Inside the shell Esc and Backspace mean Back, arrows navigate, Enter selects. In an editable text field Backspace deletes a character and does not trigger Back; Esc closes the topmost scope. Outside the shell local keys belong to the active application. Global Home is a separate configurable shortcut (`home_shortcut`, default Meta+F12, plus HomePage; [ADR-0094](../adr/0094-configurable-global-home-keys.md)); the quick menu has its own configurable shortcut (`quick_menu_shortcut`, default Meta+F11) plus the remote's Menu key, which is bound only while another window has focus, so that Menu keeps its meaning inside the shell ([ADR-0106](../adr/0106-quick-menu-presentation.md)); global interception of Esc or Backspace is forbidden. Keyboard testing is enough for the Phase 0 input gate; the physical remote is an extra usability smoke test at deployment.

## Quick menu

The quick menu (spec §5.7, [ADR-0106](../adr/0106-quick-menu-presentation.md), [UI notes](ui-quick-menu.md)) is a side sheet over what is on screen. The platform handles its key like the Home key: the unlocked-session check, the remembered Return Activity and output and the raised shell are the same, and only the request differs (`ReportQuickMenuRequest` → `QuickMenuRequested`). `Status` reports Home visible either way; whether the shell shows Home or the sheet is shell-local state.

- **Over an Activity** (the shell was not on screen with focus when the request arrived, or managed playback was on screen) the shell shows no screen. Behind the sheet is the Activity's last frame when the Activity background has one (§12.3), otherwise the live window through the transparent shell window. Back calls `Daemon1.Back`, the ordinary Return path, so the same Activity is activated. Until the platform hides the shell, no screen is drawn; a Return that does not happen within 3 s leaves Home on screen.
- **Over the shell** the sheet opens on top of the current screen. Back closes it and focus returns to its opener.
- **Open Home**, an in-shell Home key and an explicit `HomeRequested` replace the sheet with the fullscreen Home, focus on Return. A repeated quick-menu key keeps the sheet open.
- **Degraded:** without the daemon the daemon-backed rows are disabled and say so; Open Home and Close Arinn stay. Back then shows Home, because no Return is possible.
- A lock, a dismissal or Return hides the shell and ends the sheet; it never comes back by itself.

Now playing is the playing media session, otherwise a paused one; OK sends play/pause to it. Network status comes from `SystemStatus1`. The Sound row reads and changes the default output through `SystemControl1` ([system control](system-control.md)) with the volume keys' semantics: OK toggles mute, Left steps down, Right turns the sound back on and steps up; until the platform answers it shows the `SystemStatus1` mute state and no level. Sleep and Power open the shell's power scope ([power menu](ui-power.md)) over the sheet, with its confirmations, in-flight rules and refusals; the Activity stays behind, and Back returns to the sheet's row (§6.1, §6.3). Sleep sends the suspend request at once; a request already in flight is shown as it is. During ambient the quick-menu key only wakes (§5.7 *Ambient*).

## Settings, system actions and diagnostics

UI preferences (light background, language, input mapping) belong to the daemon config. Provider settings have a typed schema: id, title, description, type, current_value, constraints, enabled; types boolean, enum, string, integer, path reference. No arbitrary QML widgets or raw RPC are passed. The schema and wire interface come before the provider settings screen, not in Phase 0.

The core settings UI shows only registered capabilities; a missing playback, download or network function never pretends to succeed. System volume, network and power are executed by PlatformService. The daemon exposes power and volume to the shell as `SystemControl1` ([system control](system-control.md)). Running application names for the power confirmation come from the live registry, not a hard-coded "Kinopoisk". The confirm button never hides system inhibitor or permission errors.

## Resolved design questions

- The three equal entry modes of the design do not change the browser-first development order.
- New is not part of the MVP (owner decision on 2026-09-28). If a later version adds it, it is a generic projection of LibraryProvider, not a product-specific local-file branch.
- Prototype JavaScript state never becomes a shell backend.
- Provider errors, timeouts, the OSD and focus after list updates follow the rules above.
- Idle ambient never stops applications automatically. Automatic ambient is suppressed while an external Activity is in the foreground **or** media is playing; explicit ambient is always available. "Sleep" is OS suspend (P-04); ambient is a separate feature.
