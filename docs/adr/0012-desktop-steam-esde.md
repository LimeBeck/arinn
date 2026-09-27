# ADR-0012: Desktop provider и отдельные входы Steam/ES-DE

- Статус: Предложено (Proposed), требуется архитектурное ревью.
- Дата оформления: 2026-09-27.
- Основание: [Developer Specification v0.3](../specifications/developer-specification-v0.3.md), разделы 10, 11, 20, 50, 51, 57, 59.

> Обновление 2026-09-27: Discovery/launch/instances/graceful terminate определены [runtime baseline](../architecture/plugin-runtime.md). Этот ADR сохраняет формулировку исходного v0.3; при расхождении действуют принятые уточнения ADR-0020–0025.

## Контекст

Обычные Linux-приложения должны подключаться без изменений Arinn. Для MVP запуска Steam/ES-DE достаточно без собственной реализации библиотек игр.

## Предлагаемое решение

Реализовать arinn-plugin-desktop с ApplicationProvider: discover/list/get, launch, activate, terminate и running state. Источником могут быть .desktop entries; core их не сканирует.

Steam и EmulationStation имеют отдельные Home tiles и запускаются через generic ApplicationProvider. Расширенные game library, artwork, recent games, continue playing и metadata откладываются до отдельных providers.

Window activation проходит через platform boundary (ADR-0009); наличие activate в capability не разрешает распределять Wayland window management по plugins.

## Последствия

Рабочий игровой entry point доступен до богатых интеграций. Требуется связать application identity с running instances и platform activation без product-specific core logic.

## Альтернативы и ограничения

Собственный Steam library client не нужен в MVP. Сканирование .desktop в daemon и прямой запуск из QML исключены.

## Разрешение вопросов

Discovery/launch/instances/graceful terminate определены [runtime baseline](../architecture/plugin-runtime.md).

### Вопросы исходного ревью (история)

Правила discovery .desktop, безопасная обработка Exec/options, multiple instances, сопоставление running state и semantics terminate. ТЗ не выбирает сигналы завершения или механизм отслеживания процессов.

## Проверка решения

AC-07/AC-08. Phase 2: Applications → Chrome/Steam/ES-DE → Home overlay → Back → то же приложение; core не содержит конкретных интеграций.

[Реестр ADR](README.md) · [Карта ревью](../architecture/review-map.md)
