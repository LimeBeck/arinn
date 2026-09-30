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
| Row `constraints` | `options` (`value`, `title`) for an enum; `min` and `max` for an integer; `maxLength` for a string; `directory`, `multiple` and `maxItems` for a path |
| Row `enabled` | When false, the row is shown and cannot be edited |
| Row `pending` | A `SetValue` operation is still running |
| Row `error` | Operation error code, or empty |
| `screenState` | `loading`, `ready`, `error` or `unavailable` |

An unknown `type`, a boolean that is not true or false, an enum value outside
`options`, or an integer that is not finite is labelled Unsupported. An empty
string or single path is labelled Not set.

The layout follows the settings frame in the design handoff: title at left 120
and top 96, category column at left 120 and top 220, rows 88 by 540 with radius
26, and the detail pane from x 740. The selected category shows an ember dot
while focus is in its rows, and an 8% plate. Focus itself is a 20% light plate.
A boolean is an 88×50 switch. An integer is a minus control, the number and a
plus control; Select on the number opens a hardware-keyboard field
([ADR-0051](../adr/0051-address-and-search-entry.md)). An enum opens an option
list. A string or a single path is a text field. A path with `multiple` is a
list of text fields with Add and Remove; a folder browser waits on the design
in [#172](https://github.com/LimeBeck/arinn/issues/172).

## Changes

Select on a boolean proposes the opposite value. Minus and plus propose the
integer one step inside `min` and `max`; confirming the number field proposes
that integer when it is inside the range. Choosing an option proposes that
option's value. Confirming a string proposes the draft unless it is longer
than `maxLength` or contains a control character. Confirming a path proposes
the draft, and a multiple path proposes the whole list. The signal is
`valueChangeRequested(providerId, settingId, value)`.

The view does not write the proposed value back. The catalog marks the row
pending, calls `SetValue`, and clears pending when the operation finishes. A
failure sets `error` and leaves the previous value. There is no timer that
turns a proposal into a saved setting.

## Focus

The screen is one focus scope. Up and Down move through the category list or,
once Right has entered it, through the focused category's rows. Left and Right
move across a stepper or a path list. Left from the first control returns to
that category. The ends of each list do not move focus further. Esc, Backspace
and Back leave the row list for the categories, and on the category list emit
`dismissRequested`. Editable text consumes Backspace itself and does not leave
the field (specification §6.2, AC-18). The option list is the top scope: Esc
closes it and returns focus to the row. Replacing the row list (a catalog
publish) puts focus back on the same row control, so Select still reaches it
(AC-02). A pending row is not editable until that change settles.

## Verification

`tests/ui/tst_settings.qml` covers D-pad focus, the boolean, integer, enum,
string, single-path and multiple-path controls, a disabled row, pending and
error, an over-long string, Back, the unavailable screen, focus kept across a
row republish, and Select on a pending boolean.
`apps/arinn-shell/settings_catalog_test.cpp` drives a fake `Settings1`:
grouping, `SetValue` pending and failure, a second `SetValue` ignored while
that operation is still running, a disabled setting, string and path JSON,
`SettingsChanged`, and a missing service.

Offscreen run on 2026-09-30, Qt 6.11.2:

```sh
dbus-run-session -- build/apps/arinn-shell/arinn-settings-catalog-test
QT_QPA_PLATFORM=offscreen QT_QUICK_BACKEND=software \
  ctest --test-dir build -R '^ui-components$'
```

The catalog test passed 10 cases. `ui-components` passed, including
`tst_settings.qml` and the Home/Back text-field case. The browser catalog
test passed on the earlier revision of this screen (`Main.qml` unchanged
here). Operator tests were not run. This does not close AC-02, AC-12 or AC-18.
