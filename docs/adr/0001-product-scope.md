# ADR-0001: Arinn как оркестратор общего домашнего HTPC

- Статус: Предложено (Proposed), требуется архитектурное ревью.
- Дата оформления: 2026-09-27.
- Основание: [Developer Specification v0.3](../specifications/developer-specification-v0.3.md), разделы 1, 2, 3, 13, 49, 50, 51, 56, 61.

> Обновление 2026-09-27: Объём media/Downloads закрыт [ADR-0023](0023-mvp-product-clarifications.md), [ADR-0024](0024-local-library-mpv.md) и [ADR-0025](0025-downloads-mvp.md). Этот ADR сохраняет формулировку исходного v0.3; при расхождении действуют принятые уточнения ADR-0020–0025.

## Контекст

Основной сценарий — Balanced HTPC: браузер и онлайн-видео, локальное и сетевое медиа, Steam, ES-DE и обычные Linux-приложения. По исходному ТЗ браузер занимает около 70% времени использования. Устройство общее для двух людей.

## Предлагаемое решение

Arinn предоставляет TV UX, оркестрацию Activity, агрегацию capabilities и plugin platform поверх существующей Linux-системы. Он использует готовые браузер, медиаплеер, Steam и ES-DE.

В v1 используется единое домашнее пространство без login screen, выбора профиля и отдельных favorites/history/settings. Browser integration входит в MVP; Steam и EmulationStation имеют отдельные точки входа уже с generic ApplicationProvider. Минимальный набор plugins: demo, desktop, chrome, mpris.

Первый официальный target — Arch Linux / EndeavourOS, KDE Plasma 6, Wayland; core не зависит от pacman. Downloads сохраняется как UI route, но DownloadProvider требует отдельного API review и не включается в v1 заранее.

За пределами v1: собственные browser/media engines и desktop environment, marketplace, remote plugin repository, arbitrary QML plugins, automation/recommendation engines, multi-user accounts, cloud sync, Windows/macOS, собственные реализации Steam library и Jellyfin.

## Последствия

MVP включает ежедневный browser workflow и запуск игр без ожидания богатых библиотечных интеграций. Отсутствие внутренних аккаунтов упрощает UX; внешние приложения сохраняют собственные модели пользователей.

## Альтернативы и ограничения

Собственные движки и полноценная desktop environment исключены ТЗ. Расширенные Steam, ES-DE, Jellyfin, download и TV/webOS providers относятся к последующим этапам.

## Разрешение вопросов

Объём media/Downloads закрыт [ADR-0023](0023-mvp-product-clarifications.md), [ADR-0024](0024-local-library-mpv.md) и [ADR-0025](0025-downloads-mvp.md).

### Вопросы исходного ревью (история)

Какой конкретный end-to-end сценарий локального/сетевого медиа будет проверяться в MVP? ТЗ требует полноценный сценарий, но конкретный источник библиотеки и плеер не выбирает.

## Проверка решения

AC-03, AC-07, AC-08, AC-13. Проверить отсутствие обязательного выбора профиля и наличие browser/Steam/ES-DE entry points.

[Реестр ADR](README.md) · [Карта ревью](../architecture/review-map.md)
