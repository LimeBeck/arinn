# Arinn

TV-first оболочка для Linux HTPC: оркестрация внешних приложений и активностей через capability-based plugin API. Первый target — KDE Plasma 6 / Wayland.

Репозиторий содержит исходные ТЗ и принятые уточнения для начала разработки:

- [Исходное ТЗ — Developer Specification v0.3](docs/specifications/developer-specification-v0.3.md).
- [Дизайн-handoff: спецификация, дизайн-система, прототипы и 22 скриншота](docs/design/README.md).
- [Реестр 26 ADR](docs/adr/README.md).
- [Техническая основа](docs/architecture/implementation-baseline.md), [актуальный MVP](docs/architecture/product-decisions.md) и [план Phase 0](docs/architecture/phase-0-gate.md).
- [Карта ревью: покрытие, этапы, приёмка и открытые вопросы](docs/architecture/review-map.md).

Исходное ТЗ — Draft / Architecture Review Candidate. ADR-0001–0019 сохраняют исходное предложение; ADR-0020–0026 — Accepted, с приоритетом в уточнённых пунктах. Дизайн-handoff сохранён отдельно; [расхождения с developer ТЗ](docs/design/integration-review.md) снабжены решениями и статусами проверок.

Реализован Phase 0: Rust daemon/demo/CLI, Qt Quick shell и KWin-адаптер с выбором монитора активной Activity. [Сборка и запуск](docs/architecture/phase-0-runbook.md), [результаты и ограничения](docs/architecture/phase-0-report.md). Это архитектурный прототип; stable Plugin API и финальный UI ещё не реализованы.
