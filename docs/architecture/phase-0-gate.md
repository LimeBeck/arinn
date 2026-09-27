# Phase 0: проверка KDE/Wayland

Статус: **частично пройден**. Spike реализован; [runtime evidence и ограничения](phase-0-report.md), [команды сборки/запуска](phase-0-runbook.md). Полный gate ещё открыт.

Целевой HTPC: один экран. Два монитора используются только на машине разработки; отдельный multi-monitor setup не является требованием к пользователю HTPC.

## Выбранный путь проверки

Начать с packaged KWin script как части `arinn-platform`: global shortcut через KWin, foreground window tracking, activation shell и restore прежнего окна. Typed bridge daemon ↔ platform adapter принимает только заранее определённые операции, не arbitrary scripts от plugins. Product-specific window logic остаётся в providers, compositor operations — в platform adapter.

KWin scripting API документирует registerShortcut и workspace/window APIs. Для near/far effects отдельно исследовать KWin effect и WindowThumbnail внутри compositor; доступность такого thumbnail в произвольном shell-процессе из этого не следует. [KWin API](https://develop.kde.org/docs/plasma/kwin/api/), [KWin effects](https://develop.kde.org/docs/plasma/kwineffect/).

Не привязывать окончательную реализацию к ещё не проверенному capture path. Fallback для технического spike — обычный overlay без texture/blur. Его достаточно для проверки focus, но недостаточно для принятия визуального требования handoff; production fallback нужно обсуждать только если эффект окажется недоступен.

## Матрица gate

| Проверка | Условие успеха | Результат |
| --- | --- | --- |
| Qt/QML ↔ Rust через D-Bus | Async status/snapshot и operation, отсутствие UI blocking | PASS: Qt Test на private bus, focus во время медленного action |
| External demo process | Register, snapshot, crash/restart с новой generation | PASS: contract suite и SIGKILL transient unit |
| Chrome Home/Return | Global Home работает из browser, после Return то же окно/вкладка | PASS для окна: 20 циклов на каждом из двух outputs; DOM/tab state отдельно не проверен |
| Steam и полноэкранная игра | Home поверх activity, восстановление focus, приложение не завершается | PASS: Steam 20/20, Terraria fullscreen по 20/20 на каждом output после исправления remap |
| Удаление target window | Return не активирует другой случайный window, Home остаётся доступен | PASS на изолированном Chrome fixture |
| Повтор Home | Нет duplicate shell; input scope корректен | Повтор сохраняет target/output во всех циклах; полные modal scopes ещё не реализованы |
| Texture/near/far effects | Реальное внешнее окно, ограничения и нагрузка измерены | Не запускалась |
| Lock/session shutdown | Overlay не обходится поверх lockscreen, units завершаются корректно | SessionGuard и private-bus tests готовы; реальные проверки отложены пользователем. [Lifecycle](session-lifecycle.md) |

В отчёте сохранить версии KDE/Qt/GPU/driver, native Wayland/XWayland mode приложения, monitor scale и fullscreen mode. Минимально проверить repeated Home/Return 20 циклов на Chrome и игре. Сбой focus является fail gate; отсутствие графического эффекта — отдельное незакрытое требование дизайна.

## Исходная read-only проверка

2026-09-27: окружение текущей машины сообщает `XDG_SESSION_TYPE=wayland`, `XDG_CURRENT_DESKTOP=KDE`; доступны kwin_wayland, qdbus6, systemctl, Rust/Cargo, CMake. `kwin_wayland --version`: 6.7.5; Qt6Core/Qt6DBus/Qt6Quick через pkg-config: 6.11.2; systemd: 261.3. Вызов версии plasmashell не дал надёжного результата из-за ошибки logging/interposition; версия Plasma Shell здесь не утверждается.

На документальном этапе установка KWin scripts не выполнялась. Затем пользователь разрешил runtime spike на этой машине: временный adapter/shortcut были загружены, результаты приведены выше. Постоянный autostart не устанавливался.

Пользователь подтвердил отдельный KDE/Wayland HTPC и G60S Pro (HID mouse/keyboard), а затем разрешил считать текущую машину технически эквивалентной для spike и уточнил наличие двух мониторов. Пульт выдаёт стандартные keyboard/mouse events; отдельная интеграция и обязательный физический input gate не требуются. При развёртывании достаточно дополнительного smoke test удобства. Поведенческий Home использует настраиваемый global shortcut.
