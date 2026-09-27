# ADR-0007: Асинхронные команды и нормализованные ошибки

- Статус: Предложено (Proposed), требуется архитектурное ревью.
- Дата оформления: 2026-09-27.
- Основание: [Developer Specification v0.3](../specifications/developer-specification-v0.3.md), разделы 27, 28, 44, 54, 55.

> Обновление 2026-09-27: Timeouts, IDs, cancel, deduplication и late replies закрыты [ADR-0020](0020-dbus-contract-baseline.md). Этот ADR сохраняет формулировку исходного v0.3; при расхождении действуют принятые уточнения ADR-0020–0025.

## Контекст

Внешние приложения могут отвечать медленно или быть недоступными. Их latency не должен блокировать UI, а provider-specific stack traces не должны попадать в QML API.

## Предлагаемое решение

Потенциально внешние операции, включая launch/open/activate/play, asynchronous. У каждой такой операции нужны timeout, typed error и request ID; cancellation поддерживается там, где имеет смысл.

Core нормализует ошибки: NotFound, Unavailable, Unsupported, PermissionDenied, Timeout, InvalidArgument, Conflict, Internal. Это перечисление в ТЗ приведено как пример модели и требует уточнения wire contract.

При медленном provider операция завершается timeout/error state без блокировки UI thread. Данные загружаются asynchronous; focus/navigation полностью локальны shell и не ждут IPC.

## Последствия

UI остаётся отзывчивым при проблемах интеграций. Появляются pending/error states и необходимость явно определить момент завершения внешней операции.

## Альтернативы и ограничения

Синхронная цепочка keypress → D-Bus → plugin → response → focus change запрещена. Немедленное успешное выполнение внешней команды не предполагается.

## Разрешение вопросов

Timeouts, IDs, cancel, deduplication и late replies закрыты [ADR-0020](0020-dbus-contract-baseline.md).

### Вопросы исходного ревью (история)

Timeout по операциям, формат request ID, модель completion, отмена после timeout, retry/idempotency и судьба запоздалых ответов. ТЗ не задаёт автоматический retry команд.

## Проверка решения

Имитировать зависший provider и проверить timeout при работающем D-pad. Unknown ID возвращает NotFound; stack trace не появляется в QML API.

[Реестр ADR](README.md) · [Карта ревью](../architecture/review-map.md)
