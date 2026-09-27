# ADR-0014: Provider-owned state и минимальное persistence Arinn

- Статус: Предложено (Proposed), требуется архитектурное ревью.
- Дата оформления: 2026-09-27.
- Основание: [Developer Specification v0.3](../specifications/developer-specification-v0.3.md), разделы 3, 38, 39, 40.

> Обновление 2026-09-27: TOML/SQLite/migrations/retention/cache/favorites закрыты [технической основой](../architecture/implementation-baseline.md) и [ADR-0022](0022-state-and-interaction-baseline.md). Этот ADR сохраняет формулировку исходного v0.3; при расхождении действуют принятые уточнения ADR-0020–0025.

## Контекст

Дублирование баз браузера, Steam или медиасервиса создаёт конкурирующие источники истины. Arinn должен хранить только собственные настройки и данные.

## Предлагаемое решение

Provider владеет состоянием интеграции: Chrome tabs, MPRIS playback, desktop applications. Core агрегирует это состояние, не копируя чужие базы в собственную БД.

Persistence Arinn включает favorites, Home layout, UI preferences, provider configuration и recent Arinn activities при необходимости. Chrome history, Steam library database и media database не копируются, если их может предоставить provider.

Разделять XDG config, data, state и cache. Примеры default paths: ~/.config/arinn/, ~/.local/share/arinn/, ~/.local/state/arinn/, ~/.cache/arinn/. Manifest/data размещаются отдельно от config и runtime/persistent state.

## Последствия

Хранимые данные остаются небольшими и принадлежат Arinn. Восстановление внешнего состояния зависит от provider; отсутствие provider должно отражаться как unavailable, а не как актуальные данные из дублирующей базы.

## Альтернативы и ограничения

Собственная копия Chrome history/Steam library не является основой persistence. SQLite, JSON и другие форматы хранения в ТЗ не выбраны.

## Разрешение вопросов

TOML/SQLite/migrations/retention/cache/favorites закрыты [технической основой](../architecture/implementation-baseline.md) и [ADR-0022](0022-state-and-interaction-baseline.md).

### Вопросы исходного ревью (история)

Формат хранения, migrations, атомарность записи, retention recent activities, cache invalidation и ссылки favorites на удалённые providers. Какие данные требуют persistence между сессиями?

## Проверка решения

Проверить сохранение собственных настроек после restart, получение provider state заново и раздельные XDG paths. AC-13: данные общего домашнего пространства не разделяются профилями Arinn.

[Реестр ADR](README.md) · [Карта ревью](../architecture/review-map.md)
