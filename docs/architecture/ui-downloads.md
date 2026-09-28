<!-- SPDX-FileCopyrightText: 2026 Arinn contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Downloads fixture screen

Fixture presentation for [specification §5.4](../specifications/arinn-specification.md#54-downloads)
and [§7.5](../specifications/arinn-specification.md#75-downloads-and-transmission).
The screen draws a generic download queue. It does not talk to a daemon, D-Bus
or any download service, and it is not mounted from Home. Wiring it into the
shell is a separate change.

This is not AC-17. That criterion needs a real queue, library import exactly
once, and playback. Here the caller supplies the snapshot and receives intents.

## Boundary

`DownloadsView` lives in `apps/arinn-shell/ui/screens/downloads/` and is loaded
by a filesystem import (`tests/ui/tst_downloads.qml`). It reuses `Theme`,
`UiText`, `LogicalCanvas`, `ContentCard`, `ProgressTrack`, `ActionButton`,
`Skeleton` and `StatusPane` from `apps/arinn-shell/ui`. Those types are not
modified.

QML knows job states and action names (`pause`, `resume`, `start_now`, `retry`)
and the import marker `imported`. It does not know product names, protocols or
how a file was added. Titles, subtitles, status lines, artwork and error text
are opaque strings from the fixture. A missing action is hidden; Return does
not emit an intent for it.

## Properties

| Property | Role |
| --- | --- |
| `viewState` | `populated`, `empty`, `loading`, `error`, `unavailable` |
| `jobs` | Array of `{id, title, subtitle, state, progress, statusText, errorText, availableActions, importState, artwork, busy, enabled}` |
| `summaryText`, `statusTitle`, `statusDescription` | Optional copy. Empty values use generic English fallbacks |
| `pointerMode` | Set when the pointer moves; cleared on an arrow key |
| `toastMessage` | Set when a job already on screen changes `importState` to `imported` |

`progress` below 0, or omitted, means the size is unknown: the row shows no
percentage and no track. `verifying` is "Checking…" with no track and no invented
time remaining. `completed` without `importState: "imported"` is "Done" and has
no Watch control. Watch is a shell intent, not a download-provider action, and
appears only after import.

`availableActions` selects Pause, Resume, Start now and Retry. The labels are
fixed generic strings. The focused row shows its action; other rows keep the
control hidden. Busy and disabled jobs keep focus and do not emit.

## Signals

`pauseRequested(id)`, `resumeRequested(id)`, `startNowRequested(id)`,
`retryRequested(id)`, `watchRequested(id)`, and `refreshRequested()` for the
error and unavailable Retry control. Nothing in the view applies those intents
to the `jobs` array. A completion toast is presentation only: the first snapshot
does not toast, a job that appears already imported does not toast, and a
non-imported completion does not toast. The notice stays for 2.6 seconds.

## Focus

The view is its own focus scope. Up and Down move between rows and stop at the
ends. Left and Right stay on the current row. Tab wraps inside the queue.
Error and unavailable keep D-pad focus on Retry. Loading shows four `Skeleton`
rows and does not offer an action. Empty uses `StatusPane` without an action.
After a focused id disappears, focus moves to the previous index. Reordering
keeps the same id. Hover focuses a row unless a scroll in the last 500 ms has
suppressed it. Arrow keys return to D-pad mode.

## Known presentation gaps

`ProgressTrack` always fills with the shared progress colour, including while
paused. Watch uses the shared `ActionButton` focus treatment. The row still is
a `ContentCard` clipped to the queue artwork size. Full handoff fidelity,
3-metre TV review and AC-17 are not claimed.

## Verification

The Qt Quick case is `tests/ui/tst_downloads.qml` (`Downloads`). The existing
`ui-components` CTest loads every `tests/ui/tst_*.qml`; this screen does not add
a CMake target. Operator tests were not run.

Results of the offscreen software run are recorded in the pull request for this
change. Until that run is attached, treat this section as not yet verified.
