# ADR-0016: Общий API для CLI, diagnostics и structured logs

- Статус: Предложено (Proposed), требуется архитектурное ревью.
- Дата оформления: 2026-09-27.
- Основание: [Developer Specification v0.3](../specifications/developer-specification-v0.3.md), разделы 28, 30, 41, 42, 43, 54.

> Обновление 2026-09-27: Log schema, CLI output/exit codes и diagnostics counters определены [технической основой](../architecture/implementation-baseline.md). Этот ADR сохраняет формулировку исходного v0.3; при расхождении действуют принятые уточнения ADR-0020–0025.

## Контекст

Ошибка интеграции должна быть наблюдаемой, а не выглядеть как неработающая плитка. Диагностика нужна независимо от TV UI.

## Предлагаемое решение

Предоставить arinnctl status, providers, capabilities, activities, diagnostics через тот же публичный daemon API, который использует shell. Команда provider restart <id> желательна, но не является обязательной в v0.3.

Diagnostics показывает arinnd status, plugin status/version, API version, registered capabilities, last error и restart count. Несовместимая версия plugin видна в diagnostics.

Все компоненты пишут structured logs; минимальные поля: component, provider_id, request_id, operation, duration, error. Основной Linux backend — journald. Секреты и содержимое browser pages не включаются в обычные логи (ADR-0015).

Unavailable provider, degraded daemon connection и timeout имеют явное состояние в UI.

## Последствия

Оператор может понять причину сбоя без знания внутреннего IPC приложения. Общий API предотвращает расхождение представлений shell и CLI.

## Альтернативы и ограничения

Немые ошибки и plugin-specific stack traces в QML API не соответствуют ТЗ. Отдельный обходной CLI API не выбирается.

## Разрешение вопросов

Log schema, CLI output/exit codes и diagnostics counters определены [технической основой](../architecture/implementation-baseline.md).

### Вопросы исходного ревью (история)

Схема diagnostics и log levels, формат CLI output/exit codes, семантика restart count и длительность хранения last error. Как представлять неприменимые поля лога у операций без provider?

## Проверка решения

Сопоставить статусы UI/CLI при crash, несовместимой версии, timeout и reconnect. Проверить корреляцию request ID и отсутствие запрещённых данных в логах.

[Реестр ADR](README.md) · [Карта ревью](../architecture/review-map.md)
