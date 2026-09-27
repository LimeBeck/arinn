# ADR-0024: Локальная библиотека и внешний mpv с rich media API

- Статус: Принято для реализации (Accepted).
- Дата: 2026-09-27.
- Основание: явные ответы пользователя по продуктовым вопросам; техническая реализация выбрана по делегированному поручению.

## Контекст

Пользователь выбрал каталог локальных/смонтированных папок, metadata, внешний player, audio/subtitle selection.

## Решение

Реализовать LibraryProvider для папок и mpv MediaProvider через private JSON IPC; QML OSD остаётся в shell. Resume принадлежит library provider, transient playback — media provider. Rich typed interfaces оформлены отдельным API review.

[Подробная спецификация](../architecture/media-mvp.md) · [Отдельный review расширений API](../plugin-api/mvp-extensions.md).

## Последствия

MPRIS остаётся базовой интеграцией других плееров, но не обязан реализовывать tracks/thumbnails. Добавляется integration gate для OSD/input поверх video window.

## Альтернативы

Jellyfin как обязательный сервер и только MPRIS transport controls не удовлетворяют выбранному сценарию. Собственный media engine не вводится.

## Проверка и оставшаяся работа

Проверить новые AC-15–AC-19 из [уточнений MVP](../architecture/product-decisions.md) по применимости. Конкретные wire schemas и runtime integration tests входят в разработку. Compositor/input/media acceptance на целевом HTPC ещё не выполнялась; не считать её закрытой принятием ADR.

[Реестр ADR](README.md)
