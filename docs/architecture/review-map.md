# Карта архитектурного ревью

Источник: [Developer Specification v0.3](../specifications/developer-specification-v0.3.md). Это навигация по исходному ТЗ и предложенным ADR, а не новая версия требований. Продуктовые требования, roadmap и acceptance criteria сохраняются в исходнике полностью.

## Покрытие исходного ТЗ

Каждому разделу сопоставлены относящиеся к нему ADR. Это карта тематического покрытия, а не утверждение, что текст требований целиком продублирован в ADR.

| Раздел | Тема исходника | ADR |
| --- | --- | --- |
| 1 | Что такое Arinn | [ADR-0001](../adr/0001-product-scope.md), [ADR-0011](../adr/0011-chrome-integration.md) |
| 2 | Основные продуктовые принципы | [ADR-0001](../adr/0001-product-scope.md), [ADR-0003](../adr/0003-external-plugin-processes.md), [ADR-0004](../adr/0004-capability-contracts.md), [ADR-0010](../adr/0010-activity-home.md), [ADR-0017](../adr/0017-navigation-design-boundary.md) |
| 3 | Пользовательская модель | [ADR-0001](../adr/0001-product-scope.md), [ADR-0014](../adr/0014-state-persistence.md) |
| 4 | Основные разделы интерфейса | [ADR-0010](../adr/0010-activity-home.md), [ADR-0017](../adr/0017-navigation-design-boundary.md) |
| 5 | Home | [ADR-0010](../adr/0010-activity-home.md), [ADR-0011](../adr/0011-chrome-integration.md), [ADR-0017](../adr/0017-navigation-design-boundary.md) |
| 6 | Global Home Overlay | [ADR-0009](../adr/0009-kde-wayland-platform.md), [ADR-0010](../adr/0010-activity-home.md) |
| 7 | Global system UI | [ADR-0009](../adr/0009-kde-wayland-platform.md), [ADR-0010](../adr/0010-activity-home.md), [ADR-0013](../adr/0013-mpris-media.md) |
| 8 | Browser | [ADR-0011](../adr/0011-chrome-integration.md) |
| 9 | Chrome integration | [ADR-0011](../adr/0011-chrome-integration.md), [ADR-0015](../adr/0015-security-boundary.md) |
| 10 | Applications | [ADR-0012](../adr/0012-desktop-steam-esde.md) |
| 11 | Steam и ES-DE | [ADR-0012](../adr/0012-desktop-steam-esde.md) |
| 12 | Media | [ADR-0013](../adr/0013-mpris-media.md) |
| 13 | Downloads | [ADR-0001](../adr/0001-product-scope.md) |
| 14 | Архитектура | [ADR-0002](../adr/0002-component-boundaries.md), [ADR-0005](../adr/0005-ipc-versioning.md) |
| 15 | arinn-shell | [ADR-0002](../adr/0002-component-boundaries.md), [ADR-0015](../adr/0015-security-boundary.md), [ADR-0017](../adr/0017-navigation-design-boundary.md) |
| 16 | arinnd | [ADR-0002](../adr/0002-component-boundaries.md), [ADR-0006](../adr/0006-snapshots-subscriptions.md) |
| 17 | Plugins | [ADR-0003](../adr/0003-external-plugin-processes.md), [ADR-0015](../adr/0015-security-boundary.md) |
| 18 | Capability-based API | [ADR-0004](../adr/0004-capability-contracts.md) |
| 19 | Plugin API v1 | [ADR-0004](../adr/0004-capability-contracts.md), [ADR-0005](../adr/0005-ipc-versioning.md) |
| 20 | ApplicationProvider | [ADR-0004](../adr/0004-capability-contracts.md), [ADR-0005](../adr/0005-ipc-versioning.md), [ADR-0012](../adr/0012-desktop-steam-esde.md) |
| 21 | BrowserProvider | [ADR-0004](../adr/0004-capability-contracts.md), [ADR-0005](../adr/0005-ipc-versioning.md), [ADR-0011](../adr/0011-chrome-integration.md) |
| 22 | MediaProvider | [ADR-0004](../adr/0004-capability-contracts.md), [ADR-0005](../adr/0005-ipc-versioning.md), [ADR-0013](../adr/0013-mpris-media.md) |
| 23 | LibraryProvider | [ADR-0004](../adr/0004-capability-contracts.md), [ADR-0005](../adr/0005-ipc-versioning.md) |
| 24 | ActionProvider | [ADR-0004](../adr/0004-capability-contracts.md), [ADR-0005](../adr/0005-ipc-versioning.md) |
| 25 | Snapshot model | [ADR-0006](../adr/0006-snapshots-subscriptions.md) |
| 26 | Events | [ADR-0006](../adr/0006-snapshots-subscriptions.md) |
| 27 | Async operations | [ADR-0007](../adr/0007-async-errors.md), [ADR-0015](../adr/0015-security-boundary.md) |
| 28 | Error model | [ADR-0007](../adr/0007-async-errors.md), [ADR-0016](../adr/0016-cli-diagnostics.md) |
| 29 | Extensibility | [ADR-0004](../adr/0004-capability-contracts.md), [ADR-0005](../adr/0005-ipc-versioning.md) |
| 30 | Versioning | [ADR-0005](../adr/0005-ipc-versioning.md), [ADR-0008](../adr/0008-discovery-lifecycle.md), [ADR-0016](../adr/0016-cli-diagnostics.md) |
| 31 | Plugin Manifest | [ADR-0005](../adr/0005-ipc-versioning.md), [ADR-0008](../adr/0008-discovery-lifecycle.md) |
| 32 | Plugin discovery | [ADR-0008](../adr/0008-discovery-lifecycle.md) |
| 33 | Plugin lifecycle | [ADR-0008](../adr/0008-discovery-lifecycle.md) |
| 34 | Platform layer | [ADR-0009](../adr/0009-kde-wayland-platform.md) |
| 35 | Wayland | [ADR-0009](../adr/0009-kde-wayland-platform.md), [ADR-0018](../adr/0018-demo-testing-gates.md) |
| 36 | Global shortcuts | [ADR-0009](../adr/0009-kde-wayland-platform.md) |
| 37 | Activity model | [ADR-0010](../adr/0010-activity-home.md) |
| 38 | State ownership | [ADR-0006](../adr/0006-snapshots-subscriptions.md), [ADR-0014](../adr/0014-state-persistence.md) |
| 39 | Persistence | [ADR-0014](../adr/0014-state-persistence.md) |
| 40 | XDG filesystem layout | [ADR-0008](../adr/0008-discovery-lifecycle.md), [ADR-0014](../adr/0014-state-persistence.md) |
| 41 | CLI | [ADR-0002](../adr/0002-component-boundaries.md), [ADR-0016](../adr/0016-cli-diagnostics.md) |
| 42 | Diagnostics | [ADR-0008](../adr/0008-discovery-lifecycle.md), [ADR-0016](../adr/0016-cli-diagnostics.md) |
| 43 | Logging | [ADR-0015](../adr/0015-security-boundary.md), [ADR-0016](../adr/0016-cli-diagnostics.md) |
| 44 | Security model | [ADR-0003](../adr/0003-external-plugin-processes.md), [ADR-0005](../adr/0005-ipc-versioning.md), [ADR-0007](../adr/0007-async-errors.md), [ADR-0015](../adr/0015-security-boundary.md) |
| 45 | UI plugins запрещены | [ADR-0004](../adr/0004-capability-contracts.md), [ADR-0015](../adr/0015-security-boundary.md), [ADR-0017](../adr/0017-navigation-design-boundary.md) |
| 46 | Design direction | [ADR-0017](../adr/0017-navigation-design-boundary.md) |
| 47 | Navigation | [ADR-0017](../adr/0017-navigation-design-boundary.md) |
| 48 | Back semantics | [ADR-0010](../adr/0010-activity-home.md), [ADR-0017](../adr/0017-navigation-design-boundary.md) |
| 49 | Non-goals v1 | [ADR-0001](../adr/0001-product-scope.md), [ADR-0015](../adr/0015-security-boundary.md) |
| 50 | MVP providers | [ADR-0001](../adr/0001-product-scope.md), [ADR-0011](../adr/0011-chrome-integration.md), [ADR-0012](../adr/0012-desktop-steam-esde.md), [ADR-0013](../adr/0013-mpris-media.md), [ADR-0018](../adr/0018-demo-testing-gates.md) |
| 51 | Steam / ES-DE в MVP | [ADR-0001](../adr/0001-product-scope.md), [ADR-0010](../adr/0010-activity-home.md), [ADR-0012](../adr/0012-desktop-steam-esde.md) |
| 52 | Предлагаемая структура repository | [ADR-0002](../adr/0002-component-boundaries.md) |
| 53 | Testing | [ADR-0006](../adr/0006-snapshots-subscriptions.md), [ADR-0017](../adr/0017-navigation-design-boundary.md), [ADR-0018](../adr/0018-demo-testing-gates.md) |
| 54 | Failure behaviour | [ADR-0003](../adr/0003-external-plugin-processes.md), [ADR-0006](../adr/0006-snapshots-subscriptions.md), [ADR-0007](../adr/0007-async-errors.md), [ADR-0016](../adr/0016-cli-diagnostics.md), [ADR-0018](../adr/0018-demo-testing-gates.md) |
| 55 | Performance principles | [ADR-0007](../adr/0007-async-errors.md), [ADR-0017](../adr/0017-navigation-design-boundary.md), [ADR-0018](../adr/0018-demo-testing-gates.md) |
| 56 | Packaging | [ADR-0001](../adr/0001-product-scope.md), [ADR-0002](../adr/0002-component-boundaries.md), [ADR-0008](../adr/0008-discovery-lifecycle.md), [ADR-0009](../adr/0009-kde-wayland-platform.md), [ADR-0011](../adr/0011-chrome-integration.md) |
| 57 | Development phases | [ADR-0002](../adr/0002-component-boundaries.md), [ADR-0009](../adr/0009-kde-wayland-platform.md), [ADR-0011](../adr/0011-chrome-integration.md), [ADR-0012](../adr/0012-desktop-steam-esde.md), [ADR-0013](../adr/0013-mpris-media.md), [ADR-0018](../adr/0018-demo-testing-gates.md) |
| 58 | MVP acceptance criteria | [ADR-0008](../adr/0008-discovery-lifecycle.md), [ADR-0013](../adr/0013-mpris-media.md), [ADR-0017](../adr/0017-navigation-design-boundary.md), [ADR-0018](../adr/0018-demo-testing-gates.md) |
| 59 | Архитектурные правила | [ADR-0002](../adr/0002-component-boundaries.md), [ADR-0003](../adr/0003-external-plugin-processes.md), [ADR-0004](../adr/0004-capability-contracts.md), [ADR-0009](../adr/0009-kde-wayland-platform.md), [ADR-0010](../adr/0010-activity-home.md), [ADR-0011](../adr/0011-chrome-integration.md), [ADR-0012](../adr/0012-desktop-steam-esde.md), [ADR-0018](../adr/0018-demo-testing-gates.md) |
| 60 | Критерий правильной архитектуры | [ADR-0003](../adr/0003-external-plugin-processes.md), [ADR-0004](../adr/0004-capability-contracts.md), [ADR-0008](../adr/0008-discovery-lifecycle.md), [ADR-0018](../adr/0018-demo-testing-gates.md) |
| 61 | Итоговая модель | [ADR-0001](../adr/0001-product-scope.md), [ADR-0010](../adr/0010-activity-home.md) |

