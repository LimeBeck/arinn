# Запуск Phase 0

Экспериментальный вертикальный срез: Rust daemon, отдельный demo provider, Qt Quick shell, CLI и временный KWin script. Интерфейс `org.arinn.Spike1` не является стабильным Plugin API v1. UI проверяет IPC/focus, а не соответствие design handoff.

## Сборка

Нужны Rust с edition 2024, CMake ≥3.24, Ninja, C++20, Qt6 Quick/Quick Controls/DBus/Test ≥6.6, Python 3, Node.js (для regression test адаптера), dbus-run-session. Для живой проверки — KDE Plasma 6 Wayland, systemd user session и qdbus6. Фактически проверенные версии находятся в [отчёте](phase-0-report.md).

Из корня репозитория:

```sh
cargo build --locked
cmake -S . -B build -G Ninja
cmake --build build -j4
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
cargo fmt --all --check
node --test tests/integration/kwin_adapter.test.cjs
dbus-run-session -- python tests/integration/phase0.py
```

Private-bus suite запускает Qt Test автоматически, с offscreen/software rendering. Он проверяет реальное чтение typed snapshot QtDBus и локальный focus во время медленного действия. Живой рабочий стол не используется этим suite.

## Живая сессия

```sh
python scripts/phase0-session.py start
target/debug/arinnctl status
target/debug/arinnctl snapshot
```

Meta+F12 открывает Home на мониторе foreground Activity; Esc или Backspace возвращают туда же. Без Activity Back открывает прототип навигации. Стрелки и Enter работают локально в QML. CLI также предоставляет `home`, `back`, `activate`.

Запускаются три transient user units: `arinn-phase0-daemon`, `arinn-phase0-demo`, `arinn-phase0-shell`, с ограниченным restart-on-failure. В `.phase0/` хранится token (0600), папка — 0700. KWin script загружается только на эту сессию; постоянный autostart не устанавливается.

```sh
python scripts/phase0-session.py stop
```

Stop возвращает Activity, если она ещё доступна, выгружает script и останавливает три службы. Никаких изменений layout/scale мониторов не требуется. KDE может сохранить неактивную запись временного shortcut после выгрузки script.

## Проверка Activity оператором

Открыть тестовое приложение, затем запустить `python tests/integration/kwin_cycles.py`. Скрипт выполняет 20 циклов через KGlobalAccel и проверяет UUID возвращённого окна и output. Во время проверки не переключать/закрывать окна. Это не тест физической клавиатуры или G60S Pro и не измерение input-to-photon latency.

`tests/integration/kwin_focus.py RESOURCE_CLASS --fullscreen --output OUTPUT_NAME` — вспомогательная команда для явно выбранного тестового окна. Она меняет fullscreen/placement только этого окна, поэтому применять её следует к отдельному fixture, а не рабочему окну пользователя.

При перезапуске daemon shell и provider переподключаются, но экспериментальный KWin long-poll нужно перезагрузить: `stop`, затем `start`. Production recovery остаётся отдельной задачей.
