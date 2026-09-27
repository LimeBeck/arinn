# Arinn
## Developer Specification v0.3 — Architecture Review Candidate

**Статус:** Draft / Architecture Review Candidate  
**Целевая платформа:** Linux HTPC, KDE Plasma 6 / Wayland  
**Основной UX:** TV-first, D-pad-first, с поддержкой аэромыши  
**Архитектурный принцип:** plugin-first + capability-based API

---

# 1. Что такое Arinn

**Arinn** — TV-first оболочка для Linux HTPC, объединяющая в одном интерфейсе:

- веб-сервисы и браузер;
- локальное и сетевое медиа;
- Steam;
- EmulationStation / ES-DE;
- внешние приложения;
- системные действия;
- состояние текущей активности.

Arinn не заменяет браузер, Steam, медиаплеер или эмуляторы.

Его задача — предоставить единый домашний интерфейс над существующей Linux-системой и приложениями, скрыв обычный desktop UX там, где он не нужен.

Ключевой сценарий использования — **Balanced HTPC**:

- браузер и онлайн-видео — основной сценарий;
- локальное медиа — полноценный сценарий;
- Steam и ES-DE — полноценные точки входа;
- обычные Linux-приложения могут подключаться без изменений самого Arinn.

На текущем реальном сценарии браузер занимает примерно **70% времени использования HTPC**, поэтому browser integration является частью MVP, а не второстепенной функцией.

---

# 2. Основные продуктовые принципы

## 2.1. TV-first

Интерфейс должен быть пригоден для использования с расстояния около 3 метров.

Основное управление:

- D-pad / стрелки;
- Enter / Select;
- Back;
- Home;
- дополнительные media buttons.

Аэромышь поддерживается, особенно для браузера, но не является обязательной для навигации по самому Arinn.

Любое основное действие внутри shell должно быть доступно без мыши.

---

## 2.2. Arinn — оболочка, а не отдельная экосистема

Нельзя создавать собственные аналоги:

- браузера;
- Steam;
- медиаплеера;
- EmulationStation;
- desktop environment.

Arinn должен оркестрировать существующие приложения.

---

## 2.3. External applications are first-class citizens

Запущенный Chrome, Steam, ES-DE или медиаплеер не является чем-то, из чего пользователь обязан «выйти обратно в launcher».

Arinn должен существовать **поверх активностей**.

Глобальный вызов Home открывает интерфейс Arinn поверх текущего приложения.

При закрытии Home пользователь возвращается туда же.

---

## 2.4. Plugin-first

Ядро Arinn не должно знать, что такое:

- Chrome;
- Steam;
- ES-DE;
- Jellyfin;
- mpv;
- Transmission;
- webOS;
- конкретный онлайн-кинотеатр.

Ядро знает только **capabilities**.

Например:

```text
ApplicationProvider
BrowserProvider
MediaProvider
LibraryProvider
ActionProvider
```

Chrome, Steam и остальные интеграции реализуются внешними plugin-процессами.

---

# 3. Пользовательская модель

Arinn рассматривается как **shared household device**.

Устройство используется двумя людьми, но отдельные пользовательские профили внутри Arinn не требуются.

В v1:

- нет login screen;
- нет выбора пользователя;
- нет отдельных favorites;
- нет отдельных history;
- нет отдельных settings.

Используется единое домашнее пространство.

Поддержку отдельных профилей нельзя закладывать в UX так, чтобы она усложняла MVP.

---

# 4. Основные разделы интерфейса

Основная информационная архитектура:

```text
Home
Library
Downloads
Applications
Settings
```

При этом Home является главным экраном и главным overlay.

---

# 5. Home

Home должен отвечать на вопрос:

> Что я хочу делать сейчас?

а не:

> Какое приложение мне нужно запустить?

Основные блоки Home:

## 5.1. Continue

Объединённая секция:

**Продолжить**

Она может содержать:

- незавершённый фильм;
- сериал;
- локальное видео;
- текущую browser activity;
- активную игру;
- другое resumable content.

Если прямо сейчас существует активная Activity, она также отображается здесь.

Не нужно создавать отдельные конкурирующие концепции вроде:

- «Сейчас запущено»;
- «Продолжить просмотр»;
- «Последние приложения».

На Home это единая модель продолжения активности.

---

## 5.2. Favorites / Online

Быстрые точки входа.

Например:

```text
Browser
Кинопоиск
IVI
YouTube
Steam
EmulationStation
```

