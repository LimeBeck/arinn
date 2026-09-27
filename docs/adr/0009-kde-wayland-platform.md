# ADR-0009: PlatformService и обязательный KDE/Wayland spike

- Статус: Предложено (Proposed), требуется архитектурное ревью.
- Дата оформления: 2026-09-27.
- Основание: [Developer Specification v0.3](../specifications/developer-specification-v0.3.md), разделы 6, 7, 34, 35, 36, 56, 57, 59.

> Обновление 2026-09-27: Путь KWin adapter выбран; runtime gate остаётся непроверенным. См. [план Phase 0](../architecture/phase-0-gate.md) и [целевой HTPC](../architecture/product-decisions.md). Этот ADR сохраняет формулировку исходного v0.3; при расхождении действуют принятые уточнения ADR-0020–0025.

## Контекст

Wayland ограничивает управление чужими окнами и focus stealing; global shortcuts и overlay зависят от compositor. Основной UX невозможен без надёжного Home поверх внешней Activity.

## Предлагаемое решение

Изолировать platform-specific код за PlatformService; первая реализация — KdePlatformService для KDE Plasma 6/Wayland. Граница включает global shortcuts, application activation, overlay/window integration, session state и system actions. Plugins не распределяют между собой управление чужими окнами.

Platform layer регистрирует Home shortcut, работающий при focus в Chrome, Steam, игре и ES-DE. Shell не рассчитывает получить обычный keyboard event без focus.

До значительной разработки UI выполнить архитектурный gate: global Home, Arinn поверх Chrome, возврат focus в Chrome и тот же цикл со Steam/игрой. Phase 0 также проверяет Qt/QML, Rust daemon, D-Bus и внешний plugin process.

System overlay объединяет Home, Back, Play/Pause, volume, mute, power menu и Settings; исполнение идёт через platform layer или соответствующий provider. CEC необязателен, будущие TV/webOS интеграции остаются plugins.

## Последствия

Необходимость compositor integration проверяется рано. Успешный spike подтверждает основной workflow; провал требует пересмотра механизма до развития UI. Вторая platform implementation возможна без распределения KDE-кода по daemon.

## Альтернативы и ограничения

Конкретные KWin APIs, scripts, protocols или layer-shell в ТЗ не выбраны. Нельзя считать механизм overlay подтверждённым только на основании схемы.

## Разрешение вопросов

Путь KWin adapter выбран; runtime gate остаётся непроверенным. См. [план Phase 0](../architecture/phase-0-gate.md) и [целевой HTPC](../architecture/product-decisions.md).

### Вопросы исходного ревью (история)

Механизм shortcut/overlay/activation, требуемые permissions и поведение fullscreen игр. Как provider запрашивает platform activation через daemon и как восстанавливать прежнее окно при его закрытии?

## Проверка решения

Записать окружение, выбранный механизм и результаты Phase 0. AC-05/AC-06 плюс аналогичный цикл для Steam/игры. Проверка считается выполненной только после реального integration spike.

[Реестр ADR](README.md) · [Карта ревью](../architecture/review-map.md)
