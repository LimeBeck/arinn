<!-- SPDX-FileCopyrightText: 2026 Arinn contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Browser tab screen

Presentation for [specification §7.1](../specifications/arinn-specification.md#71-browser-chromechromium).
The screen is the Open services list: every tab from `org.arinn.Browser1` is one
row. It sends activate and close through that interface. `BrowserCatalog`
(`apps/arinn-shell/browser_catalog.cpp`) is the only D-Bus client. The QML does
not import a bus type and does not call a provider.

`Main.qml` parents `BrowserView` on the logical canvas and passes the catalog's
tabs, windows, shortcuts and `browserState`. The Home favorites row is unchanged.
`browserOpen` on the shell window shows the screen; Back emits `backRequested()`
and returns to Home.

## Daemon binding

The catalog follows the application catalog's read loop. It calls
`Daemon1.Status` and `Daemon1.Providers`, keeps ready `browser` providers whose
backend is not `unavailable`, and pages `Browser1.GetTabs`. The snapshot
signature is `(generation, revision, windows, tabs, next_token)` from
[browser1.xml](../plugin-api/browser1.xml): windows are `(id, window_token,
focused)` and tabs are `(id, window_id, title, url, active, audible)`. There is
no favicon. `window_token` is not shown.

`ActivateTab` and `CloseTab` send the provider generation, the provider tab id
and a fresh request id. The shell then polls the returned `Operation1` object
until it leaves `queued` or `running`. Duplicate calls for a tab that already
has an operation in flight are ignored.

A connected daemon with no ready browser provider, or only `unavailable`
browser backends, is `unavailable` (no browser connected). A failed snapshot is
`error`. A successful snapshot with no tabs is `empty`. Tabs on screen are
`populated`. The first read, before a snapshot exists, is `loading`.

The model keeps one focused window. If more than one snapshot window is
focused, only the first is marked. Each window keeps at most one active tab.
Local D-pad focus stays on the current window when a later snapshot still
contains it. Focus movement does not call the daemon
([specification §6.5](../specifications/arinn-specification.md#65-performance-principles)).

Titles and addresses are properties of the service rows. The catalog drops
shortcut URLs after reading `WebShortcuts1.List` and does not log titles or
addresses.

## Creating a tab

Creating a tab goes through a known shortcut id, which matches
[specification §7.1](../specifications/arinn-specification.md#71-browser-chromechromium).
This list does not show shortcuts. `open(shortcutId)` is still emitted only for
an id in the shortcut list. `BrowserCatalog.openShortcut` calls
`WebShortcuts1.Open` with that id and a request id, then observes the operation.
The shell has no open-URL method
([ADR-0048](../adr/0048-browser-wire-contract.md)).

## Screen model

`tabs` entries have `id`, `windowId`, `title`, `url`, `active` and `audible`.
`windows` entries have `id` and `focused`. Identifiers are opaque presentation
ids derived from the provider id, generation and provider-local id. The screen
does not parse them.

The list contains every tab. The active tab of the focused window is marked
"On screen" and is placed first; the rest keep snapshot order. There is no
last-use timestamp on the snapshot, so the screen does not invent one. An
audible tab is marked "Sound". The chip is the first letter of the host, after
`www.`, `m.` and `hd.` are stripped. An empty title uses the host as the title
and the full address as the subline. There is no favicon and no per-product
artwork.

`shortcuts` entries have `id` and `title`. An `url` field on a shortcut is
ignored. `browserState` is `populated`, `empty`, `loading`, `error` or
`unavailable`. `populated` shows the list. Empty, loading, error and unavailable
use the same left-aligned message block and hide the list. Empty offers Home,
which emits `backRequested()`. Error and unavailable offer Retry. Loading hides
the action.

OK on a row emits `activateTab`. OK on that row's close control emits
`closeTab` with no confirmation, including when the tab is audible. The row
then shows "Closing…" at half opacity until the snapshot drops it, and the row
collapses. A failed operation clears that state while the tab remains. Moving
focus does not call the daemon and does not change which window is focused.

| Signal | When |
| --- | --- |
| `activateTab(tabId)` | OK on a service row |
| `closeTab(tabId)` | OK on that row's close control |
| `open(shortcutId)` | A known shortcut id passed to `requestOpen` |
| `retryRequested()` | OK on Retry; the catalog refreshes |
| `backRequested()` | Escape, Backspace, Back, or OK on Home when nothing is open |

Right moves from a row to its close control and Left returns. Up and Down move
between rows and stay on the same side. Holding Up or Down moves farther.
Tab and Backtab stay inside the screen. Escape, Backspace and Back emit
`backRequested()` once, ignore auto-repeat, and leave focus where it is.
When the list is taller than the viewport, the focused row stays about two
row-heights above the bottom and the header shows the position as "n of m".

The QML imports `Theme`, `LogicalCanvas` and `UiText` from
`apps/arinn-shell/ui`. It has no product-specific conditions (R-01, R-03, AC-12).

## Verification

`tests/ui/tst_browser.qml` imports the screen from the filesystem and runs as
part of the `ui-components` CTest. `browser_catalog_test.cpp` is the private-bus
test `browser-catalog`: a tab snapshot reaches the model, one window is
focused, and activate, close and a known-shortcut open are issued and observed.

```sh
cmake -S . -B build -G Ninja && cmake --build build --target arinn-ui-test arinn-browser-catalog-test
dbus-run-session -- build/apps/arinn-shell/arinn-browser-catalog-test
QT_QPA_PLATFORM=offscreen QT_QUICK_BACKEND=software \
    build/apps/arinn-shell/arinn-ui-test -input tests/ui
```

Operator tests were not run. This binding does not lock the session or load a
KWin script.
