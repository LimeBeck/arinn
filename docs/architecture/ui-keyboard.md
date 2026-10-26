# On-screen keyboard

The on-screen keyboard lets every text field in the shell be filled with the
remote alone ([specification §6.2](../specifications/arinn-specification.md#62-keys),
[ADR-0103](../adr/0103-on-screen-keyboard.md), handoff *On-screen keyboard*
KB-1…8, [#418](https://github.com/LimeBeck/arinn/issues/418)). The same
component also types into the focused external application in its *external
mode*; there the platform layer delivers the text
([ADR-0105](../adr/0105-global-on-screen-keyboard-platform-text-delivery.md),
[transport](external-keyboard.md), [#434](https://github.com/LimeBeck/arinn/issues/434)). The open points from the handoff are DR-39…DR-48 in the
[integration review](../design/integration-review.md).

## Parts

- `KeyboardModel` (`apps/arinn-shell/keyboard_model.*`, registered as
  `Arinn.Shell/KeyboardModel`) owns the layout and the focus rules. Rows from
  the top: an optional context row (`address` or `path`), three letter rows of
  twelve (Russian or English, or the digits and symbols layer) and the action
  row: Shift · 123/ABC · RU/EN · Space · ‹ · › · Backspace · main key. Widths
  are the canvas's flex units (`arinn On-Screen Keyboard.dc.html`, layout 1a).
  Left and Right never wrap. Up and Down go to the key of the next row whose
  centre is nearest. `activate()` returns what a key does; layer, language and
  Shift keys change the model.
- `ui/OnScreenKeyboard.qml` draws the panel: up to four suggestions, the rows
  and a hint line. It applies a key to its target `TextInput` with `insert()`
  and `remove()`, which report `textEdited` as typing does. Input past
  `maximumLength` is ignored, not truncated. One focus item takes every key:
  arrows move inside the panel, OK presses, Back asks the field to hide the
  panel, Up from the suggestions (or from the top row without suggestions)
  asks it to take focus back. A hardware keyboard keeps typing into the target
  while the panel is open, and Backspace edits text. Home and Menu are not
  handled and reach the screen. On Site or search Menu opens navigation; in the
  shortcut editor and a Settings editor Menu stays with the editor, as it does
  without the panel, and Home leaves them.
- `ui/KeyboardButton.qml` is the pill in the field ("Keyboard" / "Hide"), the
  only way to open the panel.
- `ui/KeyboardIcon.qml` draws the line icons on a `Canvas`, like the other
  shell icons.

## Defaults and memory

Address and path fields open in English letters; other fields in the
interface language. The first open focuses the fifth key of the middle letter
row. The language and the last key are remembered per field (`memoryKey`) in
process memory and are lost when the shell exits; the typed text never is.
Shift is one-shot. 123/ABC keeps the focus position; RU/EN resets the symbols
layer. A reopen resets Shift and the layer.

## Fields

| Field | Context row | Suggestions | Main key |
| --- | --- | --- | --- |
| Site or search (KB-1) | address | recent entries, then open services, up to four (`BrowseCatalog.suggestions`) | Open |
| Shortcut address (KB-2) | address | open services' origins (`BrowseCatalog.openServices`) | Done |
| Shortcut title (KB-3) | none | title of the page open at that address, then the host (`BrowseCatalog.nameSuggestions`) | Done |
| String setting (KB-4) | none | none; the line names the length limit | Save |
| Path setting in Settings (KB-5) | path | none yet: subfolders need the folder-listing method (DR-27, DR-44), so the row stays empty | Save |

Matching lives in `BrowseCatalog` (C++): word prefixes of titles and hosts,
case-folded, as on the address and search screen. Open services are active
browser tabs with an `http` or `https` origin, one per origin; their value is
the origin, so `Browse1` and the web shortcut rules take it as typed.

**Site or search.** The pill sits at the right of the field; Right at the end
of the text reaches it, and without a hardware keyboard the screen starts on
it. With the panel open the title and the field move up (y 64 / 160), the
intent line stays under the field, and the list and Back give way to the
panel. With the panel hidden and an empty field the screen keeps the V-2b
recent list with Clear; while typing it lists the same suggestions as the
panel. Web shortcuts are not suggested ([§7.1](../specifications/arinn-specification.md#71-browser-chromechromium)).

**Shortcut editor and string settings.** Their fields sit in a modal sheet, so
the pill opens a full-screen typing layer as drawn in the handoff: the title
with the field's name, the field on top and the panel below. The text goes to
the form at once. In the shortcut editor Done moves on (Title to Address,
Address to Save); in a string setting Save commits the value as the sheet
does, and the provider's rejection shows under the field (SV-4). Back returns
to the field's pill. An empty shortcut title shows which host will be used,
and `Backend::saveShortcut` saves the host (DR-43). A string setting shows
"N / max" from 75% of limits up to 64, in the error colour at the limit.

**Path settings.** A `path` setting keeps its inline text editors: a single
path, a path list item and *Add path* ([ui-settings.md](ui-settings.md)). While
one is open, a pill sits at its right; Right at the end of the text reaches it,
and Left returns to the text. Without a hardware keyboard the editor opens on
the pill. The pill opens the same full-screen typing layer as a string setting,
with the `path` context row (`/ ~/ /media/ /run/media/ /mnt/ .. - _ .`) and
English letters. The text goes to the editor at once. Save proposes the path
as Enter does: a single path is proposed alone, and a list item or *Add path*
proposes the whole list. An empty list entry is not proposed, and the layer
stays open. Back returns to the pill with the text kept, and a second Back
closes the editor without proposing it. The pill is not a text field, so Esc
and Backspace are Back there, as on the string sheet's pill (§6.2, AC-18). Up and Down on the pill close the
editor and move to the next row, as they do from its text. Home leaves
without proposing the draft. Menu reaches the shell. The handoff's main key
Go belongs to the folder picker; an inline editor saves, so its main key is
Save, as #426 specifies. The daemon checks the path rules (ADR-0065) when
the value is proposed.

## External mode

The keyboard key (`keyboard_shortcut`, default Meta+F10, and the remote's
microphone) opens the panel over the focused external application when the
daemon reports `ExternalKeyboard1.Opened`.

- `ExternalKeyboard` (`apps/arinn-shell/external_keyboard.*`, QML type
  `Arinn.Shell/ExternalKeyboard`) is the daemon client:
  - `open` follows `Opened`/`Closed` of the current session; a daemon owner change ends it. A `Closed` for a session it has not seen open is final, so a late `Opened` for it shows nothing (the daemon also never sends `Closed(S)` before `Opened(S)`);
  - `input(key)` relays the panel keys the platform captured;
  - `type()`, `press()` and `close()` call the daemon asynchronously, and a `Conflict` hides the panel;
  - it validates text and key names before any call and never logs or keeps them.
- `ui/KeyboardPanel.qml` is a separate window along the bottom of the screen,
  holding `OnScreenKeyboard` with `external: true`:
  - no suggestions and no context row; the main key is "Enter" and sends `return`;
  - Backspace, ‹ and › become `backspace`, `left` and `right` for the application;
  - Up from the top row stays in the panel; there is no field to return to;
  - the keys are drawn focused although the window never has focus (`lit`).
- `Main.qml` shows the panel only while the session is open, the shell is off
  screen and the lifecycle guard is not blocked. Home, the quick menu's sheet,
  the power scope, ambient, both launch overlays (an application's included,
  also "Starting…" and "Stopping…", ADR-0110) and the player all live in the
  shell window, so the shell coming on screen for any of them hides the panel
  at once and closes the session, and so does a lock. A session that opens
  while the shell is on screen is closed at once. Back (`input("back")`) hides
  the panel and closes the session.
- `platform/keyboard` (`prepareKeyboardPanel`) sets the window up before its
  first show, so that it never takes focus:
  - on X11 it is override-redirect with `WM_HINTS input = False`;
  - on KDE/Wayland it is a frameless window that does not accept focus,
    titled `org.arinn.KeyboardPanel` for KWin's window rule and the KWin script,
    which places it.
- The panel window has no transient parent: the hidden shell window must not
  carry it.

The panel never receives key events: the remote's arrows, OK and Back reach
it only through the platform, as `ExternalKeyboard1.Input`. Menu and every
other key stay with the application; the quick-menu key and Home stay
global and replace the panel with the shell.

## Not covered yet

- **Path suggestions and the folder picker (KB-5, PK).** The handoff draws
  the path keyboard inside the folder picker. Its suggestions are the
  subfolders of the typed parent that match the last segment, folders only,
  and a missing path shows "No such folder". Both need the folder-listing
  method (DR-27, DR-44), which does not exist yet. Until then the Settings
  path editors type with the panel and an empty suggestion row
  ([#426](https://github.com/LimeBeck/arinn/issues/426)). The picker's task
  reuses the same panel with Go as its main key.
- **`~/` in a path (owner decision pending).** The `path` context row offers
  `~/` (KB-5), but `arinn_protocol::settings::path` accepts only absolute
  paths, and nothing decides yet whether `~/` expands to the home directory
  ([#479](https://github.com/LimeBeck/arinn/issues/479), `needs-owner`). Until
  then a path starting with `~/` is relative, as in the contract: Save is
  blocked and the hint asks for a leading `/`.
- **Shortcut address format.** The address rules belong to the daemon
  (ADR-0043, ADR-0045), and the shell has no copy of them. Done is not blocked
  while typing; a rejected address shows as the editor's error after Save. A
  check while typing needs a daemon validation method (a `contract-change`
  task).
- **Pattern errors.** A `pattern` constraint is checked by the provider when
  the value is saved; the shell does not evaluate provider patterns while
  typing, so the main key is not blocked before Save.

## Verification

- `arinn-external-keyboard-test` (`external-keyboard`, and `external-keyboard-x11`
  on its own Xvfb with Qt's xcb platform):
  - the client follows `Opened`, `Input` and `Closed` of its own session only;
  - invalid text and keys never leave the shell;
  - `Conflict` and a daemon owner change hide the panel;
  - the panel's external mode types, sends Enter and the editing keys, has no
    suggestions, and Up from the top row stays;
  - `Main.qml` shows the panel for a session and hides it on Back, Home, the
    shell coming on screen (also with an application launch, "Stopping…"
    included) and a lock, each closing the session; a session opened while
    the shell is on screen is closed at once; a Menu key event
    changes nothing;
  - under X11 the panel window is override-redirect with `input = False`, and
    the application keeps focus while the panel shows. The offscreen platform
    activates every window it shows, so only the X11 run checks focus.

- `arinn-keyboard-model-test` (15 cases): rows and widths, defaults, first
  focus, no wrap, nearest key, entering from above, Shift, layers, language,
  action keys and per-field memory.
- `arinn-browse-catalog-test`: recent entries before open services, the limit
  of four, inactive tabs, no echo of the exact text, open-service origins and
  shortcut name suggestions. `CatalogTest::shortcutEditorAndRevisionValidation`
  covers the host as an empty title.
- `tests/ui/tst_browse.qml`, `tst_shortcut_editor.qml` and `tst_settings.qml`:
  remote-only start on the pill, typing, the context row, Backspace and cursor
  keys, the main key, Back and reopen on the last key, suggestions and leaving
  up, a hardware keyboard while the panel is open, the panel not covering the
  field, Home while typing, the empty-name line and the counter at the limit.
  The path editors are covered by `test_path_without_keyboard_types_with_the_panel`,
  `test_path_panel_keys_and_layout`, `test_add_path_with_the_panel` and
  `test_backspace_on_a_keyboard_pill_is_back` (KB-5; the last also covers
  the string sheet's pill). They cover remote-only typing from the pill, the path context row,
  Save on a single path and on *Add path*, an empty entry kept open, a
  hardware keyboard and Backspace while the panel is open, Menu, Back to the
  pill, Home, and the panel below the field. `test_path_format_blocks_save`
  and `test_path_panel_save_waits_for_a_valid_path` cover the path format
  while typing (#479): a relative path, `~/`, `.` and `..` segments, `//`, a
  control character and 4097 bytes in 2049 characters each show their hint
  and keep Enter or Save from saving; exactly 4096 bytes, a trailing `/` and
  the root are valid.
- 2026-10-09, local build with Qt 6.12 (offscreen, software renderer): the
  three C++ tests and `arinn-ui-test -input tests/ui` (291 cases) passed; the
  rendered Site or search panel and the shortcut address layer were compared
  by eye with `kb-1.png` and `kb-2.png`. Not run: a pixel comparison at 1080p
  and 4K (#120 tooling) and the operator check with the remote on the target
  HTPC, which G-5.6 still needs.
- 2026-10-21, path editors (KB-5, #426), local build with Qt 6.10.2
  (Ubuntu 26.04 container, offscreen, software renderer):
  `arinn-ui-test -input tests/ui/tst_settings.qml` passed 26 cases after
  review. The full
  `ctest` run passed 22 of 23 tests, `ui-components` included. The one failure
  was `sentry-isolated`, a known environment failure of that container,
  where the Sentry sources are overridden. The path layer was not
  compared with `kb-5.png`; the pixel comparison and the operator check are
  still open for G-5.6.
