# Handoff: Arinn — TV shell for a Linux HTPC

> Product name: **Arinn** (Old Norse “hearth”). Always capitalised — “Arinn”, Onest 600, set right of the **A1 Classic** mark (roof + ember). Mark: `assets/arinn-mark.svg`; source sheet: `assets/logo-direction-A.png`. Brand colours: Ember #FF8A3D (system accent), Stone White #EDEDED (mark), Charcoal #0F1115 (app-icon background).

## Overview
Arinn is a full-screen TV interface for a home theater PC running Linux. It brings together three equal usage modes: online services (Browser, Kinopoisk, IVI, YouTube), local media (movies, TV shows, downloads) and games (Steam, EmulationStation). It is used from 2–4 m on a 55–75″ 4K TV, with an air mouse, keyboard, remote/D-pad and, later, a gamepad. The goal is that the Linux computer behind the TV disappears.

Visual direction: **2a «Глубина со светом» (Depth with Light)**. It is a quiet OS built from translucent planes with a spatial focus model. The environment only lights up with artwork colour when a movie or episode has focus.

## About the Design Files
The files in `design/` are **design references built in HTML**: prototypes of the intended look and behavior, not production code. The task is to **recreate them in the target environment**. For a TV shell on Linux that is typically a full-screen Chromium/Electron/Tauri window with React/Svelte/Solid, or Qt/QML. Use the project's established patterns if they exist; otherwise pick the most suitable stack. Do not ship the HTML directly.

Open `design/arinn.dc.html` in a browser (next to `support.js`) to use the interactive prototype. Click once to give it keyboard focus.

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
A floating glass panel at x72, y112–968, w500, r44, padding 40/28.
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
- `screen`: boot · home · library · detail · player · downloads · apps · settings · activity · ambient · off
- Overlays: `nav`, `sheet {kind, ret, parent}`, `power {step: menu|confirm, action}`, `dialog {kind}`, `launching`
- `activity`: id of the running external app, or null
- `focus`, plus per-screen memory of the last focus (restored on return)
- `pointer`: input mode
- Media: `prog[id] = {pos}` or `{season, ep, pos}`; `cwOrder` (Continue order, most recent first); `lib`; `newIds`
- `fav`: pinned app ids (ordered); `dls[]` `{id, st, pct, sp}`
- Player: `play {id, season, ep, pos, dur}`, `playing`, `osd`, `subs`, `audio`
- Settings toggles: `autoscan, showNew, autonext, skipIntro, night, uisounds, lightBg`
- Continue rules: at most 1 activity + 4 media. A movie within 90 s of the end is finished and removed. A finished episode advances to the next one at position 0.

Integration points: a media library scanner/metadata source (titles, years, runtime, artwork, dominant hue), a Transmission RPC adapter that exposes only the states above, an app launcher and window manager (launch, focus, and a snapshot/texture of the running app), a player (mpv or similar) with track lists, and system power actions.

## Assets
- **Fonts**: Onest (Google Fonts); Material Symbols Rounded (Google Fonts). JetBrains Mono is used only for placeholder labels and is not part of the UI.
- **Service logos**: supplied by the user and bundled in `design/.image-slots.state.json` as WebP data URLs, keys `logo-browser`, `logo-kinopoisk`, `logo-ivi`, `logo-youtube`, `logo-steam`, `logo-es`. Adopted treatment is **B, “logo in chip”**: the full-colour app icon fills its own chip (60–88 px, r18–22), and the name is always set in text next to or under it. Transparent marks get a white-10% chip background. The brand colour never floods the tile; it only appears as the 22% focus glow. Replace these with properly licensed originals in production.
- **Artwork**: placeholders only. Use backdrops (16:9), posters (2:3) and stills (16:9) from your metadata provider.

## Files
- `screenshots/`: 22 JPEG captures of each screen state (01-home … 22-ambient), matching `arinn Screens.dc.html`.
- `design/arinn.dc.html`: the interactive prototype covering all screens and states. Logic (data, i18n RU/EN, focus engine, presets) is in the `<script data-dc-script>` block.
- `design/arinn Screens.dc.html`: a canvas of all 22 screen states.
- `design/arinn Design System v2.dc.html`: tokens, the 3 m viewing-distance check, logo treatments, component state matrix, focus rules, motion, map and keys.
- `design/arinn Directions.dc.html`: the concept exploration (1a/1b/1c and the chosen 2a).
- `design/support.js`, `design/image-slot.js`, `design/.image-slots.state.json`: runtime needed to open the files locally.
