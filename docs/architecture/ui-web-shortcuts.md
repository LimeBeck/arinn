<!-- SPDX-FileCopyrightText: 2026 Arinn contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Home favorites bound to web shortcuts

The production Home favorites row reads `org.arinn.WebShortcuts1` on the daemon
service and root path ([web-shortcuts1.xml](../plugin-api/web-shortcuts1.xml),
[ADR-0043](../adr/0043-web-shortcuts.md)). This is the shell binding for
[specification §5.1](../specifications/arinn-specification.md#51-home). It does
not add, edit or remove shortcuts, and it does not change the daemon.

## What the shell shows

`Backend` subscribes to `WebShortcuts1.Changed` before the first `List`, then
re-reads the list on that signal and on the same refresh that reads daemon
status. `List` is one message, `(revision, [(id, title, url, icon)])`.

Each tile shows the shortcut **title** and a **monogram** (the first character
of the trimmed title, uppercased) inside the existing neutral tile plate. An
empty title keeps an empty monogram, which is the neutral placeholder. The
`icon` field is an opaque packaged reference; the shell does not load it and
does not draw third-party logos. The URL is discarded after the D-Bus decode.
Neither the URL nor the title is written to a log.

Home state follows that list, plus the existing return target:

| State | When |
| --- | --- |
| `unavailable` | The daemon is not connected |
| `loading` | Connected, and `List` has not completed |
| `error` | `List` failed and there is nothing to show |
| `empty` | `List` succeeded with no shortcuts and there is no return target |
| `ready` | At least one shortcut, or a return target |

A return target wins over a list error so Continue stays on screen. Applications
do not make Home ready: they stay on the Applications route, including group
rows and open windows. An empty favorites list is the Home empty state even
when applications exist.

## Opening a shortcut

The favorites row and the Applications route share `entryRequested`. `launch`
sends `WebShortcuts1.Open(id, request_id)` when the id is in the shortcut list,
and `Application1.Launch` otherwise. `Open` returns an `Operation1` path. The
shell polls `Operation1.Get` the same way it polls an application launch: the
tile stays busy, a duplicate request is ignored, and the operation state is
shown when it finishes. `succeeded` is "Done". Any other terminal state shows
the operation error string.

Until a browser provider is wired into the daemon, `Open` still finishes
`Unavailable`. The shell calls `Open` anyway and shows that result. It does not
pre-disable the tile and it does not open the URL itself.

## Verification

`apps/arinn-shell/catalog_test.cpp` (`webShortcuts`, private bus) covers loading
(recorded from `changed` when the daemon appears), an empty list, `Changed`
refreshing the list, monograms, absence of the URL and icon in the presentation
model, `Open` deduplicated while the operation is running, a failed operation
whose error is `Unavailable`, a succeeded operation, an unknown id, a failed
`List` (error) and daemon disappearance (unavailable). The existing catalog
cases still drive `Application1.Launch` and now expect Home to be empty when
the fixture serves no shortcuts.

The offscreen Qt Quick Home test is `tests/ui/tst_home.qml`. It drives
`HomeView` with local fixtures. This change does not modify `HomeView`:
`favoriteEntries` and `entryRequested` already carry a shortcut id.

`apps/arinn-shell/backend_test.cpp` is the production shell check started from
`tests/integration/phase1.py`. It asserts that Home favorites are the daemon
web-shortcut list, that an application does not make Home ready, and that the
application tile is on the Applications route and not under the favorites row.
Local focus still survives a refresh, and keyboard movement during a running
activate still reaches that tile and returns to the menu button.

Local run on 2026-09-28, Qt 6.11.2, offscreen platform and software rendering,
private session bus: `dbus-run-session -- arinn-catalog-test` passed (the
existing catalog cases and `webShortcuts`), and `arinn-ui-test -input tests/ui`
passed, including `HomeNavigation`. The same Qt build then ran
`arinn-shell-test` against `arinnd` and the delayed demo provider:
`BackendTest::typedWireAndLocalFocus` passed. Operator tests were not run.
