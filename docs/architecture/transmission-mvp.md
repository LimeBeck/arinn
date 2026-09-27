# Transmission в MVP

Статус: принято по ответу пользователя и ADR-0025. Core получает только DownloadProvider entities и operations; Transmission остаётся внешним сервисом.

`arinn-plugin-transmission` подключается к настраиваемому RPC endpoint; default — loopback `http://127.0.0.1:9091/transmission/rpc`. Provider не включает RPC на внешнем интерфейсе и не меняет конфигурацию существующего сервера без отдельного действия установки. Для remote endpoint поддерживается HTTPS и credential reference; passwords/headers/session tokens не попадают в logs.

При connect согласовать поддерживаемую RPC version и capabilities; старые и новые RPC naming conventions остаются деталями adapter. Записать версии, реально проверенные integration tests, в diagnostics. Нельзя предположить совместимость с любым Transmission только из названия сервиса.

Session-id challenge HTTP 409 обрабатывается по RPC specification с повтором отклонённого запроса после обновления token. Это не retry уже исполненной команды. При неоднозначном сетевом timeout mutation не повторяется автоматически: сначала refresh job state. [Transmission RPC specification](https://github.com/transmission/transmission/blob/main/docs/rpc-spec.md).

Backend читается polling: 1 s, пока открыт Downloads или есть active transfers; 10 s в idle; reconnect 1/2/4/8/16/30 s с jitter. Provider публикует snapshot revision только при изменении нормализованных данных. При потере RPC cached строки явно unavailable/stale, команды disabled.

После завершения медиа проходит generic DownloadProvider → LibraryImport1 flow. Данные не перемещаются и не удаляются Arinn автоматически, seeding не прерывается ради индексации. Configured download destination должен входить в library roots либо иметь явный mapping; при его отсутствии загрузка остаётся completed, импорт — unavailable.

MVP показывает и управляет заданиями, добавленными штатным Transmission клиентом. Добавление задания из Arinn, tracker settings и torrent-specific statistics не являются требованиями текущего дизайна. Приёмка включает одно реальное задание с несколькими media files и non-media output, disconnect/reconnect, duplicate completion delivery и отсутствие дублирования Library/New.

[API review и модель jobs](../plugin-api/mvp-extensions.md).
