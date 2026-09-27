# ADR-0020: D-Bus, идентификаторы и надёжность Plugin API

- Статус: Принято для реализации (Accepted).
- Дата: 2026-09-27.
- Основание: пользователь поручил самостоятельно закрыть технические решения; это решение исполнителя в пределах делегированной задачи, не утверждение о состоявшемся отдельном ревью пользователя.
- Уточняет: ADR-0002, ADR-0004, ADR-0005, ADR-0006, ADR-0007, ADR-0015, ADR-0016, ADR-0018; исходные ADR сохраняют историю и ссылку на это уточнение.

## Контекст

Для начала skeleton требуются конкретные transport, routing, revision и operation semantics. Пользователь поручил самостоятельно выбрать технические детали.

## Решение

Принять session D-Bus на обеих IPC-границах, QtDBus/zbus, typed core fields, EntityRef и generation/revision, asynchronous operation objects, bounded payload и явную timeout/error model. Подробные правила и defaults зафиксированы в технической основе; они обязательны для skeleton.

Подробный контракт: [спецификация](../architecture/implementation-baseline.md). См. также [техническую основу](../architecture/implementation-baseline.md) и [Phase 0 gate](../architecture/phase-0-gate.md).

## Последствия

Один transport сокращает число IPC stacks; versioned typed interfaces сохраняют языковую независимость. Полные wire-схемы browser/media/library/settings будут добавляться перед соответствующим provider; это implementation deliverable, а не вопрос предпочтения пользователя.

## Альтернативы

JSON-RPC поверх Unix socket потребовал бы отдельного framing/subscription/lifetime слоя. Не выбран для общего API; JSON остаётся требованием Chrome Native Messaging.

## Оставшаяся работа и проверка

Ответы P-01–P-05 получены и зафиксированы в ADR-0023; они не меняют transport. Contract tests должны проверить reconnect/races/timeout; готовая runtime реализация пока отсутствует.

Принятие технического выбора не означает прохождение тестов, готовность wire API всех capabilities или принятие противоречащих друг другу продуктовых требований.

[Реестр ADR](README.md)
