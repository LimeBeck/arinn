# Уточнения MVP по ответам пользователя

Дата: 2026-09-27. Статус: принято по явным ответам пользователя. Эти уточнения имеют приоритет над противоречащими местами исходного developer v0.3 и design handoff; исходные файлы сохранены как получены.

| ID | Ответ / решение | Следствие |
| --- | --- | --- |
| P-01 | Back на Home возвращает в активное приложение; без него открывает навигацию. | DR-01 закрыт. Верхний modal/sheet/nav обрабатывает Back прежде Home. |
| P-02 | В MVP каталог локальных и смонтированных сетевых папок, метаданные, внешний плеер с выбором аудио и субтитров. | DR-03: выбран сценарий; реализация — library + mpv providers, MPRIS сохраняется для других плееров. |
| P-03 | Transmission входит в MVP. | DR-02 закрыт; добавление DownloadProvider отдельно описано в ADR-0025 и API review. |
| P-04 | «Сон» отправляет компьютер в OS suspend; заставка — отдельная функция. | DR-06 закрыт. Ambient не подменяет suspend. |
| P-05 | Приёмка на другом HTPC с KDE/Wayland; Air Mouse G60S Pro BT 2.4G с русской раскладкой, имитирует мышь/клавиатуру. | Дополнено P-06/P-07: отдельная интеграция пульта не требуется. |
| P-06 | Текущая машина технически эквивалентна HTPC и подходит для spike; подключены два монитора. | Локальная runtime проверка разрешена. Home выбирает экран Activity, см. [ADR-0026](../adr/0026-multiple-monitors.md). |
| P-07 | Пульт мапится на стандартные клавиши, включая Backspace и Esc. | Input gate проверяется стандартными keyboard/mouse events; физический пульт не блокирует Phase 0. [Mapping и scope](activity-and-interaction.md). |
| P-08 | На целевом HTPC будет один экран; два монитора относятся к машине разработки. | Основной сценарий приёмки — один экран без выбора output пользователем. Два экрана проверены как дополнительная устойчивость реализации. |

## Таблица Home/Back

| Состояние | Back | Home |
| --- | --- | --- |
| Modal / power confirm / sheet / nav | Закрыть верхний scope или вернуться на родительский шаг; focus к opener. | Показать Home, закрыв transient UI без подтверждения destructive действия. |
| Nested screen | На родительский экран; на верхнем уровне раздела — Home. | Home. |
| Home + доступная return Activity | Закрыть Home, активировать ту же Activity. | Оставить Home, focus на Return; не делать toggle. |
| Home без return Activity | Открыть global nav. | Оставить Home. |
| Внешнее приложение | Back принадлежит внешнему приложению. | Platform открывает Arinn overlay и запоминает return Activity. |

Если сохранённая Activity исчезла, Home остаётся открыт, показывает краткое unavailable состояние и выбирает focus по правилам interaction baseline. Следующий Back действует как при отсутствии Activity. Открытие Home само по себе не pause/stop.

## Suspend и ambient

«Сон» вызывает platform system suspend с учётом inhibitors и permissions. При отказе показывается ошибка; UI не сообщает об успешном сне по таймеру. При wake восстановить Home/Activity context и переподключить providers. Restart и poweroff сохраняют confirm с Cancel по умолчанию.

Ambient включается отдельным действием или idle timeout 10 min из дизайна; любая клавиша/движение указателя будит UI, первый wake event поглощается и не запускает случайное действие. При активной внешней Activity или playing media автоматический ambient не включается. OS suspend не вызывается автоматически этим idle timer.

## Обновлённый состав MVP и этапы

Обязательные plugins: demo, desktop, chrome, mpris, library, mpv, transmission. Core по-прежнему не содержит имен этих продуктов в capability routing.

Phase 0 — platform/core spike; Phase 1 — skeleton/demo; Phase 2 — desktop/Steam/ES-DE; Phase 3 — Chrome; Phase 4 — MPRIS **и** локальная библиотека/mpv; Phase 5 — Transmission, связь Downloads → Library, Home/settings/persistence/diagnostics/autostart/packaging. Расширенные игровые библиотеки, Jellyfin и TV/webOS остаются после MVP.

К AC-01–AC-14 добавляются:

- AC-15: сканирование локальной и смонтированной сетевой папки; metadata/fallback, сохранение записи при недоступном mount; запуск фильма и эпизода.
- AC-16: play/pause/seek, выбор audio/subtitles, resume после restart и корректный Continue в mpv workflow.
- AC-17: Transmission queue, pause/resume/start-now/retry, error/unavailable, completed media появляется в Library/New один раз и доступно для запуска.
- AC-18: Home/Back соответствует таблице для стандартных клавиатурных/мышиных событий; Esc и Backspace поддерживаются с учётом текстового ввода. G60S Pro использует тот же input path; проверка физического пульта — дополнительный smoke test, не отдельная интеграция.
- AC-19: Suspend действительно усыпляет HTPC; wake/reconnect работает. Ambient проверяется отдельно и не инициирует suspend.

Продуктовых вопросов, блокирующих Phase 0, после этих ответов не осталось. Технический spike выполнен на текущей машине по дополнительному разрешению пользователя; [результаты](phase-0-report.md). Развёртывание на одноэкранном HTPC проверяется позже; физический пульт не блокирует разработку стандартного input path.
