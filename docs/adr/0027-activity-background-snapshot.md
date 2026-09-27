# ADR-0027: compositor-снимок Activity для отдельного Qt shell

- Статус: Accepted — путь для первой реализации; прототип не равен готовой интеграции Home.
- Дата: 2026-09-27.
- Основание: разрешённый пользователем Phase 0 и runtime experiment на KDE/Wayland.

## Контекст

Handoff §Activity допускает live compositor texture **или снимок последнего кадра** реального внешнего окна. Требования near/far задают масштаб, размытие и затемнение. Shell остаётся отдельным Qt-процессом; переносить весь UI в compositor не требуется.

## Решение

Первый production path — последний кадр выбранной Activity: доверенный KWin platform component получает WindowThumbnail конкретного window identity, экспортирует ограниченный кадр, daemon сообщает готовность shell. Shell читает только подготовленный кадр и выполняет near/far визуализацию локально. Один и тот же `ActivityBackground.qml` работает с compositor source item и с экспортированным изображением.

Снимок привязан к generation + Activity + запросу Home. При Return, смене Activity, lock/sleep, потере provider/daemon или отмене запроса он становится невалидным. Устаревший ответ не может заменить текущий фон. Кадр хранится только в приватном runtime directory и удаляется по завершении использования; screenshot paths не принимаются от произвольных providers. Повторный Home не запускает захват заново. UI не ждёт PNG перед локальным focus.

Эти lifecycle-правила — требования к следующей production реализации. В текущем коде захват запускается **только операторским тестом**, автоматического capture при Home пока нет.

## Проверка и ограничения

[Runtime experiment](../architecture/background-experiment.md) подтвердил живой источник реального Chrome-окна, near/far shaders и чтение экспортированного кадра отдельным Qt-процессом. Прямая передача GPU texture между процессами не проверена. PNG export порядка 0.3 s не подходит для непрерывного video path; нужны уменьшение разрешения/асинхронная доставка и измерения перед включением в Home.

Попытка ScreenShot2 с desktop permission была отклонена KWin как NoAuthorized, причина не установлена. Permission checks не отключались; временный entry удалён. Проверенный WindowThumbnail доступен внутри разрешённого доверенного KWin script. Product/provider code не получает произвольного доступа к compositor.

Непрерывный live texture path откладывается до отдельного performance/lifecycle review. Полный визуальный контракт (переходы, app tint, TV readability) проверяется при реализации UI.
