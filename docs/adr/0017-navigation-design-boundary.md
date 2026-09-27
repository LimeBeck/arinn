# ADR-0017: Локальная D-pad navigation и отдельная дизайн-спецификация

- Статус: Предложено (Proposed), требуется архитектурное ревью.
- Дата оформления: 2026-09-27.
- Основание: [Developer Specification v0.3](../specifications/developer-specification-v0.3.md), разделы 2, 4, 5, 15, 45, 46, 47, 48, 53, 55, 58.

> Обновление 2026-09-27: Back и устройство подтверждены [ADR-0023](0023-mvp-product-clarifications.md); focus/keys заданы [interaction baseline](../architecture/activity-and-interaction.md). Этот ADR сохраняет формулировку исходного v0.3; при расхождении действуют принятые уточнения ADR-0020–0025.

## Контекст

TV-first shell используется с расстояния около трёх метров. Управление focus является interaction state shell и не может зависеть от внешнего provider или mouse hover.

## Предлагаемое решение

Все основные действия доступны D-pad/стрелками, Enter/Select, Back и Home; media buttons поддерживаются. Аэромышь поддерживается, но не обязательна для shell navigation.

Focus — first-class state. Каждый focusable component различает default, focused, pressed, disabled, loading. Видимый focus нельзя строить только на слабом изменении цвета, mouse hover или тонкой рамке. Navigation локальна shell и не ждёт backend latency.

Приоритет Back: modal → overlay → nested screen → Home → active Activity. Arinn сам рисует UI capabilities и не загружает UI plugins.

Визуальное направление исходного ТЗ: Quiet Spatial OS, Calm OS → Cinematic Content. Пользователь предоставил отдельный [дизайн-handoff](../design/handoff/README.md) с направлением 2a «Глубина со светом», финальными визуальными параметрами, дизайн-системой и примерами. Этот ADR сохраняет поведенческие ограничения developer ТЗ; визуальный контракт оформлен в [ADR-0019](0019-design-handoff-contract.md). Расхождение Back на Home отмечено в [карте согласования](../design/integration-review.md), без молчаливого выбора одной версии.

## Последствия

UI можно разрабатывать и проверять на demo provider. Дизайн реализуется с соблюдением D-pad и состояний focus; Home/Back contract требует согласования двух исходников. Архитектурный пакет ссылается на дизайн-систему.

## Альтернативы и ограничения

Mouse-only UX, focus через IPC и provider-specific UI запрещены ТЗ. Визуальные параметры не выведены произвольно из направления Quiet Spatial OS.

## Разрешение вопросов

Back и устройство подтверждены [ADR-0023](0023-mvp-product-clarifications.md); focus/keys заданы [interaction baseline](../architecture/activity-and-interaction.md).

### Вопросы исходного ревью (история)

Handoff уже определяет spatial navigation, focus scopes, память focus, основные keys и empty/error/loading views. Остаются согласование Back, mapping реального пульта и focus при динамическом исчезновении provider items; см. карту согласования.

## Проверка решения

AC-02, AC-14: D-pad walkthrough и проверка читаемости focus на телевизоре с трёх метров. UI tests проверяют focus graph; задержки provider не задерживают navigation.

[Реестр ADR](README.md) · [Карта ревью](../architecture/review-map.md)
