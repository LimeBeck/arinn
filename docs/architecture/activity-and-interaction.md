# Activity, interaction state и согласование дизайна

Статус: технические правила приняты по ADR-0022; ответы P-01–P-08 зафиксированы в product-decisions.md. Дата: 2026-09-27.

## Activity и Continue

Activity descriptor: opaque `id`, `owner EntityRef`, `generation`, `kind` (application/browser/media/content), `title`, `artwork_ref`, `status` (active/paused/resumable/unavailable), `available_actions`, optional `parent_ref`, `related_refs`, `platform_window_token`, `last_interaction_at`. Core не анализирует URL/title для связывания сущностей.

Provider владеет внешней сущностью; daemon строит агрегированную Activity из descriptors. Desktop даёт fallback application activity, browser — tab activity, media — playback. Только явная связь IDs/validated platform window token позволяет объединить их в одну группу. Нельзя склеивать два окна по одинаковому title или две media sessions по названию фильма. Если связь неизвестна, лучше сохранить отдельные сущности, чем возобновить неверную.

Foreground Activity определяется platform window плюс активная entity provider внутри него; display precedence внутри доказанной группы: явный content playback → browser tab → generic application. Media controls добавляются к карточке через related media ref. Начальная UI projection: одна foreground/последняя running Activity + до четырёх resumable content entries, как в дизайне. Ограничение карточек не ограничивает registry. Остальные окна/вкладки доступны через Applications/Browser.

Arinn хранит порядок своих recent launch targets, но provider хранит media progress/watched и library state. Правило дизайна «90 s до конца» исполняет provider, если выбранная media integration поддерживает такую policy; оно не помечает произвольное browser video просмотренным. Поведение выбранной библиотеки описано в [media MVP](media-mvp.md).

Activity исчезла, пока открыт Home: удалить live карточку и перевести focus на ближайший доступный item в той же строке, затем Favorites, затем nav. Return в исчезнувшую entity возвращает Unavailable и оставляет Home. Без Activity Home не пытается активировать desktop случайным способом.

## Локальный UI и pending operations

Focus scopes, input modes, per-screen focus memory, OSD visibility и timeline preview принадлежат shell. Provider state никогда не является условием перехода focus. После удаления item focus выбирается по ближайшему прежнему индексу; при закрытии modal/sheet возвращается к opener, а если он исчез — к ближайшему доступному focusable в родительском scope.

Launch 1100 ms — длительность визуального цикла, а не гарантированное завершение. Если operation ещё pending, продолжать спокойный loading state; если target Activity уже доступна, переходить к ней. Ошибка немедленно открывает error state. Startup 1.9 s не удерживает UI от degraded Home при недоступном backend. Никаких фиктивных success по таймеру.

При скрытом player OSD: Left/Right выполняет ±10 s только при поддержке seek и открывает OSD на timeline; OK/Up/Down только открывают OSD. При видимом timeline OK = playPause. Back сначала скрывает OSD; следующий Back покидает playback view в managed mpv session со stop/save resume и возвратом на исходный Home/Detail, не завершает произвольное внешнее приложение. Global Home остаётся platform action.

H/M/P/A/O — development shortcuts прототипа, не обязательные global keys. Production mapping использует semantic Home/Back/Menu/Select/media actions и настраивается под устройство. Буква H в браузере не перехватывается глобально. Первый D-pad input переключает pointer mode; hover suppression 500 ms после scroll сохраняется.

Уточнение пользователя: G60S Pro отправляет стандартные клавиатурные/мышиные события, включая Backspace и Esc. Специальный драйвер, SDK и отдельный протокол пульта не нужны. В shell Esc и Backspace означают Back; стрелки — navigation, Enter — Select. В редактируемом текстовом поле Backspace удаляет символ и не запускает Back; Esc закрывает верхний scope. Вне shell локальные клавиши принадлежат активному приложению. Global Home остаётся отдельным настраиваемым shortcut (в spike Meta+F12); глобальный перехват Esc/Backspace запрещён. Проверка стандартной клавиатурой достаточна для input gate Phase 0; физический пульт — дополнительный smoke test удобства при развёртывании.

## Settings, system actions и диагностика

UI preferences (light background, show New, language, input mapping) принадлежат daemon config. Настройки provider имеют typed schema: id/title/description/type/current_value/constraints/enabled; types boolean, enum, string, integer, path reference. Произвольные QML widgets или raw RPC не передаются. Реализация schema и wire interface предшествует экрану provider settings, не Phase 0.

Core settings UI отображает только зарегистрированные возможности; отсутствующая playback/download/network функция не имитирует успех. System volume/network/power выполняются PlatformService. Running app names для confirm берутся из live registry, не из hardcoded Kinopoisk. Кнопка подтверждения не пропускает системные inhibit/permission errors.

## Что закрыто без дополнительных вопросов

- Три равноправные точки входа в дизайне не меняют явно заданный browser-first порядок разработки.
- New — generic projection LibraryProvider, не product-specific local-file branch. Provider может не поставлять New; пустая секция скрыта.
- JS state прототипа не становится backend shell.
- Provider errors, timeouts, OSD и focus после обновления списков имеют явные правила выше.
- Idle ambient никогда автоматически не останавливает приложения. Автовход подавляется при играющем media и foreground external Activity; явный ambient доступен пользователю. «Сон» означает OS suspend по явному ответу пользователя; ambient — отдельная функция.

## Ответы пользователя

Все P-01–P-08 получены и зафиксированы в [уточнениях MVP](product-decisions.md). Back возвращает в Activity, media — folders/library + внешний player, Transmission входит в MVP, Sleep — suspend, целевой HTPC с одним экраном и G60S Pro со стандартным keyboard/mouse mapping. Зависимые media/API решения описаны в [media MVP](media-mvp.md) и [API review](../plugin-api/mvp-extensions.md).
