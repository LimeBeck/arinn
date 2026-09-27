# ADR-0022: Владение Activity, persistence и interaction state

- Статус: Принято для реализации (Accepted).
- Дата: 2026-09-27.
- Основание: пользователь поручил самостоятельно закрыть технические решения; это решение исполнителя в пределах делегированной задачи, не утверждение о состоявшемся отдельном ревью пользователя.
- Уточняет: ADR-0010, ADR-0013, ADR-0014, ADR-0017, ADR-0019; исходные ADR сохраняют историю и ссылку на это уточнение.

## Контекст

Прототип хранит все данные локально, но production shell должен только отображать provider state. Большинство конфликтов таймеров, focus и ownership разрешаются технически.

## Решение

Принять explicit EntityRef relationships, provider-owned playback/library, core Activity aggregation, локальный focus/OSD/preview и отдельные operation pending states. Persistence Arinn: TOML config и SQLite собственного state согласно технической основе. DR-04, DR-07, DR-08 и техническая часть DR-09/10 закрыты этими правилами.

Подробный контракт: [спецификация](../architecture/activity-and-interaction.md). См. также [техническую основу](../architecture/implementation-baseline.md) и [Phase 0 gate](../architecture/phase-0-gate.md).

## Последствия

Core не угадывает совпадения по titles/URLs и не копирует чужие базы. Browser-first приоритет сохранён; визуальный prototype timer не становится фактом завершения операции.

## Альтернативы

Heuristic merge по названию, база media в shell и фиктивный success по animation timeout не используются.

## Оставшаяся работа и проверка

P-01–P-05 закрыты в ADR-0023; media выбран в ADR-0024. Compositor effects требуют Phase 0; media wire schema оформляется по принятому API review.

Принятие технического выбора не означает прохождение тестов, готовность wire API всех capabilities или принятие противоречащих друг другу продуктовых требований.

[Реестр ADR](README.md)
