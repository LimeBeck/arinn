# Архитектурные решения Arinn

Основание — [Developer Specification v0.3](../specifications/developer-specification-v0.3.md), Draft / Architecture Review Candidate. Исходник сохранён без изменений. ADR группируют архитектурные решения по темам, а не заменяют все 61 раздел отдельными решениями.

ADR-0001–0019 сохраняют исходное предложение v0.3 (Proposed). **ADR-0020–0027 приняты для реализации (Accepted)**: технические решения выбраны по поручению пользователя, продуктовые уточнения — по его явным ответам. Принятые уточнения имеют приоритет в затронутых пунктах; ни один статус не означает завершённую разработку или пройденный hardware gate. Формулировки «предпочтительно», «желательно», примеры API и незакрытые вопросы исходного ТЗ не превращаются в утверждённые детали реализации.

Разделы «Последствия», «Альтернативы и ограничения», «Открытые вопросы» и «Проверка решения» содержат пояснения для ревью; они не свидетельствуют об уже проведённых экспериментах или обсуждениях альтернатив. Дополнительные вопросы явно отделены от решения. При расхождении с пересказом следует свериться с исходным ТЗ и оформить уточнение.

## Реестр

| ADR | Тема | Разделы ТЗ | Статус |
| --- | --- | --- | --- |
| [ADR-0001](0001-product-scope.md) | Arinn как оркестратор общего домашнего HTPC | 1, 2, 3, 13, 49, 50, 51, 56, 61 | Proposed |
| [ADR-0002](0002-component-boundaries.md) | Qt/QML shell, Rust daemon и публичный D-Bus API | 14, 15, 16, 41, 52, 56, 57, 59 | Proposed |
| [ADR-0003](0003-external-plugin-processes.md) | Изоляция интеграций во внешних процессах | 2, 17, 44, 54, 59, 60 | Proposed |
| [ADR-0004](0004-capability-contracts.md) | Пять capabilities и представление без plugin UI | 2, 18, 19, 20, 21, 22, 23, 24, 29, 45, 59, 60 | Proposed |
| [ADR-0005](0005-ipc-versioning.md) | Типизированный и версионируемый Plugin API | 14, 19, 20, 21, 22, 23, 24, 29, 30, 31, 44 | Proposed |
| [ADR-0006](0006-snapshots-subscriptions.md) | Состояние через snapshots, revisions и subscriptions | 16, 25, 26, 38, 53, 54 | Proposed |
| [ADR-0007](0007-async-errors.md) | Асинхронные команды и нормализованные ошибки | 27, 28, 44, 54, 55 | Proposed |
| [ADR-0008](0008-discovery-lifecycle.md) | XDG discovery и lifecycle через systemd user services | 30, 31, 32, 33, 40, 42, 56, 58, 60 | Proposed |
| [ADR-0009](0009-kde-wayland-platform.md) | PlatformService и обязательный KDE/Wayland spike | 6, 7, 34, 35, 36, 56, 57, 59 | Proposed |
| [ADR-0010](0010-activity-home.md) | Activity и единая модель Continue/Home overlay | 2, 4, 5, 6, 7, 37, 48, 51, 59, 61 | Proposed |
| [ADR-0011](0011-chrome-integration.md) | Chrome/Chromium через MV3 extension и Native Messaging | 1, 5, 8, 9, 21, 50, 56, 57, 59 | Proposed |
| [ADR-0012](0012-desktop-steam-esde.md) | Desktop provider и отдельные входы Steam/ES-DE | 10, 11, 20, 50, 51, 57, 59 | Proposed |
| [ADR-0013](0013-mpris-media.md) | MediaProvider на основе MPRIS | 7, 12, 22, 50, 57, 58 | Proposed |
| [ADR-0014](0014-state-persistence.md) | Provider-owned state и минимальное persistence Arinn | 3, 38, 39, 40 | Proposed |
| [ADR-0015](0015-security-boundary.md) | Доверенные локальные plugins с валидацией IPC | 9, 15, 17, 27, 43, 44, 45, 49 | Proposed |
| [ADR-0016](0016-cli-diagnostics.md) | Общий API для CLI, diagnostics и structured logs | 28, 30, 41, 42, 43, 54 | Proposed |
| [ADR-0017](0017-navigation-design-boundary.md) | Локальная D-pad navigation и отдельная дизайн-спецификация | 2, 4, 5, 15, 45, 46, 47, 48, 53, 55, 58 | Proposed |
| [ADR-0018](0018-demo-testing-gates.md) | Demo provider, общий contract suite и архитектурные gates | 35, 50, 53, 54, 55, 57, 58, 59, 60 | Proposed |
| [ADR-0019](0019-design-handoff-contract.md) | Дизайн-handoff как отдельный контракт представления | Design handoff + developer §15, 46–48, 55 | Proposed |
| [ADR-0020](0020-dbus-contract-baseline.md) | D-Bus, идентификаторы и надёжность Plugin API | Уточнения 2026-09-27 | Accepted |
| [ADR-0021](0021-plugin-runtime-ownership.md) | Transient systemd services и отдельный Chrome native bridge | Уточнения 2026-09-27 | Accepted |
| [ADR-0022](0022-state-and-interaction-baseline.md) | Владение Activity, persistence и interaction state | Уточнения 2026-09-27 | Accepted |
| [ADR-0023](0023-mvp-product-clarifications.md) | Back, системный сон и целевой HTPC | Уточнения 2026-09-27 | Accepted |
| [ADR-0024](0024-local-library-mpv.md) | Локальная библиотека и внешний mpv с rich media API | Уточнения 2026-09-27 | Accepted |
| [ADR-0025](0025-downloads-mvp.md) | DownloadProvider и Transmission в MVP | Уточнения 2026-09-27 | Accepted |
| [ADR-0026](0026-multiple-monitors.md) | Home на экране активной Activity | Два монитора, 2026-09-27 | Accepted |
| [ADR-0027](0027-activity-background-snapshot.md) | Last-frame Activity background и граница compositor/shell | Phase 0 experiment, 2026-09-27 | Accepted |

## Ревью и последующие изменения

Решение получает Accepted после явного архитектурного review либо самостоятельного технического выбора в пределах поручения пользователя; дата и основание фиксируются в ADR. Отклонённое решение получает Rejected; заменённое — Superseded со ссылкой на новый ADR. Это предложенный порядок ведения документации, а не утверждение о состоявшемся принятии.

Новые решения получают следующий свободный номер. Изменения архитектурной границы и Plugin API оформляются явно; конкретные wire schemas и SDK documentation появятся отдельно после API review.

[Карта покрытия ТЗ, приёмки и открытых вопросов](../architecture/review-map.md). Полученный [дизайн-handoff](../design/README.md) сохранён отдельно; его связь с архитектурой описана в ADR-0017 и ADR-0019, а [расхождения и их решения](../design/integration-review.md) имеют явный текущий статус.

Начинать реализацию следует с [технической основы](../architecture/implementation-baseline.md), [уточнённого MVP](../architecture/product-decisions.md) и [Phase 0 gate](../architecture/phase-0-gate.md).
