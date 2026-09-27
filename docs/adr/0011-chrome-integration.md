# ADR-0011: Chrome/Chromium через MV3 extension и Native Messaging

- Статус: Предложено (Proposed), требуется архитектурное ревью.
- Дата оформления: 2026-09-27.
- Основание: [Developer Specification v0.3](../specifications/developer-specification-v0.3.md), разделы 1, 5, 8, 9, 21, 50, 56, 57, 59.

> Обновление 2026-09-27: Два процесса Chrome bridge/provider и reconnect определены [ADR-0021](0021-plugin-runtime-ownership.md). Этот ADR сохраняет формулировку исходного v0.3; при расхождении действуют принятые уточнения ADR-0020–0025.

## Контекст

Browser workflow — основной ежедневный сценарий MVP. Управление вкладками необходимо без создания browser engine внутри shell.

## Предлагаемое решение

Использовать обычный Chrome/Chromium. Предлагаемая цепочка: Chrome Extension API → Arinn Chrome Extension (Manifest V3) → Native Messaging → arinn-plugin-chrome → Plugin API → arinnd → shell.

Extension запрашивает минимально необходимые permissions. Интеграция работает с windows, tabs, URL, title, favicon и active state, без необходимости scraping, DOM automation, произвольного JavaScript и анализа содержимого страниц в v1.

MVP позволяет запустить Chrome, открыть URL, получить окна/вкладки и активную вкладку, переключить/создать/закрыть вкладку, показать title/URL/favicon при наличии и вернуться в существующую browser activity. Tab может содержать id, windowId, title, url, favicon, active, audible; IDs opaque.

Онлайн-сервисы представлены отдельными плитками Home с открытием URL. BrowserProvider отвечает за browser-level сущности, ApplicationProvider — за запуск приложения. Native Messaging manifest устанавливает packaging.

## Последствия

Arinn получает browser integration через существующий браузер. Extension и native host имеют отдельные lifecycle, которые требуется согласовать с daemon supervision и reconnect.

## Альтернативы и ограничения

Собственный WebView/browser engine исключён. CDP и DOM automation не выбраны архитектурой v0.3.

## Разрешение вопросов

Два процесса Chrome bridge/provider и reconnect определены [ADR-0021](0021-plugin-runtime-ownership.md).

### Вопросы исходного ревью (история)

Кто запускает Native Messaging host и как он связан с контролируемым systemd plugin process? Поведение при закрытом браузере, нескольких browser instances/profiles, disconnect extension; разрешения extension и schema сообщений.

## Проверка решения

AC-03, AC-04, AC-05, AC-06. Проверить tab operations, direct online-service tiles и восстановление browser activity. Phase 3 должен дать usable ежедневный browser workflow.

[Реестр ADR](README.md) · [Карта ревью](../architecture/review-map.md)
