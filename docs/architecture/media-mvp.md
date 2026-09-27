# Медиа MVP: папки, библиотека и внешний mpv

Статус: принято для реализации по ADR-0024. Сценарий явно выбран пользователем; компоненты и технические defaults выбраны исполнителем.

## Providers

`arinn-plugin-library` реализует LibraryProvider, сканирует configured roots локальной файловой системы, включая уже смонтированные сетевые папки. Собственного SMB/NFS клиента, mount management или Jellyfin server для MVP не требуется. Scan асинхронный: initial scan, explicit rescan, filesystem hints и периодическая сверка каждые 10 min при autoscan. Сетевой I/O ограничен worker pool и deadlines; не блокирует core.

Library metadata/resume DB — собственное состояние provider, не core. Stable item UUID сохраняется в его SQLite; совпадение inode/size/mtime помогает распознать rename только внутри доступного root, не является глобальным ID. Недоступный mount делает root unavailable; не помечать все фильмы удалёнными. Symlinks за пределами configured roots не обходятся автоматически. «Убрать из медиатеки» скрывает запись, не удаляет файл.

Метаданные MVP: title/year/season/episode из filename и sidecar NFO, duration/track info через ffprobe, local poster/fanart рядом с видео. При отсутствии данных — название файла, generic artwork и неизвестные поля, без выдуманного сюжета или года. XML NFO разбирается без external entities/network DTD. Онлайн-enrichment не является обязательной зависимостью usable MVP; может добавляться provider-конфигурацией позднее. Такой fallback позволяет начать с любых папок, не требуя от пользователя API key.

`arinn-plugin-mpv` реализует MediaProvider и дополнительные typed playback interfaces. Сам mpv — отдельный player process с обычным video window. Команды и события идут через private Unix JSON IPC socket в XDG_RUNTIME_DIR (directory 0700, без публичного /tmp endpoint). Shell не обращается к сокету напрямую и не встраивает собственный media engine. mpv предоставляет JSON IPC и track-list; выбор внешнего player опирается на эти возможности. [mpv manual](https://mpv.io/manual/stable/).

## Shell OSD и платформа

Video рисует mpv, OSD/track sheets рисует Arinn через platform overlay. Важно проверить два режима: OSD hidden — video window получает input adapter и посылает semantic events через plugin/daemon; OSD visible — shell получает focus. Не дублировать seek между mpv bindings и shell. В поставляемой конфигурации mpv отключить конфликтующие default bindings и OSC, определив Arinn actions. Никаких глобальных обычных букв для управления другими приложениями.

Это дополнительный integration gate перед media UI: overlay над видео без потери playback, корректный input, audio/subtitle sheets и Home/Return. Возможность плеера выбирать track подтверждается API, работоспособность QML overlay на целевом HTPC ещё не подтверждена.

## Контракт и владение resume

MediaProvider base остаётся совместимым с MPRIS. Rich features доступны отдельными typed interfaces/feature flags `MediaTracks1`, `MediaTimeline1`, `MediaOpen1`, описанными в API дополнении. MPRIS provider не обязан объявлять их. В UI отсутствующая возможность скрывается/disabled, а не превращается в no-op.

Library launch возвращает content EntityRef и вызывает выбранный MediaOpen provider через daemon routing. Media session сообщает тот же content ref. Library provider получает progress updates через daemon subscription и пишет resume каждые 5 s, на pause/seek/end/stop. Только library — persistent source of truth; mpv watch-later не создаёт второй конкурирующий resume store для управляемых Arinn сессий.

Resume >0 отображается как «Продолжить». Правило 90 s из дизайна применяется к обычным фильмам/эпизодам с duration >180 s; для короткого/неизвестного duration завершение только по подтверждённому EOF. При manual seek в конец watched не ставится без playback/EOF confirmation. Следующий episode переводится на position 0 только при однозначном порядке season/episode. Auto-next — отдельная настройка. Missing file/mount не стирает resume.

Chapters и buffered ranges отображаются только при наличии данных. Thumbnail seek preview — отдельный асинхронный extractor/cache в provider (например, ffmpeg), ограниченный по частоте и отменяемый; он не перематывает основной mpv для получения картинки. Пока thumbnail не готов, timeline и время остаются рабочими. Skip intro доступен только при реальных chapter/segment metadata; не вводится собственный распознаватель заставок.

## Проверка

AC-15/16: local file, mounted network file, missing file, mount loss/recovery, два audio tracks, external/embedded subtitle, pause/seek/restart resume, конец фильма и переход эпизода. Provider crash не закрывает весь shell; player запускается в отдельном application scope, reconnect к принадлежащему Arinn player возможен без kill чужих mpv.

Codec, hardware decoding, HDR и passthrough не объявляются гарантированными по документации: это проверка на отдельном HTPC. Library/parser и IPC contract tests запускаются на машине разработки.
