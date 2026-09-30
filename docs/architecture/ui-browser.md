<!-- SPDX-FileCopyrightText: 2026 Arinn contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Browser tab screen

Presentation for [specification §7.1](../specifications/arinn-specification.md#71-browser-chromechromium).
The screen is the Open services list: every tab from `org.arinn.Browser1` is one
row. It sends activate and close through that interface. `BrowserCatalog`
(`apps/arinn-shell/browser_catalog.cpp`) is the only D-Bus client. The QML does
not import a bus type and does not call a provider.

`Main.qml` parents `BrowserView` on the logical canvas and passes the catalog's
tabs, windows, shortcuts and `browserState`. Home and navigation read the same
catalog for the current service, the open-shortcut marks and the unavailable
state. `browserOpen` on the shell window shows the screen; Back emits
`backRequested()` and returns to Home.

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
The highlighted row stays on the same tab when a later snapshot still contains
it. The "On screen" mark follows the focused window in that snapshot. Moving
the highlight does not call the daemon
([specification §6.5](../specifications/arinn-specification.md#65-performance-principles)).

Titles and addresses are properties of the service rows. The catalog keeps
shortcut origins in memory so it can match them, then drops the URL strings.
It does not log titles or addresses, and the shortcut list published to QML
has no URL and no icon. A matched row receives the shortcut's packaged icon
reference when that reference is non-empty. QML draws it only when the
reference is a local `file:`, `qrc:`, `image:` or absolute path; any other
token stays the letter chip. Origin comparison is scheme, host and port, with
`www.` ignored and default ports (`http` 80, `https` 443) filled in. The row
letter is the first character of the host after one `www.`, `m.` or `hd.`
prefix, computed in the catalog.

## Creating a tab

Creating a tab goes through a known shortcut id, which matches
[specification §7.1](../specifications/arinn-specification.md#71-browser-chromechromium).
This list does not show shortcuts. `open(shortcutId)` is still emitted only for
an id in the shortcut list. `BrowserCatalog.openShortcut` calls
`WebShortcuts1.Open` with that id and a request id, then observes the operation.
The shell has no open-URL method
([ADR-0048](../adr/0048-browser-wire-contract.md)).

## Screen model

`tabs` entries have `id`, `windowId`, `title`, `url`, `active`, `audible` and
`saved`. `saved` is true when the tab origin matches a web shortcut. The match
is the same origin comparison as the open-shortcut dot (scheme, host, port,
`www.` ignored), done in `BrowserCatalog`.
`windows` entries have `id` and `focused`. Identifiers are opaque presentation
ids derived from the provider id, generation and provider-local id. The screen
does not parse them.

The list contains every tab. The active tab of the focused window is marked
"On screen" and is placed first. The remaining windows follow last use: the
catalog records a monotonic stamp when a window becomes the focused window.
The stamp is in memory only and is cleared when the daemon generation changes
or the daemon disappears. An audible tab is marked "Sound". The chip is the
matched shortcut icon when the catalog supplies a loadable reference, otherwise
the letter described above. An empty title uses the host as the title and the
full address as the subline. There is no favicon and no per-product artwork.

`shortcuts` entries have `id` and `title`. An `url` field on a shortcut is
ignored. `browserState` is `populated`, `empty`, `loading`, `error` or
`unavailable`. `populated` shows the list. Empty, loading, error and unavailable
use the same left-aligned message block and hide the list. Empty offers Home,
which emits `backRequested()`. Error and unavailable offer Retry. Loading hides
the action.

OK on a row emits `activateTab`. A bookmark control sits between the row and
its close control. OK on that control emits `savePage`. `Main.qml` connects
that signal to `BrowserCatalog.addShortcut`, which sends `WebShortcuts1.Add`
with the row title (the page title, or the host when the page title is empty)
and the page address, an empty icon reference and a request id. The shell
follows the method reply. Success marks the row saved: the control shows
`bookmark_added` and the row carries the mark "Shortcut". The new shortcut is
an Applications › Online tile once `WebShortcuts1.Changed` refreshes the
favorites model. A row whose origin already matches a shortcut shows that
saved state and OK does not call `Add` again. A rejected address sets
`saveError`, separate from `operationState`, so activate and close failures
do not use the same label. The screen maps `InvalidArgument`, `Conflict` and
`NotFound` to short messages. The label clears when focus moves and four
seconds after it appears. The list position stays visible beside it. The
control is disabled when `browserState` is `unavailable`, and `Add` is not
called; the list itself is hidden in that state. OK on the close control emits
`closeTab` with no confirmation, including when the tab is audible. The row
then shows "Closing…" at half opacity until the snapshot drops it, and the row
collapses. A failed operation clears that state while the tab remains. Moving
focus does not call the daemon and does not change which window is focused.
The bottom hint labels → as Save, because that key reaches the bookmark before
Close.

| Signal | When |
| --- | --- |
| `activateTab(tabId)` | OK on a service row |
| `savePage(tabId)` | OK on that row's bookmark control when the page is not saved and a browser is connected |
| `closeTab(tabId)` | OK on that row's close control |
| `open(shortcutId)` | A known shortcut id passed to `requestOpen` |
| `retryRequested()` | OK on Retry. A failed browser provider calls `Daemon1.Restart`. A ready provider with no browser connected only re-reads state. The shell does not launch the browser. |
| `backRequested()` | Escape, Backspace, Back, or OK on Home when nothing is open |

Right moves from a row to its bookmark control and then to its close control.
Left moves back the same way. Up and Down move between rows and stay on the
same side. Holding Up or Down moves farther.
Tab and Backtab stay inside the screen. Escape, Backspace and Back emit
`backRequested()` once, ignore auto-repeat, and leave focus where it is.
When the list is taller than the viewport, the focused row stays about two
row-heights above the bottom and the header shows the position as "n of m".

The QML imports `Theme`, `LogicalCanvas` and `UiText` from
`apps/arinn-shell/ui`. It has no product-specific conditions (R-01, R-03, AC-12).
Origin matching and last-use order live in `BrowserCatalog`, not in QML.

## Home and navigation

The focused window's active tab is `currentService` (`id`, `title`,
`subtitle`). Home shows it as the first Continue card. When more than one tab
is open, one of the four further Continue entries is "N more open" and opens
this screen; with a single tab that card is hidden. `openShortcutIds` lists
shortcut ids whose origin matches an **active** tab. A favorites tile with
`webShortcut: true` and that id shows an ember dot. When `browserState` is
`unavailable`, those tiles dim, their subtitle is "Unavailable", they stay
focusable, and OK opens this screen instead of `WebShortcuts1.Open`. A status
chip on Home reads "Browser unavailable".

Global navigation lists **Open services** directly under the five sections, with
no divider. The item stays when the list is empty; the count badge is hidden
then. A subline "On screen: {title}" is shown only when a service is current.
The panel runs from y 72 to y 1008. Whether the return-activity row is a web
service is a caller-supplied `returnIsWebService` flag. The shell does not
infer it from a window class (R-01); the compositor link is still open
([#101](https://github.com/LimeBeck/arinn/issues/101)).

## Opening

`openShortcut` sets `opening` while that operation is queued or running.
`LaunchOverlay` waits 400 ms before it appears, so a fast success shows nothing.
After 8 s it adds "The browser is taking longer than usual" and Cancel. Cancel
calls `Operation1.Cancel` when the operation path is known (best effort) and
hides the overlay. After 30 s, or when `openingError` is set, a dialog offers
Retry and Close. Retry calls `Open` again for the same shortcut id. Home
closes that dialog without Retry, or cancels a slow open, and emits
`homeRequested`; the shell then shows Home. The scrim
is a fixed neutral tint; icon hues are not read. The overlay is skipped for tab
activate and close, which share `operationState` but not `opening`.

## Verification

`tests/ui/tst_browser.qml` imports the screen from the filesystem and runs as
part of the `ui-components` CTest. It covers focus, empty, many and unavailable
states, and uses a model-supplied letter instead of parsing a shortcut URL.
`tests/ui/tst_browser_shortcut.qml` covers the bookmark focus order (row, then
bookmark, then close), the saved mark, the mapped save-error text, clearing
that text when focus moves, and the guard used when no browser is connected.
The private-bus case `saveOpenPage` checks `Add` (title, address, empty icon),
a rejected address, an origin that is already saved, and that `Add` is not
called while the browser is unavailable. `saveSurvivesStaleListUntilRemoved`
checks a `List` reply that arrives while `Add` is in flight, then a later list
that no longer contains the shortcut. `productionSaveWiring` loads `Main.qml`
and calls `requestSave`, the function OK on the bookmark uses, through the
`savePage` binding. `saveErrorTimesOut` checks the four-second
clear. The browser-catalog test loads the shell window, so it runs with
`QT_QPA_PLATFORM=offscreen`.
`tests/ui/tst_home.qml` covers the Continue card rules, the navigation item and
the unavailable shortcut tile. `tests/ui/tst_launch.qml` covers the overlay
delays with shortened timers. `browser_catalog_test.cpp` is the private-bus
test `browser-catalog`: a tab snapshot reaches the model, one window is
focused, activate, close and a known-shortcut open are observed, and a second
case checks last-use order, origin matching (scheme, port, `www.`), cancel and
the two retry paths.

```sh
cmake -S . -B build -G Ninja && cmake --build build --target arinn-ui-test arinn-browser-catalog-test
dbus-run-session -- build/apps/arinn-shell/arinn-browser-catalog-test
QT_QPA_PLATFORM=offscreen QT_QUICK_BACKEND=software \
    build/apps/arinn-shell/arinn-ui-test -input tests/ui
```

Operator tests were not run. This binding does not lock the session or load a
KWin script.
