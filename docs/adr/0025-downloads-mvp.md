# ADR-0025: DownloadProvider и Transmission в MVP

- Статус: Принято для реализации (Accepted).
- Дата: 2026-09-27.
- Основание: пользователь явно включил Transmission в MVP; техническая реализация выбрана по делегированному поручению.

## Контекст

Исходное developer ТЗ откладывало Downloads. Уточнение пользователя меняет scope MVP; Rule 10 требует отдельно оформить обобщённое расширение API.

## Решение

Ввести generic DownloadProvider с typed job state/actions, отдельный Transmission adapter и LibraryImport1 для completed media. [Отдельный API review](../plugin-api/mvp-extensions.md) фиксирует операции, ограничения и идемпотентность импорта. [Runtime интеграции](../architecture/transmission-mvp.md) описывает RPC, reconnect и связь с библиотекой.

## Последствия

Базовых capabilities теперь шесть. Downloads становится работающим разделом MVP; completed и imported — разные состояния, torrent terminology остаётся внутри plugin. Core не обращается к Transmission напрямую. Исходное ТЗ сохраняется без изменений, это явное уточнение имеет приоритет.

## Альтернативы

Зарезервированный Downloads route не удовлетворяет уточнению пользователя. Raw Transmission RPC в core и обход через произвольные Actions не используются.

## Проверка и оставшаяся работа

Проверить AC-17 из [уточнений MVP](../architecture/product-decisions.md): queue/actions/error/reconnect/completion/import без дублей Library/New. Wire schemas, serializers и executable contract tests входят в разработку. Принятие модели API не означает реализованную или протестированную интеграцию.

[Реестр ADR](README.md)
