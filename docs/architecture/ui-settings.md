<!-- SPDX-FileCopyrightText: 2026 Arinn contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Settings view

Presentation for [specification §5.6](../specifications/arinn-specification.md#56-settings),
bound to `org.arinn.Settings1` ([ADR-0065](../adr/0065-provider-settings-contract.md)).
The behaviour rules live in the specification and in
[Activity and interaction](activity-and-interaction.md#settings-system-actions-and-diagnostics).
This screen does not close a Phase 5 gate.

`SettingsCatalog` (`apps/arinn-shell/settings_catalog.cpp`) reads the daemon on
the session bus. It calls `ListProviders` and `GetSettings`, groups the rows by
the design categories, and calls `SetValue` as an operation. `SettingsChanged`
and `ProvidersChanged` reload the snapshot; a missing daemon is unavailable.
`catalogChanged` is emitted only when the rows or the screen state changed.
It also reads library folder availability; see *Unavailable library folders*
below.
`Main.qml` binds that model to `SettingsView` when Home's screen is Settings.
The QML does not call a daemon, a plugin process or a provider-supplied widget
(R-03, R-04). Values are not logged.

## The shell's own preferences

The daemon's `arinn` source also holds the shell's presentation preferences
([ADR-0112](../adr/0112-shell-preferences-as-core-settings.md),
[ADR-0136](../adr/0136-hearth-light-visual-direction.md)): `light_field`
(Appearance › Light field, on by default), `light_follows_time` (Appearance ›
Light follows the time of day, on by default) and `ambient_timeout` (Appearance ›
Ambient after: 5, 10, 15, 30 or 60 minutes, 10 by default). They are ordinary
rows on this screen. `SettingsCatalog` also exposes them as `lightField`,
`lightFollowsTime` and `ambientTimeoutMinutes` for `Main.qml`, which binds the
light field to `lightField` and the time-of-day light to
`lightFollowsTime`. An invalid hand-edited value of any of these keys in `config.toml`
reads as its default, with one warning line and no value in the log. The
same source lists `browser_application` and, when search engines are
packaged, `search_engine` under Applications. The daemon checks its own schema
with the same rules as a provider schema, so every row has a category the
screen knows (#682). The titles, descriptions and option titles of the `arinn`
source are shown translated from the shell's catalogue. For every other
provider `SettingsCatalog` reads `Settings1.GetTranslations` after
`GetSettings` and shows each title, description, option title and integer
unit from the provider's table for `QLocale().name()` (`ru_RU`), then for its
language (`ru`), else the provider's English source text
([ADR-0129](../adr/0129-provider-setting-translations.md)). A daemon without
the method, an error or a table from another generation leaves the source
texts. The library, managed player and Transmission providers ship Russian
texts.

## What the view receives

Category titles are model data. The QML does not embed those names or any
product name. `SettingsCatalog` publishes the design categories that have at least one row,
in the design order, once a snapshot has loaded, so the screen opens on the
first category with rows. A category with no rows is not listed: a missing
playback, download or network setting is not shown as a working control, and
there is no empty pane ([#764](https://github.com/LimeBeck/arinn/issues/764),
#756 B16). A reload keeps the selected category by id, else the nearest
index; when the focused row's category leaves the list, focus moves to that
nearest category.

| Field | Role |
| --- | --- |
| Category `id`, `title` | Left-hand list entry, in the given order |
| Category `icon` | Optional mark name (`video_library`, `play_circle`, `volume_up`, `wifi`, `apps`, `palette`, `memory`, drawn as the filled 38 px Material-style icons of handoff se-lf-1; `memory` is the gear the handoff shows for System); anything else is a plain mark |
| Row `providerId`, `providerName` | Provider that declared the setting. The name is a caption on the focused row only; at rest a row is label and description as in handoff 14 (owner decision on #566, 2026-11-01). `providerName` is empty for Arinn's own settings (`arinn`), so they have no source line, and the names of the providers this repository ships (`Local library`, `Local music`, `Playback`, `Downloads`, `Desktop applications`, `Media sessions`) are translated by the shell when both the plugin id and the name match. The plugin id comes from `Daemon1.Providers`, because `Settings1.ListProviders` carries only the instance id; until that listing answers, after the daemon changes owner, and when a new listing fails, the instance id stands in. Any other name is shown as the provider sends it (#756 B16) |
| Row `id`, `categoryId`, `title`, `description` | One setting |
| Row `type` | `boolean`, `enum`, `string`, `integer` or `path` |
| Row `currentValue` | Current value from `GetSettings` |
| Row `constraints` | `options` (`value`, `title`) for an enum; `min`, `max`, and when present `step` and `unit`, for an integer; `maxLength` for a string; `directory`, `multiple` and `maxItems` for a path |
| Row `enabled` | When false, the row is shown and cannot be edited. The detail line is the generic "Cannot be changed now"; the contract carries no reason |
| Row `pending` | A `SetValue` operation is still running |
| Row `deferred` | The operation error was `Unsupported`. The value is stored and applies at the provider's next start, whether the operation finished as succeeded or failed. The note stays until that provider's generation changes, and it is forgotten when the shell restarts |
| Row `unavailablePaths` | Only on a path row with `multiple` and `directory`: the folders of `currentValue` that the provider of that row reports unavailable through `Library1.GetRoots`, matched by path as written. Empty when nothing is reported |
| Row `error` | The daemon's error text, or empty. `Timeout` is the shell's own deadline and is shown as "Arinn did not reply. Try again." |
| `screenState` | `loading`, `ready`, `error` or `unavailable` |

An unknown `type`, a boolean that is not true or false, an enum value outside
`options`, or an integer that is not finite is labelled Unsupported. An empty
string or single path is labelled Not set.

The layout follows the settings frame in the design handoff: title at left 120
and top 96, category column at left 120 and top 220, rows 88 by 540 with radius
26, and the detail pane from x 740. The selected category shows an ember dot
while focus is in its rows, and an 8% plate. Focus itself is a 20% light plate,
or 12% on a disabled row. A boolean is an 88×50 switch. Every other kind shows
its value and a chevron; a disabled row shows a lock instead and Select does
nothing.

Select on an enum, integer or string opens a sheet. The screen behind it drops
to 55% opacity. Back closes the sheet without saving and returns focus to the
row. An enum sheet opens on the current option, scrolled so that option is
on screen and centred (handoff EN-2), keeps a check on that option, keeps the
focused option on screen while moving, and pages with Page Up/Down or Channel Up/Down when there are more than six
options. The position is "N of M". An integer sheet steps by `step` (1 when the
constraints omit it) with Left/Right or Up/Down; after the key has been held
for one second the step is ten times larger. The value stays inside `min` and
`max`, the button at a bound drops to 30% opacity, and the note says which
bound it is. A `unit` string in the integer constraints is shown beside the
value; until the contract task adds that key, the unit is blank. Digits from a
keyboard replace the draft. Select saves the draft. A string sheet prefills
the current value. Enter or Save sends it; the counter uses `maxLength` when
that constraint is present. The field's keyboard button opens the
[on-screen keyboard](ui-keyboard.md) in a full-screen typing layer (KB-4), whose
main key saves. Without a keyboard the field is dimmed and the sheet opens on
that button; Save and Cancel stay available. `KeyboardMonitor` reports a
keyboard only when an input device of that type is present. Arrow keys and other
remote navigation do not mark one. The first character key enables the field.

A single path, and a path list that is not a list of folders, stay inline text
fields with Add and Remove. A list of folders (`multiple` and `directory`) opens
the Library folders screen and the folder picker instead
([Library folders](#library-folders-lf-pk), owner decision on #756 C24,
2026-11-04, replacing the inline-list decision of #566). While a path editor is open, its keyboard pill opens the
[on-screen keyboard](ui-keyboard.md) in the same full-screen typing layer, with
the `path` context row (KB-5); its main key saves as Enter does. Without a
keyboard the editor opens on that pill.

While a path is typed, the shell checks the structural rules of
`arinn_protocol::settings::path` ([ADR-0065](../adr/0065-provider-settings-contract.md)):
one leading `/`, no empty, `.` or `..` segment, no control character and at
most 4096 bytes of UTF-8. A path that breaks one shows a translated hint under
the row, and under the field in the typing layer, and Enter and Save do
nothing until it is fixed (§6.2). An empty draft is not saved either and shows
no hint.

`~` names the user's home folder (owner decision of 2026-10-26, spec §5.6,
[#479](https://github.com/LimeBeck/arinn/issues/479)). The shell expands it
itself, so the proposed value, the stored setting and the contract stay
absolute:

- A leading `~/` is replaced in the field as soon as it is typed, by hand or
  with the `~/` context key, so the user sees, checks and saves the real path.
- A bare `~` stays in the field and is expanded on Save, since it may still
  become `~user/`. `~user/` is not supported and is rejected as relative.
- The check runs on the expanded path: `~/../x` still breaks the `..` rule.
- The home folder is the shell's `$HOME` (`main.cpp` passes it to
  `SettingsView.homeFolder`); the shell reads no file system for it (R-05).
  When it is unset or not an absolute path, `~` stays relative and is
  rejected, as by the contract.

Subfolder suggestions in the typing layer (KB-5, DR-44) are not built yet; the
folder picker below lists folders through `Folders1`.

### Unavailable library folders

Specification §5.6 shows a library folder as unavailable only when the library
provider reports it ([ADR-0071](../adr/0071-settings-editor-contract.md),
[#540](https://github.com/LimeBeck/arinn/issues/540)). The shell never checks a
path itself.

- `SettingsCatalog` reads `Daemon1.Providers` for the ready providers with the
  `library` capability and their daemon paths. It asks each one's
  `Library1.Features`, and calls `Library1.GetRoots` only when the list holds
  `LibraryRoots1`. It reads them on start, on `ProvidersChanged`, on a daemon
  owner change and on `Daemon1.StateChanged` for that provider's `library`
  capability.
- One library change brings `StateChanged` and then `ProvidersChanged`, so
  reads are coalesced per provider: one read at a time, and a trigger that
  arrives meanwhile runs it once more when it ends
  ([#558](https://github.com/LimeBeck/arinn/issues/558)). `Features` is
  read for each coalesced read because the daemon may announce a feature
  change without changing the provider generation.
- `Unsupported` means no unavailable state. `Unavailable` means the daemon has
  not read the roots yet: no folder is marked, and the next `StateChanged`
  reads again. Any other error, or no `LibraryRoots1`, also marks nothing.
- A root is matched to a folder of a folder list (a path row with `multiple`
  and `directory`) of the same provider, by its path as written; the row gets
  it in `unavailablePaths`. ADR-0071 does not say which setting the roots come
  from, so the shell does not name one: every folder list of that provider is
  matched. A list of files is not. The bundled library provider has one
  folder list, `roots`.
- In the inline folder list, such a folder keeps its place. Its path is in the
  secondary text colour, with the error-coloured note "Unavailable. Its titles
  stay in the library as unavailable." under it (handoff LF-4, without the
  `folder_off` chip, which the inline list does not have). Editing and Remove
  work as for any folder. Remove only takes the folder out of the setting,
  and its Russian label uses the handoff's verb for taking out, never the
  verb for deleting; nothing is deleted (§5.6). The folder returns to normal when the provider
  stops reporting it.

## Changes

Select on a boolean proposes the opposite value. Choosing an option, confirming
the integer draft, or confirming a string proposes that value. A string longer
than `maxLength` or containing a control character is not proposed. Confirming
a path proposes the draft, and a multiple path proposes the whole list. The
signal is `valueChangeRequested(providerId, settingId, value)`.

The view does not write the proposed value back. The catalog marks the row
pending, calls `SetValue`, and clears pending when the operation finishes. A
failure sets `error` to the daemon's text and leaves the previous value, except
an operation error of `Unsupported`, which is the next-start note below. There
is no timer that turns a proposal into a saved setting.

While the sheet is still open and the operation is still pending after 300 ms,
the chosen enum option shows a spinner. Back closes the sheet anyway; the
result arrives as a toast. A success that applied now closes the sheet and, for
anything except a boolean, shows "Saved" with the setting title and the new
value for 3 seconds. An operation whose error is `Unsupported` is stored for
the provider's next start, whether that operation finished as succeeded or
failed: the same toast says it applies after Arinn restarts, and the row keeps
that note until `GetSettings` returns a new generation. There is no Restart
action. The daemon currently turns the provider's `Unsupported` into an
ordinary success with an empty error, so a live next-start change is shown as
applied until the daemon keeps that code on the operation (#196). A rejection with
any other error leaves the sheet open with the daemon's
text; a string error clears on the next key. A rejected boolean springs back,
shows an error toast, and keeps the text on the row until focus leaves it. A
shell deadline (`Timeout`) uses "Arinn did not reply. Try again."

## Focus

The screen is one focus scope. Up and Down move through the category list or,
once Right has entered it, through the focused category's rows. Left from a
row returns to that category. A path list still moves Left and Right across
its fields. The ends of each list do not move focus further. Esc, Backspace
and Back leave the row list for the categories, and on the category list emit
`dismissRequested`. Editable text consumes Backspace itself and does not leave
the field (specification §6.2, AC-18). An enum, integer or string sheet is the
top scope: Esc closes it without saving and returns focus to the row. Inside
the integer sheet, Left and Right step the draft. Inside the string sheet,
Down moves from the field to Save and Cancel. In a path editor, Right at the
end of the text reaches its keyboard pill and Left returns. Esc, Back and
Backspace on a keyboard pill (a path editor's or the string sheet's) close
the editor without proposing the draft: the pill is not a text field. Replacing the row list (a
catalog publish) puts focus back on the same row control, or back into an open
sheet, so Select still reaches it (AC-02). A pending row is not editable until
that change settles.

While a row has focus, the bottom-left hint bar reads "OK Edit · Back To
sections" (handoff se-lf-1), or "OK Unavailable" where OK does nothing on the
focused part: a row with `enabled` false (ED-2), a row still saving, a boolean
or integer whose value is unsupported, an unknown type, or Add on a path list at
its `maxItems` (LF-5). `partActionable` mirrors the OK handlers. When the rows overflow, 72 px fade masks cover
the top and bottom edges of the list, and scrolling keeps the focused row clear
of them; at either end of the list that mask is hidden.

## Verification

`tests/ui/tst_settings.qml` covers D-pad focus, the boolean, the integer sheet
(bounds, hold acceleration, unit when present), the enum sheet (paging, a
wrapping label, and a 64-option sheet that opens scrolled to its current
option), the string sheet, typing it with the on-screen keyboard, single-path and
multiple-path text, typing a path, a list item and *Add path* with the on-screen keyboard, an
unavailable folder (its note, Remove on it, and its return to normal), a disabled row, pending and error text, an over-long
string, saving toasts, a deferred row note, Back, the unavailable screen,
focus kept across a row republish, and Select on a pending boolean.
`apps/arinn-shell/settings_catalog_test.cpp` drives a fake `Settings1`:
grouping, each value kind, `SetValue` pending, applied, deferred (`Unsupported`
on both a succeeded and a failed operation, cleared when the generation
changes) and rejected with the daemon's text, a second `SetValue` ignored while
that operation is still running, a disabled setting, string and path JSON, an
integer `unit` and `step` when the JSON already contains them,
`SettingsChanged`, a missing service, and `KeyboardMonitor` ignoring remote
navigation keys. For library roots, the fake daemon also answers
`Daemon1.Providers`, `Library1.Features` and `Library1.GetRoots`: `Unavailable`
first, then one root unavailable after a library `StateChanged` and available
after the next, no read on another capability's `StateChanged`, no `GetRoots`
without `LibraryRoots1`, and `Unsupported` marking nothing. On its own
connection, so that `GetRoots` replies can be held back, it checks that a
burst of `StateChanged` and `ProvidersChanged` during a read gives one more
read, that features can appear and disappear within the same provider
generation, and that a list of files is not matched (#558).

Offscreen run on 2026-09-30, Qt 6.11.2:

```sh
cmake --build build --target arinn-shell arinn-settings-catalog-test arinn-ui-test arinn-browser-catalog-test
dbus-run-session -- build/apps/arinn-shell/arinn-settings-catalog-test
QT_QPA_PLATFORM=offscreen QT_QUICK_BACKEND=software \
  build/apps/arinn-shell/arinn-ui-test -input tests/ui
QT_QPA_PLATFORM=offscreen QT_QUICK_BACKEND=software \
  dbus-run-session -- build/apps/arinn-shell/arinn-browser-catalog-test
python scripts/check-docs.py
```

`arinn-shell` linked. The catalog test passed 13 cases, including `Unsupported` on a succeeded operation and on a failed one, and `KeyboardMonitor` staying unavailable after remote navigation keys. `arinn-ui-test -input tests/ui` passed 196 cases, including every `SettingsView` case in `tst_settings.qml` (the same suite as `ctest -R '^ui-components$'`, which also passed). `arinn-browser-catalog-test` passed 10 cases; it loads `Main.qml`, which binds `KeyboardMonitor`. `python scripts/check-docs.py` passed. Operator tests were not run. This does not close AC-02, AC-12 or AC-18. The catalog test and `check-docs.py` were run again after the next-start and keyboard fixes; the UI and browser-catalog counts above are from the earlier run of this change.

## TV design review (#764, part one)

- The Appearance texts use plain words: "A soft glow along the bottom of the
  screen and behind the item in focus" and "In the evening and at night the
  glow is dimmer and the screen a little warmer"
  (`crates/arinnd/src/runtime/settings_routing.rs`, the `QT_TRANSLATE_NOOP`
  table in `settings_catalog.cpp`, `fill_ru.py`). The Russian text no longer
  uses the underfloor-heating wording (#756 B16).
- A toggle row keeps 24 px between its text and the 88 px toggle, so a long
  description wraps before it ([#734](https://github.com/LimeBeck/arinn/issues/734)).
- Verification: `settings_catalog_test.cpp` (categories with rows only, no
  source line for `arinn`), `translations_test.cpp` (the library's name, the
  Appearance copy), `tst_settings.qml`
  (`test_toggle_row_text_keeps_clear_of_the_toggle`) and the daemon's
  `settings_tests.rs`. The Library folders screen and the folder picker
  (LF-1…8, PK-1…10, C24) are the second part of #764. The TV re-check was not
  done.

## Library folders (LF, PK)

[#764](https://github.com/LimeBeck/arinn/issues/764), the handoff's *Settings
editors* LF-1…8 and PK-1…10, on `org.arinn.Folders1`
([ADR-0071](../adr/0071-settings-editor-contract.md)).

- **Row.** A folder list (`path`, `multiple`, `directory`) is a plain row with
  "N folders", plus "· N unavailable" when the provider reports any, and a
  chevron. OK emits `SettingsView.foldersRequested(row)`. `Main.qml` then opens
  `LibraryFoldersView` over Settings (`focusedScreen` `libraryFolders`) and binds
  it to the live row, so pending, errors and unavailable folders show at once.
  If the setting disappears (a reload, a disconnect, a schema change), the
  screen closes instead of editing an empty list. Each opening starts afresh:
  no picker, confirmation or pending add or removal carries over, and an add or
  removal settles only on the row of the setting it was made on. The screen
  takes pointer input itself, so nothing reaches Settings underneath.
- **Wording.** `SettingsCatalog` marks a folder list `libraryFolders` when its
  provider has the `library` capability in `Daemon1.Providers`. Such a list
  uses the handoff's library wording ("Library folders", films and series,
  "from the library"). Any other provider's folder list uses its row title,
  its description for the empty state, and plain folder wording ("from the
  list", "Unavailable", "Adding…").
- **Screen (LF-1).**
  - The title is followed by the count, and *Add folder* sits right of the title
    (Up from the first row).
  - Rows follow Open services: min-h128 r26, a chip 76 with `folder`, the last
    path segment at 30/600, and the full path in monospace at 24. A 128-wide
    Remove square is reached with Right.
  - The order is the order added.
  - Edits go out as `valuesRequested(paths)`, which Main sends to
    `SettingsCatalog.setValue`. The daemon validates and stores them.
- **States.**
  - Empty (LF-2): the 160 plate, "No folders yet" and *Add folder* in focus.
  - Unavailable (LF-4): `folder_off` on a 6 % chip and the error-coloured
    status; the folder can still be removed.
  - At `maxItems`, 64 by default (LF-5): *Add folder* at 40 % stays focusable,
    and OK explains in a toast.
  - Remove (LF-6, LF-7): the power-confirm dialog, "Remove “{name}” from the
    library?". Cancel has the default focus and Remove is destructive. The verb
    is always *Remove*, and nothing is deleted. Then focus goes to the neighbour.
    The toast "Folder removed" waits until the row no longer lists the folder;
    if the save fails, the toast shows the error instead.
  - Added (LF-8): the new row is appended and focused, with "Adding to the
    library…" while the setting is pending. The catalog keeps the old value
    until the save settles, so the screen shows the new folder itself
    meanwhile; a refused add leaves the list, and the error shows in a toast.
    Once saved, the add is done, so a later save does not read "Adding…". The
    catalog clears `pending` before its refresh brings the new value, so the
    folder stays until the row lists it or reports an error, and the screen
    takes no second change meanwhile.
  - A daemon refusal shows under the title in words, with the same mapping as
    `SettingsView.daemonErrorText`, never as the error code.
  - One change at a time: while the setting is pending or disabled, *Add
    folder* and *Remove* explain in a toast, and a folder chosen in the picker
    is refused there with the same text. `SettingsCatalog.setValue` would drop
    a second save while one is pending.
- **Picker (PK).** `FolderPicker` reads `FolderBrowser`
  (`folder_browser.{h,cpp}`):
  - `Places()` gives the start points: the home folder, each mount and the whole
    computer, with an Ember dot on the current one, and a note when there are no
    mounts (PK-9). The start points scroll when there are more than fit, keeping
    the focused one on screen.
  - `List(path, after, limit)` is read page by page, 256 names at a time, up to
    2048 names. Names are sorted case-insensitively with numbers in numeric
    order, folders only.
  - The path line shows the parent in `text-2` and the last segment in `text-1`.
    A long parent collapses its middle to `/…/`.
  - *Choose this folder* chooses the path shown (Up from the first row). While
    a listing loads, Choose holds focus for the folders, and the first folder
    takes it once `Folders1` answers. Each time the picker opens, it reads the
    folder it shows again.
  - Keys:
    - Up and Down move;
    - OK or Right enters a folder;
    - Left goes to the start points;
    - Back goes up one level, or at a start point first to the start points
      and then out.
  - Empty (PK-5) moves focus to Choose. No access (PK-6) dims Choose to 40 %.
    A share that has gone (PK-7) offers Retry and *To the home folder*, side by
    side: Right and Left move between them, Left from Retry goes to the start
    points and Up to Choose. Arinn never mounts anything.
  - When `Places()` fails or returns nothing before a folder opens, the picker
    shows "Folders cannot be read right now" with Retry, which asks for the
    places again. With no start point to go to, Back leaves the picker.
  - A folder already in the list is refused in the picker with an error plate
    (PK-10). Paths are compared without trailing slashes, so `/srv/films/` and
    `/srv/films` are one folder. A start point that ends in `/` joins its children with one
    slash and still counts as that start point.
- **Not built.**
  - Typing a path in the picker (PK-8);
  - paging with Ch± and the letter jump;
  - opening the library filtered to a folder from its row (OK on a row does
    nothing yet);
  - the empty library's link to this screen.
- **Verification.**
  - `tests/ui/tst_library_folders.qml` covers the rows, unavailable, remove,
    empty, the limit, picking through the start points and the list, the
    refusal, PK-5…7, a failed removal, a pending change and a failed
    `Places()`.
  - `folder_browser_test.cpp` covers Places, a failed Places read, sorting,
    paging, the error states and a newer open winning.
  - A real disk, share and TV were not tested.
