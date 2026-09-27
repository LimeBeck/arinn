# API review: медиа и загрузки для уточнённого MVP

Дата: 2026-09-27. Статус: принято на уровне модели/семантики по ADR-0024/0025. Это отдельное расширение исходного v0.3, а не утверждение, что DownloadProvider там уже был. API ещё не опубликован как stable v1; до реализации provider schema фиксируется в versioned D-Bus XML и contract fixtures.

## Общие правила

Транспорт и operation model — [implementation baseline](../architecture/implementation-baseline.md). Ниже notation языка спецификации, не готовые Rust/QML signatures. Все mutating методы принимают request_id и expected generation, возвращают Operation; IDs opaque. Optional values в wire schema имеют explicit has-value flag, не магические пустые словари. Списки snapshot paginated и привязаны к generation/revision.

Extension interfaces доступны через introspection и registered feature flags, а core routing не зависит от продукта. Неизвестная feature не делает base capability несовместимой. Неизвестная top-level capability в старом daemon вызывает Unsupported при registration, поэтому DownloadProvider поставляется с поддерживающим его daemon.

## Rich media без обязательств для MPRIS

| Interface | Typed data | Методы / семантика |
| --- | --- | --- |
| MediaOpen1 | MediaSource: content EntityRef, URI, start_position_us, preferred audio/subtitle language; URI доступен только provider, не логируется | Open(source): создаёт managed session, result — session EntityRef. Источник должен быть разрешён library provider; UI не передаёт raw arbitrary command/URL. |
| MediaTracks1 | Track: id, kind audio/subtitle, language, label, codec, channels, selected, external | ListTracks(session); SelectTrack(session, kind, track_id); DisableSubtitles(session). Выбор audio требует реального ID; subtitle off — отдельное действие. |
| MediaTimeline1 | Chapter: id, start_us, title; BufferedRange: start_us/end_us; Thumbnail: asset_ref, timestamp_us | GetTimeline(session); RequestThumbnail(session, position_us). Неподдерживаемый thumbnail возвращает Unsupported; base seek работает независимо. |
| LibraryResume1 | Resume: content_id, position_us, duration_us, completed, observed_at | Получение resumable entries; SetProgress от связанной media session через daemon; RestartFromBeginning по явному действию. Изменение watched не является shell-local boolean. |
| LibraryImport1 | ImportCandidate: source EntityRef, mapped local file reference, completion identity | ImportCompleted: асинхронная сверка completed файлов в configured roots; результат — Library EntityRefs. Повтор безопасен и не дублирует записи. |

Эти интерфейсы применимы к mpv, другим players и library backends; они не содержат mpv property names. Raw player IPC остаётся в plugin.

## DownloadProvider v1

Новая шестая base capability: `download`. Она описывает download job, а не torrent protocol. Интерфейс `org.arinn.DownloadProvider1` предоставляет GetSnapshot, GetJob, Pause, Resume, StartNow, Retry; methods для удаления файлов и произвольного Transmission RPC отсутствуют.

| Поле DownloadJob | Тип и семантика |
| --- | --- |
| id | Opaque string, namespace provider |
| title | UTF-8 display title |
| state | queued, downloading, paused, verifying, completed, error |
| completed_bytes / total_bytes | uint64; total имеет has-value, если ещё неизвестен |
| rate_bytes_per_second | uint64 |
| eta_seconds | int64; -1 неизвестно |
| progress | double 0..1 с has-value; UI не показывает фиктивный процент при неизвестном размере |
| available_actions | enum set pause/resume/start_now/retry |
| error_code / error_message | нормализованный code и безопасный текст |
| outputs | Список typed completed output refs, а не сырые произвольные paths в QML |
| library_refs / import_state | Связанные Library EntityRefs; pending/imported/unavailable/unsupported, независимо от download state |

Pause прекращает transfer, Resume продолжает с сохранением queue policy, StartNow явно обходит очередь для выбранного job. Retry не создаёт новый job: повторяет допустимое восстановление существующего; неподдерживаемая операция = Unsupported. Recheck filesystem/verify — provider-specific часть Retry, а не гарантированное повторное скачивание всех данных.

Completed означает, что данные загружены и проверены backend; продолжающийся torrent seeding не делает файл незавершённым. «Добавлено в медиатеку» показывается только после LibraryImport success. Для non-media download — «Готово», без кнопки «Смотреть». Для медиа кнопка активна после получения Library EntityRef и открывает Detail.

UI соответствует design handoff: pause/resume/start now/retry, progress/status, service unavailable, completion toast. Дополнительное backend verifying состояние показывается как «Проверка…», без выдуманного ETA. Peers/seeds/ratio/trackers в общий API и default UI не входят.

MVP управляет уже добавленными заданиями Transmission. Форма добавления magnet/.torrent не показана в handoff и не является условием usable интеграции: задания можно добавлять штатным Transmission UI. Будущее AddSource требует typed source/validation и отдельного UI, а не передачи произвольного RPC через ActionProvider.

## Generic completion → library

Provider публикует transition в completed со стабильным completion identity. Daemon маршрутизирует событие выбранному LibraryImport1 provider. Download provider знает backend paths, library — configured roots; связь remote path → local mount задаётся явной конфигурацией. Отсутствие mapping означает import unavailable, а не сканирование произвольного пути.

Library provider сверяет файлы, создаёт/обновляет свои записи и возвращает refs. Completion ledger хранится в library provider и ключуется `(download provider, stable job ID, output identity)`; reconnect/replay не создаёт дубли New и toast. Browser downloads или другой download manager могут использовать тот же поток.

## Обоснование и границы review

Generic ActionProvider не подходит для структурированного списка jobs, revisions, статусов и доступности действий; product-specific Transmission API в core нарушил бы Rule 1. Поэтому выделяется DownloadProvider. LibraryImport1 отделяет завершение загрузки от индексирования контента и работает с любым download backend.

Выбранные типы, state machine и ownership закрывают архитектурный вопрос расширения. Точные D-Bus XML, serializers, SDK и executable contract suite — обязательные результаты реализации, не выполненные проверки. Лимиты transport и ошибки берутся из ADR-0020.