Кинопоиск, IVI и аналогичные сервисы могут технически открываться через Chrome, но для пользователя должны существовать как отдельные плитки.

Пользователь не должен сначала выбирать Browser → Bookmark → Кинопоиск.

---

## 5.3. New

Контент, который соответствующие providers считают новым или рекомендуемым.

Arinn не обязан самостоятельно строить recommendation engine.

Он отображает данные providers.

---

# 6. Global Home Overlay

Home должен быть доступен независимо от того, какое приложение сейчас активно.

Например:

```text
Chrome
    ↓ Home

Arinn overlay

    ↓ Back

Chrome
```

То же самое должно работать для:

- Steam;
- игры;
- ES-DE;
- медиаплеера.

Открытие Home:

- не завершает приложение;
- не ставит приложение принудительно на паузу, если этого не требует provider;
- не уничтожает его состояние.

Shell должен появляться визуально поверх Activity.

---

# 7. Global system UI

Системные функции не должны быть разбросаны между отдельными приложениями.

Arinn предоставляет единый system overlay для действий вроде:

- Home;
- Back;
- Play/Pause;
- volume;
- mute;
- power menu;
- Settings.

Конкретное действие выполняется либо platform layer, либо соответствующим `ActionProvider`.

CEC не является обязательной частью архитектуры Arinn.

Управление телевизором или, например, интеграция с webOS в дальнейшем должны подключаться тем же plugin-механизмом, не попадая в core.

---

# 8. Browser

Browser является одним из важнейших компонентов Arinn.

Arinn **не реализует собственный WebView/browser engine**.

Используется обычный Chrome/Chromium.

## MVP browser integration должна позволять:

- запустить Chrome;
- открыть URL;
- получить список окон/вкладок;
- определить активную вкладку;
- переключиться на вкладку;
- создать вкладку;
- закрыть вкладку;
- показать title;
- показать URL;
- получить favicon при наличии;
- вернуться в существующую browser activity.

Таким образом вкладки браузера могут отображаться в UI Arinn без создания собственного браузера.

---

# 9. Chrome integration

Предлагаемая архитектура:

```text
Chrome
   │
   │ Extension API
   ▼
Arinn Chrome Extension
   │
   │ Native Messaging
   ▼
arinn-plugin-chrome
   │
   │ Arinn Plugin API
   ▼
arinnd
```

Chrome extension:

- Manifest V3;
- Native Messaging;
- минимально необходимые permissions.

В v1 не требуется:

- scraping страницы;
- DOM automation;
- произвольное выполнение JavaScript;
- анализ пользовательского содержимого страницы.

Plugin работает с browser-level сущностями:

- windows;
- tabs;
- URL;
- title;
- favicon;
- active state.

---

# 10. Applications

Arinn должен уметь работать с обычными Linux-приложениями.

Базовая интеграция должна позволять:

```text
discover
launch
activate
terminate
get running state
```

Источником приложений могут быть `.desktop` entries.

За это отвечает отдельный provider, например:

```text
arinn-plugin-desktop
```

Core не должен самостоятельно сканировать desktop-файлы.

---

# 11. Steam и ES-DE

Steam и EmulationStation должны быть доступны уже в MVP как отдельные точки входа.

На первом этапе они могут использовать generic `ApplicationProvider`.

То есть MVP достаточно уметь:

```text
Arinn
 → Steam
```

и:

```text
Arinn
 → ES-DE
```

без реализации собственного Steam library client.

Расширенная интеграция:

- библиотека игр;
- artwork;
- recent games;
- continue playing;
- metadata;

может быть реализована позже отдельными providers.

---

# 12. Media

Arinn должен уметь обнаруживать текущие media sessions.

Для Linux MVP основной механизм — MPRIS.

Например:

```text
arinn-plugin-mpris
```

Provider сообщает:

- title;
- subtitle / artist;
- artwork;
- playback state;
- duration;
- position;
- available actions.

И поддерживает:

```text
play
pause
playPause
seek
next
previous
stop
```

Shell не должен напрямую обращаться к MPRIS.

---

# 13. Downloads

Раздел Downloads сохраняется в информационной архитектуре Arinn.

Однако отдельный `DownloadProvider` **не вводится в Plugin API v1 заранее**.

Интеграция с Transmission и другими download managers должна пройти отдельный API review.

До этого раздел может существовать как зарезервированный UI route.

