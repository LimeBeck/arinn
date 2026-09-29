<!-- SPDX-FileCopyrightText: 2026 Arinn contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Browser tab screen

Fixture presentation for [specification §7.1](../specifications/arinn-specification.md#71-browser-chromechromium).
The screen lists windows and tabs and emits browser intents. It is not bound to
the daemon. Production data and commands stay behind `arinnd`. The daemon
exports `org.arinn.Browser1`
([issue #83](https://github.com/LimeBeck/arinn/issues/83)). Binding this fixture
to `GetTabs`, `ActivateTab` and `CloseTab` is the follow-up.

The QML is `apps/arinn-shell/ui/screens/browser/BrowserView.qml`. It imports
`Theme`, `StatusPane`, `ContentCard`, `LogicalCanvas`, `ActionButton` and
`UiText` from `apps/arinn-shell/ui`. It is not registered in the shell resources
and is not reachable from Home. A later binding task can parent it on the
logical canvas in `Main.qml` and map daemon snapshots onto the properties below.

## Fixture model

`tabs` is an explicit array. Each entry has `id`, `windowId`, `title`, `url`,
`active` and `audible`. There is no favicon property and the screen does not
draw one. Identifiers are opaque: the screen does not parse them and does not
group windows by title.

The presentation model keeps a single active tab. If the fixture marks more
than one tab `active`, only the first is shown as active and exposed as
`activeTabId`. Duplicate tab ids are dropped. `focusedWindowId` is the one
window whose tabs are on screen. It starts as the active tab's window, then
follows D-pad focus along the window row.

`shortcuts` is an explicit array of already-known shortcut ids and display
titles. An `url` field on a shortcut is ignored. `open` is emitted only when
the id is in that array. The screen never accepts a typed address. Creating a
tab in this screen goes through a known shortcut id, which matches
[specification §7.1](../specifications/arinn-specification.md#71-browser-chromechromium)
for this fixture.

`browserState` is `populated`, `empty`, `loading`, `error` or `unavailable`.
`unavailable` means no browser is connected. `populated` shows the window row,
the focused window's tabs, Activate and Close. The other states show
`StatusPane` and hide the tab list. Loading hides the status action and the
shortcut row. Empty, error and unavailable keep the shortcut row so a known
shortcut can still be opened. Optional `statusTitle` and `statusDescription`
replace the default copy.

## Intents and focus

| Signal | When |
| --- | --- |
| `activateTab(tabId)` | OK on a tab card, or OK on Activate, for the focused tab |
| `closeTab(tabId)` | OK on Close, for the focused tab |
| `open(shortcutId)` | OK on a known shortcut |
| `retryRequested()` | OK on the status action |
| `backRequested()` | Escape, Backspace or Back |

Arrows, Tab and Backtab move inside the screen and stop at the ends of each
row. Escape, Backspace and Back emit `backRequested()` and leave focus where
it is. The rows are windows, tabs, Activate/Close, then shortcuts. Status
replaces the first three rows when the list is hidden. Focus changes are local;
nothing on this screen sends a provider request in order to move focus
([specification §6.5](../specifications/arinn-specification.md#65-performance-principles)).

Titles and addresses are drawn on the tab cards only. The screen does not log
them, and it has no product-specific conditions (R-01, R-03, AC-12).

## Verification

`tests/ui/tst_browser.qml` imports the screen directory from the filesystem.
The existing `ui-components` CTest loads every `tests/ui/tst_*.qml`, so this
file runs with that test. No gallery page and no CMake change are required.

```sh
cmake -S . -B build -G Ninja && cmake --build build --target arinn-ui-test
QT_QPA_PLATFORM=offscreen QT_QUICK_BACKEND=software \
    build/apps/arinn-shell/arinn-ui-test -input tests/ui BrowserTabs
```

Operator tests were not run. This screen has no platform binding and does not
lock the session or load a KWin script.
