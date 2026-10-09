# On-screen keyboard

The on-screen keyboard lets every text field in the shell be filled with the
remote alone ([specification §6.2](../specifications/arinn-specification.md#62-keys),
[ADR-0103](../adr/0103-on-screen-keyboard.md), handoff *On-screen keyboard*
KB-1…8, [#418](https://github.com/LimeBeck/arinn/issues/418)). It is part of
the shell, not a platform input method, so it works the same on KDE/Wayland and
X11/EWMH. The open points from the handoff are DR-39…DR-48 in the
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
  handled and reach the screen.
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

## Not covered yet

- **Paths (KB-5).** The handoff draws the path keyboard in the folder picker
  (PK), which waits on the folder-listing method (DR-27). The single-path and
  multiple-path text editors in Settings stay hardware-keyboard fields until
  then ([#426](https://github.com/LimeBeck/arinn/issues/426)).
- **Pattern errors.** A `pattern` constraint is checked by the provider when
  the value is saved; the shell does not evaluate provider patterns while
  typing, so the main key is not blocked before Save.

## Verification

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
- 2026-10-09, local build with Qt 6.12 (offscreen, software renderer): the
  three C++ tests and `arinn-ui-test -input tests/ui` (291 cases) passed; the
  rendered Site or search panel and the shortcut address layer were compared
  by eye with `kb-1.png` and `kb-2.png`. Not run: a pixel comparison at 1080p
  and 4K (#120 tooling) and the operator check with the remote on the target
  HTPC, which G-5.6 still needs.
