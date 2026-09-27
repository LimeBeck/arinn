# ADR-0019: Дизайн-handoff как отдельный контракт представления

- Статус: Предложено (Proposed), требуется архитектурное ревью.
- Дата оформления: 2026-09-27.
- Основание: [Design handoff](../design/handoff/README.md), разделы Fidelity, Design Tokens, Input model, Screens / Views, State Management, Assets; [Developer Specification v0.3](../specifications/developer-specification-v0.3.md), §15, 46–48, 55; уточнение пользователя о предоставлении отдельного дизайна.

> Обновление 2026-09-27: Продуктовые расхождения закрыты [ADR-0023](0023-mvp-product-clarifications.md) и [API review](../plugin-api/mvp-extensions.md); compositor/OSD эффекты остаются проверяемыми gates, не вопросами предпочтения. Этот ADR сохраняет формулировку исходного v0.3; при расхождении действуют принятые уточнения ADR-0020–0025.

## Контекст

Получен самостоятельный комплект дизайна: спецификация, HTML-прототип, экраны, Design System v2, исследование направлений и 22 скриншота. Он уточняет Quiet Spatial OS направлением 2a «Глубина со светом». Визуальные параметры объявлены финальными, но prototype integration points местами выходят за Plugin API v1 и MVP developer ТЗ.

## Предлагаемое решение

Хранить исходный комплект без изменений в docs/design/handoff и ссылаться на него как на источник визуальных параметров. Реализовывать presentation на Qt 6/QML по ADR-0002. Сам handoff требует воссоздать дизайн в целевой среде, следовать выбранному стеку и не поставлять HTML как приложение; упоминание Electron/Tauri не заменяет выбранную архитектуру.

Соблюдать заданный визуальный контракт:

- Логический canvas 1920×1080, uniform scale `min(vw/1920, vh/1080)`, safe area и размеры из handoff.
- Onest и Material Symbols Rounded; минимальные размеры 24 для текста и 26 для зависимого от него действия; tabular numerals и запас для русских подписей.
- Ember означает статус/прогресс/бренд (обновлённый handoff от 2026-09-27; ранее Amber), focus обозначается светом, масштабом и кольцом поверх artwork. Остальные токены, radii, elevation и motion берутся из исходника.
- Light field зависит от artwork hue, плавно меняется за 900 ms и отключается настройкой; на системных экранах отсутствует.
- D-pad/pointer modes, spatial navigation, topmost focus scopes `dialog > power > sheet > nav > screen`, память focus и 500 ms suppression hover после scroll.
- Overlay, loading, empty, error, disabled, pressed и focus states проверяются по дизайн-системе и экранным примерам. Placeholder artwork не считается готовым контентом.

Модель JavaScript-прототипа описывает демонстрацию поведения. Provider-owned библиотека, downloads и playback не переносятся в shell как собственный backend. Shell сохраняет interaction state, а данные и команды получает через daemon. Локальный preview таймлайна возможен как interaction state, но фактическая позиция и playback принадлежат provider.

Конфликтующие требования и новые capabilities фиксируются в [карте согласования](../design/integration-review.md). Наличие экрана не считается автоматическим расширением MVP или готовым контрактом API.

## Последствия

Разработчикам доступны точные визуальные ориентиры и полный reference bundle. Не требуется заново придумывать дизайн-систему. Динамические API, compositor effects и спорные переходы должны быть согласованы до соответствующей реализации; HTML mockup не доказывает их работоспособность на Wayland.

## Альтернативы и ограничения

Поставка HTML как shell противоречит handoff. Замена Qt/QML стеком прототипа и перенос simulated integrations в QML не следуют из предоставления дизайна. Разрешение противоречий молчаливым приоритетом одного исходника скрывает изменение требований.

## Разрешение вопросов

Продуктовые расхождения закрыты [ADR-0023](0023-mvp-product-clarifications.md) и [API review](../plugin-api/mvp-extensions.md); compositor/OSD эффекты остаются проверяемыми gates, не вопросами предпочтения.

### Вопросы исходного ревью (история)

Back на Home; Downloads/MVP; library scanner и player controls сверх MPRIS; source of truth для resume; compositor texture/snapshot и допустимая деградация эффектов; Sleep против ambient; prototype timers против фактического завершения операции. Детали перечислены в карте согласования.

## Проверка решения

Сопоставить реализацию с Design System v2 и 22 screenshot states при logical 1920×1080 и целевом 4K scale. Проверить D-pad/pointer, scope trapping, возврат focus, 3-meter readability и поведение при delayed/unavailable providers. Реальные overlay/compositor возможности подтвердить Phase 0. Эти runtime/UI проверки пока не выполнялись.

[Реестр ADR](README.md) · [Дизайн](../design/README.md)