Это позволяет не замораживать плохо проработанную capability только ради одного приложения.

---

# 14. Архитектура

Высокоуровневая схема:

```text
┌─────────────────────────────┐
│         arinn-shell         │
│          Qt 6/QML           │
└──────────────┬──────────────┘
               │
               │ D-Bus
               ▼
┌─────────────────────────────┐
│            arinnd           │
│             Rust            │
│                             │
│  State / Routing / Registry │
│  Plugin Supervisor          │
│  Capability Aggregation     │
└──────────────┬──────────────┘
               │
               │ Arinn Plugin API
               ▼
       ┌───────┴────────┐
       │                │
       ▼                ▼

 Chrome plugin      MPRIS plugin
 Desktop plugin     Demo plugin
 Steam plugin       ...
```

---

# 15. arinn-shell

Технологии:

```text
Qt 6
QML
```

Shell отвечает исключительно за presentation layer.

В его обязанности входят:

- navigation;
- focus management;
- animations;
- rendering;
- overlays;
- screens;
- design system;
- interaction state.

Shell не должен:

- запускать Chrome самостоятельно;
- читать `.desktop`;
- обращаться к MPRIS;
- управлять Steam;
- выполнять shell commands;
- знать IPC сторонних приложений.

Все действия идут через `arinnd`.

---

# 16. arinnd

`arinnd` — минимальное ядро и orchestrator Arinn.

Предпочтительный язык:

```text
Rust
```

Основные обязанности:

- Plugin Registry;
- Capability Registry;
- plugin lifecycle;
- state aggregation;
- routing commands;
- subscriptions;
- snapshot/revision management;
- error normalization;
- platform abstraction;
- configuration;
- persistence небольшого количества собственного state.

Core должен оставаться относительно маленьким.

Правило:

> Если функциональность можно реализовать как provider без изменения core, она должна быть реализована как provider.

---

# 17. Plugins

Plugin — отдельный OS process.

Запрещается строить plugin architecture через загрузку `.so`/native libraries непосредственно в `arinnd`.

Причина:

- crash isolation;
- независимые зависимости;
- возможность писать plugins на разных языках;
- независимое обновление;
- стабильная IPC boundary;
- отсутствие ABI coupling с Rust.

Схема:

```text
arinnd
   │
   ├── arinn-plugin-desktop
   ├── arinn-plugin-chrome
   ├── arinn-plugin-mpris
   ├── arinn-plugin-steam
   └── ...
```

Падение одного plugin не должно приводить к падению:

- shell;
- daemon;
- других plugins.

---

# 18. Capability-based API

Plugin не объявляет:

```text
type = chrome
```

Вместо этого он объявляет capabilities:

```text
BrowserProvider
ApplicationProvider
```

Например:

```text
Chrome Plugin
 ├─ BrowserProvider
 └─ ApplicationProvider
```

Steam plugin потенциально:

```text
Steam Plugin
 ├─ ApplicationProvider
 └─ LibraryProvider
```

Core ничего не должен знать о семантике конкретного продукта.

---

# 19. Plugin API v1

Базовые capabilities:

```text
ApplicationProvider
BrowserProvider
MediaProvider
LibraryProvider
ActionProvider
```

Это минимальный публичный контракт первой версии.

---

# 20. ApplicationProvider

Пример концептуального API:

```text
listApplications()
getApplication(id)

launch(id, options)
activate(id)
terminate(id)

listRunning()
```

Application ID является opaque.

Core не должен разбирать его содержимое.

Например:

```text
application_id = "steam"
```

имеет смысл только внутри provider.

---

# 21. BrowserProvider

Минимальный API:

```text
getWindows()
getTabs()

open(url)
activateTab(tabId)
closeTab(tabId)
createTab(url)

getActiveTab()
```

Допустимые свойства Tab:

```text
id
windowId
title
url
favicon
active
audible
```

IDs opaque.

---

# 22. MediaProvider

Минимальный API:

```text
listSessions()
getSession(id)

play(id)
pause(id)
playPause(id)
seek(id, position)
next(id)
previous(id)
stop(id)
```

Media session содержит:

```text
id
title
subtitle
artwork
state
duration
position
capabilities
```

---

# 23. LibraryProvider

LibraryProvider предоставляет коллекцию content entities.

Это могут быть:

- фильмы;
- сериалы;
- игры;
- ROM;
- другие типы launchable content.

Core не должен иметь отдельной логики SteamLibrary/JellyfinLibrary.

