# ADR-0018: Demo provider, общий contract suite и архитектурные gates

- Статус: Предложено (Proposed), требуется архитектурное ревью.
- Дата оформления: 2026-09-27.
- Основание: [Developer Specification v0.3](../specifications/developer-specification-v0.3.md), разделы 35, 50, 53, 54, 55, 57, 58, 59, 60.

> Обновление 2026-09-27: Test harness/fixtures и начальные performance budgets выбраны в [технической основе](../architecture/implementation-baseline.md). Runtime проверки остаются работой разработки. Этот ADR сохраняет формулировку исходного v0.3; при расхождении действуют принятые уточнения ADR-0020–0025.

## Контекст

UI должен развиваться без обязательного наличия Chrome/Steam/MPRIS. Plugin API и отказоустойчивость нужно проверять одинаково для разных providers.

## Предлагаемое решение

arinn-plugin-demo обязателен: разработка UI без внешних приложений, contract tests, воспроизведение ошибок и документация Plugin API.

Минимальные уровни проверки:

- Unit: daemon, state aggregation, parsing, routing, plugin SDK.
- Общий plugin contract suite для любого provider: registration, valid IDs, рост snapshot revision, NotFound для unknown ID, reconnect после restart; применимые проверки учитывают объявленные capabilities.
- Integration: shell/core, core/plugin, crash/restart и disappear/reappear.
- UI: D-pad navigation и focus graph.

Failure scenarios: crash Chrome не ломает Home/Steam/media; недоступный daemon не роняет shell, который показывает degraded state и reconnect; медленный provider приводит к timeout/error без блокировки UI.

Сохранить Phase 0–6 исходника: architecture spike → core skeleton → desktop applications → Chrome → media → product MVP → rich integrations. KDE/Wayland spike — gate перед значительной разработкой UI.

Приёмка MVP включает все AC-01–AC-14, а архитектурный review — Rules 1–10 и подключение нового provider без правок shell/core при достаточных capabilities.

## Последствия

Поведение можно воспроизводить без live-интеграций, а совместимость providers проверяется единым набором. Demo не заменяет реальную проверку KDE/Wayland, браузера и телевизора.

## Альтернативы и ограничения

Только happy-path или только live-приложения недостаточны по ТЗ. Оформление ADR само по себе не подтверждает прохождение spike или acceptance criteria.

## Разрешение вопросов

Test harness/fixtures и начальные performance budgets выбраны в [технической основе](../architecture/implementation-baseline.md). Runtime проверки остаются работой разработки.

### Вопросы исходного ревью (история)

Test harness, fixtures demo, capability-specific contracts, окружение интеграционных/UI tests и измеримые performance budgets. Численные latency/FPS требования в исходнике отсутствуют.

## Проверка решения

Порядок работ и привязка AC/Rules находятся в карте ревью. Эти проверки планируются для разработки; текущий пакет содержит только документацию, без утверждения о пройденной продуктовой приёмке.

[Реестр ADR](README.md) · [Карта ревью](../architecture/review-map.md)
