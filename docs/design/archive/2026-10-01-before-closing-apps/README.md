# Handoff: Arinn — TV shell for a Linux HTPC

> Product name: **Arinn** (Old Norse “hearth”). Always capitalised — “Arinn”, Onest 600, set right of the **A1 Classic** mark (roof + ember). Mark: `assets/arinn-mark.svg`; source sheet: `assets/logo-direction-A.png`. Brand colours: Ember #FF8A3D (system accent), Stone White #EDEDED (mark), Charcoal #0F1115 (app-icon background).

## Overview
Arinn is a full-screen TV interface for a home theater PC running Linux. It brings together three equal usage modes: online services (Browser, Kinopoisk, IVI, YouTube), local media (movies, TV shows, downloads) and games (Steam, EmulationStation). It is used from 2–4 m on a 55–75″ 4K TV, with an air mouse, keyboard, remote/D-pad and, later, a gamepad. The goal is that the Linux computer behind the TV disappears.

Visual direction: **2a «Глубина со светом» (Depth with Light)**. It is a quiet OS built from translucent planes with a spatial focus model. The environment only lights up with artwork colour when a movie or episode has focus.

## About the Design Files
The files in `design/` are **design references built in HTML**: prototypes of the intended look and behavior, not production code. The task is to **recreate them in the target environment**. For a TV shell on Linux that is typically a full-screen Chromium/Electron/Tauri window with React/Svelte/Solid, or Qt/QML. Use the project's established patterns if they exist; otherwise pick the most suitable stack. Do not ship the HTML directly.

Open `design/arinn.dc.html` in a browser (next to `support.js`) to use the interactive prototype. Click once to give it keyboard focus.

Open services in the prototype: Kinopoisk, IVI and YouTube open as web-service windows. Inside one, **N** simulates the site opening a link in a new window. The **Open services** screen is reached from the nav or the Continue card. Demo props: `simulateErrors` (Kinopoisk is slow → cancel; IVI fails → dialog; Browser fails) and `browserOff` (browser unavailable state).

## Fidelity
**High-fidelity.** Colors, typography, spacing, radii, focus states and motion timings are final. Artwork is placeholder: striped gradients labelled `poster`, `backdrop`, `still`, `video` stand in for real images from the metadata source.

## Coordinate system
- Everything is authored at a **1920×1080 logical canvas**. On a 4K TV, multiply by 2 (device pixel ratio 2).
- The stage scales uniformly to fit: `scale = min(vw/1920, vh/1080)`.
- Safe area: left content edge **120**, right edge **96**, top status row at **y 44–108**.

---

## Design Tokens

