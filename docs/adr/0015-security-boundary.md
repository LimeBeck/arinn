# ADR-0015: Доверенные локальные plugins с валидацией IPC

- Статус: Предложено (Proposed), требуется архитектурное ревью.
- Дата оформления: 2026-09-27.
- Основание: [Developer Specification v0.3](../specifications/developer-specification-v0.3.md), разделы 9, 15, 17, 27, 43, 44, 45, 49.

> Обновление 2026-09-27: Лимиты, peer registration и logging privacy заданы [ADR-0020](0020-dbus-contract-baseline.md), [ADR-0021](0021-plugin-runtime-ownership.md). Этот ADR сохраняет формулировку исходного v0.3; при расхождении действуют принятые уточнения ADR-0020–0025.

## Контекст

MVP работает с локально установленными plugins, которые считаются доверенным software. Тем не менее ошибки данных и команд не должны разрушать shell/core или давать произвольное выполнение через API.

## Предлагаемое решение

Не позиционировать MVP как hostile-plugin sandbox. Сохранять process isolation, IPC/command validation, bounded message size и timeouts. QML не выполняет arbitrary shell commands, Plugin API не предоставляет arbitrary code injection.

Plugin поставляет data/metadata/actions/capabilities, но не QML, HTML или другой произвольный UI. Chrome extension получает минимальные permissions.

В обычные логи не попадают cookies, authorization headers, passwords и browser page content.

## Последствия

Протокол имеет защитные границы даже для доверенного software. Изоляция процессов ограничивает crash propagation, но не обещает защиту от злонамеренного локального plugin.

## Альтернативы и ограничения

Загрузка произвольного UI, командный shell как API и in-process native plugins исключены. Marketplace и remote plugin repository за пределами v1.

## Разрешение вопросов

Лимиты, peer registration и logging privacy заданы [ADR-0020](0020-dbus-contract-baseline.md), [ADR-0021](0021-plugin-runtime-ownership.md).

### Вопросы исходного ревью (история)

Конкретные лимиты payload, валидация URL/команд/metadata, идентификация IPC peer и политика diagnostic logging. ТЗ не определяет, как обрабатывать чувствительные query parameters URL в логах.

## Проверка решения

Проверить malformed/oversized IPC, invalid arguments и timeout после фиксации лимитов. Review должен подтвердить отсутствие QML shell execution, UI injection и запрещённых логируемых данных.

[Реестр ADR](README.md) · [Карта ревью](../architecture/review-map.md)
