# Activity background — runtime experiment

2026-09-27, KWin 6.7.5 / Qt 6.11.2 / Mesa Intel Iris Xe. Проверена feasibility графического пути; автоматическая интеграция в Home ещё не выполнена. Решение: [ADR-0027](../adr/0027-activity-background-snapshot.md).

## Что проверено

Операторский `kwin_background.py` создаёт временный KWin QML script. Он выбирает ровно foreground window UUID, проверенный перед запуском, и создаёт невидимое, не принимающее focus окно с WindowThumbnail. Получает кадры raw, near, far и raw-later. После завершения script выгружается. В тесте используется только отдельное Chrome fixture-окно с локальной HTML-страницей и счётчиком времени.

`ActivityBackground.qml` применяет scale .95/.90, Qt MultiEffect blur с максимумом 36 и near factor 8/36, opacity .6/.32 на чёрном фоне и scrim .3/.6. Это техническая реализация параметров handoff; точное совпадение blur-kernel и color management с HTML-референсом ещё не утверждается.

`background_preview.py` читает raw PNG в **отдельном Qt-процессе** и повторно рисует тот же компонент. Следовательно, внешний shell может использовать last-frame path без переноса пользовательского UI внутрь KWin. Пиксели raw и raw-later отличаются в области счётчика; источник не был фиктивной статической картинкой. Изображения проверены визуально.

- [Near](evidence/background-near.png).
- [Far](evidence/background-far.png).
- [Тот же snapshot в отдельном Qt renderer](evidence/background-external.png).

## Измерения и границы

На смешанном DPI окружении запрос логического 1920×1080 давал физический PNG 3360×1890 в KWin context с DPR 1.75. Это выявленное ограничение прототипа: привязка к output не гарантирует нужный размер offscreen export. Четыре последовательных grab/save занимали порядка 257–367 ms каждое, включая GPU readback и PNG encoding. Последний прогон: raw 296 ms, near 367 ms, far 260 ms, raw-later 264 ms. Это не input latency и не GPU frame-time benchmark.

Перед подключением capture к Home нужно ограничить физический размер кадра, исключить работу PNG encoding из критического пути compositor, задать deadline/fallback и измерить frame-time/память на целевом TV. Live texture sharing между процессами и DRM/protected content не проверены. Эксперимент не содержит полного capture lifecycle при lock/return/restart — он запускается явно оператором на disposable Activity.

ScreenShot2 API был исследован отдельно: даже с временно объявленным desktop permission возвращал `NoAuthorized`. Защита KWin не отключалась, временная permission-запись удалена. Этот путь не объявлен рабочим.

## Воспроизведение

Нужны запущенный Phase 0, foreground disposable Activity, QtQuick.Effects, qml6. Команды выполняются из корня репозитория:

```sh
python tests/integration/kwin_background.py
python tests/integration/background_preview.py
```

Кадры находятся в `.phase0/background-probe/` (приватная папка, игнорируется Git). Тест не отправляет их в сеть. В evidence репозитория включены только изображения созданной для эксперимента HTML-страницы, без пользовательских окон. Не запускать этот операторский тест во время lock/suspend; production session guard ещё не связан с capture pipeline.

Источники API: [KWin QML effects](https://develop.kde.org/docs/plasma/kwineffect/), [WindowThumbnail implementation](https://invent.kde.org/plasma/kwin/-/blob/Plasma/6.7/src/scripting/windowthumbnailitem.cpp), [QML Workspace windows](https://invent.kde.org/plasma/kwin/-/blob/Plasma/6.7/src/scripting/workspace_wrapper.h).
