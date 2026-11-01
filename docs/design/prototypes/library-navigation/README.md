# Library navigation prototype

Editable HTML/JS derivative of the owner's [interactive design prototype](../../handoff/design/arinn.dc.html), prepared for #629 / #630. The owner requested a prototype-first workflow on 2026-11-01 and then asked to continue shell alignment after viewing the prototype. LibraryView follows this presentation in the same PR. The historical handoff and its runtime remain unchanged; owner-only merge is still required.

Serve the repository root, then open:

```sh
python -m http.server 8765 --bind 127.0.0.1
```

[Open the prototype](http://127.0.0.1:8765/docs/design/prototypes/library-navigation/library.dc.html). It opens directly in Library. Onest comes from the repository; React and the icon font use pinned public CDN versions. An internet connection is needed for those dependencies. All content, folders and playback are synthetic demo data; the prototype does not scan files or fetch media metadata.

The derivative retains the handoff's 1920×1080 stage, typography, colours, seven-column poster grid, card sizes, pills, focus styles, spatial remote navigation and series detail layout. Placeholder artwork stays in the original design's abstract style.

- **Catalogue / Folders:** two view pills, using the original category-pill treatment. All / Movies / TV Shows appear only in Catalogue.
- **Folders:** configured roots, nested physical directories and files, a breadcrumb and Up one level. Escape/Back goes up and restores the opener. Both modes keep their own location. A physical directory named Series is distinct from the aggregate TV Shows category.
- **Series:** retain the original season pills and episode rail. A separate Without season group demonstrates files whose season is unknown. Physical episode cards open that same series at the selected episode.
- **Empty / unavailable folders:** keep the path and Back action; an unavailable root retains its last-known entries.

Use the mouse or arrow keys and Enter. Escape/Backspace is Back; H returns Home. The remaining original prototype screens are still accessible through its navigation menu.

Direct scenes (append to the prototype URL):

| Query | State |
| --- | --- |
| `?scene=library` | Catalogue |
| `?scene=folders` | Configured roots |
| `?scene=nested` | Inside Media |
| `?scene=empty-folder` | Empty nested folder |
| `?scene=offline` | Unavailable root with retained content |
| `?scene=seasons` | Series with known season groups |
| `?scene=unknown-season` | Without season |

Russian is the default; add `&lang=en` to any scene for English.

## Browser previews

These are 1920×1080 Chromium captures of the HTML prototype itself:

| State | Preview |
| --- | --- |
| Catalogue | [Screenshot](screenshots/catalogue.png) |
| Inside Media | [Screenshot](screenshots/folders.png) |
| Seasons | [Screenshot](screenshots/seasons.png) |
| Without season | [Screenshot](screenshots/without-season.png) |
| Empty folder | [Screenshot](screenshots/empty-folder.png) |
| Unavailable root | [Screenshot](screenshots/offline.png) |

Browser checks cover switching views, nested folders, opening a physical episode
in series detail, returning to its exact card, Back/up restoration, preserving
the physical location across modes, empty/offline states, separate unknown-season
episodes, direct season-scene Back, returning from Home and English labels.

To review only the differences from the source design:

```sh
git diff --no-index -- docs/design/handoff/design/arinn.dc.html docs/design/prototypes/library-navigation/library.dc.html
```

The implementation specification and ADR-0124 describe the product behavior. This prototype proposes its visual presentation; it does not amend the historical handoff or mark roadmap gates complete.

## Shell implementation previews

These captures come from the Qt LibraryView after alignment with the prototype,
using synthetic fixtures. Fixtures use English labels; Russian UI translations
are included and checked separately. Empty artwork slots remain generic; the
prototype's abstract placeholder artwork is not a production poster source.

| State | Qt shell |
| --- | --- |
| Inside Media | [Screenshot](implementation/library-folder-nested.png) |
| Empty folder | [Screenshot](implementation/library-folder-empty.png) |
| Unavailable root | [Screenshot](implementation/library-folder-offline.png) |
| Season rail | [Screenshot](implementation/library-series-groups.png) |
| Without season | [Screenshot](implementation/library-series-without-season.png) |

The focused QML suite checks mode-specific card/scroll restoration, raised season
rails with visible captions, two-step detail Back, physical episode-to-series
selection and return to the file card, and focus ownership under an overlay.
The capture manifest also renders these states at 1080p and a 4K canvas.
