<!-- SPDX-FileCopyrightText: 2026 Arinn contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Home favorites bound to web shortcuts

The production Home favorites row reads `org.arinn.WebShortcuts1` on the daemon
service and root path ([web-shortcuts1.xml](../plugin-api/web-shortcuts1.xml),
[ADR-0043](../adr/0043-web-shortcuts.md)). This is the shell binding for
[specification §5.1](../specifications/arinn-specification.md#51-home). Home
does not edit or remove shortcuts. Adding one is the Open services bookmark
action, which calls `WebShortcuts1.Add` from `BrowserCatalog`
([ui-browser.md](ui-browser.md)). The daemon is unchanged.

## What the shell shows

`Backend` subscribes to `WebShortcuts1.Changed` before the first `List`. That
signal calls `readShortcuts` only. It does not re-read the application catalog.
Daemon `StateChanged` (including focus changes) reloads status and the catalog
and does not call `List`. `List` runs on the first connection, on an explicit
refresh, and on `WebShortcuts1.Changed`. `List` is one message,
`(revision, [(id, title, url, icon)])`.

Until pinned favorites exist, the Home row is web shortcuts first and then the
application entries. Steam and ES-DE stay on that row. The Applications route
lists those application targets and then the same web shortcuts in **Online**
(`group` is `online` on the merged list only). A shortcut still has no URL in
either model. `WebShortcuts1.Changed` refreshes the shortcut list, and that
refresh updates the Online group.

Each shortcut tile shows the **title** and a **monogram** (the first grapheme
of the trimmed title, uppercased) inside the existing neutral tile plate. An
empty title, or a title whose first unit is an unpaired surrogate, keeps an
empty monogram, which is the neutral placeholder. The favorites model sets
`webShortcut: true` so Home can tell these tiles from application tiles. The
`icon` field is an opaque packaged reference; the favorites row does not load
it and does not draw third-party logos. The URL is discarded after the D-Bus
decode. Neither the URL nor the title is written to a log. Open-services
matching of that URL is described in [ui-browser.md](ui-browser.md); the
favorites model still has no URL and no icon.

Home state follows the return target, the shortcut list, and the application
catalog together:

| State | When |
| --- | --- |
| `unavailable` | The daemon is not connected |
| `loading` | Connected, and a list that could fill the row has not completed |
| `error` | Shortcuts or the application catalog failed, and there is nothing to show |
| `empty` | Both lists succeeded empty and there is no return target |
| `ready` | A return target, a shortcut, or an application entry |

`applicationsState` is separate (`unavailable`, `loading`, `error`, `empty`,
`ready`). A failed catalog is `error` there even when the shortcut list is
empty, so Home does not present that failure as an empty shortcut list. A
return target, a shortcut, or an application entry still makes Home `ready`.

## Opening a shortcut

The favorites row and the Applications route share `entryRequested`. `launch`
sends `WebShortcuts1.Open(id, request_id)` when the id is in the shortcut list,
and `Application1.Launch` when the id is an application. Shortcut ids and
application ids do not collide. An id that is in neither list shows
"Shortcut is no longer available". `Open` returns an `Operation1` path. The
shell polls `Operation1.Get` the same way it polls an application launch: the
tile stays busy, a duplicate request is ignored, and the operation state is
shown when it finishes. `succeeded` is "Done". Any other terminal state shows
the operation error string.

When no browser provider is connected, `Open` still finishes `Unavailable`.
The shell calls `Open` anyway and shows that result. It does not pre-disable
the tile and it does not open the URL itself.

## Verification

`apps/arinn-shell/catalog_test.cpp` (`webShortcuts`, private bus) covers loading
(recorded from `changed` when the daemon appears), an empty list, `Changed`
refreshing the shortcut list without re-reading the application catalog,
monograms (including a surrogate-pair grapheme and an unpaired surrogate),
absence of the URL and icon in the presentation model, `Open` deduplicated
while the operation is running, a failed operation whose error is `Unavailable`,
a succeeded operation, an unknown id ("Shortcut is no longer available"), a
focus `StateChanged` that does not call `List`, a failed `List` (Home error
while `applicationsState` stays empty) and daemon disappearance (unavailable).
The catalog cases expect Home `ready` when applications exist, `empty` when
both lists are empty, and `error` when the catalog fails. A failed catalog with
a shortcut still loaded keeps `applicationsState` at `error` and Home `ready`.

The offscreen Qt Quick Home test is `tests/ui/tst_home.qml`. It drives
`HomeView` with local fixtures. Favorites still open through `entryRequested`
unless the tile is marked unavailable, which is the Open services behaviour in
[ui-browser.md](ui-browser.md).

`apps/arinn-shell/backend_test.cpp` is the production shell check started from
`tests/integration/phase1.py`. It asserts that Home favorites list web shortcuts
and then the application entry, that the application tile is visible on Home
under the favorites row (and still present on the Applications route), and that
local focus survives a refresh. Keyboard movement during a running activate
still reaches that tile and returns to the menu button.

Local run on 2026-09-29, Qt 6.11.2, offscreen platform and software rendering,
private session bus: `dbus-run-session -- arinn-catalog-test` passed (the
catalog cases and `webShortcuts`, 10 passed). The same Qt build then ran
`arinn-shell-test` against `arinnd` and the delayed demo provider:
`BackendTest::typedWireAndLocalFocus` passed. Operator tests were not run.
