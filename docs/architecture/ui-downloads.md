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
`hoverSuppressMs` has suppressed it. Keys return to D-pad mode, and the shell's
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
paused. Watch uses the shared `ActionButton` focus treatment. The row still is
a `ContentCard` clipped to the queue artwork size. Full handoff fidelity,
3-metre TV review and AC-17 are not claimed.

## Verification

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
