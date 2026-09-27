# ADR-0010: Activity и единая модель Continue/Home overlay

- Статус: Предложено (Proposed), требуется архитектурное ревью.
- Дата оформления: 2026-09-27.
- Основание: [Developer Specification v0.3](../specifications/developer-specification-v0.3.md), разделы 2, 4, 5, 6, 7, 37, 48, 51, 59, 61.

> Обновление 2026-09-27: Activity schema/ownership/dedup закрыты [ADR-0022](0022-state-and-interaction-baseline.md); Back закрыт явным ответом в [ADR-0023](0023-mvp-product-clarifications.md). Этот ADR сохраняет формулировку исходного v0.3; при расхождении действуют принятые уточнения ADR-0020–0025.

## Контекст

Список процессов не описывает занятие пользователя: Chrome — application, конкретная вкладка — activity. Внешнее приложение является нормальным состоянием Arinn.

## Предлагаемое решение

Использовать Activity для текущего занятия: вкладки, игры, ES-DE, movie playback. Activity не обязана совпадать с Application.

Главные routes: Home, Library, Downloads, Applications, Settings. Home — главный экран и global overlay. Continue объединяет resumable content и активную Activity без конкурирующих секций «Сейчас запущено», «Продолжить просмотр», «Последние приложения».

Favorites/Online содержит прямые точки входа Browser, онлайн-сервисов, Steam и EmulationStation. New отображает новое/рекомендуемое от providers, без собственного recommendation engine.

Home появляется поверх Activity, не завершает приложение, не уничтожает состояние и не ставит его принудительно на паузу, если этого не требует provider. Закрытие Home возвращает в прежнюю Activity.

Back последовательно обрабатывается на уровнях modal → overlay → nested screen → Home → active Activity; Back внутри Home не завершает активное приложение.

## Последствия

Пользователь продолжает занятие без повторного выбора приложения. Нужна согласованная идентичность Activity между источниками данных; layout и визуальные переходы заданы отдельным [дизайном](../design/handoff/README.md); API и расхождения поведения требуют [согласования](../design/integration-review.md).

## Альтернативы и ограничения

Модель «внешнее приложение означает выход из launcher» противоречит ТЗ. Независимые конкурирующие секции продолжения исключены.

## Разрешение вопросов

Activity schema/ownership/dedup закрыты [ADR-0022](0022-state-and-interaction-baseline.md); Back закрыт явным ответом в [ADR-0023](0023-mvp-product-clarifications.md).

### Вопросы исходного ревью (история)

Schema Activity, producer/owner, resumable lifecycle, связь tab/application/media session и устранение дублей Continue. Что показывать при отсутствии Activity или её исчезновении во время overlay?

## Проверка решения

AC-05, AC-06, AC-07, AC-08. Проверить сохранение состояния приложения при Home/Back, единый Continue и требуемый приоритет Back.

[Реестр ADR](README.md) · [Карта ревью](../architecture/review-map.md)
