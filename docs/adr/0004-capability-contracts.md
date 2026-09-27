# ADR-0004: Пять capabilities и представление без plugin UI

- Статус: Предложено (Proposed), требуется архитектурное ревью.
- Дата оформления: 2026-09-27.
- Основание: [Developer Specification v0.3](../specifications/developer-specification-v0.3.md), разделы 2, 18, 19, 20, 21, 22, 23, 24, 29, 45, 59, 60.

> Обновление 2026-09-27: Identity/routing определены [ADR-0020](0020-dbus-contract-baseline.md). Шестая capability DownloadProvider добавлена отдельным review [ADR-0025](0025-downloads-mvp.md). Этот ADR сохраняет формулировку исходного v0.3; при расхождении действуют принятые уточнения ADR-0020–0025.

## Контекст

Новые интеграции должны добавляться установкой provider, а не условиями по именам продуктов в core и QML. Единый TV UI должен сохранять навигацию и визуальную целостность.

## Предлагаемое решение

Plugin API v1 содержит ApplicationProvider, BrowserProvider, MediaProvider, LibraryProvider, ActionProvider. Plugin может реализовывать несколько capabilities; registry и routing опираются на них.

| Capability | Концептуальные операции |
| --- | --- |
| ApplicationProvider | listApplications, getApplication, launch, activate, terminate, listRunning |
| BrowserProvider | getWindows, getTabs, open, activateTab, closeTab, createTab, getActiveTab |
| MediaProvider | listSessions, getSession, play, pause, playPause, seek, next, previous, stop |
| LibraryProvider | listCollections, listItems, getItem, launch |
| ActionProvider | Generic actions с machine-readable id, title, icon, enabled |

Application и browser IDs opaque: core не разбирает их содержимое. Например, application_id="steam" интерпретирует только соответствующий provider.

Plugins поставляют data, metadata, actions и capabilities; произвольные QML/HTML и другой загружаемый UI запрещены. Весь UI рисует Arinn.

Если capabilities достаточно, новая интеграция не меняет arinnd/arinn-shell. Если недостаточно — требуется отдельное изменение Plugin API, применимое шире одного продукта. DownloadProvider не вводится до такого review.

## Последствия

Интеграции становятся независимыми от presentation. Расширения, требующие новых семантических операций, проходят API review; произвольный plugin UI не может использоваться как обход ограничения.

## Альтернативы и ограничения

Product-specific ветвления и типы вроде SteamLibrary/JellyfinLibrary противоречат ТЗ. ActionProvider предназначен для generic действий и не разрешает arbitrary code injection.

## Разрешение вопросов

Identity/routing определены [ADR-0020](0020-dbus-contract-baseline.md). Шестая capability DownloadProvider добавлена отдельным review [ADR-0025](0025-downloads-mvp.md).

### Вопросы исходного ревью (история)

Как связаны plugin_id и provider_id, как маршрутизировать одинаковые opaque IDs у разных providers? Как выбирать provider при нескольких реализациях capability? Какие wire-типы и метод исполнения Action определить?

## Проверка решения

AC-11, AC-12; Rules 1, 4, 9, 10. Установка нового provider с существующими capabilities не требует правки shell/core. Концептуальные операции ещё не являются замороженными IPC signatures.

[Реестр ADR](README.md) · [Карта ревью](../architecture/review-map.md)