Концептуально:

```text
listCollections()
listItems(collectionId)
getItem(itemId)
launch(itemId)
```

---

# 24. ActionProvider

Generic actions для интеграций, которые не являются библиотекой, браузером или media session.

Например:

- включение телевизора;
- переключение TV input;
- custom system action;
- plugin-specific command.

Action должен иметь machine-readable ID и presentation metadata.

Например:

```text
Action {
    id
    title
    icon
    enabled
}
```

---

# 25. Snapshot model

Для UI предпочтительнее модель:

```text
snapshot + revision
```

вместо большого количества mutable imperative events.

Пример:

```text
BrowserSnapshot {
    revision: 154,
    windows: [...]
}
```

При изменении состояния plugin публикует новую revision.

Это должно упростить:

- reconnect;
- restart plugin;
- restart shell;
- race conditions;
- synchronization.

---

# 26. Events

Plugin API должен поддерживать subscriptions.

Например:

```text
StateChanged(providerId, revision)
ProviderAvailable(providerId)
ProviderUnavailable(providerId)
ActivityChanged(...)
```

После получения `StateChanged` consumer может получить новый snapshot.

Не требуется передавать всё состояние непосредственно внутри event.

---

# 27. Async operations

Все операции, потенциально взаимодействующие с внешним приложением, считаются asynchronous.

Например:

```text
launch()
open()
activate()
play()
```

API не должен предполагать мгновенное выполнение.

Нужны:

- timeout;
- typed error;
- request ID;
- cancellation там, где это имеет смысл.

---

# 28. Error model

Ошибки должны нормализоваться core API.

Например:

```text
NotFound
Unavailable
Unsupported
PermissionDenied
Timeout
InvalidArgument
Conflict
Internal
```

Plugin-specific stack trace не должен попадать в QML API.

---

# 29. Extensibility

Дополнительные свойства объектов должны иметь механизм расширения без изменения контракта.

Для D-Bus допустимо использовать структуру вида:

```text
a{sv}
```

для optional metadata.

При этом основные поля должны оставаться typed.

Нельзя превращать весь API в untyped dictionary.

---

# 30. Versioning

Plugin API version:

```text
1
```

Plugin manifest должен указывать поддерживаемую версию API.

Например:

```text
api_version = 1
```

При несовместимой версии plugin не запускается как рабочий provider и отображается в diagnostics.

---

# 31. Plugin Manifest

Каждый plugin имеет manifest.

Пример:

```toml
id = "chrome"
name = "Chrome Integration"
version = "0.1.0"
api_version = 1

executable = "arinn-plugin-chrome"

capabilities = [
  "browser",
  "application"
]
```

Plugin ID должен быть стабильным.

---

# 32. Plugin discovery

Использовать XDG layout.

Предпочтительные директории:

```text
$XDG_DATA_HOME/arinn/plugins
$XDG_DATA_DIRS/arinn/plugins
```

System plugins и user plugins должны использовать один формат manifest.

---

# 33. Plugin lifecycle

`arinnd` отвечает за:

```text
discover
start
handshake
monitor
restart
stop
```

Supervision желательно реализовать через systemd user services / transient units.

Не следует самостоятельно превращать `arinnd` в полноценный process supervisor, если lifecycle уже может надёжно предоставить systemd.

После старта plugin обязан выполнить handshake и зарегистрировать capabilities.

---

# 34. Platform layer

Platform-specific функции должны находиться за отдельной abstraction boundary.

Первый target:

```text
KDE Plasma 6
Wayland
```

Не следует распределять KDE-specific код по всему daemon.

Пример:

```text
PlatformService
 ├─ global shortcuts
 ├─ application activation
 ├─ overlay/window integration
 ├─ session state
 └─ system actions
```

Реализация:

```text
KdePlatformService
```

В будущем теоретически может появиться другая реализация без изменения остальных компонентов.

---

# 35. Wayland

Необходимо учитывать ограничения Wayland:

- приложение не может произвольно управлять чужими окнами;
- focus stealing ограничен;
- global hotkeys compositor-dependent;
- overlay semantics compositor-dependent.

Поэтому управление окнами нельзя размазывать по plugins.

Для этого существует platform layer.

До реализации значительной части UI необходимо сделать отдельный integration spike:

```text
KDE Plasma 6 + Wayland

1. Global Home shortcut
2. Открытие Arinn поверх Chrome
3. Возвращение focus обратно в Chrome
4. То же самое для Steam/игры
```