### Color (OKLCH, sRGB hex approximations in brackets)
| Token | Value | Use |
|---|---|---|
| bg | `oklch(0.17 0.005 260)` [#131416] | System background |
| bg-deep | `oklch(0.07 0.003 260)` [#050506] | Screensaver (OLED) |
| plate | `rgba(255,255,255,.06)` | Card/tile/row at rest |
| plate-focus | `rgba(255,255,255,.20)` | Card/tile/row in focus |
| plate-highlight | `inset 0 1px 0 rgba(255,255,255,.08)` | Top light edge at rest |
| glass | `oklch(0.22 0.006 260 / .74)` + `backdrop-filter: blur(40px) saturate(1.3)` | Nav, sheets, power, dialogs, HUD |
| text-1 | `oklch(0.97 0.004 260)` [#F4F5F7] | Titles; fill of focused buttons |
| text-2 | `oklch(0.84 0.006 260)` [#C9CBCF] | Metadata, descriptions |
| text-3 | `oklch(0.68 0.008 260)` [#9A9DA3] | Hints only |
| ink | `oklch(0.17 0.006 260)` [#131417] | Text on light fill |
| ember | `oklch(0.75 0.17 50)` [#FF8A3D] | Logo, progress, “running”, current section, “done”, toggle on |
| error | `oklch(0.66 0.21 22)` [#E8474F] | Errors, destructive action tint (22% alpha at rest) |
| scrim | `rgba(0,0,0,.55–.60)` | Under power/dialog |

Ember is **never** a focus colour. Focus is always light/brightness.

### Light field (media focus only)
Three radial layers keyed to artwork hue `H` (take it from the dominant colour of the backdrop):
```
radial-gradient(55% 70% at 74% 22%, oklch(0.48 0.10 H / .8), transparent 70%),
radial-gradient(38% 45% at 36% 58%, oklch(0.42 0.09 H-7 / .4), transparent 70%),
radial-gradient(40% 40% at 100% 90%, oklch(0.32 0.06 H+18 / .45), transparent 70%)
```
- Crossfade **900 ms** (opacity). It always lags behind the focus change, so fast scrolling does not flicker.
- Home: opacity 1. Library: opacity 0.6. Apps, games, settings and system screens: none.
- Settings → Appearance → “Световой фон” (ambient light) turns it off globally.
- Light under text never exceeds L 0.48.

### Typography — Onest (Google Fonts, full Cyrillic + Latin)
| Role | Size / weight / tracking / line-height |
|---|---|
| display (detail title) | 112 / 700 / −3% / 1.0 |
| hero (Home) | 84 / 700 / −2.5% / 1.02 |
| page title | 56 / 700 / −2% |
| dialog title | 48–52 / 700 / −2% |
| section label | 30 / 600 |
| body | 30 / 400 / 1.45 |
| control label | 28–32 / 600 |
| meta | 26 / 500 |
| eyebrow | 24 / 600 / +8% / UPPERCASE |
| caption (companion only) | 24 / 500 |

Rules: nothing below 24; nothing an action depends on below 26. Use tabular numerals for times, percentages and the clock. Allow Russian labels ~30% longer than English: buttons hug their content and never have a fixed width.

Icons: **Material Symbols Rounded**, FILL 1, weight 500. Sizes are 28–44 in controls and 88 in empty/error states. Always paired with a text label unless unambiguous (play/pause, ±10 s, check).

### Spacing
8 icon↔label · 14 eyebrow↔title · 22 hero blocks · 24 between tiles · 32 between cards (fits ×1.06 focus) · 48 between rows · 96 right margin · 120 left content line.

### Radii
12 chip (small) · 18–22 logo chip · 20 poster · 22 episode still · 26 card/tile · 28–30 row/large tile · 44–48 panel/dialog · pill (height/2) for buttons, tabs, HUD.

### Depth / elevation
| Layer | Treatment |
|---|---|
| z0 | Background + light field. Receded activity: scale .90, blur 36, brightness .32 |
| z1 | Plates at rest: white 6% + top highlight 1 px |
| z2 | Focus: scale 1.06 (cards) / 1.08 (app tiles) / 1.02 (rows), translateY −6 px, shadow `0 50px 80px -30px rgba(0,0,0,.9)`, inner ring `inset 0 0 0 3px rgba(255,255,255,.45)` |
| z2+ | Focus glow. Media: `0 0 120px 20px oklch(0.62 0.13 H / .5)`. Apps: `0 0 60px 0 oklch(0.62 0.13 H / .22)`. System rows: none |
| z3 | Nav and sheets: glass. The screen underneath scales to .98 at brightness .55 |
| z4 | Power and dialogs: scrim, and everything else is inert |

The focus ring must be drawn as an **overlay element on top of the artwork** (a sibling at `inset:0` with `pointer-events:none`). An inset box-shadow on the card itself is hidden by the image child.

### Motion (ease-out `cubic-bezier(.2,.8,.2,1)` unless noted)
| Duration | What |
|---|---|
| 120 ms | Press: scale −0.04, then fire the action |
| 200–220 ms | Focus in/out (transform, ring, background, brightness) |
| 400–450 ms | Row and page scroll (translate) |
| 450 ms | Screen enter: fade + scale 1.012→1. No lateral slides between screens |
| 900 ms | Light-field crossfade |
| 320 ms | Nav: translateX −40→0 + fade. Sheet: translateX 64→0 + fade |
| 180 / 500 ms | Player OSD in / out |
| 1100 ms | App launch: logo chip scale .8→1→1.04 + “Открывается …” |
| 450 ms | Return to activity: scale .93→1, blur 12→0 |
| 4 s every 8 s | Screensaver clock drift (ease-in-out) |

---

## Input model
- **D-pad mode** (arrow keys, remote, gamepad): cursor hidden, full focus treatment.
- **Pointer mode** (air mouse): starts on any mousemove. The cursor is visible and hover moves focus. Lift is halved (×1.03, −2 px) so items under the cursor don't jump. After any row or page scroll, hover is ignored for **500 ms** to prevent runaway focus. The first arrow key returns to D-pad mode.
- **Spatial navigation**: candidates are focusables in the active scope that lie in the pressed direction. Score = gap along the axis + perpendicular offset × weight. The weight is 2 for left/right and 1.2 for up/down. The perpendicular offset is multiplied by 0.3 when the two items overlap on the other axis.
- **Scopes** (focus is trapped in the topmost): dialog › power › sheet › nav › screen.
- Horizontal rows keep focus **one item from the edge** (D-pad) or scroll only when the item would be clipped (pointer).

### Keys
| Key | Action |
|---|---|
| Arrows | Move focus |
| OK / Enter | Select. In the player: show OSD; on the timeline, play/pause |
| Back / Esc | Up the hierarchy. On Home root: open global nav |
| Home (H) | Always show Arinn. From an activity: Arinn over it, focus on “Return” |
| Menu (M) | Toggle global nav. In Apps (long-press Menu / O): pin/unpin to Home |
| + / − | Volume HUD (non-modal) |
| Space | Play/pause in the player |

---

## Screens / Views

Live renders of every state are in `design/arinn Screens.dc.html`. Each is a preset of the prototype (`<dc-import name="arinn" preset="…">`).

### 01–03, 16 · Home
Purpose: “What do we want to do right now?” It fits on one screen with no long vertical feed.
- **Top-left**: mark 36 + “Arinn” 28/600.
- **Status (top-right, y 44, h 64, gap 24)**: download chip (only while downloading: pill h48, ember ↓ icon + “82%” 24/600), mute icon (only when muted), wifi, clock 30/500.
- **Hero (x120, y136, w1300)**: eyebrow, title 84, meta 28. It always describes the focused item and what OK will do:
  - activity → “СЕЙЧАС ЗАПУЩЕНО” + ember dot / app name / “Вернуться”
  - continue media → “ПРОДОЛЖИТЬ” / title / “1:13:42 из 2:46:00 · Осталось 1 ч 32 мин” (series: “С1 Э4 · <episode> · Осталось 37 мин”)
  - favorite → “ИЗБРАННОЕ · ОНЛАЙН|ИГРЫ” / name / “Откроется на весь экран” or “Запущено · Вернуться”
  - new → “НОВОЕ” / title / “2023 · 2 ч 21 мин · Genre”
- **Продолжить** (label y372, cards y422): 400×225, r26, gap 32. Max 1 activity + 4 media. It scrolls horizontally when it overflows (4 visible).
  - Activity card: plate, “Сейчас запущено” with an ember dot, logo chip 64 (top-right), name 34/600, “Вернуться” 26.
  - Media card: backdrop, bottom gradient `rgba(10,11,13,.9)→0` over 70%, title 30/600, sub 24/500, progress 6 px (track white 25%, fill ember).
- **Избранное** (label y700, or y372 without Continue): tiles 264×132, r26, gap 24, logo chip 60 + name 28/600, ember running dot at top-right 12. Accepts any count (the user pins and unpins).
- **Новое** (label y930): posters 180×270, r20, gap 24, max 5. Focusing this row translates the content −360, hides the hero and Continue, and shows the focused title + meta inline next to the “Новое” label. The whole section disappears if there is no local media or it's turned off in Settings.
- **Home over activity (16)**: the running app is rendered behind Arinn as a receded layer (see Activity below). With focus on the activity card it comes **near**: scale .95, blur 8, brightness .6, scrim .3, plus an app-hue tint. Otherwise it is **far**: scale .90, blur 36, brightness .32, scrim .6.

### 04 · Global navigation
A floating glass panel at x72, y72–1008, w500, r44, padding 40/28.
- Header: mark 38 + “Arinn” 28/600.
- Items, h88, r28, icon 40 + label 30/600: Главная, Медиатека, Загрузки (ember % badge while downloading), Приложения, Настройки. The current section has an ember dot 10 on the right.
- Separator, then the running activity (h100: ember dot, name 28/600, “Вернуться” 24), then “Питание” (h80).
- Rest: transparent, text-2. Focus: plate-focus + ring + scale 1.02.
- Invoked by Back on Home root, the Menu key, the pointer's left-edge hotspot (28 px strip with a visible 6×96 handle in pointer mode), or Left when nothing is further left on Home, Library, Downloads or Apps.
- The screen underneath scales to .98 at brightness .55. Clicking the scrim or pressing Back closes it.

### 05, 17, 19 · Library
- Title “Медиатека” 56/700 at x120 y96. Tabs at y190: Все · Фильмы · Сериалы (pill h64; focus = white fill + ink; selected but unfocused = ember underline 4 px). Focusing a tab selects it.
- Grid: 7 columns × 210 px, gaps 48 vertical / 40 horizontal, starting x120 y310. Each cell has a poster 210×315 r20, then 16 px, then the title 26 (2 lines max, 600 when focused) and the year 24 text-3. The row pitch is 475 and the grid scrolls so the focused row is at the top.
- Badges: “Сериал” (top-right pill); “Нет файла” (error icon, bottom-left) when the file is missing.
- Missing artwork: a gradient plate with a `movie` icon and the title set in type.
- **Loading (19)**: identical grid geometry with shimmering skeletons (1.4 s linear). Shown on the first open of a session. The layout does not jump.
- **Empty (17)**: centered 160 icon plate (r48), “Медиатека пуста” 52/700, body 30 max-width 820, one button “Открыть настройки” → Settings › Media › Library folders.

### 06 · Movie detail
- Full-bleed backdrop + light field at 50% (screen blend) + scrims: left `bg .96→0` over 68%, bottom `bg 1→0` over 55%.
- Content at x120, padTop 320: eyebrow “ФИЛЬМ”, title 112, meta 30 “2024 · 2 ч 46 мин · 12+ · Фантастика · Приключения”, description 30/1.45 max 3 lines max-width 960.
- Buttons (h88, pill, icon 42 + label 30/600, gap 20): primary **“Продолжить с 1:13:42”** if started, otherwise **“Смотреть”**; “С начала” (only if started); “Аудио и субтитры”. Focus starts on the primary.
- Progress line: 300×6 ember bar + “Осталось 1 ч 32 мин” 26.
- **Missing file**: a notice plate (error icon 44, “Этот файл больше недоступен” 30/600, “Проверьте папку медиатеки в Настройках → Медиа.” 26). The only action is “Убрать из медиатеки”.

### 07 · TV show detail
Same as movie with padTop 180. The primary is “Продолжить · С1 Э4”. Below: season tabs (one per season) and a row of episode stills 420×236 r22 with gap 40 (3 visible). A watched episode shows a check badge; the current one shows a progress bar. The caption is “4. Восьмикратная ограда” 26 + “58 мин” 24. Focusing tabs or episodes translates the page −570 and dims the top block to .18.

### 08–11 · Player
- **Hidden OSD (08)**: video only, cursor hidden.
- **OSD (09)**: top gradient 260 and bottom gradient 420. Title 48/700 at x120 y72 (series subline 28). Clock at top-right.
  - Timeline row at bottom 180: current time 28/600 · timeline · duration 28.
  - Timeline track: 8 px, white 24%, buffered white 18%, fill ember, chapter ticks 4 px.
  - Timeline focused: track 14 px + ring 3 px, knob 40, thumbnail 336×189 r18 with a 3 px white border, and a time chip above the knob.
  - Bottom row at y bottom 72: pills h76 “Субтитры · Русские”, “Аудио · Русский”, “Ещё”, plus hint “←/→ — 10 секунд · OK — пауза” 24.
  - No transport row. Paused: a centered 160 glass circle with the pause icon.
- **Behavior**: while the OSD is hidden, ←/→ seek ±10 s and show the OSD with focus on the timeline, and OK/arrows only show the OSD. With the OSD visible, OK on the timeline toggles play/pause. The OSD auto-hides 4 s after the last input while playing and never while paused. Back hides the OSD; Back again exits to where playback started (Home or Detail) and saves progress.
- **Sheets (10, 11)**: right glass panel w720, padding 112/56/64. Eyebrow “ВОСПРОИЗВЕДЕНИЕ”, title 52/700, options min-h104 r28. Focus = white fill + ink. The selected option has an ember check (ink when focused). Subline 26 (e.g. “Многоголосый · 5.1”). Selecting closes the sheet after 180 ms and returns focus to the opener. From Detail, “Аудио и субтитры” opens a two-row sheet (chevrons) that drills into each list.

### 12, 18 · Downloads
- Title 56 + summary 28 (“1 загружается · 32 МБ/с”).
- Rows h156, r30, padding 0/36/0/18: still 214×120 r18 · title 32/600 (+ “Сезон 3” 26) · status 26 with icon · bar 8 px · action pill (visible on focus only) “OK — …”.

| State | Status text | Icon / colour | Bar | Action |
|---|---|---|---|---|
| downloading | `82% · 32 МБ/с · 12 мин` | downloading / text-2 | ember | Пауза |
| queued | В очереди | schedule / text-3 | — | Загрузить сейчас |
| paused | `Пауза · 40%` | pause_circle / text-3 | white 40% | Продолжить |
| done | Готово · Добавлено в медиатеку | check_circle / ember | — | **Смотреть** (white-filled pill) → Detail |
| error | Не удалось загрузить | error / error | — | Повторить |

- When a download completes, the item is added to Library and to the front of Home › Новое, and a toast “<Title> загружен — можно смотреть” appears.
- **Service unavailable (18)**: cloud_off icon (error colour), “Загрузки недоступны”, “Не удалось подключиться к службе загрузок. Проверьте соединение и попробуйте снова.”, and button “Повторить”. Never show Transmission terminology (peers, seeds, ratio, tracker) in the default UI.

### 13 · Apps
- Title 56. Groups have a label 30/600 and tiles below them with gap 32:
  - Онлайн: Браузер, Кинопоиск, IVI, YouTube, tiles 380×196.
  - Игры: Steam, EmulationStation, tiles 480×196 (wider for character).
  - Другое: “Добавить приложение” (480×120, 2 px dashed white 18%, desc “Любой сайт как приложение”).
- Tile: r30, padding 26/28, logo chip 76 (88 on large tiles) top-left, “Запущено” + ember dot and a pin icon (`keep`, when on Home) top-right, name 32/600 bottom-left.
- The bottom hint shows only while an app tile has focus: “OK — открыть · Меню — закрепить на главной”.

### 14 · Settings
- Title 56. Left category list x120 w540: rows h88 r26, icon 38 + label 30/600. The selected category has an ember dot; it gets plate 8% while focus is in the right pane. Focusing a category selects it.
- Right pane x740: eyebrow = category name; rows min-h112 r28 padding 20/32. Each row has a label 30/600 + desc 26 and either a toggle (88×50, on = ember track + ink knob) or a value 26 + chevron.
- Categories and rows (exact copy in the prototype source, `ROWS`): Медиа, Воспроизведение, Звук, Сеть, Приложения, Оформление, Система.
- Back: right pane → category list → Home.

### 15 · Power
- Centered glass modal w800, r48, padding 52/48/44, title “Питание” 52.
- Rows min-h112 r30: 64 icon circle + label 32/600 + desc 26:
  - Сон — “Экран погаснет, приложения останутся открытыми” (immediate → screensaver)
  - Перезагрузка — “Займёт около минуты”
  - Выключение — “Полностью выключить компьютер”
- Restart and Shut Down open a confirm step: “Выключить Arinn?” / “Перезагрузить Arinn?” + body “Будет закрыто: Кинопоиск.” (or “Запущенных приложений нет.”). Two pills h96: **Отмена (default focus)** and the action, which has an error tint at 22% at rest. Back returns to the menu.
- Invoked from Nav › Питание or the P key.

### Browser failed (18b)
A dialog over the current screen: error icon 64, “Браузер не удалось открыть” 48, body “Попробуйте ещё раз. Если ошибка повторится, перезагрузите Arinn.”, and buttons “Попробовать снова” (default focus) and “Закрыть”.

### Open services (ADR-0050) · OS-1…7, H-1…6
Each web service is its own full-screen Chrome window with no Chrome UI. Arinn lists, switches and closes them. The live canvas is in `design/arinn Open Services.dc.html` and the screenshots are `screenshots/os-*.png`. The only data used is page title, address, and the `active` and `audible` flags.
- **Screen “Открытые сервисы” / Open services**: title 56/700 at x120 y96 (same as other screens), followed by a count in 40/600 text-3. A flat list starts at x120 y210, w≈1500, with rows gap 16. Order: the service on screen first, then by last use.
- **Row**: min-h128, r26, padding 22/28. It has a chip 76 r22, a title 30/600 (1 line with ellipsis; 2 lines when focused), and a subline 24/500 text-2 with the domain plus status: ember dot + “На экране” and/or ember `volume_up` + “Звук”. A separate close button 128×128 r26 sits to the right of the row, gap 16.
- **Focus**: the row uses the standard `row` lift (×1.02, plate 20%, 3 px white-45% ring, 22% glow in the service hue). → moves to the close button (white fill + ink, ×1.06) and ← returns to the row. OK on the row switches to that window. OK on close closes it with no confirmation, even when audio is playing. Back returns to the previous screen. The bottom hint reads “OK Перейти · → Закрыть · Назад Выйти”.
- **Chip**: use the web-shortcut icon when the address matches a packaged shortcut. Otherwise show the first letter of the domain (www./m./hd. stripped), 34/700, on a white-12% chip. No favicons, and no per-product special cases.
- **Empty title**: show the domain as the title and the full address as the subline.
- **Closing (OS-3)**: the row drops to 50% opacity and a spinner replaces the close icon, with the status “Закрывается…”. Focus moves to the next row. When the browser confirms, the row collapses in 220 ms.
- **Many (OS-4)**: vertical scroll that keeps focus 2–3 rows from the bottom, with top/bottom fade masks and a position “9 из 18” at top-right (26/500). Holding an arrow accelerates the scroll.
- **Nothing open (OS-6)**: `stacks` 88 text-3, “Ничего не открыто” 48/700, body 30, and the button “На главную”.
- **Browser unavailable (OS-7)**: `public_off` 88 in the error colour, “Браузер недоступен”, “Онлайн-сервисы сейчас не открываются. Медиатека, загрузки и игры работают.”, and the button “Повторить” (reconnects the plugin).
- **Home (H-1)**: the first Continue card is the current service (“Сейчас запущено” · chip · page title · “домен · Вернуться”). The second card, “Ещё N открыто”, shows stacked chips and opens the list; hide it when only one service is open. An ember dot on a Favorites web-shortcut tile means “already open”. Arinn computes this by matching window addresses to shortcut URLs, so no contract change is needed.
- **Nav (H-2)**: a new item directly below the main sections (no divider), min-h88, padding 12/24, gap 16. It has `stacks` 40, “Открытые сервисы” 30/600, and the subline “На экране: {name}” 22/500 with an ember dot (only when a web service is current). The count uses the same style as the Downloads badge (24/600 ember). When the list is empty the item stays and the count is hidden. When the current activity is a web service, this item replaces the bottom activity row; for external apps (Steam, ES, free-browsing Chrome) that row stays. **The nav panel now spans y72–1008** (was y112–968) so both fit.
- **Browser unavailable on Home (H-3)**: web-shortcut tiles at 40% opacity with greyscale chips and the subline “Недоступно”. They stay focusable, and OK opens OS-7. The status bar shows the chip `public_off` + “Браузер недоступен”. The rest of Home works.
- **Opening (H-4, H-5)**: the existing launch overlay (see Activity) plus an indeterminate bar 360×6 below the text, with a 120 px ember segment looping in 1.4 s. Skip the overlay if the window appears within 400 ms. After 8 s, add “Браузер запускается дольше обычного” and the button “Отменить” (Back cancels too).
- **Failed (H-6)**: after the 30 s timeout or on error, show a dialog as in 18b: “{name} не открылся”, “Браузер не ответил за 30 секунд. Попробуйте ещё раз.”, and the buttons “Повторить” (default) and “Закрыть” (focus goes back to the tile).
- **Transition (#80)**: Arinn cannot animate the Chrome window. Opening: 0–220 ms press feedback, then 220–500 ms Arinn recedes (×.97, blur 12) and fades to a dark tint of the service hue, then the window is raised. Return via Home uses the existing `lt-return` (450 ms from blur), with focus on the current-service card. Switching between services always goes through Arinn.
- **Free-browsing “Браузер” tile**: replaced by the address and search screen (#106, below). Whether a normal Chrome window with tabs stays available is an open owner decision (V-1a/b).

### Address and search (#106) · IN-1…9, T-1, OS-8, V-1…3
A screen for typing a site address or a search query on a hardware keyboard. The result opens as another web service, following all of the rules above (H-4…H-6, OS-7, the Open services row). The live canvas is `design/arinn Browser Input.dc.html` and the screenshots are `screenshots/in-*.png`.
- **Screen**: a separate screen (`screen: browse`), not an overlay. Title “Сайт или поиск” 56/700 at x120 y96. Back/Esc returns to where the user came from, and the typed text is discarded.
- **Field**: x120 y210, w1480, min-h132, r34, padding 26/40/26/32, focused plate white-12% + 3 px white-45% ring. It has a 56 icon at left and text 48/500 with line-height 1.3, plus a 4×54 ember caret. The placeholder is “Адрес сайта или что найти” in text-3. The field grows to 3 lines for long input and keeps the end with the caret in view; earlier lines fade out under a 40 px top mask.
- **Intent line** (below the field, padding-left 32, 30/500 text-2): key chip “Enter” (26/600 on white-12%) + what Enter will do. It updates on every keystroke:
  - Address: `public` icon, “открыть сайт {url with scheme}”; for long input, only the domain.
  - Search: `search` icon, “искать «{query}» в {engine}”.
  - Already open: ember dot + “перейти в открытый {name}” in text-1. Enter switches to that window and creates no duplicate.
  - Invalid scheme: `error` icon and the text “Arinn открывает только адреса http и https”, both in the error colour. Enter does nothing and the field is not cleared.
  - A shortcut suggestion is focused: “открыть {shortcut}”.
- **Shortcut suggestions**: up to 3 web shortcuts, matched by word prefix in the name or address; this is not history. Label “Ярлыки” 26/600, rows w1040 min-h104 r26 with chip 64 + name 30/600 + domain 24, plus “Открыт” with an ember dot when that shortcut’s window is open. ↓ from the field moves to the suggestions and ↑ from the first returns. Any printable key returns focus to the field and is inserted into the text.
- **Input rules**: see the table at the end of the canvas.
  - http/https: used as typed.
  - IPv4, localhost, `*.local` or anything with a port: `http://` is added.
  - A dotted host with a TLD of 2+ letters and no spaces: `https://` is added.
  - Any other scheme: invalid.
  - Everything else: search.
  - Matching against open windows compares the host without `www.`.
- **No keyboard (IN-8)**: the field dims to 60% with the `keyboard_off` icon, and the status bar shows `keyboard_off`. Title “Для ввода нужна клавиатура” 44/700 and a body 30. Shortcuts stay openable with the remote, with focus on the first. The first letter keypress switches the screen to IN-1 with that character in the field.
- **Browser unavailable (IN-9)**: dimmed field, then the OS-7 text and the “Повторить” button.
- **Home tile (T-1)**: recommended rename to “Сайт или поиск” with a neutral chip holding `travel_explore` 36 instead of the Chrome logo (T-1b). T-1a keeps “Браузер”.
- **Second entry (OS-8)**: a “Сайт или поиск” pill button (h80, `add` icon) at the right of the Open services title, reached with ↑ from the first row. There is no new nav item. A search results page is an ordinary row: the title comes from the search engine, with domain `ya.ru` and the letter chip Y.
- **Owner decisions** are shown as pairs; the search engine choice changes only the name in the intent line (plus a Settings › Apps row when it is selectable):
  - V-1a: remove Chrome with tabs. V-1b: keep it as a secondary pill “Открыть Chrome с вкладками” below the field.
  - V-2a: no recent entries. V-2b: up to 5 recent entries with the `history` icon, shown when the field is empty, with the button “Очистить”.
  - V-3a: no saving. V-3b: a bookmark button between the row and Close in Open services; after saving it shows `bookmark_added` and the mark “Ярлык”, and the shortcut goes to Apps › Online.

### Settings editors (ADR-0065, #169) · ED, EN, IN, STR, SV, LF, PK
What opens behind a Settings row chevron. One generic editor per setting kind; providers supply no UI (R-04). The live canvas is `design/arinn Settings Editors.dc.html` (open with `#only=LF-3` etc. to render a single frame). Frames are 1920×1080.
- **Rows (ED-1)**: unchanged from 14. A `boolean` toggles in place. Every other kind shows its value (26) and a chevron, and OK opens its editor.
- **enabled = false (ED-2)**: the row stays visible and focusable. Label and value are in text-3, a `lock` icon replaces the chevron, and the focus plate is 12% instead of 20%. OK does nothing. The desc line reads “Сейчас изменить нельзя”, or the reason if the contract provides one.
- **Editor container**: the right sheet from 10/11 (w720, padding 112/56/64, glass `oklch(0.22 0.006 260 / .95)`). Settings recedes behind it (scale .98, brightness .55). Eyebrow = category, title 52/700, optional desc 26. Back closes without saving, with no prompt, and focus returns to the row.
- **enum (EN-1…3)**: options min-h104 r28 padding 18/28, label 30/600 (wraps, never truncated) + optional sub 26. Focus = white fill + ink, ×1.02. The current option has an ember `check` 40. Focus opens on the current option. OK saves, and the sheet closes after 180 ms. With more than 6 options the list scrolls with focus kept mid-sheet, top/bottom fades and a position “45 из 64” (24/600) right of the eyebrow. Ch+/Ch− or PgUp/PgDn page.
- **integer (IN-1, IN-2)**: stepper plate h240 r36 with − / + circles 96 and the value 160/600 tabular + unit 40/600. Below it: “мин. N · было N · макс. N” 26. ←/→ (and ↑/↓) step by `step`; after 1 s of holding, the step is ×10. Keyboard digits type the value. At a bound the button dims to 30% and a note “Это максимум / минимум” appears. There is no wrap-around. OK saves.
- **string (STR-1, STR-2)**: the field from the shortcut editor (h112 r30, text 36, ember caret 4×46). The current value is prefilled with the caret at the end. The counter “15 из 32 символов” shows when `maxLength` is set. Enter saves, Esc cancels, ↓ goes to the Сохранить / Отмена pills (h88). Without a keyboard the sheet follows IN-8: the field is at 60%, `keyboard_off` shows in the status bar, “Для ввода нужна клавиатура” 36/700, body, the command chip `arinnctl settings set`, and a single “Назад” button.
- **Saving (SV-1…6)**:
  - Pending longer than 300 ms: a spinner replaces the check on the chosen option. Back still closes the sheet and the result arrives as a toast.
  - Applied (SV-2): the sheet closes, the row shows the new value, and a toast “Сохранено · {setting}: {value}” shows for 3 s. Toggles get no toast.
  - Applies at next start (SV-3): a toast with `restart_alt`, and the row desc “Применится после перезапуска Arinn” with an ember `restart_alt` until restart.
  - Rejected: the sheet stays open and keeps the input. For string, an error line under the field (SV-4) that clears on the next keystroke. For enum/integer, an error plate above the list or stepper (SV-5); the check stays on the old value and focus stays on the rejected option. No reply: “Arinn не ответил. Попробуйте ещё раз”. A rejected toggle springs back, shows an error toast, and keeps the reason in the row desc until focus leaves it (SV-6). All error text comes from the daemon.
- **Library folders (LF-1…8)**: a full screen, not a sheet. The row value is “N папок”, plus “· 1 недоступна” when any folder is unavailable. Title 56 + count 40/600 text-3. The “Добавить папку” pill h80 sits right of the title (↑ from the first row). Rows follow Open services: min-h128 r26, chip 76 `folder`, name 30/600 (last path segment), full path 24 mono text-2, and a 128-wide “Убрать” square reached with →. OK on a row opens the library filtered to that folder. Order = order added.
  - Empty (LF-2): 160 plate `create_new_folder`, “Папок пока нет” 52/700, body 30, and the focused button “Добавить папку”. The empty library (17) links here.
  - Unavailable (LF-4): the row stays in place with a `folder_off` chip at 6%, the name in text-2, and an error-coloured status “Недоступна. Фильмы из неё остаются в медиатеке как недоступные”. It can be removed. It returns to normal on its own when the share comes back.
  - Limit 64 (LF-5): “Добавить папку” at 40% but still focusable (OK shows the explanation as a toast), plus the note “Это предел — 64 папки…” under the title, a scrollbar and a bottom fade.
  - Remove (LF-6, LF-7): the power-confirm dialog (w900 r48), “Убрать «{name}» из медиатеки?”. The body says items leave the library and nothing is deleted from disk. Focus starts on “Отмена”; “Убрать” is error-tinted. The verb is always «Убрать», never «Удалить». Afterwards focus goes to the neighbouring row and a toast “Папка убрана” shows.
  - Added (LF-8): the new row is appended and focused, with the status “Добавляется в медиатеку…” and an ember spinner.
- **Folder picker (PK-1…10)**: the second screen after “Добавить папку”. Title “Выбор папки” 56. Path line x120 y200 w1180 h104 r30 in 32 mono, with the parent in text-2 and the last segment in text; long paths collapse the middle to “/…/”. “Выбрать эту папку” pill h88 right of it chooses the path shown (not the focused row); reach it with ↑ from the first row.
  - Left column (w540): start points “Домашняя папка”, each mount under `/media`, `/run/media`, `/mnt` (`usb` / `lan` icon, mount name), and “Весь компьютер” `/`. The current one has an ember dot. With no mounts, a note “Диски и сетевые папки появятся здесь, когда система их подключит” (PK-9). The list updates live.
  - Right column: an eyebrow with the folder count and position “4 из 148”, then rows min-h96 r26 with `folder` 38, a name 30/600 (2 lines, then ellipsis) and a chevron. Folders only; no files and no dot-folders. Sorted by name, case-insensitive. Unreadable folders show `lock` and no chevron.
  - Keys: ↑/↓ move, OK or → enters, ← goes to the start points, Back goes up one level (at a start point, first to the left column, then closes the picker), Ch± pages, and a letter jumps to the first match. Home follows the global rules and discards the pick.
  - Empty (PK-5): “Вложенных папок нет” 44/700 with the body “Файлы здесь не показываются…”; focus goes to “Выбрать эту папку”. No access (PK-6): `lock` 80, “Нет доступа к папке”, and choose dims to 40%. Share gone (PK-7): `link_off` in the error colour, “Сетевая папка отключилась”, “Повторить” (focused) and “К домашней папке”; the start point shows “Отключена”. Arinn never offers to mount anything.
  - Keyboard (PK-8): “/” or any letter turns the path line into a field (`keyboard` icon, caret). The list filters as you type, Tab completes, Enter goes there, Esc cancels. Absolute paths only.
  - Rejected (PK-10): an error plate above the list with the daemon text (“Эта папка уже в медиатеке”, limit reached); the picker stays open.
- **Open questions**: whether `enabled = false` carries a reason; whether integer has a unit; whether “applies at next start” should offer “Перезапустить сейчас”; whether the library provider reports folder availability as an event.

### 20 · Ambient / screensaver
- Background bg-deep. Clock 160/500 in `oklch(0.60 0.004 260)`, date 32 in `oklch(0.45…)`, optional line “Кинопоиск на паузе” with a dim ember dot.
- The whole block moves to a random position every 8 s with a 4 s ease-in-out move, so no static high-contrast pixels remain.
- Entered via Sleep, an idle timeout (Settings › Appearance, default 10 min) or the A key. Any key wakes the previous screen.

### Activity (external apps)
- Launch: a full-screen overlay at `oklch(0.12 / .92)` with a 180 logo chip (glow in app hue) and “Открывается Кинопоиск…” 36. Then the external app goes full-screen.
- The prototype renders the running app as a striped placeholder. In the product this is the real window. For the receded layer, use a live compositor texture or a last-frame snapshot, blurred.
- Home over activity: see Home 16. Selecting the activity card, the nav “Вернуться” item, or its Favorites tile returns to it (450 ms scale-up + unblur).

### Toast and volume HUD
- Toast: bottom-center pill h84, glass, ember icon 36 + text 28/500, shown for 2.6 s.
- Volume: top-center pill 560×88, glass, icon 40 + bar 8 px (white) + value 30/600, hides 1.8 s after the last change, non-modal.
- Startup: mark 120 (ember with glow) + “Arinn” 92/600, 1.9 s.

---

## State Management (as modelled in the prototype)
- `screen`: boot · home · library · detail · player · downloads · apps · settings · services · activity · ambient · off
- `svcs[]` `{id, app|null, t (page title), a (address), audio, closing}`: open web-service windows in recency order. `activity` is either an app id (external) or `w:{id}` (a web service window).
- Overlays: `nav`, `sheet {kind, ret, parent}`, `power {step: menu|confirm, action}`, `dialog {kind}`, `launching`
- `activity`: id of the running external app, or null
- `focus`, plus per-screen memory of the last focus (restored on return)
- `pointer`: input mode
- Media: `prog[id] = {pos}` or `{season, ep, pos}`; `cwOrder` (Continue order, most recent first); `lib`; `newIds`
- `fav`: pinned app ids (ordered); `dls[]` `{id, st, pct, sp}`
- Player: `play {id, season, ep, pos, dur}`, `playing`, `osd`, `subs`, `audio`
- Settings toggles: `autoscan, showNew, autonext, skipIntro, night, uisounds, lightBg`
- Continue rules: at most 1 activity + 4 media. A movie within 90 s of the end is finished and removed. A finished episode advances to the next one at position 0.

Integration points: a browser extension (`tabs` + `nativeMessaging`) that exposes open windows as `{title, url, active, audible}` and accepts focus/close commands; a media library scanner/metadata source (titles, years, runtime, artwork, dominant hue), a Transmission RPC adapter that exposes only the states above, an app launcher and window manager (launch, focus, and a snapshot/texture of the running app), a player (mpv or similar) with track lists, and system power actions.

## Assets
- **Fonts**: Onest (Google Fonts); Material Symbols Rounded (Google Fonts). JetBrains Mono is used only for placeholder labels and is not part of the UI.
- **Service logos**: supplied by the user and bundled in `design/.image-slots.state.json` as WebP data URLs, keys `logo-browser`, `logo-kinopoisk`, `logo-ivi`, `logo-youtube`, `logo-steam`, `logo-es`. Adopted treatment is **B, “logo in chip”**: the full-colour app icon fills its own chip (60–88 px, r18–22), and the name is always set in text next to or under it. Transparent marks get a white-10% chip background. The brand colour never floods the tile; it only appears as the 22% focus glow. Replace these with properly licensed originals in production.
- **Artwork**: placeholders only. Use backdrops (16:9), posters (2:3) and stills (16:9) from your metadata provider.

## Files
- `screenshots/`: 22 JPEG captures of each screen state (01-home … 22-ambient), matching `arinn Screens.dc.html`.
- `design/arinn.dc.html`: the interactive prototype covering all screens and states. Logic (data, i18n RU/EN, focus engine, presets) is in the `<script data-dc-script>` block.
- `design/arinn Screens.dc.html`: a canvas of all 22 screen states.
- `design/arinn Browser Input.dc.html`: the address and search screen (#106): design answers, input states IN-1…9, tile T-1a/b, OS-8, owner-decision variants V-1…3, and input rules. Screenshots are in `screenshots/in-*.png`.
- `design/arinn Open Services.dc.html`: open services (ADR-0050), switchable RU/EN through the `lang` prop: design answers, the list screen OS-1…7, Home/nav/launch states H-1…6, and the transition spec. Screenshots are in `screenshots/os-*.png`.
- `design/arinn Settings Editors.dc.html`: Settings value editors per kind (ADR-0065, #169): design answers, editors ED/EN/IN/STR, saving SV-1…6, Library folders LF-1…8, folder picker PK-1…10, the contract-to-UI map and open questions. Add `#only={ID}` to the URL to render one frame.
- `design/arinn Design System v2.dc.html`: tokens, the 3 m viewing-distance check, logo treatments, component state matrix, focus rules, motion, map and keys.
- `design/arinn Directions.dc.html`: the concept exploration (1a/1b/1c and the chosen 2a).
- `design/support.js`, `design/image-slot.js`, `design/.image-slots.state.json`: runtime needed to open the files locally.
