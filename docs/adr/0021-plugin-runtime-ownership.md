# ADR-0021: Transient systemd services и отдельный Chrome native bridge

- Статус: Принято для реализации (Accepted).
- Дата: 2026-09-27.
- Основание: пользователь поручил самостоятельно закрыть технические решения; это решение исполнителя в пределах делегированной задачи, не утверждение о состоявшемся отдельном ревью пользователя.
- Уточняет: ADR-0003, ADR-0008, ADR-0011, ADR-0012; исходные ADR сохраняют историю и ссылку на это уточнение.

## Контекст

Native Messaging host запускается Chrome, тогда как обычный plugin должен обслуживаться systemd. Discovery и restart нуждаются в детерминированных правилах.

## Решение

Принять TOML manifests и XDG priority, explicit reload, transient user services с bounded restart, отдельные runtime/backend statuses. Chrome host является adapter process браузера и подключается к systemd-supervised Chrome provider, не конкурируя с ним за lifecycle.

Подробный контракт: [спецификация](../architecture/plugin-runtime.md). См. также [техническую основу](../architecture/implementation-baseline.md) и [Phase 0 gate](../architecture/phase-0-gate.md).

## Последствия

Plugin install не требует отдельного handwritten unit для каждой интеграции. Crash/reload не должен закрывать пользовательские приложения; cgroup separation проверяется интеграционно.

## Альтернативы

Не использовать daemon как второй supervisor и не запускать один и тот же Native Messaging host одновременно Chrome и systemd.

## Оставшаяся работа и проверка

Runtime сценарии systemd/Chrome предстоит реализовать и проверить. Продуктовых подтверждений по lifecycle не требуется.

Принятие технического выбора не означает прохождение тестов, готовность wire API всех capabilities или принятие противоречащих друг другу продуктовых требований.

[Реестр ADR](README.md)
