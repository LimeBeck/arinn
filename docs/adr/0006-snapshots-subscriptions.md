# ADR-0006: Состояние через snapshots, revisions и subscriptions

- Статус: Предложено (Proposed), требуется архитектурное ревью.
- Дата оформления: 2026-09-27.
- Основание: [Developer Specification v0.3](../specifications/developer-specification-v0.3.md), разделы 16, 25, 26, 38, 53, 54.

> Обновление 2026-09-27: Generation/revision и subscribe/read race закрыты [ADR-0020](0020-dbus-contract-baseline.md). Этот ADR сохраняет формулировку исходного v0.3; при расхождении действуют принятые уточнения ADR-0020–0025.

## Контекст

Reconnect и перезапуски shell/plugin не должны требовать восстановления состояния по длинной последовательности imperative events. Источником состояния интеграции остаётся provider.

## Предлагаемое решение

Использовать snapshot + revision как предпочтительную модель передачи состояния. При изменении provider публикует новую revision.

API поддерживает subscriptions: StateChanged(providerId, revision), ProviderAvailable(providerId), ProviderUnavailable(providerId), ActivityChanged(...). После StateChanged consumer может запросить snapshot; весь state не обязан находиться в событии.

Daemon агрегирует provider state. Shell после временной недоступности daemon отображает degraded state и пытается reconnect.

## Последствия

Полный snapshot упрощает resynchronization. Требуется согласовать порядок подписки и чтения, размер snapshots и защиту от устаревших ответов; эти детали исходник не определяет.

## Альтернативы и ограничения

Большое количество mutable imperative events не является предпочтительной основой синхронизации. Delta-протокол исходным ТЗ не утверждён.

## Разрешение вопросов

Generation/revision и subscribe/read race закрыты [ADR-0020](0020-dbus-contract-baseline.md).

### Вопросы исходного ревью (история)

Область revision: provider, capability или snapshot? Как revision ведёт себя после restart и нужна ли generation/session identity? Как исключить гонку subscribe/read и обработать пропущенные события?

## Проверка решения

Contract tests проверяют рост revision и reconnect после restart; integration tests — disappear/reappear и восстановление shell. Точный invariant для restart зависит от ответа на открытые вопросы.

[Реестр ADR](README.md) · [Карта ревью](../architecture/review-map.md)
