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
`Main.qml` binds that model to `SettingsView` when Home's screen is Settings.
The QML does not call a daemon, a plugin process or a provider-supplied widget
(R-03, R-04). Values are not logged.

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
row. An enum sheet opens on the current option, keeps a check on that option,
and pages with Page Up/Down or Channel Up/Down when there are more than six
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
end of the text reaches its keyboard pill and Left returns; Back on the pill
closes the editor without proposing the draft. Replacing the row list (a
catalog publish) puts focus back on the same row control, or back into an open
sheet, so Select still reaches it (AC-02). A pending row is not editable until
that change settles.

## Verification

`tests/ui/tst_settings.qml` covers D-pad focus, the boolean, the integer sheet
(bounds, hold acceleration, unit when present), the enum sheet (paging and a
wrapping label), the string sheet, typing it with the on-screen keyboard, single-path and
multiple-path text, typing a path, a list item and *Add path* with the on-screen keyboard, a disabled row, pending and error text, an over-long
string, saving toasts, a deferred row note, Back, the unavailable screen,
focus kept across a row republish, and Select on a pending boolean.
`apps/arinn-shell/settings_catalog_test.cpp` drives a fake `Settings1`:
grouping, each value kind, `SetValue` pending, applied, deferred (`Unsupported`
on both a succeeded and a failed operation, cleared when the generation
changes) and rejected with the daemon's text, a second `SetValue` ignored while
that operation is still running, a disabled setting, string and path JSON, an
integer `unit` and `step` when the JSON already contains them,
`SettingsChanged`, a missing service, and `KeyboardMonitor` ignoring remote
navigation keys.

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