Этот spike является архитектурным gate.

---

# 36. Global shortcuts

Home должен работать, даже если focus находится:

- в Chrome;
- Steam;
- игре;
- ES-DE.

Регистрацией global shortcut занимается platform layer.

Shell не должен полагаться на получение обычного keyboard event, когда он не находится в focus.

---

# 37. Activity model

Arinn вводит абстракцию **Activity**.

Activity — то, чем пользователь занимается прямо сейчас.

Например:

```text
Chrome tab
Steam game
ES-DE
movie playback
```

Activity не обязательно является Application.

Например:

```text
Chrome = application
YouTube tab = activity
```

Activity позволяет Home отображать meaningful «Продолжить», а не только список процессов.

---

# 38. State ownership

Каждый provider является источником истины для собственного состояния.

Например:

```text
Chrome plugin → browser tabs

MPRIS plugin → playback

Desktop plugin → installed applications
```

Core агрегирует состояние, но не пытается дублировать его в собственной базе данных.

---

# 39. Persistence

Arinn должен хранить только собственное состояние:

- favorites;
- Home layout;
- UI preferences;
- provider configuration;
- recent Arinn activities при необходимости.

Не следует копировать:

- Chrome history;
- Steam library database;
- media database;

если source provider может предоставить эти данные сам.

---

# 40. XDG filesystem layout

Пример:

```text
~/.config/arinn/
~/.local/share/arinn/
~/.local/state/arinn/
~/.cache/arinn/
```

Соответственно:

```text
config
plugin manifests/data
persistent state
cache
```

должны храниться раздельно.

---

# 41. CLI

Необходимо предоставить:

```text
arinnctl
```

Минимальные команды:

```text
arinnctl status
arinnctl providers
arinnctl capabilities
arinnctl activities
arinnctl diagnostics
```

Желательно:

```text
arinnctl provider restart <id>
```

CLI должен использовать тот же публичный daemon API, что и shell.

---

# 42. Diagnostics

Diagnostics должны позволять увидеть:

```text
arinnd status

plugin status

plugin version

API version

registered capabilities

last error

restart count
```

Ошибки интеграций не должны превращаться в ситуацию «плитка просто ничего не делает».

---

# 43. Logging

Все компоненты пишут structured logs.

Минимальные поля:

```text
component
provider_id
request_id
operation
duration
error
```

Основной runtime backend на Linux:

```text
journald
```

В обычных логах запрещено писать:

- cookies;
- authorization headers;
- passwords;
- browser page content.

---

# 44. Security model

MVP не позиционируется как hostile-plugin sandbox.

Plugins считаются локально установленным доверенным software.

При этом архитектура всё равно должна соблюдать:

- process isolation;
- IPC validation;
- command validation;
- bounded message size;
- timeout;
- отсутствие arbitrary shell execution из QML;
- отсутствие arbitrary code injection через Plugin API.

---

# 45. UI plugins запрещены

Plugin не должен предоставлять QML, HTML или другой произвольный UI для загрузки в shell.

Plugin предоставляет:

```text
data
metadata
actions
capabilities
```

UI рисует Arinn.

Это необходимо для:

- визуальной целостности;
- TV navigation;
- accessibility;
- безопасности;
- стабильности API.

---

# 46. Design direction

Базовое визуальное направление:

**Quiet Spatial OS**

Принцип:

> Calm OS → Cinematic Content

Shell должен быть визуально спокойным и давать контенту быть основным визуальным объектом.

Не требуется имитировать:

- Android TV;
- Apple TV;
- Steam Big Picture;
- desktop launcher.

Arinn должен иметь собственный визуальный язык.

---

# 47. Navigation

Focus является first-class state.

Любой focusable component обязан иметь явно различимые состояния:

```text
default
focused
pressed
disabled
loading
```

Пользователь с расстояния около трёх метров должен однозначно видеть текущий focus.

Нельзя полагаться только на:

- небольшое изменение цвета;
- mouse hover;
- тонкую рамку.

---

# 48. Back semantics

`Back` должен вести себя предсказуемо.

Приоритет:

```text
modal
   ↓
overlay
   ↓
nested screen
   ↓
Home
   ↓
active Activity
```

Не следует закрывать активное приложение только потому, что пользователь нажал Back внутри Home.

---

# 49. Non-goals v1

В первую версию намеренно не входят:

