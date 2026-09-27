# Согласование developer ТЗ и дизайн-handoff

Источники: [Developer Specification v0.3](../specifications/developer-specification-v0.3.md) и [Design handoff](handoff/README.md). Ни один исходник не изменён. Исходные расхождения сохранены ниже как история. Текущие решения получены 2026-09-27 из явных ответов пользователя и делегированного технического выбора.

## Обновление handoff от 2026-09-27

Текущий комплект получен из `design_handoff_arinn (1)/design_handoff_arinn`; предыдущий сохранён в [архиве](archive/2026-09-27-before-brand/README.md). Обновлены README, три HTML-референса (прототип, экраны, дизайн-система) и все 22 скриншота; добавлены SVG A1 Classic и лист логотипов в `assets/`.

Основные изменения: A1 Classic вместо amber dot mark; Ember вместо amber; новый error token. Размер знака: 36 в Home, 38 в nav, 120 при startup. Световой focus сохранён, Ember не используется как цвет focus. Фон и текст UI сохраняют прежние токены.

Новый README по-прежнему описывает Back на Home как открытие nav и Sleep как screensaver. Принятые пользовательские уточнения DR-01 и DR-06 ниже остаются в силе; обновление визуальных материалов их не отменяет. Runtime gates также остаются открытыми.

## Текущий статус

| Вопрос | Решение / оставшаяся проверка |
| --- | --- |
| DR-01 | Закрыт пользователем: Back на Home → Activity, без Activity → nav. [Таблица переходов](../architecture/product-decisions.md). |
| DR-02 | Закрыт пользователем: Transmission в MVP; [DownloadProvider API review](../plugin-api/mvp-extensions.md) оформлен отдельно. |
| DR-03 | Выбраны folders library + внешний mpv; tracks/timeline API описан. [Media MVP](../architecture/media-mvp.md). OSD поверх внешнего видео требует runtime gate. |
| DR-04 | Закрыт: library provider хранит resume, media provider — playback, shell — interaction state. [Media MVP](../architecture/media-mvp.md). |
| DR-05 | WindowThumbnail и near/far проверены, last-frame можно читать отдельным Qt renderer. Capture lifecycle/performance ещё требуют реализации. [Experiment](../architecture/background-experiment.md). [Phase 0](../architecture/phase-0-gate.md). |
| DR-06 | Закрыт пользователем: Sleep = OS suspend; ambient отдельно. [Продуктовые решения](../architecture/product-decisions.md). |
| DR-07 | Закрыт: browser-first порядок сохранён, UI limit 1+4 не ограничивает registry; New — generic Library projection. [Interaction](../architecture/activity-and-interaction.md). |
| DR-08 | Закрыт: animation duration не означает completion, pending/error определяется operation. [Interaction](../architecture/activity-and-interaction.md). |
| DR-09 | Закрыт на уровне архитектуры: typed settings schema, platform actions, live registry. Wire schema перед settings implementation. [Interaction](../architecture/activity-and-interaction.md). |
| DR-10 | Player keys определены; G60S Pro выдаёт стандартные клавиши. Esc/Backspace → Back в shell, с исключением Backspace при редактировании текста; отдельная интеграция не нужна. [Interaction](../architecture/activity-and-interaction.md). |

Дизайн-токены остаются заданными. Продуктовых вопросов, блокирующих старт Phase 0, не осталось; непроверенные platform возможности не выдаются за подтверждённые.

## Совпадающие направления

TV/D-pad-first, аэромышь, общее домашнее пространство, Home/Library/Downloads/Applications/Settings, единый Continue, отдельные online/Steam/ES-DE entry points и возврат в Activity присутствуют в обоих документах. Направление 2a «Глубина со светом» конкретизирует Quiet Spatial OS. В handoff «Сейчас запущено» — подпись Activity внутри Continue и hero; отдельная конкурирующая секция из этого не следует.

Handoff допускает Qt/QML и требует учитывать установленный стек. Поэтому поставка HTML-прототипа и смена стека не требуются. Source files остаются reference assets, независимо от JavaScript-модели демонстрации.

## Исходные расхождения и недостающие контракты (история)

