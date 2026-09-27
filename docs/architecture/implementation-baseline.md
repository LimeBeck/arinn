# Техническая основа разработки

Дата: 2026-09-27. Статус: выбрано для реализации по поручению пользователя самостоятельно закрыть технические решения. Это дополнение к developer v0.3 и design handoff; исходники не редактируются. Принятые решения — ADR-0020–0022. Числовые лимиты ниже являются выбранными defaults проекта, а не требованиями сторонних спецификаций.

## Стек и границы

- Shell: Qt 6 Quick/QML, небольшой C++ слой QtDBus и typed models; UI thread не выполняет blocking D-Bus calls. CMake, Qt Quick Test/Qt Test.
- Core/CLI/первые plugins: Rust workspace, Tokio, zbus, serde для файлов конфигурации; cargo test и общий contract harness. Версии зависимостей фиксируются lockfile при создании skeleton, без плавающих production dependencies.
- Session D-Bus для shell/CLI ↔ daemon и daemon ↔ plugins. Публичное имя `org.arinn.Daemon1`, корень `/org/arinn/Daemon1`, interfaces с суффиксом major version `1`. Provider имеет unique bus owner и экспортирует `/org/arinn/Provider1`; core proxy доступен клиентам под `/org/arinn/Daemon1/providers/p_<uuid_hex>`. UUID хранится в registry; opaque plugin IDs не преобразуются напрямую в object paths.
- Только daemon является интеграционным API shell. Клиент не обращается к provider endpoint, MPRIS или KWin напрямую.
- Начальный build target — текущее стабильное окружение Arch/EndeavourOS, KDE 6/Wayland. Проверки portability на другие DE откладываются.