- собственный browser engine;
- собственный media engine;
- полноценный desktop environment;
- plugin marketplace;
- загрузка arbitrary QML plugins;
- remote plugin repository;
- automation engine;
- recommendation engine Arinn;
- multi-user accounts;
- cloud sync;
- Windows;
- macOS;
- собственная Steam library implementation;
- собственная Jellyfin implementation.

---

# 50. MVP providers

Минимальный набор:

```text
arinn-plugin-demo
arinn-plugin-desktop
arinn-plugin-chrome
arinn-plugin-mpris
```

Demo plugin обязателен.

Он нужен для:

- разработки UI без внешних приложений;
- contract tests;
- reproduction bugs;
- Plugin API documentation.

---

# 51. Steam / ES-DE в MVP

Несмотря на отсутствие специальных Steam/ES-DE providers, Home должен содержать отдельные точки входа:

```text
Steam
EmulationStation
```

Их запуск в MVP обеспечивается `ApplicationProvider`.

Таким образом ключевой Balanced HTPC UX присутствует с первой usable версии.

---

# 52. Предлагаемая структура repository

```text
arinn/
├── apps/
│   └── arinn-shell/
│
├── crates/
│   ├── arinnd/
│   ├── arinn-protocol/
│   ├── arinn-plugin-sdk/
│   └── arinn-platform/
│
├── plugins/
│   ├── demo/
│   ├── desktop/
│   ├── chrome/
│   └── mpris/
│
├── tools/
│   └── arinnctl/
│
├── chrome-extension/
│
├── packaging/
│
├── docs/
│   ├── architecture/
│   ├── plugin-api/
│   └── adr/
│
└── tests/
    ├── contract/
    └── integration/
```

Структура может корректироваться, но separation:

```text
shell
core
protocol
plugins
platform
```

должен сохраняться.

---

# 53. Testing

Минимальные уровни тестирования:

## Unit

Для:

- daemon;
- state aggregation;
- parsing;
- routing;
- plugin SDK.

## Plugin contract tests

Один набор contract tests должен запускаться против любого provider.

Например:

```text
provider registers
provider exposes valid IDs
snapshot revision increases
unknown ID returns NotFound
provider restart reconnects correctly
```

## Integration tests

```text
shell/core
core/plugin
plugin crash
plugin restart
provider disappear/reappear
```

## UI tests

Обязательно проверять D-pad navigation и focus graph.

---

# 54. Failure behaviour

Если Chrome plugin упал:

- Home продолжает работать;
- Steam продолжает запускаться;
- media control продолжает работать;
- Browser section отображается как unavailable.

Если `arinnd` временно недоступен:

- shell не падает;
- отображает degraded state;
- пытается reconnect.

Если provider долго отвечает:

- shell не блокирует UI thread;
- операция завершается timeout/error state.

---

# 55. Performance principles

UI interaction не должна ждать provider IPC.

Focus navigation должна быть полностью локальной для shell.

Provider data загружаются asynchronous.

Не допускается:

```text
keypress
 ↓
D-Bus request
 ↓
plugin
 ↓
response
 ↓
change focus
```

Focus/navigation не должны зависеть от backend latency.

---

# 56. Packaging

Первый официальный target:

```text
Arch Linux / EndeavourOS
KDE Plasma 6
Wayland
```

Однако архитектура не должна зависеть от pacman.

Бинарные компоненты:

```text
arinnd
arinn-shell
arinnctl
arinn-plugin-*
```

Chrome Native Messaging manifest устанавливается packaging layer.

---

# 57. Development phases

## Phase 0 — Architecture spike

Проверить:

- Qt 6/QML shell;
- Rust daemon;
- D-Bus IPC;
- KDE/Wayland global Home;
- overlay поверх Chrome;
- возврат focus;
- external plugin process.

Результат Phase 0 должен подтвердить жизнеспособность основной архитектуры.

---

## Phase 1 — Core skeleton

Реализовать:

```text
arinnd
arinn-shell
arinnctl
arinn-protocol
Plugin Registry
demo plugin
```

Shell получает все данные только через daemon.

---

## Phase 2 — Application Provider

Реализовать desktop application provider.

Получить работающий сценарий:

```text
Arinn
 → Applications
 → Chrome / Steam / ES-DE
 → Home
 → Arinn overlay
 → Back
 → приложение
```

---

## Phase 3 — Chrome

Реализовать:

