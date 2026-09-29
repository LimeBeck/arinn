<!-- SPDX-FileCopyrightText: 2026 Arinn contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Settings view

Fixture-driven presentation for [specification §5.6](../specifications/arinn-specification.md#56-settings).
The behaviour rules live in the specification and in
[Activity and interaction](activity-and-interaction.md#settings-system-actions-and-diagnostics).
This screen does not close a Phase 5 gate.

`apps/arinn-shell/ui/screens/settings/` owns `SettingsView` only. It imports
`Theme`, `UiText` and `StatusPane` and does not modify them. Category and
setting rows are drawn in this screen so focus can be the handoff's light plate. It is not registered in the shell executable, `Main.qml` or
Home. Nothing here talks to a daemon, a D-Bus interface, a plugin process or a
provider-supplied widget (R-03, R-04).

## Fixture schema

The view renders the arrays it is given and no others. Category titles in the
specification — Media, Playback, Sound, Network, Applications, Appearance,
System — are fixture data. The QML does not embed those names or any product name.

| Field | Role |
| --- | --- |
| Category `id`, `title` | Left-hand list entry, in the given order |
| Category `capability` | Optional capability name. Empty means the category does not depend on a provider |
| Row `id`, `categoryId`, `title`, `description` | One setting in that category |
| Row `type` | `boolean`, `enum`, `string`, `integer` or `path` |
| Row `currentValue` | Value currently supplied by the caller |
| Row `constraints` | `values` for an enum; `minimum`, `maximum`, `step` for an integer; `maximumLength` for a string; `kind` of `file` or `directory` for a path |
| Row `enabled` | When false, the row is visible and cannot emit a change |
| Row `capability` | Optional. The row is unavailable unless this capability is registered |
| Capability `id`, `available` | A missing entry, or `available: false`, is not registered for display |
| Capability `title`, `description` | Copy for the category unavailable pane |

An unknown `type`, a boolean that is not true or false, an enum value outside
`constraints.values`, or an integer that is not finite is labelled Unsupported.
An empty string or path is labelled Not set. Unsupported and unavailable copy
uses the error colour from the component theme.

The layout follows the settings frame in the design handoff: title at left 120
and top 96, category column at left 120 and top 220, rows 88 by 540 with radius
26, and the detail pane from x 740. A category may carry an optional `icon`
name (`video_library`, `play_circle`, `volume_up`, `wifi`, `apps`, `palette`,
`memory`); anything else is a plain mark. The selected category shows an ember
dot on the right while focus is in its rows, and an 8% plate. Focus itself is
a 20% light plate, without the shared component focus ring. A boolean is only
an 88×50 switch: ember track and ink knob at the end when on, a 20% white track
and light knob at the start when off. Other values sit on the right with a
chevron. A path does not add a folder or file caption.

## Intents

Select on a boolean proposes the opposite value. Select on an enum proposes the
next entry in `constraints.values`, wrapping, or the first entry when the
current value is unsupported. Select on an integer proposes `currentValue + step`
when that stays inside the minimum and maximum. Select on a string or path opens
a local editor; confirming it proposes the draft, and a string longer than
`maximumLength` is not proposed. The signal is `valueChangeRequested(settingId, value)`.

The view does not assign the proposed value back to `currentValue`. The caller
decides whether a later fixture update shows it. There is no timer that turns a
proposal into a saved setting.

## Missing capabilities

Playback, download and network are ordinary capability names on the fixture, not
special cases in the view. When the category's capability is missing or
`available` is false, the pane is a `StatusPane` with `kind: "unavailable"` and
no action. Rows for that category are not created, so a stored "on" value cannot
appear as a successful control. When only a row names a missing capability, that
row shows Unavailable in the error colour, with no toggle and no value, and
Select does not emit an intent. A neighbouring row whose capability is ready
stays editable. An empty category that does not require a capability uses
`kind: "empty"`.

## Focus

The screen is one focus scope. Up and Down move through the category list or,
once Right has entered it, through the focused category's focusable rows. Left
from a row returns to that category. The ends of each list do not move focus
further. Tab is handled the same way as Down. Esc and Backspace leave the row
list for the categories, and on the category list emit `dismissRequested`
without focusing anything outside this view. Editable text consumes Backspace
itself. Arrow keys are accepted inside the scope.

## Verification

The Qt Quick case `tests/ui/tst_settings.qml` imports this directory from the
filesystem. It covers category focus, a boolean change intent, an enum change
intent, and missing playback, network and download capabilities shown as
unavailable. The existing `ui-components` CTest passes `-input tests/ui`, which
loads every `tst_*.qml`; this screen is not added to the gallery or to Home.

Offscreen run on 2026-09-29, Qt 6.11.2, software rendering, after the handoff frame restyle:

```sh
QT_QPA_PLATFORM=offscreen QT_QUICK_BACKEND=software \
  build/apps/arinn-shell/arinn-ui-test -input tests/ui
```

56 passed, 0 failed. That includes the existing component and Home cases and
the Settings cases: category focus, boolean and enum change intents, unavailable
playback, unavailable network, and an unavailable download row. Operator tests
were not run. This does not close AC-02, AC-14 or G-5.1.