Qt предоставляет asynchronous D-Bus calls; zbus — подключение к session bus. Это основание выбора bindings, а не проверка реализации Arinn: [QtDBus](https://doc.qt.io/qt-6/qdbusconnection.html), [zbus](https://docs.rs/zbus/latest/zbus/connection/struct.Builder.html).

## Идентификаторы и маршрутизация

`plugin_id` — стабильный ID пакета из manifest. `provider_id` — стабильный ID настроенного экземпляра plugin; по умолчанию равен plugin_id. Несколько экземпляров объявляются конфигурацией daemon. Один provider регистрирует несколько capabilities.

`EntityRef = (provider_id, capability, local_id)`: все три поля передаются раздельно как strings. Core знает capability, но не разбирает local_id. Ephemeral entities дополнительно адресуются с `generation`; stale generation даёт Conflict, никогда не перенаправляется к новому объекту с тем же ID. Favorites хранят только объявленные provider стабильные launch targets, а не временные tab IDs.

Команда на entity всегда идёт её provider. Для generic команды (например, открыть URL без entity) используется сохранённый default capability provider; при единственном доступном provider он выбирается автоматически. При нескольких без default требуется выбор в Settings, без скрытого product-specific приоритета. Все providers можно агрегировать в список.

## Snapshot и reconnect

Область revision — пара `(provider_id, capability)`. `generation` — UUID, выдаваемый daemon на каждую новую регистрацию, включая повторную регистрацию после restart daemon. Revision — uint64, начинается с 1 и строго растёт только при изменениях внутри generation. Одинаковые snapshots не требуют новой revision. Переполнение требует новой регистрации/generation.

StateChanged содержит provider_id, capability, generation, revision. Snapshot атомарно содержит ту же пару generation/revision и typed entries. Consumer устанавливает signal match до чтения первого snapshot, буферизует notifications, принимает snapshot и запрашивает следующий, если уже увидел большую revision. События можно coalesce; устаревшие ответы отбрасываются. Новый owner/generation инвалидирует весь старый snapshot и pending operations, затем выполняется полная загрузка.

Large lists читаются страницами до 200 entries или 256 KiB encoded body, что наступит раньше. Page token привязан к immutable generation/revision и истекает через 30 s; Conflict означает начать чтение заново. Старый complete snapshot остаётся видимым до завершения нового; страницы разных revisions не смешиваются. Signal dispatch и snapshot publication атомарны относительно внутреннего state commit.

D-Bus unique names и NameOwnerChanged позволяют отслеживать lifetime соединения; прикладную generation и revisions вводит сам Arinn. [D-Bus specification](https://dbus.freedesktop.org/doc/dbus-specification.html).

## Команды и ошибки

Typed capability methods принимают `request_id` (UUID от caller), возвращают operation object path немедленно после валидации и постановки в очередь. Operation имеет states queued/running/succeeded/failed/cancelled, typed error, start/end times и result entity refs. Клиент читает operation после установки подписки: сигнал мог прийти раньше ответа метода. InvalidArgument/NotFound/Unsupported/PermissionDenied при валидации возвращаются сразу как D-Bus error.

Ошибки: NotFound, Unavailable, Unsupported, PermissionDenied, Timeout, InvalidArgument, Conflict, Internal, Cancelled; пространство `org.arinn.Error1.*`. Operation state cancelled соответствует Cancelled. Тексты ошибок безопасны для UI; stack trace остаётся в diagnostics logs без payload.

Defaults: IPC admission/read — 3 s; controls/activate/seek — 5 s; launch/open/createTab — 30 s; handshake — 10 s. После timeout результат команды неизвестен, пока состояние не сверено snapshot. Успех launch означает подтверждённую отправку запуска или найденный running instance, но не готовность страницы/видео. UI ждёт фактическую Activity для перехода.

Cancel — best effort только если операция объявляет cancellable. Внешний side effect отмена не откатывает. Daemon не повторяет mutating commands автоматически. В пределах одного daemon epoch повтор `(caller unique owner, request_id)` возвращает ту же operation; другое содержимое с тем же ID — Conflict. Operation удерживается 5 min после завершения; после reconnect нельзя считать сохранённой гарантию дедупликации. На disconnect/generation change pending завершаются Unavailable, запоздалые ответы игнорируются.

## Ограничения и валидация

Body сообщения до 256 KiB; строка ID до 256 bytes UTF-8, обычный текст до 8 KiB, URL до 16 KiB. Artwork/video не передаются inline в snapshot: provider сообщает typed asset reference, daemon загружает или отдаёт cache asset. В QML не попадает arbitrary HTML/JS. Optional metadata — ограниченный a{sv} со scalar values и string arrays, до 32 keys; основные модели остаются typed.

Native bridge применяет тот же лимит, разбивает большие snapshots на страницы и не пишет логи в stdout. В обоих направлениях validation до allocation по недоверенной длине. Устанавливать memory/queue caps на соединение; максимум 32 concurrent operations/provider, переполнение — Unavailable с retryable metadata; UI не запускает retry автоматически.

Проверять тип, длину, принадлежность provider и допустимость команды. Browser URL по умолчанию http/https; javascript/data/file и arbitrary schemes не принимаются generic open. Другие схемы только явно объявленными typed actions provider. .desktop запускается системным desktop-entry механизмом без shell interpolation из QML.

Обычные логи не содержат URL, titles вкладок, payload, содержимое страниц и секреты. Только IDs, operation, outcome и безопасный error code. Debug logging не снимает это ограничение.

## Хранение

`$XDG_CONFIG_HOME/arinn/config.toml`: UI preferences, provider config/defaults и настройки Home. Запись через временный файл в той же директории, fsync и atomic rename. Shell изменяет конфигурацию через daemon; schema_version и migration до запуска providers.

`$XDG_STATE_HOME/arinn/state.sqlite3`: favorites/order, stable provider-instance mapping, recent Arinn launch targets и diagnostic counters. SQLite transaction на изменение и schema migration; migration failure оставляет прежнюю БД и включает diagnostics/degraded state. SQLite выбран для транзакций, без собственной ORM requirement. [Обоснование атомарности SQLite](https://www.sqlite.org/atomiccommit.html).

Recent launch targets: до 100, retention 30 дней. Browser tab titles/URL/history не архивируются. Favorites сохраняются при удалении provider и помечаются unavailable, удаляются только действием пользователя. Секреты не записываются в обычный TOML: credential reference в системное secret storage при появлении интеграции, которой это требуется.

`$XDG_CACHE_HOME/arinn/`: artwork с LRU budget 512 MiB; сброс cache не теряет preferences. Provider databases принадлежат providers под XDG data/state в подкаталогах provider ID (имя каталога — registry UUID, без подстановки opaque ID в путь). Состояние playback не дублируется core. Snapshots live entities хранятся в памяти.

## Наблюдаемость и тесты

Logs JSON в stderr → journald; уровни error/warn/info/debug, default info. Отсутствующие provider_id/request_id/error — null. duration_ms — monotonic elapsed duration. Diagnostics содержит runtime state, backend availability, generation, versions, capabilities, last safe error/code/time, restarts_since_daemon_start и cumulative_restart_count. Last error сохраняется до следующего error/ручного clear, помечается recovered_at после успешного восстановления.

CLI: human text по умолчанию, `--json` для всех read commands; exit 0 success, 1 runtime failure, 2 invalid usage, 3 daemon unavailable. `provider restart <id>` и `providers reload` включить в технический baseline.

Contract harness запускает любой provider через manifest и проверяет только объявленные capabilities. Demo fixtures: empty/populated, delayed, invalid ID, crash, reconnect, revision race, unavailable backend. Rust unit/integration через cargo test; QML interaction через Qt Quick Test, IPC adapter через Qt Test.

Начальные engineering budgets на целевом устройстве: p95 input-to-focus ≤50 ms при тёплом UI и 60 Hz, p95 frame time ≤16.7 ms без тяжёлого initial load; медленный provider не влияет на focus. Это цели для измерения, не результаты. Perf regression оценивается на одном зафиксированном окружении; обычный CI не выдаёт synthetic FPS за TV acceptance.
