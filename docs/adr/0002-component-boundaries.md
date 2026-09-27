# ADR-0002: Qt/QML shell, Rust daemon и публичный D-Bus API

- Статус: Предложено (Proposed), требуется архитектурное ревью.
- Дата оформления: 2026-09-27.
- Основание: [Developer Specification v0.3](../specifications/developer-specification-v0.3.md), разделы 14, 15, 16, 41, 52, 56, 57, 59.

> Обновление 2026-09-27: Bindings/names/API defaults и autostart выбраны в [ADR-0020](0020-dbus-contract-baseline.md), [ADR-0021](0021-plugin-runtime-ownership.md). Этот ADR сохраняет формулировку исходного v0.3; при расхождении действуют принятые уточнения ADR-0020–0025.

## Контекст

Presentation, системная оркестрация и внешние интеграции должны развиваться независимо. Product-specific логика в QML или daemon нарушает цель расширения через providers.

## Предлагаемое решение

Использовать Qt 6/QML для arinn-shell; Rust — предпочтительный язык arinnd. Shell отвечает за rendering, navigation, focus, animations, overlays, screens, design system и interaction state. Все команды и данные интеграций проходят через arinnd по D-Bus.

Daemon содержит Plugin/Capability Registry, lifecycle orchestration, state aggregation, routing, subscriptions, snapshots/revisions, нормализацию ошибок, platform abstraction, configuration и небольшое собственное persistence. Функциональность, которую можно вынести в provider, реализуется в provider.

arinnctl использует тот же публичный daemon API, что и shell. Shell не запускает приложения самостоятельно, не читает .desktop, не обращается к MPRIS и не выполняет shell commands.

Сохранить границы shell/core/protocol/plugins/platform. Предложенная структура: apps/arinn-shell; crates/{arinnd,arinn-protocol,arinn-plugin-sdk,arinn-platform}; plugins/{demo,desktop,chrome,mpris}; tools/arinnctl; chrome-extension; packaging; docs/{architecture,plugin-api,adr}; tests/{contract,integration}. Это план размещения, а не уже существующие компоненты.

Packaging поставляет arinnd, arinn-shell, arinnctl и arinn-plugin-*; Native Messaging manifest устанавливается packaging layer.

## Последствия

UI и CLI имеют единый источник состояния и ошибок. IPC становится обязательной границей и требует отдельного контракта. Имена директорий можно уточнить, не нарушая разделение ответственности.

## Альтернативы и ограничения

Прямое управление сторонними приложениями из shell исключено. ТЗ задаёт D-Bus для shell ↔ daemon, но не выбирает транспорт daemon ↔ plugin (ADR-0005).

## Разрешение вопросов

Bindings/names/API defaults и autostart выбраны в [ADR-0020](0020-dbus-contract-baseline.md), [ADR-0021](0021-plugin-runtime-ownership.md).

### Вопросы исходного ревью (история)

Какие D-Bus bus name, object paths, interfaces и binding libraries использовать? Как оформить autostart после входа в HTPC session?

## Проверка решения

Phase 0 проверяет Qt/QML, Rust и D-Bus; Phase 1 — получение shell всех данных через daemon. AC-01, AC-12 и Rule 3 проверяются отдельно.

[Реестр ADR](README.md) · [Карта ревью](../architecture/review-map.md)
