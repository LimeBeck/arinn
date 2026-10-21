<!-- SPDX-FileCopyrightText: 2026 Arinn contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Downloads screen

Presentation for [specification §5.4](../specifications/arinn-specification.md#54-downloads)
and [§7.5](../specifications/arinn-specification.md#75-downloads-and-transmission).
The screen draws a generic download queue. It does not talk to D-Bus itself:
the caller supplies the snapshot and receives intents. In the shell, `Main.qml`
mounts it on the Home route `downloads` and binds it to `DownloadCatalog`
([Shell binding](#shell-binding), [#233](https://github.com/LimeBeck/arinn/issues/233)).

AC-17 also needs a real Transmission queue, library import exactly once and
playback on the device. That operator check has not been run.

## Boundary

`DownloadsView` lives in `apps/arinn-shell/ui/screens/downloads/` and is loaded
by a filesystem import (`tests/ui/tst_downloads.qml`). It reuses `Theme`,
`UiText`, `LogicalCanvas`, `ContentCard`, `ProgressTrack`, `ActionButton`,
`Skeleton` and `StatusPane` from `apps/arinn-shell/ui`. Those types are not
modified. The row's artwork slot is the screen-local `QueueArtwork`
([Summary and artwork](#summary-and-artwork)).

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
| `summaryText`, `statusTitle`, `statusDescription` | Optional copy. Empty values use generic English fallbacks; see [Summary and artwork](#summary-and-artwork) for the summary |
| `pointerMode` | Set when the pointer moves; cleared on an arrow key |
| `toastMessage` | Set when a job already on screen changes `importState` to `imported`, or by `showToast(message)` |
| `toastQueue` | Notices that arrived while another was shown or while the screen was hidden; each is shown in turn for `toastDurationMs` once the screen is on display. A notice cut short by leaving is shown again on return |
| `autoToast` | `true` (default): the view raises its own completion toast. The shell sets `false` and calls `showToast()` from `DownloadCatalog` |

`progress` below 0, or omitted, means the size is unknown: the row shows no
percentage and no track. `verifying` is "Checking…" with no track and no invented
time remaining. `completed` without `importState: "imported"` is "Done" and has
no Watch control. Watch is a shell intent, not a download-provider action, and
appears only after import.

`availableActions` selects Pause, Resume, Start now and Retry. The labels are
fixed generic strings. The focused row shows its action; other rows keep the
control hidden. Busy and disabled jobs keep focus and do not emit.

## Summary and artwork

[#360](https://github.com/LimeBeck/arinn/issues/360). Without `summaryText` the
line under the title is:

| Queue | Summary |
| --- | --- |
| At least one row `downloading` | "N downloading" (N counts those rows) |
| Every row `completed`, imported or not | "N done" |
| Anything else (queued, paused, verifying or failed rows, nothing downloading) | Hidden |
| `empty`, `loading`, `error`, `unavailable` | Hidden; the state's own pane is shown |

A hidden summary keeps its line, so the rows do not move when it appears.
Both strings are plural forms (`%n`) with Russian translations in
`apps/arinn-shell/i18n`.

The artwork slot (214 × 120, 18 px corners, handoff 13) draws the job's
`artwork` cropped to the slot. Without artwork, or until it loads, the slot
shows the handoff's striped queue plate (10 px bands at 135° under a sheen
at 155°) in neutral grey. The colours are screen-local constants, not theme
tokens. The slot has no monogram chip and invents no artwork. A row's
`artworkReady` and its plate's `showsArtwork` report what is drawn.

In the shell, `Main.qml` sets `artwork` only for an imported row: the first
library ref (`DownloadCatalog.watchTarget`, the id Watch uses) goes to
`LibraryCatalog.posterFor()`, which returns the poster of the top-level entry
`entryIdFor()` names, a film or the episode's series. That is the
`image://arinn-assets/` URL of the library card, so the image loads through
the asset image provider ([ADR-0062](../adr/0062-asset-contract.md)).
An entry without a poster leaves the plate; its fanart is not used. The
binding follows `LibraryCatalog.entries`, so a download imported before the
library lists the item gets its poster on the library's next read.
`Downloads1` and the daemon contract are unchanged, and QML makes no
provider call of its own.

## Signals

`pauseRequested(id)`, `resumeRequested(id)`, `startNowRequested(id)`,
`retryRequested(id)`, `watchRequested(id)`, `refreshRequested()` for the
error and unavailable Retry control, and `backRequested()` when the focused
item receives Escape, Backspace or the remote Back key. Back does not change
`jobs`, `viewState` or focus; the parent decides where it leads. Nothing in
the view applies action intents to the `jobs` array. A completion toast is
presentation only: the first snapshot does not toast, a job that appears
already imported does not toast, and a non-imported completion does not toast.
The notice stays for `toastDurationMs` (2600). Hover after a scroll is ignored
for `hoverSuppressMs` (500). Both durations are named constants in
`DownloadsView.qml`, not theme tokens.

## Focus

The view is its own focus scope. Up and Down move between rows and stop at the
ends. Left and Right stay on the current row. Tab wraps inside the queue.
Error and unavailable keep D-pad focus on Retry. Loading shows four `Skeleton`
rows and does not offer an action. Empty uses `StatusPane` without an action.
After a focused id disappears, focus moves to the previous index. Reordering
keeps the same id. Hover focuses a row unless a scroll in the last
`hoverSuppressMs` has suppressed it. When pointer mode turns on while the
pointer rests on a row, that row takes focus without another hover event, as
`Surface` does ([#396](https://github.com/LimeBeck/arinn/issues/396)). Keys
return to D-pad mode, and the shell's
key cooldown applies ([ADR-0095](../adr/0095-shell-input-mode-ownership.md)).
Escape, Backspace and Back emit `backRequested()` from a row, from Retry, and
from the loading scope, and stay inside this screen.

## Shell binding

`DownloadCatalog` (`apps/arinn-shell/download_catalog.{h,cpp}`) reads
`org.arinn.Downloads1` ([ADR-0067](../adr/0067-download-wire-contract.md))
asynchronously, the same way `LibraryCatalog` reads `Library1`:

- It lists the providers with the `download` capability from
  `Daemon1.Providers` whose runtime is `ready`. A provider whose backend is
  `unavailable` still serves its cached rows (ADR-0067); those rows offer no
  download command (Pause, Resume, Start now, Retry) and none is sent for
  them. Watch stays on an imported row: it opens a library item, which does
  not depend on the download backend.
- A `Providers` page over 200 entries fails the catalog.
- `GetDownloads` pages must carry the provider's generation, a non-nil UUID,
  and one revision across the pages. A continuation token must be an id
  (1–256 bytes), and a page whose D-Bus encoding is over 256 KiB fails the
  provider. The size is computed while decoding with D-Bus alignment, as
  `DownloadPage::encoded_len()` counts it. A repeated token, a torn read or a malformed row fails that
  provider, and so does a page with more than 200 rows. A row is malformed
  when it breaks a `Download` invariant of
  `crates/arinn-protocol/src/download.rs`:
  - a size or progress value without its flag, or progress outside 0..1;
  - a bad state or action token, a repeated action or more than eight;
  - an error code outside `error`, or an unknown code in it;
  - an unknown import state, library refs before `imported`, or a ref whose
    capability is not `library`;
  - repeated or empty output ids;
  - an id (job, output, ref) over 256 bytes or with NUL, or a title, error
    message or output title over 8 KiB or with NUL.
- It reloads on `Daemon1.StateChanged` with capability `download` and on
  `ProvidersChanged`. A 30 s timer is only a fallback. A read during which a
  newer notification arrived is dropped and read again instead of published,
  at most three times in a row so a busy queue still updates.
- `catalogChanged` is emitted only when the jobs or `downloadsState` changed;
  a reload that finds the same snapshot is silent.

| `downloadsState` | When |
| --- | --- |
| `loading` | Before the first read |
| `unavailable` | No daemon; no ready download provider; every backend unavailable with nothing cached; every provider answered `Unavailable` |
| `error` | The daemon's provider list could not be read, or every provider failed otherwise |
| `empty`, `populated` | The queue was read |

Rows map the contract to the view's job fields:

- `id` is a UUIDv5 of the length-prefixed provider id and the job id, so ids
  that contain separators cannot collide.
- `state` passes through; any other token becomes `unsupported`, shown as "Unsupported state".
- `progress` is −1 without `has_progress`, so no percentage appears.
- `subtitle` holds the size ("1.5 GB of 4.0 GB"). While downloading it also holds the rate and, when `eta_seconds` is not −1, the time left.
- `availableActions` lists the wire tokens, with `start-now` renamed `start_now`.
- `errorText` is the provider's message, or a generic text for its error code.

Completed rows show "Adding to the library…" while the import is `pending`.
They say that no library folder covers the download when it is `unavailable`.
Output titles and file URIs are never presented; `Downloads1` carries no paths.

`request(id, action)` sends Pause, Resume, StartNow or Retry only when the row
lists the action and no command for it is in flight. It then follows the
returned `Operation1` (31 s limit). The row is `busy` meanwhile. A failure shows
on the row ("Could not pause · Service unavailable") until the download changes
state. Either way the catalog reloads. A command belongs to the provider
generation it was sent to: when a reload shows another generation, the row is
no longer busy and that command's result is ignored.

`downloadImported(message)` fires once per download per shell session. It fires
when a download seen earlier as not imported becomes `imported`. A download
already imported on first sight does not fire. Neither does a reload or a later
re-import.

Watch calls `watchTarget(id)`, the first library ref of an imported download.
`LibraryCatalog.entryIdFor()` maps that ref to its top-level entry, a film or
the episode's series. Main then opens the library on that entry's Detail. When the library catalog
does not list the item yet, Main switches to the library, asks the catalog to
read again, and opens the Detail once a later read lists it. Leaving the
library drops that request. Back
from the screen returns Home.

## Known presentation gaps

`ProgressTrack` always fills with the shared progress colour, including while
paused. Watch uses the shared `ActionButton` focus treatment. The summary sits
under the title rather than beside it, and carries no transfer rate. The
artwork is not dimmed on unfocused rows as in the handoff. Full handoff fidelity,
3-metre TV review and AC-17 are not claimed.

## Verification

Summary and artwork (#360), local run on 2026-10-21, Qt 6.8.3, offscreen,
software backend:

- `ui-components`, `Downloads::` cases: `test_summary_line` (active, all
  done, one done, nothing active, paused and failed),
  `test_empty_queue_keeps_the_empty_state` and
  `test_artwork_poster_and_placeholder` (a data-URL portrait drawn as a
  centre crop; the neutral striped plate; the plate again after the
  artwork goes away). The summary and artwork cases fail on the previous
  view.
- `library-catalog`: `posterForUsesTheTopLevelEntrysPoster`, and
  `downloadsScreenWatchesTheImportedFilm` now checks in `Main.qml` that the
  imported row carries the film's poster URL, that the running row has none,
  and that a download imported before the library lists it gets the poster
  after the library's next read.
- The TV check was not done.

Shell binding (#233), local run on 2026-10-04, Qt 6.11.2, offscreen:

- `download-catalog` (`download_catalog_test.cpp`, private bus with a fake
  `Downloads1`) covers every state, paging and revisions, an unavailable
  backend, actions as operations, import states, the toast-once rule and the
  `StateChanged` reload: 6 cases passed.
- `library-catalog` adds `downloadsScreenWatchesTheImportedFilm`. It loads
  `Main.qml` and checks the route and focus, Pause reaching the daemon, Watch
  opening the film's Detail, and Back.
- Removing the toast-once guard, or the view's focus handoff, fails the
  matching case.
- A real Transmission run and the TV check were not done.

Earlier fixture verification:

The Qt Quick case is `tests/ui/tst_downloads.qml` (`Downloads`). The existing
`ui-components` CTest loads every `tests/ui/tst_*.qml`; this screen does not add
a CMake target. Operator tests were not run.

Local run on 2026-09-29, Qt 6.8.3, `QT_QPA_PLATFORM=offscreen` and
`QT_QUICK_BACKEND=software`. The runner is `tests/ui/main.cpp`
(`QUICK_TEST_MAIN`) with `-input tests/ui` and the `Downloads::` cases.
Result: 22 cases passed, 0 failed (Qt Test reports 24 including init and
cleanup), about 3.5 s. That includes Escape, Backspace and Back from the
queue, Retry, loading and the empty state. `python scripts/check-docs.py`
passed after the Back note.

The image's Ubuntu Qt is 6.4.2, so `find_package(Qt6 6.6)` was not configured
and `UiText`'s `font.features` does not load there. The run used a separate
Qt 6.8.3 prefix. The Home and component cases in the same runner were not
executed for this change. CI has not run yet. GPU rendering, a 3-metre TV check
and operator tests were not run.