## Порядок разработки из исходного ТЗ (история)

Актуальные изменения: локальная библиотека/mpv и Transmission включены в MVP по ответу пользователя. [Обновлённые этапы и AC-15–AC-19](product-decisions.md) имеют приоритет над отложенными Downloads в таблице ниже.

| Этап | Результат | Связанные ADR |
| --- | --- | --- |
| 0 — Architecture spike | Qt/QML, Rust, D-Bus, внешний plugin process; Home поверх Chrome и Steam/игры с возвратом focus. Gate до значительного UI. | [ADR-0002](../adr/0002-component-boundaries.md), [ADR-0003](../adr/0003-external-plugin-processes.md), [ADR-0009](../adr/0009-kde-wayland-platform.md), [ADR-0018](../adr/0018-demo-testing-gates.md) |
| 1 — Core skeleton | arinnd, shell, CLI, protocol, Plugin Registry и demo; данные shell только через daemon. | [ADR-0002](../adr/0002-component-boundaries.md), [ADR-0004](../adr/0004-capability-contracts.md), [ADR-0005](../adr/0005-ipc-versioning.md), [ADR-0006](../adr/0006-snapshots-subscriptions.md), [ADR-0008](../adr/0008-discovery-lifecycle.md), [ADR-0016](../adr/0016-cli-diagnostics.md), [ADR-0018](../adr/0018-demo-testing-gates.md) |
| 2 — Applications | Desktop provider; Chrome/Steam/ES-DE → Home → Back → приложение. | [ADR-0009](../adr/0009-kde-wayland-platform.md), [ADR-0010](../adr/0010-activity-home.md), [ADR-0012](../adr/0012-desktop-steam-esde.md) |
| 3 — Chrome | MV3 extension, Native Messaging, BrowserProvider, tabs и online-service tiles; usable browser workflow. | [ADR-0004](../adr/0004-capability-contracts.md), [ADR-0005](../adr/0005-ipc-versioning.md), [ADR-0010](../adr/0010-activity-home.md), [ADR-0011](../adr/0011-chrome-integration.md) |
| 4 — Media | MPRIS provider и global media UI. | [ADR-0007](../adr/0007-async-errors.md), [ADR-0013](../adr/0013-mpris-media.md) |
| 5 — Product MVP | Home/Continue/Favorites/Applications/Settings, activities, persistence, diagnostics, error UX, autostart и packaging. | [ADR-0001](../adr/0001-product-scope.md), [ADR-0002](../adr/0002-component-boundaries.md), [ADR-0008](../adr/0008-discovery-lifecycle.md), [ADR-0010](../adr/0010-activity-home.md), [ADR-0014](../adr/0014-state-persistence.md), [ADR-0015](../adr/0015-security-boundary.md), [ADR-0016](../adr/0016-cli-diagnostics.md), [ADR-0017](../adr/0017-navigation-design-boundary.md), [ADR-0018](../adr/0018-demo-testing-gates.md) |
| 6 — Rich integrations | После стабилизации API: Steam/ES-DE/Jellyfin/download/TV providers. Download capability — только после отдельного review. | [ADR-0001](../adr/0001-product-scope.md), [ADR-0004](../adr/0004-capability-contracts.md), [ADR-0005](../adr/0005-ipc-versioning.md) |