| ID | Developer ТЗ | Design handoff | Что требуется решить |
| --- | --- | --- | --- |
| DR-01 · Back/Home | §6, 48 и AC-06: закрытие Home возвращает к Activity; Back завершается переходом Home → active Activity. | Input model / Global navigation: Back на Home root открывает nav; Back в nav закрывает его. Возврат к Activity — через Return card/nav/favorite. | Определить Back на Home с Activity и без неё. Нужны отдельные переходы закрытия Home и открытия nav, без бесконечного цикла Home ↔ nav и потери resume. Ни одна версия не выбрана автоматически. |
| DR-02 · Downloads | §13: только зарезервированный route до отдельного API review; DownloadProvider в v1 заранее не вводится. | Downloads / State Management: реальные статусы, pause/resume/retry, очередь, progress badge, Transmission adapter и автоматическое добавление в Library/New. | Решить сроки реализации и отдельно спроектировать download capability и взаимодействие с Library. Сохранённый дизайн не означает включение работающего Downloads в MVP. |
| DR-03 · Player/library | §12, 22, 23: MPRIS transport controls и концептуальный LibraryProvider; собственного media engine нет. | Library / Detail / Player: scanner, metadata, missing files/removal, audio/subtitle track lists, chapters, buffered range, seek thumbnails; settings добавляют autoscan, autonext, skipIntro. | Выбрать внешние providers/player и определить недостающие операции и metadata. UI плеера допустим над внешним engine, но текущего MPRIS-контракта недостаточно для всего дизайна. mpv упомянут как пример, не выбранная реализация. |
| DR-04 · Ownership/resume | §38–39: provider — источник истины; core не копирует media database. | State Management: `prog`, `lib`, `newIds`, `dls`, `play`, Continue order; фильм завершён за 90 s до конца, episode advances. | Разнести mock state между interaction state и provider data; определить владельца watched/resume и completion policy. Не превращать демонстрационные JS arrays в БД shell. |
| DR-05 · Activity texture | §34–36: platform boundary, gate Home/overlay/focus на Wayland. | Activity / Depth: live compositor texture или last-frame snapshot; near/far scale, blur, dimming и app hue, возврат с анимацией. | Проверить доступ к texture/snapshot и реализацию эффектов в Phase 0; отдельно согласовать fallback при недоступном capture. Не считать screenshot-пример доказательством compositor capability. |
| DR-06 · Sleep/ambient | §7, 34: system actions через platform/provider; semantics Sleep не определена. | Power: «Сон» немедленно ведёт в screensaver; Ambient также включается по idle timeout (10 min по умолчанию). | Определить, означает ли «Сон» suspend, выключение дисплея или только ambient. Нужны правила idle inhibition для играющего видео/игры и wake. Не выдавать прототипный screensaver за OS suspend. |
| DR-07 · Scope and Continue | §1: browser — основной сценарий (~70%); §5, 37: разные resumable activities без указанного лимита. | Overview: три равных режима; Continue — максимум 1 Activity + 4 media, New описан через local media. | Уточнить, «равные» означает доступность или изменение приоритета разработки. Определить отображение нескольких active/resumable activities и универсальность New при других LibraryProviders. Дизайн-лимит UI не должен неявно ограничить registry. |
| DR-08 · Async launch | §27, 54–55: asynchronous completion, timeout/error, UI не ждёт backend. | Motion: launch 1100 ms, startup 1.9 s; прототип переключает демонстрационные состояния. | Отделить duration анимации от результата запуска: когда показывать pending, success, timeout. Таймер не доказывает запуск приложения. |
| DR-09 · Settings/actions | §24, 34: generic actions/platform boundary без полной settings schema. | Settings: сеть, звук, приложения, library folders, playback и system; power confirm перечисляет закрываемые приложения. | Определить typed settings/actions, capability availability и источник running apps. Наличие settings row не разрешает прямой системный вызов из QML. |
| DR-10 · Keys/player | §2, 36: реальные remote/Home/media buttons и global shortcut. | Keys используют H/M/P/A/O и ± как prototype bindings; Player одновременно описывает ←/→ как seek+OSD и arrows как только показ OSD. | Уточнить приоритет player keys, mapping реального пульта, global/local scope и long press. Буквенные shortcuts не считать завершённой интеграцией пульта. |

## Привязка к ADR

| Вопросы | Решения |
| --- | --- |
| DR-01, DR-07 | [Activity/Home](../adr/0010-activity-home.md), [Navigation](../adr/0017-navigation-design-boundary.md) |
| DR-02, DR-03, DR-09 | [Capabilities](../adr/0004-capability-contracts.md), [Plugin API](../adr/0005-ipc-versioning.md), [Media](../adr/0013-mpris-media.md) |
| DR-04 | [State ownership](../adr/0014-state-persistence.md), [Activity](../adr/0010-activity-home.md) |
| DR-05, DR-06, DR-10 | [Platform/gate](../adr/0009-kde-wayland-platform.md), [Navigation](../adr/0017-navigation-design-boundary.md) |
| DR-08 | [Async operations](../adr/0007-async-errors.md) |
| Все | [Design handoff contract](../adr/0019-design-handoff-contract.md) |

## Использование визуальных референсов

Design handoff объявляет colors, typography, spacing, radii, focus и motion timings финальными. Они сохраняются как визуальные требования, а DR-вопросы касаются поведения, возможностей платформы и API. Полные численные значения находятся в исходнике и Design System v2, чтобы не создавать расходящиеся копии токенов.

В handoff номера внутри разделов Screens / Views местами отличаются от имён файлов screenshots. Для проверки конкретного состояния использовать фактическое имя файла и его изображение; например, `04-home-over-activity.jpg`, `10-player-osd.jpg`, `16-power.jpg`, `22-ambient.jpg`. Исходная нумерация не исправлялась.

Весь bundle сохранён вместе с hidden image-slot state, support.js и image-slot.js. Runtime прототипа не превращается в production dependency. Artwork остаётся placeholder, а указание handoff о production fonts/logos сохраняется; дополнительные загрузки assets при документировании не выполнялись.
