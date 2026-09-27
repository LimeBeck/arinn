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

With the player OSD hidden, Left/Right seek ±10 s only if seek is supported and open the OSD on the timeline; OK/Up/Down only open the OSD. With the timeline visible, OK is playPause. Back first hides the OSD; the next Back leaves the playback view of a managed mpv session with stop and resume save, and returns to the Home or Detail it came from. It never terminates an arbitrary external application. Global Home stays a platform action. (Input routing while the OSD is hidden is open: [OQ-01](../specifications/arinn-specification.md#21-open-questions).)

H/M/P/A/O are prototype development shortcuts, not required global keys. The production mapping uses semantic Home/Back/Menu/Select/media actions and is configurable per device. The letter H is never grabbed globally in the browser. The first D-pad input switches from pointer mode; hover suppression of 500 ms after scrolling stays.

Remote input ([ADR-0030](../adr/0030-phase0-machine-and-standard-input.md)): the G60S Pro emits standard keyboard and mouse events, including Backspace and Esc; no special driver, SDK or protocol is needed. Inside the shell Esc and Backspace mean Back, arrows navigate, Enter selects. In an editable text field Backspace deletes a character and does not trigger Back; Esc closes the topmost scope. Outside the shell local keys belong to the active application. Global Home is a separate configurable shortcut (Meta+F12 in the spike); global interception of Esc or Backspace is forbidden. Keyboard testing is enough for the Phase 0 input gate; the physical remote is an extra usability smoke test at deployment.

## Settings, system actions and diagnostics

UI preferences (light background, show New, language, input mapping) belong to the daemon config. Provider settings have a typed schema: id, title, description, type, current_value, constraints, enabled; types boolean, enum, string, integer, path reference. No arbitrary QML widgets or raw RPC are passed. The schema and wire interface come before the provider settings screen, not in Phase 0.

The core settings UI shows only registered capabilities; a missing playback, download or network function never pretends to succeed. System volume, network and power are executed by PlatformService. Running application names for the power confirmation come from the live registry, not a hard-coded "Kinopoisk". The confirm button never hides system inhibitor or permission errors.

## Resolved design questions

- The three equal entry modes of the design do not change the browser-first development order.
- New is a generic projection of LibraryProvider, not a product-specific local-file branch. A provider may not supply New; an empty section is hidden (API gap: [OQ-05](../specifications/arinn-specification.md#21-open-questions)).
- Prototype JavaScript state never becomes a shell backend.
- Provider errors, timeouts, the OSD and focus after list updates follow the rules above.
- Idle ambient never stops applications automatically. Automatic ambient is suppressed while an external Activity is in the foreground **or** media is playing; explicit ambient is always available. "Sleep" is OS suspend (P-04); ambient is a separate feature.