## Приёмка MVP

Ни один критерий пока не отмечен выполненным. Полные формулировки находятся в §58 ТЗ.

| Критерий | Проверяемый результат | ADR |
| --- | --- | --- |
| AC-01 | Автозапуск после входа в HTPC session и показ Home | [ADR-0002](../adr/0002-component-boundaries.md), [ADR-0008](../adr/0008-discovery-lifecycle.md) |
| AC-02 | Основной UI управляется D-pad, Select и Back | [ADR-0017](../adr/0017-navigation-design-boundary.md) |
| AC-03 | Chrome/онлайн-сервис открывается из Home | [ADR-0011](../adr/0011-chrome-integration.md) |
| AC-04 | Видны существующие tabs, работает переключение | [ADR-0011](../adr/0011-chrome-integration.md) |
| AC-05 | Глобальный Home поверх Chrome | [ADR-0009](../adr/0009-kde-wayland-platform.md), [ADR-0010](../adr/0010-activity-home.md), [ADR-0011](../adr/0011-chrome-integration.md) |
| AC-06 | Закрытие Home возвращает в ту же browser activity | [ADR-0009](../adr/0009-kde-wayland-platform.md), [ADR-0010](../adr/0010-activity-home.md), [ADR-0011](../adr/0011-chrome-integration.md) |
| AC-07 | Steam запускается отдельной плиткой | [ADR-0012](../adr/0012-desktop-steam-esde.md) |
| AC-08 | EmulationStation запускается отдельной плиткой | [ADR-0012](../adr/0012-desktop-steam-esde.md) |
| AC-09 | MPRIS session видна и управляется | [ADR-0013](../adr/0013-mpris-media.md) |
| AC-10 | Crash Chrome plugin не роняет shell/daemon | [ADR-0003](../adr/0003-external-plugin-processes.md), [ADR-0018](../adr/0018-demo-testing-gates.md) |
| AC-11 | Установка/удаление provider без product-specific core logic | [ADR-0004](../adr/0004-capability-contracts.md), [ADR-0008](../adr/0008-discovery-lifecycle.md) |
| AC-12 | В shell нет product-specific условий для capabilities | [ADR-0002](../adr/0002-component-boundaries.md), [ADR-0004](../adr/0004-capability-contracts.md) |
| AC-13 | Нет выбора профиля или авторизации пользователя Arinn | [ADR-0001](../adr/0001-product-scope.md), [ADR-0014](../adr/0014-state-persistence.md) |
| AC-14 | Focus и основные действия различимы на TV с трёх метров | [ADR-0017](../adr/0017-navigation-design-boundary.md) |

