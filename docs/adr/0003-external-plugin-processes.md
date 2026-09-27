# ADR-0003: Изоляция интеграций во внешних процессах

- Статус: Предложено (Proposed), требуется архитектурное ревью.
- Дата оформления: 2026-09-27.
- Основание: [Developer Specification v0.3](../specifications/developer-specification-v0.3.md), разделы 2, 17, 44, 54, 59, 60.

> Обновление 2026-09-27: Lifecycle и failure state разграничены в [ADR-0021](0021-plugin-runtime-ownership.md). Этот ADR сохраняет формулировку исходного v0.3; при расхождении действуют принятые уточнения ADR-0020–0025.

## Контекст

Интеграции имеют независимые зависимости и могут падать. Их обновление и выбор языка не должны связываться с Rust ABI daemon.

## Предлагаемое решение

Каждый plugin является отдельным OS process. Не загружать .so/native libraries интеграций внутрь arinnd. Общение идёт через стабильную IPC boundary Plugin API.

Падение plugin не приводит к падению shell, daemon или других plugins. При падении Chrome provider Home, запуск Steam и media control продолжают работать, а Browser section становится unavailable.

## Последствия

Обеспечиваются crash isolation, независимые зависимости, обновления и языки реализации. Возникает необходимость handshake, диагностики, reconnect и supervision. Process isolation не означает sandbox для недоверенного кода.

## Альтернативы и ограничения

In-process native plugins прямо запрещены исходным ТЗ. Hostile-plugin sandbox не является обещанием MVP.

## Разрешение вопросов

Lifecycle и failure state разграничены в [ADR-0021](0021-plugin-runtime-ownership.md).

### Вопросы исходного ревью (история)

Как различать аварийное завершение, административную остановку и временную недоступность в lifecycle-состояниях? Политику restart уточнить в ADR-0008.

## Проверка решения

AC-10: принудительно завершить Chrome plugin и проверить остальные компоненты. Integration tests покрывают crash/restart и disappear/reappear.

[Реестр ADR](README.md) · [Карта ревью](../architecture/review-map.md)
