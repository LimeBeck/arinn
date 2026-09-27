# ADR-0013: MediaProvider на основе MPRIS

- Статус: Предложено (Proposed), требуется архитектурное ревью.
- Дата оформления: 2026-09-27.
- Основание: [Developer Specification v0.3](../specifications/developer-specification-v0.3.md), разделы 7, 12, 22, 50, 57, 58.

> Обновление 2026-09-27: Session priority/units/anchors заданы [runtime baseline](../architecture/plugin-runtime.md); rich playback выбран в [ADR-0024](0024-local-library-mpv.md). Этот ADR сохраняет формулировку исходного v0.3; при расхождении действуют принятые уточнения ADR-0020–0025.

## Контекст

Arinn нужен единый media UI поверх существующих плееров и браузеров. Собственный media engine не требуется.

## Предлагаемое решение

Для Linux MVP обнаруживать media sessions через arinn-plugin-mpris. Shell получает данные и отправляет команды только через arinnd и MediaProvider.

Session содержит id, title, subtitle/artist, artwork, state, duration, position и capabilities/available actions. Операции: listSessions, getSession, play, pause, playPause, seek, next, previous, stop.

Global media UI отображает текущую session и доступные действия; конкретная реализация MPRIS остаётся в plugin.

## Последствия

Контроль нескольких внешних media-приложений доступен в одном UI. Реальная поддержка действий зависит от session capabilities; Activity aggregation должна учитывать media и browser источники.

## Альтернативы и ограничения

Прямой MPRIS доступ из shell и собственный media engine исключены. Выбор локального player/library source этим ADR не сделан.

## Разрешение вопросов

Session priority/units/anchors заданы [runtime baseline](../architecture/plugin-runtime.md); rich playback выбран в [ADR-0024](0024-local-library-mpv.md).

### Вопросы исходного ревью (история)

Приоритет active session при нескольких players, единицы position/duration/seek, частота обновления progress, transport artwork и связь с Activity.

## Проверка решения

AC-09. Phase 4: current media, artwork, play/pause, seek, next/previous. Проверить session disappearance и Unsupported для недоступных операций после согласования контракта.

[Реестр ADR](README.md) · [Карта ревью](../architecture/review-map.md)