## Обязательные архитектурные правила

Полный текст правил — §59 ТЗ; их обязательность внутри предложенной архитектуры сохраняется, хотя сами ADR ещё на ревью.

| Rule | Ограничение | ADR |
| --- | --- | --- |
| 1 | Core знает capabilities, а не продукты | [ADR-0004](../adr/0004-capability-contracts.md) |
| 2 | Интеграции — внешние процессы | [ADR-0003](../adr/0003-external-plugin-processes.md) |
| 3 | Shell не общается с внешними приложениями напрямую | [ADR-0002](../adr/0002-component-boundaries.md) |
| 4 | Plugins не поставляют произвольный UI | [ADR-0004](../adr/0004-capability-contracts.md), [ADR-0015](../adr/0015-security-boundary.md), [ADR-0017](../adr/0017-navigation-design-boundary.md) |
| 5 | Platform-specific код изолирован | [ADR-0009](../adr/0009-kde-wayland-platform.md) |
| 6 | External Activity — нормальное состояние Arinn | [ADR-0010](../adr/0010-activity-home.md) |
| 7 | Browser integration входит в MVP | [ADR-0001](../adr/0001-product-scope.md), [ADR-0011](../adr/0011-chrome-integration.md) |
| 8 | Steam/ES-DE — first-class entry points | [ADR-0012](../adr/0012-desktop-steam-esde.md) |
| 9 | Достаточные capabilities позволяют не менять arinnd | [ADR-0004](../adr/0004-capability-contracts.md), [ADR-0008](../adr/0008-discovery-lifecycle.md) |
| 10 | Расширение API проходит отдельное, не product-specific review | [ADR-0004](../adr/0004-capability-contracts.md), [ADR-0005](../adr/0005-ipc-versioning.md) |