- Chrome extension;
- Native Messaging host;
- BrowserProvider;
- tab list;
- switching;
- closing;
- opening URL;
- отдельные online-service tiles.

После этой стадии Arinn уже должен быть usable для основного ежедневного сценария.

---

## Phase 4 — Media

Реализовать MPRIS provider и global media UI.

Получить:

```text
current media
artwork
play/pause
seek
next/previous
```

---

## Phase 5 — Product MVP

Довести:

- Home;
- Continue;
- Favorites;
- Applications;
- Settings;
- activity handling;
- persistence;
- diagnostics;
- error UX;
- startup/autostart;
- packaging.

---

## Phase 6 — Rich integrations

После стабилизации Plugin API:

- Steam Provider;
- ES-DE Provider;
- Jellyfin Provider;
- Transmission/download provider;
- TV/webOS integration;
- другие plugins.

Каждая интеграция должна добавляться без изменения архитектуры core.

---

# 58. MVP acceptance criteria

MVP считается архитектурно успешным, если выполняются следующие сценарии.

### AC-01 — Startup

После входа в HTPC session Arinn запускается автоматически и показывает Home.

### AC-02 — Remote navigation

Основной интерфейс полностью управляется стрелками/D-pad, Select и Back.

### AC-03 — Browser

Пользователь может открыть Chrome/онлайн-сервис из Home.

### AC-04 — Browser tabs

Arinn показывает существующие Chrome tabs и позволяет переключаться между ними.

### AC-05 — Global Home

Из Chrome пользователь вызывает Home и получает Arinn поверх браузера.

### AC-06 — Resume

После закрытия Home пользователь возвращается в ту же browser activity.

### AC-07 — Steam

Steam запускается отдельной плиткой Home.

### AC-08 — ES-DE

EmulationStation запускается отдельной плиткой Home.

### AC-09 — Media

Активная MPRIS media session отображается в Arinn и управляется через него.

### AC-10 — Plugin isolation

Crash Chrome provider не приводит к crash shell или daemon.

### AC-11 — Dynamic provider

Provider можно установить/удалить без добавления provider-specific логики в `arinnd`.

### AC-12 — Capability abstraction

Shell не содержит условий вроде:

```text
if provider == "chrome"
if application == "steam"
```

для бизнес-логики capabilities.

### AC-13 — Shared household

Весь основной UX работает без выбора профиля или авторизации пользователя Arinn.

### AC-14 — 3-meter UX

Текущий focus и основные действия однозначно различимы при использовании интерфейса с телевизора.

---

# 59. Архитектурные правила

Следующие правила считаются обязательными.

### Rule 1

**Core знает capabilities, а не продукты.**

### Rule 2

**Интеграции являются внешними процессами.**

### Rule 3

**Shell не общается со сторонними приложениями напрямую.**

### Rule 4

**Plugins не поставляют произвольный UI.**

### Rule 5

**Platform-specific код изолирован.**

### Rule 6

**External Activity является нормальным состоянием Arinn, а не выходом из него.**

### Rule 7

**Browser integration является MVP functionality.**

### Rule 8

**Steam и ES-DE являются first-class entry points, даже до появления специализированных providers.**

### Rule 9

**Новая интеграция не должна требовать изменения `arinnd`, если существующие capabilities достаточны.**

### Rule 10

Если для новой интеграции существующих capabilities недостаточно, изменение Plugin API должно оформляться отдельно и быть применимо не только к одному конкретному продукту.

---

# 60. Критерий правильной архитектуры

Желаемый сценарий расширения:

```text
Разработчик решил добавить новый сервис
        ↓
пишет arinn-plugin-foo
        ↓
объявляет capabilities
        ↓
устанавливает manifest
        ↓
arinnd обнаруживает provider
        ↓
Arinn начинает использовать его
```

без изменений:

```text
arinnd
arinn-shell
```

Если для каждой новой интеграции приходится править core или добавлять product-specific код в QML, архитектурная цель Arinn не достигнута.

---

# 61. Итоговая модель

Arinn следует рассматривать не как launcher, media center или desktop shell в традиционном смысле.

Архитектурно это:

```text
TV UX
   +
Activity Orchestrator
   +
Capability Aggregator
   +
Plugin Platform
```

над существующим Linux HTPC.

Именно это разделение должно позволить Arinn постепенно наращивать поддержку браузеров, игр, media-сервисов, телевизоров и других приложений, не превращая daemon в монолит из специальных интеграций.