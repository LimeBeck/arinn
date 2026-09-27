# Phase 0 — результаты эксперимента

Дата: 2026-09-27. Текущая машина использована с разрешения пользователя как технически эквивалентная HTPC. Реализован и проверен вертикальный срез; **полный gate пока не закрыт**. [Запуск и воспроизведение](phase-0-runbook.md).

## Окружение

KDE/Wayland, KWin 6.7.5, Qt 6.11.2, Rust 1.98.1, systemd 261.3. Intel Iris Xe (Tiger Lake GT2), kernel driver i915, Mesa 26.2.3, KWin OpenGL/EGL compositor. Версия Plasma Shell отдельно не подтверждалась.

| Output | Физический режим | Scale | Логическая геометрия |
| --- | --- | --- | --- |
| DP-1 | 1920×1080, ~75 Hz | 1 | 0,0 — 1920×1080 |
| eDP-1 | 2880×1800, 90 Hz | 1.75 | 1920,140 — 1646×1029 |

Конфигурация дисплеев не менялась. Shell выбирает QScreen по имени output, KWin обеспечивает placement/activation. В тестах fullscreen устанавливается тестовому окну через KWin; это подтверждает compositor fullscreen, но не эксклюзивный renderer mode/direct scanout.

Целевой HTPC, по последующему уточнению пользователя, будет иметь **один экран**. Двухэкранный прогон — проверка на машине разработки. Физически одноэкранную конфигурацию в этом прогоне не создавали; при развёртывании достаточно повторить smoke test на единственном экране.

## Подтверждённые результаты

| Проверка | Результат / evidence |
| --- | --- |
| Typed Rust → D-Bus → Qt/QML | Реальный snapshot прочитан Qt Test; standalone demo в отдельном процессе. Private-bus suite PASS. |
| Async actions | 7-секундный provider даёт Timeout после 5 s; status отвечает без ожидания действия. Left/Right focus в QML меняется в пределах 500 ms во время операции. Это функциональный лимит теста, не FPS/input latency benchmark. |
| Chrome, DP-1 | 20/20 Home → повтор Home → Back, тот же UUID и output. Fullscreen; Chrome запущен с `--ozone-platform=wayland`, отдельным profile и локальной fixture-страницей. [JSON](evidence/chrome-DP-1.json). |
| Chrome, eDP-1 | 20/20, fullscreen и scale 1.75. [JSON](evidence/chrome-eDP-1.json). |
| Steam, DP-1 | 20/20, окно клиента, не fullscreen. [JSON](evidence/steam-DP-1.json). Режим Wayland/XWayland этого окна отдельно не измерен. |
| Terraria, DP-1 и eDP-1 | По 20/20 циклов, compositor fullscreen, SDL Wayland (подтверждён логом). [DP-1](evidence/terraria-DP-1.json), [eDP-1](evidence/terraria-eDP-1.json). Проверка игрового окна/меню, без прохождения игрового мира. |
| Crash provider | SIGKILL demo → unavailable → systemd restart → новая generation; PID daemon и shell не изменились. [JSON](evidence/live-failures.json). |
| Удалённая Activity | Закрытие отдельного Chrome fixture при открытом Home очищает return target; Back не активирует постороннее окно. [JSON](evidence/live-failures.json). |
| Validation/recovery | Неверный token, чужой platform caller, неизвестный item, stale generation отвергнуты. После restart daemon demo регистрируется заново. Private-bus suite PASS. |
| Сборка | cargo build/test, clippy `-D warnings`, fmt check, CMake/Ninja, интеграционный Qt suite и JS regression remap/closed-window прошли. |

Home вызывался через `KGlobalAccel.invokeShortcut` зарегистрированного Meta+F12, а не физической кнопкой. Возврат проверялся по KWin window UUID и output. Это не проверка сохранения DOM/input в произвольной вкладке и не тест Chrome extension. Массивы cycle_ms включают три команды, polling и запуск CLI-процессов; их нельзя сопоставлять с бюджетом input-to-photon.

Пользователь уточнил, что пульт выдаёт стандартные клавиши (Esc, Backspace и т. д.). Его отдельная интеграция не требуется; физический пульт не является блокером Phase 0. Qt Test дополнительно проверяет открытие навигации через Backspace и закрытие через Esc.

## Оставшаяся работа

- Terraria повторно проверена на обоих экранах. Первоначальный запуск был закрыт пользователем до проверки; это не свидетельство ошибки запуска. Следующая попытка выявила временное исчезновение fullscreen-окна при потере focus: цель возврата очищалась. Исправление хранит detached UUID и восстанавливает только ту же KWin identity при remap; после него прошли 40 циклов. Возврат к окну с новым UUID через production Activity matching ещё не реализован.
- Compositor texture/near/far/blur не реализованы и не измерены. Простой прозрачный overlay не закрывает визуальный контракт дизайна. [KWin effects API](https://develop.kde.org/docs/plasma/kwineffect/) описывает WindowThumbnail внутри compositor; переносимость этого пути в отдельный Qt shell не доказана.
- Lock/logout/suspend, inhibitors, wake, unplug/mirror outputs и изменение display layout при открытом Home ещё не проверены. Script проверяет ScreenSaver.GetActive перед Home; этого недостаточно для lock acceptance.
- Восстановление KWin bridge после daemon crash реализовано: QTimer отслеживает D-Bus owner, повторяет потерянный long-poll, отбрасывает старые ответы и повторно публикует сохранённый Activity context. Проверено SIGKILL при скрытом и открытом Home без reload adapter; [evidence](evidence/daemon-recovery.json). Перезапуск самого compositor и выход из сессии остаются отдельными сценариями.
- Spike содержит один provider, без manifest discovery, SDK, persistence, autostart и capability routing. Временные interfaces `Spike1` не объявлены стабильными. Error names, subscriptions, operation cancellation, backpressure и полный transport size limit ещё требуют production реализации.
- UI — две карточки и навигационный placeholder. Design system, modal focus scopes, browser/media/download providers и требования AC-01–AC-19 не объявлены выполненными.

## Очистка

После тестов выгружен временный KWin script и остановлены созданные `arinn-phase0-*` units. Проверка очистки: пустой list-units и isScriptLoaded=false. Настройки Steam/Proton и игровые сохранения для исправления запуска не редактировались; Terraria запускалась с отдельным savedirectory в /tmp.

## Технические выводы

KWin script подходит для первичного Home/Return с двумя дисплеями; [API KWin](https://develop.kde.org/docs/plasma/kwin/api/) предоставляет window/output tracking и activation. Прямая связка async QtDBus и zbus работает. Для возвращаемых структур zbus потребовался одноэлементный tuple, иначе wire body не совпадал с introspection и Qt decoding. Исправление закреплено интеграционным тестом и [фактическим XML](../plugin-api/phase0.xml).

Можно развивать production core/SDK и provider lifecycle. Значительный визуальный UI следует отложить до закрытия оставшихся platform/effects gates, чтобы не строить его на неподтверждённом compositor path.
