# Дизайн Arinn

Актуальный handoff предоставлен пользователем из `/home/lime/Downloads/design_handoff_arinn (1)/design_handoff_arinn/` и скопирован в `handoff/` без изменения содержимого файлов, включая скрытый файл с логотипами. Предыдущий комплект сохранён в [архиве](archive/2026-09-27-before-brand/README.md). Дата включения в репозиторий: 2026-09-27. Номер версии всему handoff не присваивался; «v2» — часть имени файла дизайн-системы.

## Исходные материалы

- [Бренд Arinn](brand/README.md): Brand Guide v0.1 и референс вариантов логотипа, предоставленные 2026-09-27; отличия от исходного handoff описаны рядом с материалами.
- [SVG знака A1 Classic](handoff/assets/arinn-mark.svg) и [исходный лист логотипов](handoff/assets/logo-direction-A.png).
- [Design handoff](handoff/README.md): спецификация внешнего вида, поведения, токены и integration points.
- [Интерактивный прототип](handoff/design/arinn.dc.html).
- [Все экраны](<handoff/design/arinn Screens.dc.html>).
- [Design System v2](<handoff/design/arinn Design System v2.dc.html>).
- [Исследование направлений](<handoff/design/arinn Directions.dc.html>).
- [22 скриншота состояний](handoff/screenshots/).
- [Логотипы в image slots](handoff/design/.image-slots.state.json); support.js и image-slot.js сохранены рядом с HTML.

Для просмотра прототипа открыть `handoff/design/arinn.dc.html` в браузере и кликнуть по нему для keyboard focus, как описано в handoff. HTML служит design reference и не является production shell. Внешние font/runtime dependencies могут требовать сеть; offline-работа всего прототипа здесь не подтверждена.

Handoff объявляет цвета, типографику, размеры, радиусы, focus и motion timings финальными. Artwork остаётся placeholder; рекомендации по production assets сохранены в исходнике.

## Связь с разработкой

[Developer Specification v0.3](../specifications/developer-specification-v0.3.md) задаёт архитектуру и MVP. [ADR-0019](../adr/0019-design-handoff-contract.md) связывает визуальную спецификацию с Qt/QML presentation layer. [Карта согласования](integration-review.md) содержит историю расхождений, принятые решения и оставшиеся runtime gates.

Исходные design files не редактируются при обсуждении решений. Изменения требований должны быть явно оформлены, а не спрятаны в копии прототипа.
