# Discovery, supervision и Chrome bridge

Статус: принято для реализации по ADR-0021, 2026-09-27. Runtime ещё не реализован.

## Manifest и discovery

Manifest — TOML, обязательные поля `id`, `name`, `version`, `api_version = 1`, `executable`, `capabilities`; optional `args` (array of strings). `executable` после установки — абсолютный путь к executable regular file. Ни executable, ни args не являются shell expression. Capabilities: application, browser, media, library, action, download (добавлено ADR-0025); их API major — 1. Неизвестная capability не регистрируется молча: manifest получает Unsupported в diagnostics.

Discovery читает `*.toml` непосредственно в `$XDG_DATA_HOME/arinn/plugins`, затем в `arinn/plugins` каждого элемента `$XDG_DATA_DIRS` по порядку. XDG defaults применяются по спецификации. User manifest перекрывает system manifest с тем же ID; при невалидном user manifest provider отключён с явной ошибкой, без неожиданного fallback. Дубликаты ID в одном уровне — Conflict; не выбирать файл случайно. Source path отображается в diagnostics.

На startup и при `arinnctl providers reload` registry перечитывается; наблюдение файлов и hot reload по таймеру не требуются MVP. Удаление manifest при reload останавливает provider unit и убирает capabilities; запущенные им пользовательские приложения не завершаются. Изменение executable/API/capabilities требует restart provider; изменение display metadata обновляет registry без перезапуска. Favorites остаются unavailable.

## Units и ownership

Daemon: обычный `arinnd.service`, привязан к graphical session, без user lingering. Shell запускается session autostart launcher после входа в Plasma и обращается к тому же daemon. Autostart не должен создавать второй экземпляр shell/daemon; packaging тестирует shutdown graphical session.

Provider: systemd **transient user service**, создаётся daemon через systemd D-Bus API. Имя unit строится из registry UUID; ExecStart — validated binary + argv. Shell-строк нет. Только systemd перезапускает process; daemon контролирует handshake и availability.

Defaults: Type=exec, Restart=on-failure, RestartSec=2s, StartLimitIntervalSec=60s, StartLimitBurst=5; TimeoutStopSec=5s. После исчерпания лимита — failed, ошибка в diagnostics и ручной restart, без бесконечного цикла. Restart очищает failed/start-limit состояние только по явному запросу. Handshake timeout завершает зависший plugin, учитывается в том же restart budget.

Provider runtime: discovered → starting → ready; при crash — restarting/failed; при explicit stop — stopped, без auto-restart; несовместимый manifest/API — incompatible, рабочая capability не появляется. Состояние backend отдельно: available/unavailable/degraded. Закрытый Chrome или отсутствие MPRIS player не означает crash plugin.

Работающие внешние приложения не входят в plugin service cgroup: запуск desktop application выполняется session application launcher и отдельным application unit/scope. Это необходимо проверить в integration test: restart/remove desktop plugin не убивает Chrome/Steam. Установленный системой launcher должен разбирать .desktop, DBusActivatable и field codes по стандарту. [Desktop Entry Specification](https://specifications.freedesktop.org/desktop-entry/latest-single/).

Документация systemd подтверждает поддержку настроек transient services; конкретные defaults выше — выбор Arinn. [Transient settings](https://systemd.io/TRANSIENT-SETTINGS/). Restart semantics дополнительно сверяются с установленными systemd.service(5)/systemd.unit(5) при реализации.

## Handshake и восстановление

1. Registry валидирует manifest и создаёт unit с provider_id и одноразовым registration token в runtime credentials/file (0600, не логировать).
2. Plugin подключается к session bus и экспортирует provider object.
3. Plugin вызывает Register(plugin_id, provider_id, api_major, api_minor, version, capabilities, object_path, token). Daemon берёт sender из D-Bus envelope, сверяет UID, launch token и принадлежность PID к ожидаемому unit/cgroup. Токен не заменяет sandbox.
4. Daemon принимает только capabilities из manifest, возвращает daemon_epoch, generation и согласованные limits; provider доступен только после успешной регистрации и первого valid snapshot.
5. NameOwnerChanged инвалидирует provider. При restart daemon providers замечают смену owner, получают новый registration challenge для существующего unit и регистрируются снова с новой generation. Registry не запускает duplicate unit.

Token replay и второй active owner одного provider дают PermissionDenied/Conflict. API v1 поддерживает только major=1; minor additions не меняют существующие required fields или signatures. Unknown optional metadata keys игнорируются, неизвестные enum значения отображаются как unsupported, не как success. Wire XML фиксируется отдельно по capability; major несовместимость отражается до запуска рабочего provider.

## Chrome: два процесса вместо двух supervisors одного процесса

`Chrome MV3 extension ↔ Native Messaging bridge ↔ arinn-plugin-chrome ↔ arinnd`.

Chrome запускает короткоживущий `arinn-chrome-host` при connectNative. Host только валидирует/framing/передаёт сообщения; provider остаётся долгоживущим systemd service. Host ↔ provider — отдельный typed D-Bus interface на session bus; plugin-specific transport не проникает в core. После disconnect host завершается, Chrome plugin продолжает жить с backend unavailable или с оставшимися browser connections.

Chrome действительно владеет lifetime native host и использует stdin/stdout; persistent port создаётся connectNative. [Native Messaging](https://developer.chrome.com/docs/extensions/develop/concepts/native-messaging). Поэтому host не является ещё одним экземпляром systemd provider.

В extension permissions — tabs, nativeMessaging; content scripts и host_permissions для чтения страниц не добавляются. Manifest allowed_origins содержит точный extension ID. Incognito по умолчанию не подключается. Каждая bridge connection получает UUID, табы имеют opaque локальный ID, составленный самим plugin из connection identity и browser ID. Core не разбирает его. Разные browser profiles могут дать несколько connections без создания профилей Arinn.

Extension reconnect: 1, 2, 4, 8, 16, 30 s с jitter и потолком 30 s; после успешного connect полный snapshot, затем events. Это восстановление соединения, не повтор пользовательской mutating команды. Старые tab IDs после потери browser connection недействительны.

Если browser закрыт, launch идёт через выбранный ApplicationProvider; open URL ждёт доступного browser connection в пределах общего launch deadline. При нескольких connections выбирается default/последняя foreground connection, только если она ещё существует. Внутри выбранного browser frontend activation таба не заменяет compositor activation окна — последнее запрашивается через PlatformService.

## Desktop и media edges

Desktop provider сообщает installed entries и running instances. Window identity выдаёт PlatformService. Закрытие приложения сначала направляется как graceful close окон; при невозможности — Unsupported, без kill по имени процесса. Force kill не является обычным terminate MVP. Несколько окон представлены отдельными instances; activate выбирает последнее использованное доступное окно.

Media selection: session, явно выбранная пользователем; затем session, явно связанная с foreground Activity; затем playing session с самым недавним observed state change; затем most recent paused. Никакого выбора по названию продукта. Position/duration/absolute seek — signed int64 microseconds, unknown duration = -1, position неотрицательна. Provider публикует position anchor при state/seek и не чаще 1 Hz при playback; shell локально интерполирует по monotonic clock и сверяется с anchors. Actions фильтруются по capabilities.
