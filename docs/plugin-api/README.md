# Plugin API: текущий baseline

Plugin API v1 ещё не выпущен как stable. Архитектура/семантика выбраны; Rust SDK, D-Bus XML всех capabilities и contract suite создаются при реализации.

- [ADR-0020: transport, identity, state и операции](../adr/0020-dbus-contract-baseline.md).
- [Техническая основа и лимиты](../architecture/implementation-baseline.md).
- [Manifest, registration, supervision](../architecture/plugin-runtime.md).
- [Отдельный review media/download extensions](mvp-extensions.md).

Текущие capabilities: application, browser, media, library, action, download. Последняя добавлена явным расширением MVP по ответу пользователя, а не извлечена из исходного v0.3.

Перед реализацией каждой capability фиксируется versioned introspection XML с typed model и contract fixtures. Phase 0 требует только registry, application/demo snapshot, operations и state lifecycle; browser/media/download schemas не должны задерживать проверку KDE/Wayland.

Для выполненного spike сохранена фактическая [introspection XML](phase0.xml) интерфейса `org.arinn.Spike1`. Rust types — `crates/arinn-protocol`, Qt decoding проверяется интеграционным тестом. Имена и модель экспериментальные: singleton provider, сокращённый snapshot, временный KWin bridge. Это не замена production XML всех capabilities. При возврате структуры из zbus используется одноэлементный tuple `(Snapshot,)`, чтобы wire body содержал именно один struct, совпадающий с XML и QtDBus.
