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
([ADR-0112](../adr/0112-shell-preferences-as-core-settings.md)): `light_field`
(Appearance › Light field, on by default) and `ambient_timeout` (Appearance ›
Ambient after: 5, 10, 15, 30 or 60 minutes, 10 by default). They are ordinary
rows on this screen. `SettingsCatalog` also exposes them as `lightField` and
`ambientTimeoutMinutes` for `Main.qml`, which binds the light field behind Home
to `lightField`. An invalid hand-edited value of either key in `config.toml`
reads as its default, with one warning line and no value in the log. The
titles, descriptions and option titles of the `arinn`
source are shown translated; provider texts are shown as sent.

## What the view receives

Category titles are model data. The QML does not embed those names or any
product name. `SettingsCatalog` publishes the seven design categories, in
order, once a snapshot has loaded. A category with no rows is an empty pane:
a missing playback, download or network setting is not shown as a working
control.

| Field | Role |
| --- | --- |
| Category `id`, `title` | Left-hand list entry, in the given order |
| Category `icon` | Optional mark name (`video_library`, `play_circle`, `volume_up`, `wifi`, `apps`, `palette`, `memory`); anything else is a plain mark |
| Row `providerId`, `providerName` | Provider that declared the setting. The name is a caption |
| Row `id`, `categoryId`, `title`, `description` | One setting |
| Row `type` | `boolean`, `enum`, `string`, `integer` or `path` |
| Row `currentValue` | Current value from `GetSettings` |
| Row `constraints` | `options` (`value`, `title`) for an enum; `min`, `max`, and when present `step` and `unit`, for an integer; `maxLength` for a string; `directory`, `multiple` and `maxItems` for a path |
| Row `enabled` | When false, the row is shown and cannot be edited. The detail line is the generic "Cannot be changed now"; the contract carries no reason |
| Row `pending` | A `SetValue` operation is still running |
| Row `deferred` | The operation error was `Unsupported`. The value is stored and applies at the provider's next start, whether the operation finished as succeeded or failed. The note stays until that provider's generation changes, and it is forgotten when the shell restarts |
| Row `unavailablePaths` | Only on a path row with `multiple`: the folders of `currentValue` that the provider of that row reports unavailable through `Library1.GetRoots`, matched by path as written. Empty when nothing is reported |
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

A single path, and a path with `multiple`, stay inline text fields with Add and
Remove. While a path editor is open, its keyboard pill opens the
[on-screen keyboard](ui-keyboard.md) in the same full-screen typing layer, with
the `path` context row (KB-5); its main key saves as Enter does. Without a
keyboard the editor opens on that pill. Subfolder suggestions, the folder list
and the folder picker in specification §5.6 wait on the folder-listing
contract ([DR-27](../design/integration-review.md)).

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
- `Unsupported` means no unavailable state. `Unavailable` means the daemon has
  not read the roots yet: no folder is marked, and the next `StateChanged`
  reads again. Any other error, or no `LibraryRoots1`, also marks nothing.
- A root is matched to a folder of a `multiple` path row of the same provider
  by its path as written; the row gets it in `unavailablePaths`.
- In the inline folder list, such a folder keeps its place. Its path is in the
  secondary text colour, with the error-coloured note "Unavailable. Its titles
  stay in the library as unavailable." under it (handoff LF-4, without the
  `folder_off` chip, which the inline list does not have). Editing and Remove
  work as for any folder. Remove only takes the folder out of the setting;
  nothing is deleted (§5.6). The folder returns to normal when the provider
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
without `LibraryRoots1`, and `Unsupported` marking nothing.

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
