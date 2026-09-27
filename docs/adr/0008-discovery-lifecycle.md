# ADR-0008: XDG discovery и lifecycle через systemd user services

- Статус: Предложено (Proposed), требуется архитектурное ревью.
- Дата оформления: 2026-09-27.
- Основание: [Developer Specification v0.3](../specifications/developer-specification-v0.3.md), разделы 30, 31, 32, 33, 40, 42, 56, 58, 60.

> Обновление 2026-09-27: Manifest schema, XDG precedence, transient units, explicit reload и restart policy закрыты [ADR-0021](0021-plugin-runtime-ownership.md). Этот ADR сохраняет формулировку исходного v0.3; при расхождении действуют принятые уточнения ADR-0020–0025.

## Контекст

Provider должен подключаться через установку manifest без продуктовых изменений core. На Linux уже существует supervisor, поэтому daemon не должен дублировать его целиком.

## Предлагаемое решение

Каждый plugin имеет manifest со стабильным id, name, version, api_version, executable и capabilities. System и user plugins используют один формат. Пример manifest в ТЗ имеет TOML-представление; окончательную schema нужно зафиксировать в Plugin API.

Предпочтительные места discovery: $XDG_DATA_HOME/arinn/plugins и arinn/plugins в каждом элементе $XDG_DATA_DIRS.

arinnd оркестрирует discover/start/handshake/monitor/restart/stop. После старта plugin проходит handshake и регистрирует capabilities. Для supervision предпочтительны systemd user services или transient units; конкретный вариант ещё требует выбора.

Packaging устанавливает binaries и Chrome Native Messaging manifest. Архитектура не привязана к pacman.

## Последствия

Установка plugin становится самостоятельной операцией, а lifecycle использует возможности Linux. Необходимо согласовать конфликты discovery и границу ответственности daemon/systemd.

## Альтернативы и ограничения

Самостоятельный полноценный process supervisor в arinnd не рекомендуется ТЗ. Выбор между постоянными user services и transient units пока не сделан.

## Разрешение вопросов

Manifest schema, XDG precedence, transient units, explicit reload и restart policy закрыты [ADR-0021](0021-plugin-runtime-ownership.md).

### Вопросы исходного ревью (история)

Приоритет user/system manifest и дубликатов ID, executable lookup, refresh discovery, поведение удаления работающего provider, restart backoff/лимиты, handshake timeout. AC-11 не уточняет требование hot reload без restart daemon.

## Проверка решения

AC-11; установка/удаление provider без product-specific правок. Проверить handshake, несовместимую версию, restart/reconnect и отображение restart count/last error.

[Реестр ADR](README.md) · [Карта ревью](../architecture/review-map.md)