## Текущая готовность к разработке

Пользователь делегировал технические решения и ответил на пять продуктовых вопросов. Технический baseline принят в ADR-0020–0022, продуктовые уточнения и расширения — ADR-0023–0025. Исходные вопросы сохранены в ранних ADR как история, рядом стоят ссылки на решения.

| Область | Текущий статус | Документ |
| --- | --- | --- |
| Transport, bindings, identity, revisions, operations, errors/limits | Выбрано для реализации | [Техническая основа](implementation-baseline.md) |
| Discovery, manifest, systemd, Chrome host lifecycle | Выбрано для реализации | [Plugin runtime](plugin-runtime.md) |
| Activity, focus, ownership и timers | Выбрано для реализации | [Interaction baseline](activity-and-interaction.md) |
| Persistence, diagnostics, test harness, performance budgets | Выбрано для реализации | [Техническая основа](implementation-baseline.md) |
| Back, Sleep, media/Downloads scope, устройство | Подтверждено пользователем | [Продуктовые решения](product-decisions.md) |
| Local library и mpv | Компоненты и ownership выбраны | [Media MVP](media-mvp.md) |
| DownloadProvider и rich media interfaces | Отдельный API review модели завершён, wire schemas предстоит реализовать | [API дополнения](../plugin-api/mvp-extensions.md) |
| KWin overlay/focus и эффекты | Home/Return и daemon recovery проверены; near/far и last-frame path подтверждены экспериментом, production capture lifecycle/performance ещё открыты | [Отчёт Phase 0](phase-0-report.md) |
| Shell OSD поверх mpv | Требуется integration gate до media UI | [Media MVP](media-mvp.md) |
| Input G60S Pro | Стандартные keyboard/mouse events; отдельного блокера Phase 0 нет | [Продуктовые решения](product-decisions.md) |

До Phase 0 нет ожидаемых ответов пользователя. Нужны обычные результаты разработки: skeleton, versioned wire schemas, SDK/contract fixtures, runtime spike и затем реализации providers. Это не скрытые вопросы «что выбрать», а работа по принятым решениям.

Полные исходники [дизайна](../design/README.md) и developer ТЗ сохранены без изменений. [Карта согласования](../design/integration-review.md) показывает, что закрыто, а что требует фактической проверки. Полноценный stable Plugin API v1 не объявляется выпущенным до wire schema/SDK/tests.
