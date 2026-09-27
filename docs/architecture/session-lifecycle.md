# Блокировка, сон и завершение графической сессии

Shell отслеживает `org.freedesktop.ScreenSaver.ActiveChanged` на session bus и `org.freedesktop.login1.Manager.PrepareForSleep` на system bus. При lock или подготовке сна окно скрывается независимо от состояния daemon. Activity context сохраняется. После wake shell сначала повторно проверяет lock state; старый ответ GetActive не может отменить более новый сигнал. При недоступном ScreenSaver состояние считается заблокированным до подтверждения обратного.

Transient units Phase 0 имеют `PartOf=graphical-session.target` и `After=graphical-session.target`: остановка графической сессии распространяется на daemon/demo/shell. Остановка всего desktop target в тестах не выполнялась.

## Проверено

Private-bus Qt Test использует настоящие D-Bus subscriptions к поддельным ScreenSaver/login1 services: lock/unlock, sleep/wake, unlock во время sleep, исчезновение и возвращение владельца ScreenSaver. Полный IPC/input suite проходит. На разблокированной KDE-сессии проверены `blocked=false`, 20 циклов Chrome Home/Return и фактические PartOf/After properties всех трёх units. Это проверка реакции на события, а не утверждение о физическом suspend или compositor lockscreen.

## Отложено по просьбе пользователя

2026-09-27 пользователь выбрал «Оставить реальные проверки на потом». Реальная блокировка, suspend/wake и logout не выполнялись. Эти acceptance-пункты остаются открытыми; это не отсутствие продуктового решения.

Для согласованной оператором lock/unlock проверки подготовлен `tests/integration/kwin_lock.py`: сначала открыть Home поверх disposable Activity, затем запустить тест. Он **блокирует настоящий экран** и ожидает ручной unlock до 180 s. Тест проверяет property `org.arinn.SpikeSession1.blocked` и сохранение return target. Визуальную проверку отсутствия shell поверх lockscreen выполняет оператор. Suspend проверяется отдельно через штатное меню KDE, без обхода inhibitors; автоматического suspend или logout в test suite нет.

Полноценный MVP power action ещё требует отображения отказов/inhibitors и проверки восстановления providers после wake. SessionGuard не реализует саму команду suspend.
