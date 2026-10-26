// SPDX-FileCopyrightText: 2026 Arinn contributors
//
// SPDX-License-Identifier: GPL-3.0-or-later

#include "settings_catalog.h"
#include <QDeadlineTimer>
#include <QInputDevice>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QJsonValue>
#include <QUuid>
#include <QtDBus>
#include <QtQml>
#include <algorithm>
#include <cmath>
#include <iterator>
#include <limits>
namespace {
constexpr auto service = "org.arinn.Daemon1";
constexpr auto path = "/org/arinn/Daemon1";
constexpr auto settingsInterface = "org.arinn.Settings1";
constexpr auto libraryInterface = "org.arinn.Library1";
// Daemon1.Providers page and Library1.GetRoots reply (ADR-0071).
constexpr auto providersPage = "(sa(sssssoasssststttt)s)";
constexpr auto rootsReply = "(sta(sb))";
constexpr auto rootsFeature = "LibraryRoots1";
// Apply waits 5 s (ADR-0065). Leave room for the daemon to finish the
// operation and for one extra Get.
constexpr int operationWaitMs = 10000;
constexpr int fallbackPollMs = 30000;
constexpr int callMs = 3000;
struct CategoryDef {
  const char *id;
  const char *title;
  const char *icon;
};
constexpr CategoryDef kCategories[] = {
    {"media", QT_TRANSLATE_NOOP("SettingsCatalog", "Media"), "video_library"},
    {"playback", QT_TRANSLATE_NOOP("SettingsCatalog", "Playback"), "play_circle"},
    {"sound", QT_TRANSLATE_NOOP("SettingsCatalog", "Sound"), "volume_up"},
    {"network", QT_TRANSLATE_NOOP("SettingsCatalog", "Network"), "wifi"},
    {"applications", QT_TRANSLATE_NOOP("SettingsCatalog", "Applications"), "apps"},
    {"appearance", QT_TRANSLATE_NOOP("SettingsCatalog", "Appearance"), "palette"},
    {"system", QT_TRANSLATE_NOOP("SettingsCatalog", "System"), "memory"},
};
QDBusMessage settingsCall(const char *method) {
  return QDBusMessage::createMethodCall(service, path, settingsInterface,
                                        method);
}
QString errorCode(const QDBusMessage &reply) {
  auto name = reply.errorName();
  const auto dot = name.lastIndexOf(QLatin1Char('.'));
  if (dot >= 0)
    name = name.mid(dot + 1);
  return name.isEmpty() ? QStringLiteral("Unavailable") : name;
}
bool serviceMissing(const QDBusMessage &reply) {
  const auto name = reply.errorName();
  return name.contains(QLatin1String("ServiceUnknown")) ||
         name.contains(QLatin1String("NameHasNoOwner"));
}
QJsonValue parseJson(const QString &text, bool *ok) {
  QJsonParseError error;
  const auto wrapped =
      QJsonDocument::fromJson((QLatin1Char('[') + text + QLatin1Char(']')).toUtf8(),
                              &error);
  if (error.error != QJsonParseError::NoError || !wrapped.isArray() ||
      wrapped.array().isEmpty()) {
    *ok = false;
    return {};
  }
  *ok = true;
  return wrapped.array().at(0);
}
QString jsonString(const QString &text) {
  QJsonArray wrap;
  wrap.append(text);
  const auto encoded =
      QString::fromUtf8(QJsonDocument(wrap).toJson(QJsonDocument::Compact));
  return encoded.mid(1, encoded.size() - 2);
}
bool controlCharacter(const QString &text) {
  for (const auto &ch : text) {
    const auto code = ch.unicode();
    if (code < 0x20 || code == 0x7f)
      return true;
  }
  return false;
}
qsizetype scalarCount(const QString &text) { return text.toUcs4().size(); }
QVariant valueFor(const QString &kind, const QString &json, bool multiple) {
  bool ok = false;
  const auto parsed = parseJson(json, &ok);
  if (!ok)
    return {};
  if (kind == QLatin1String("boolean"))
    return parsed.isBool() ? QVariant(parsed.toBool()) : QVariant();
  if (kind == QLatin1String("integer")) {
    if (!parsed.isDouble())
      return {};
    const auto number = parsed.toDouble();
    if (!std::isfinite(number) || number != std::trunc(number))
      return {};
    if (number < static_cast<double>(std::numeric_limits<qlonglong>::min()) ||
        number > static_cast<double>(std::numeric_limits<qlonglong>::max()))
      return {};
    return QVariant::fromValue(static_cast<qlonglong>(number));
  }
  if (kind == QLatin1String("path") && multiple) {
    if (!parsed.isArray())
      return {};
    QStringList paths;
    for (const auto &item : parsed.toArray()) {
      if (!item.isString())
        return {};
      paths.append(item.toString());
    }
    return paths;
  }
  if (kind == QLatin1String("string") || kind == QLatin1String("enum") ||
      kind == QLatin1String("path"))
    return parsed.isString() ? QVariant(parsed.toString()) : QVariant();
  return {};
}
QVariantMap constraintsFor(const QString &kind, const QString &json) {
  QVariantMap map;
  if (json.isEmpty())
    return map;
  const auto doc = QJsonDocument::fromJson(json.toUtf8());
  if (!doc.isObject())
    return map;
  const auto object = doc.object();
  if (kind == QLatin1String("integer")) {
    if (object.value(QStringLiteral("min")).isDouble())
      map.insert(QStringLiteral("min"), object.value(QStringLiteral("min")).toInteger());
    if (object.value(QStringLiteral("max")).isDouble())
      map.insert(QStringLiteral("max"), object.value(QStringLiteral("max")).toInteger());
    // `unit` and `step` are display hints. The contract task adds `unit`;
    // until then a missing key leaves the unit blank. `step` defaults to 1.
    const auto unit = object.value(QStringLiteral("unit"));
    if (unit.isString()) {
      const auto text = unit.toString();
      if (!text.isEmpty() && text.toUtf8().size() <= 16 && !controlCharacter(text))
        map.insert(QStringLiteral("unit"), text);
    }
    const auto stepValue = object.value(QStringLiteral("step"));
    if (stepValue.isDouble()) {
      const auto number = stepValue.toDouble();
      if (std::isfinite(number) && number >= 1.0 && number == std::trunc(number) &&
          number <= static_cast<double>(std::numeric_limits<int>::max()))
        map.insert(QStringLiteral("step"), static_cast<int>(number));
    }
  } else if (kind == QLatin1String("string")) {
    if (object.value(QStringLiteral("max_length")).isDouble())
      map.insert(QStringLiteral("maxLength"),
                 object.value(QStringLiteral("max_length")).toInteger());
  } else if (kind == QLatin1String("enum")) {
    QVariantList options;
    for (const auto &item : object.value(QStringLiteral("options")).toArray()) {
      if (!item.isObject())
        continue;
      const auto option = item.toObject();
      options.append(QVariantMap{{QStringLiteral("value"),
                                  option.value(QStringLiteral("value")).toString()},
                                 {QStringLiteral("title"),
                                  option.value(QStringLiteral("title")).toString()}});
    }
    map.insert(QStringLiteral("options"), options);
  } else if (kind == QLatin1String("path")) {
    map.insert(QStringLiteral("directory"),
               object.value(QStringLiteral("directory")).toBool(false));
    map.insert(QStringLiteral("multiple"),
               object.value(QStringLiteral("multiple")).toBool(false));
    if (object.value(QStringLiteral("max_items")).isDouble())
      map.insert(QStringLiteral("maxItems"),
                 object.value(QStringLiteral("max_items")).toInteger());
  }
  return map;
}
// The daemon's own settings (provider `arinn`, ADR-0065, ADR-0112) send
// English source texts; the shell shows them translated. Provider texts are
// shown as the provider sends them.
constexpr auto kDaemonProvider = "arinn";
[[maybe_unused]] constexpr const char *kDaemonTexts[] = {
    QT_TRANSLATE_NOOP("SettingsCatalog", "Browser application"),
    QT_TRANSLATE_NOOP("SettingsCatalog",
                      "Application ID launched when a web shortcut opens with "
                      "no browser running; empty uses the packaged choice"),
    QT_TRANSLATE_NOOP("SettingsCatalog", "Search engine"),
    QT_TRANSLATE_NOOP("SettingsCatalog", "Used when typed text is not an address"),
    QT_TRANSLATE_NOOP("SettingsCatalog", "Light field"),
    QT_TRANSLATE_NOOP("SettingsCatalog",
                      "Soft light from the focused item's artwork behind Home"),
    QT_TRANSLATE_NOOP("SettingsCatalog", "Ambient after"),
    QT_TRANSLATE_NOOP("SettingsCatalog", "Idle time on an Arinn screen before "
                                         "the ambient screen appears"),
    QT_TRANSLATE_NOOP("SettingsCatalog", "5 min"),
    QT_TRANSLATE_NOOP("SettingsCatalog", "10 min"),
    QT_TRANSLATE_NOOP("SettingsCatalog", "15 min"),
    QT_TRANSLATE_NOOP("SettingsCatalog", "30 min"),
    QT_TRANSLATE_NOOP("SettingsCatalog", "60 min"),
};
QString daemonText(const QString &providerId, const QString &text) {
  if (providerId != QLatin1String(kDaemonProvider))
    return text;
  return QCoreApplication::translate("SettingsCatalog", text.toUtf8().constData());
}
// The ambient timeouts the daemon offers, in minutes (ADR-0112).
constexpr int kAmbientMinutes[] = {5, 10, 15, 30, 60};
bool categoryKnown(const QString &id) {
  for (const auto &category : kCategories)
    if (id == QLatin1String(category.id))
      return true;
  return false;
}
void registerSettingsCatalog() {
  qmlRegisterType<SettingsCatalog>("Arinn.Shell", 1, 0, "SettingsCatalog");
  qmlRegisterType<KeyboardMonitor>("Arinn.Shell", 1, 0, "KeyboardMonitor");
}
Q_COREAPP_STARTUP_FUNCTION(registerSettingsCatalog)
} // namespace

KeyboardMonitor::KeyboardMonitor(QObject *parent) : QObject(parent) {
  scan();
}
void KeyboardMonitor::scan() {
  bool found = false;
  const auto devices = QInputDevice::devices();
  for (const auto *device : devices) {
    if (device && device->type() == QInputDevice::DeviceType::Keyboard) {
      found = true;
      break;
    }
  }
  if (found == m_available)
    return;
  m_available = found;
  emit availableChanged();
}

struct SettingsCatalog::RootScan {
  quint64 ticket = 0;
  quint64 epoch = 0;
  int pages = 0;
  QSet<QString> tokens;
  QHash<QString, QString> found;
  QHash<QString, QString> generations;
};

struct SettingsCatalog::Load {
  quint64 epoch = 0;
  int remaining = 0;
  QList<ProviderRecord> providers;
};

SettingsCatalog::SettingsCatalog(QObject *parent) : QObject(parent) {
  auto bus = QDBusConnection::sessionBus();
  bus.connect(QString::fromLatin1(service), QString::fromLatin1(path),
              QString::fromLatin1(settingsInterface),
              QStringLiteral("SettingsChanged"), this,
              SLOT(onSettingsChanged(QString)));
  bus.connect(QString::fromLatin1(service), QString::fromLatin1(path),
              QString::fromLatin1(service), QStringLiteral("ProvidersChanged"),
              this, SLOT(refresh()));
  bus.connect(QString::fromLatin1(service), QString::fromLatin1(path),
              QString::fromLatin1(service), QStringLiteral("ProvidersChanged"),
              this, SLOT(refreshRoots()));
  bus.connect(QString::fromLatin1(service), QString::fromLatin1(path),
              QString::fromLatin1(service), QStringLiteral("StateChanged"), this,
              SLOT(onStateChanged(QString, QString, QString, qulonglong)));
  auto watcher = new QDBusServiceWatcher(
      QString::fromLatin1(service), bus, QDBusServiceWatcher::WatchForOwnerChange,
      this);
  connect(watcher, &QDBusServiceWatcher::serviceOwnerChanged, this,
          [this](const QString &, const QString &, const QString &) {
            ++m_epoch;
            m_pending.clear();
            m_deferred.clear();
            m_errors.clear();
            m_deferredGeneration.clear();
            m_saveGeneration.clear();
            m_tickets.clear();
            m_providers.clear();
            m_libraryPaths.clear();
            m_libraryGenerations.clear();
            m_rootFeatures.clear();
            m_rootsReading.clear();
            m_rootsAgain.clear();
            m_roots.clear();
            m_rootTickets.clear();
            m_loaded = false;
            m_error = false;
            m_unreachable = false;
            // Drop the in-flight load. Its callbacks see the new epoch and
            // must not clear the refresh this call starts.
            m_refreshing = false;
            m_dirty = false;
            publish();
            refresh();
            refreshRoots();
          });
  m_timer.setInterval(fallbackPollMs);
  connect(&m_timer, &QTimer::timeout, this, &SettingsCatalog::refresh);
  m_timer.start();
  refresh();
  refreshRoots();
}
QString SettingsCatalog::screenState() const {
  if (m_unreachable)
    return QStringLiteral("unavailable");
  if (!m_loaded)
    return m_error ? QStringLiteral("error") : QStringLiteral("loading");
  return QStringLiteral("ready");
}
QString SettingsCatalog::keyFor(const QString &providerId,
                                const QString &settingId) const {
  return providerId + QLatin1Char('\n') + settingId;
}
const SettingsCatalog::SettingRecord *
SettingsCatalog::find(const QString &providerId, const QString &settingId) const {
  for (const auto &provider : m_providers) {
    if (!provider.ok || provider.id != providerId)
      continue;
    for (const auto &setting : provider.settings)
      if (setting.id == settingId)
        return &setting;
  }
  return nullptr;
}
void SettingsCatalog::onSettingsChanged(const QString &) { refresh(); }
void SettingsCatalog::refresh() {
  if (m_refreshing) {
    m_dirty = true;
    return;
  }
  m_refreshing = true;
  m_dirty = false;
  auto load = std::make_shared<Load>();
  load->epoch = m_epoch;
  auto pending = new QDBusPendingCallWatcher(
      QDBusConnection::sessionBus().asyncCall(settingsCall("ListProviders"),
                                              callMs),
      this);
  connect(pending, &QDBusPendingCallWatcher::finished, this,
          [this, load](auto *call) {
            const auto reply = call->reply();
            call->deleteLater();
            if (load->epoch != m_epoch)
              return;
            if (reply.type() == QDBusMessage::ErrorMessage ||
                reply.arguments().isEmpty()) {
              m_refreshing = false;
              if (reply.type() == QDBusMessage::ErrorMessage &&
                  serviceMissing(reply)) {
                m_unreachable = true;
                m_loaded = false;
                m_error = false;
                m_providers.clear();
              } else if (!m_loaded) {
                m_error = true;
                m_unreachable = false;
              }
              publish();
              if (m_dirty) {
                m_dirty = false;
                refresh();
              }
              return;
            }
            const auto arg =
                qvariant_cast<QDBusArgument>(reply.arguments().at(0));
            arg.beginArray();
            while (!arg.atEnd()) {
              ProviderRecord provider;
              arg.beginStructure();
              arg >> provider.id >> provider.name;
              arg.endStructure();
              load->providers.append(provider);
            }
            arg.endArray();
            if (load->providers.isEmpty()) {
              finishLoad(load);
              return;
            }
            load->remaining = load->providers.size();
            for (int index = 0; index < load->providers.size(); ++index)
              readProvider(load, index);
          });
}
void SettingsCatalog::readProvider(const std::shared_ptr<Load> &load, int index) {
  auto msg = settingsCall("GetSettings");
  msg << load->providers.at(index).id;
  auto pending = new QDBusPendingCallWatcher(
      QDBusConnection::sessionBus().asyncCall(msg, callMs), this);
  connect(pending, &QDBusPendingCallWatcher::finished, this,
          [this, load, index](auto *call) {
            const auto reply = call->reply();
            call->deleteLater();
            if (load->epoch != m_epoch)
              return;
            auto &provider = load->providers[index];
            if (reply.type() != QDBusMessage::ErrorMessage &&
                reply.arguments().size() >= 2) {
              provider.generation = reply.arguments().at(0).toString();
              const auto arg =
                  qvariant_cast<QDBusArgument>(reply.arguments().at(1));
              arg.beginArray();
              while (!arg.atEnd()) {
                SettingRecord setting;
                arg.beginStructure();
                arg >> setting.id >> setting.category >> setting.title >>
                    setting.description >> setting.kind >> setting.valueJson >>
                    setting.constraints >> setting.enabled;
                arg.endStructure();
                if (categoryKnown(setting.category))
                  provider.settings.append(setting);
              }
              arg.endArray();
              provider.ok = true;
            }
            if (--load->remaining == 0)
              finishLoad(load);
          });
}
void SettingsCatalog::finishLoad(const std::shared_ptr<Load> &load) {
  m_refreshing = false;
  if (load->epoch != m_epoch)
    return;
  int okCount = 0;
  for (const auto &provider : load->providers)
    if (provider.ok)
      ++okCount;
  if (okCount == 0 && !load->providers.isEmpty()) {
    if (!m_loaded)
      m_error = true;
  } else {
    m_loaded = true;
    m_error = false;
    m_unreachable = false;
    m_providers = load->providers;
  }
  publish();
  if (m_dirty) {
    m_dirty = false;
    refresh();
  }
}
// Library root availability (ADR-0071, spec §5.6). The library providers and
// their daemon paths come from Daemon1.Providers; each one that lists
// LibraryRoots1 in Features is asked for Library1.GetRoots. Unsupported means
// no unavailable state, Unavailable means not read yet: both leave the folders
// as they are until the provider's next library StateChanged. One change
// brings StateChanged and then ProvidersChanged, so reads are coalesced per
// provider: one read at a time, run once more if asked again meanwhile.
// Features cannot change within a provider generation and is read once per
// generation.
void SettingsCatalog::refreshRoots() {
  auto scan = std::make_shared<RootScan>();
  scan->ticket = ++m_rootScan;
  scan->epoch = m_epoch;
  readLibraryProviders(scan, QString());
}
void SettingsCatalog::readLibraryProviders(const std::shared_ptr<RootScan> &scan,
                                           const QString &token) {
  if (++scan->pages > 1024 || scan->tokens.contains(token))
    return;
  scan->tokens.insert(token);
  auto msg = QDBusMessage::createMethodCall(service, path, service,
                                            QStringLiteral("Providers"));
  msg << token;
  auto pending = new QDBusPendingCallWatcher(
      QDBusConnection::sessionBus().asyncCall(msg, callMs), this);
  connect(pending, &QDBusPendingCallWatcher::finished, this,
          [this, scan](auto *call) {
            const auto reply = call->reply();
            call->deleteLater();
            if (scan->epoch != m_epoch || scan->ticket != m_rootScan ||
                reply.type() == QDBusMessage::ErrorMessage ||
                reply.signature() != QLatin1String(providersPage))
              return;
            const auto arg =
                qvariant_cast<QDBusArgument>(reply.arguments().at(0));
            QString snapshot, next;
            arg.beginStructure();
            arg >> snapshot;
            arg.beginArray();
            while (!arg.atEnd()) {
              QString id, plugin, source, name, version, runtime, backend,
                  generation, error;
              QStringList capabilities;
              QDBusObjectPath endpoint;
              qulonglong revision = 0, errorAt = 0, recovered = 0,
                         restarts = 0, cumulative = 0;
              arg.beginStructure();
              arg >> id >> plugin >> source >> name >> version >> endpoint >>
                  capabilities >> runtime >> backend >> generation >> revision >>
                  error >> errorAt >> recovered >> restarts >> cumulative;
              arg.endStructure();
              if (capabilities.contains(QLatin1String("library")) &&
                  runtime == QLatin1String("ready") && !id.isEmpty() &&
                  !endpoint.path().isEmpty()) {
                scan->found.insert(id, endpoint.path());
                scan->generations.insert(id, generation);
              }
            }
            arg.endArray();
            arg >> next;
            arg.endStructure();
            if (!next.isEmpty()) {
              readLibraryProviders(scan, next);
              return;
            }
            m_libraryPaths = scan->found;
            m_libraryGenerations = scan->generations;
            QStringList gone;
            for (auto it = m_roots.cbegin(); it != m_roots.cend(); ++it)
              if (!m_libraryPaths.contains(it.key()))
                gone.append(it.key());
            for (const auto &id : gone)
              dropRoots(id);
            for (auto it = m_libraryPaths.cbegin(); it != m_libraryPaths.cend();
                 ++it)
              readRoots(it.key());
          });
}
void SettingsCatalog::onStateChanged(const QString &providerId,
                                     const QString &capability, const QString &,
                                     qulonglong) {
  if (capability != QLatin1String("library"))
    return;
  if (m_libraryPaths.contains(providerId))
    readRoots(providerId);
  else
    refreshRoots();
}
void SettingsCatalog::readRoots(const QString &providerId) {
  const auto objectPath = m_libraryPaths.value(providerId);
  if (objectPath.isEmpty())
    return;
  if (m_rootsReading.contains(providerId)) {
    m_rootsAgain.insert(providerId);
    return;
  }
  m_rootsReading.insert(providerId);
  const auto ticket = ++m_rootTickets[providerId];
  const auto epoch = m_epoch;
  const auto generation = m_libraryGenerations.value(providerId);
  const auto cached = m_rootFeatures.constFind(providerId);
  if (!generation.isEmpty() && cached != m_rootFeatures.cend() &&
      cached->first == generation) {
    if (cached->second) {
      readRootList(providerId, objectPath, ticket, epoch);
    } else {
      dropRoots(providerId);
      finishRootRead(providerId);
    }
    return;
  }
  auto features = QDBusMessage::createMethodCall(
      service, objectPath, libraryInterface, QStringLiteral("Features"));
  auto pending = new QDBusPendingCallWatcher(
      QDBusConnection::sessionBus().asyncCall(features, callMs), this);
  connect(pending, &QDBusPendingCallWatcher::finished, this,
          [this, providerId, objectPath, generation, ticket, epoch](auto *call) {
            const auto reply = call->reply();
            call->deleteLater();
            if (epoch != m_epoch || ticket != m_rootTickets.value(providerId))
              return;
            const bool answered = reply.type() != QDBusMessage::ErrorMessage &&
                                  reply.signature() == QLatin1String("as");
            const bool roots =
                answered && reply.arguments().at(0).toStringList().contains(
                                QLatin1String(rootsFeature));
            if (answered && !generation.isEmpty())
              m_rootFeatures.insert(providerId, qMakePair(generation, roots));
            // Without LibraryRoots1 the provider never reports a folder as
            // unavailable (ADR-0071).
            if (!roots) {
              dropRoots(providerId);
              finishRootRead(providerId);
              return;
            }
            readRootList(providerId, objectPath, ticket, epoch);
          });
}
void SettingsCatalog::readRootList(const QString &providerId,
                                   const QString &objectPath, quint64 ticket,
                                   quint64 epoch) {
  auto roots = QDBusMessage::createMethodCall(service, objectPath, libraryInterface,
                                              QStringLiteral("GetRoots"));
  auto pending = new QDBusPendingCallWatcher(
      QDBusConnection::sessionBus().asyncCall(roots, callMs), this);
  connect(pending, &QDBusPendingCallWatcher::finished, this,
          [this, providerId, ticket, epoch](auto *call) {
            const auto reply = call->reply();
            call->deleteLater();
            if (epoch != m_epoch || ticket != m_rootTickets.value(providerId))
              return;
            if (reply.type() == QDBusMessage::ErrorMessage ||
                reply.signature() != QLatin1String(rootsReply)) {
              dropRoots(providerId);
              finishRootRead(providerId);
              return;
            }
            const auto arg = qvariant_cast<QDBusArgument>(reply.arguments().at(0));
            QString generation;
            qulonglong revision = 0;
            QHash<QString, bool> states;
            arg.beginStructure();
            arg >> generation >> revision;
            arg.beginArray();
            while (!arg.atEnd()) {
              QString rootPath;
              bool available = true;
              arg.beginStructure();
              arg >> rootPath >> available;
              arg.endStructure();
              states.insert(rootPath, available);
            }
            arg.endArray();
            arg.endStructure();
            if (!m_roots.contains(providerId) ||
                m_roots.value(providerId) != states) {
              m_roots.insert(providerId, states);
              publish();
            }
            finishRootRead(providerId);
          });
}
void SettingsCatalog::finishRootRead(const QString &providerId) {
  m_rootsReading.remove(providerId);
  if (m_rootsAgain.remove(providerId))
    readRoots(providerId);
}
void SettingsCatalog::dropRoots(const QString &providerId) {
  if (m_roots.remove(providerId))
    publish();
}
void SettingsCatalog::readPreferences() {
  bool light = m_lightField;
  int minutes = m_ambientMinutes;
  for (const auto &provider : m_providers) {
    if (!provider.ok || provider.id != QLatin1String(kDaemonProvider))
      continue;
    for (const auto &setting : provider.settings) {
      const auto value = valueFor(setting.kind, setting.valueJson, false);
      if (setting.id == QLatin1String("light_field") &&
          value.typeId() == QMetaType::Bool)
        light = value.toBool();
      if (setting.id == QLatin1String("ambient_timeout")) {
        // Only an offered value applies; anything else keeps the last one.
        bool ok = false;
        const int parsed = value.toString().toInt(&ok);
        if (ok && std::find(std::begin(kAmbientMinutes), std::end(kAmbientMinutes),
                            parsed) != std::end(kAmbientMinutes))
          minutes = parsed;
      }
    }
  }
  if (light == m_lightField && minutes == m_ambientMinutes)
    return;
  m_lightField = light;
  m_ambientMinutes = minutes;
  emit preferencesChanged();
}
void SettingsCatalog::publish() {
  syncDeferred();
  readPreferences();
  QVariantList categories;
  QVariantList rows;
  QSet<QString> live;
  if (m_loaded && !m_unreachable) {
    for (const auto &category : kCategories) {
      categories.append(QVariantMap{
          {QStringLiteral("id"), QString::fromLatin1(category.id)},
          {QStringLiteral("title"), tr(category.title)},
          {QStringLiteral("icon"), QString::fromLatin1(category.icon)}});
    }
    for (const auto &category : kCategories) {
      const auto categoryId = QString::fromLatin1(category.id);
      for (const auto &provider : m_providers) {
        if (!provider.ok)
          continue;
        for (const auto &setting : provider.settings) {
          if (setting.category != categoryId)
            continue;
          auto constraints = constraintsFor(setting.kind, setting.constraints);
          if (provider.id == QLatin1String(kDaemonProvider) &&
              constraints.contains(QStringLiteral("options"))) {
            QVariantList options;
            for (const auto &option :
                 constraints.value(QStringLiteral("options")).toList()) {
              auto map = option.toMap();
              map.insert(QStringLiteral("title"),
                         daemonText(provider.id,
                                    map.value(QStringLiteral("title")).toString()));
              options.append(map);
            }
            constraints.insert(QStringLiteral("options"), options);
          }
          const auto multiple =
              setting.kind == QLatin1String("path") &&
              constraints.value(QStringLiteral("multiple")).toBool();
          const auto key = keyFor(provider.id, setting.id);
          live.insert(key);
          const auto currentValue =
              valueFor(setting.kind, setting.valueJson, multiple);
          QVariantMap row{
              {QStringLiteral("providerId"), provider.id},
              {QStringLiteral("providerName"), provider.name},
              {QStringLiteral("id"), setting.id},
              {QStringLiteral("categoryId"), setting.category},
              {QStringLiteral("title"), daemonText(provider.id, setting.title)},
              {QStringLiteral("description"),
               daemonText(provider.id, setting.description)},
              {QStringLiteral("type"), setting.kind},
              {QStringLiteral("currentValue"), currentValue},
              {QStringLiteral("constraints"), constraints},
              {QStringLiteral("enabled"), setting.enabled},
              {QStringLiteral("pending"), m_pending.contains(key)},
              {QStringLiteral("deferred"), m_deferred.contains(key)},
              {QStringLiteral("error"), m_errors.value(key)}};
          // The folders of this list that the library provider reports
          // unavailable, matched by path as written (ADR-0071). The contract
          // does not name the setting the roots come from, so every folder
          // list (`directory` and `multiple`) of that provider is matched; a
          // list of files is not.
          if (multiple && constraints.value(QStringLiteral("directory")).toBool()) {
            QStringList unavailable;
            const auto states = m_roots.constFind(provider.id);
            if (states != m_roots.cend())
              for (const auto &entry : currentValue.toStringList())
                if (!states->value(entry, true) && !unavailable.contains(entry))
                  unavailable.append(entry);
            row.insert(QStringLiteral("unavailablePaths"), unavailable);
          }
          rows.append(row);
        }
      }
    }
  }
  QSet<QString> stalePending;
  for (const auto &key : m_pending)
    if (!live.contains(key))
      stalePending.insert(key);
  for (const auto &key : stalePending)
    m_pending.remove(key);
  QStringList staleErrors;
  for (auto it = m_errors.cbegin(); it != m_errors.cend(); ++it)
    if (!live.contains(it.key()))
      staleErrors.append(it.key());
  for (const auto &key : staleErrors)
    m_errors.remove(key);
  QStringList staleDeferred;
  for (const auto &key : m_deferred)
    if (!live.contains(key))
      staleDeferred.append(key);
  for (const auto &key : staleDeferred) {
    m_deferred.remove(key);
    m_deferredGeneration.remove(key);
  }
  // A reload follows every ProvidersChanged. Announcing unchanged rows makes
  // the open settings screen rebuild (#441).
  if (categories == m_categories && rows == m_rows &&
      screenState() == m_publishedState)
    return;
  m_categories = categories;
  m_rows = rows;
  m_publishedState = screenState();
  emit catalogChanged();
}
QString SettingsCatalog::encodeValue(const ProviderRecord &,
                                     const SettingRecord &row,
                                     const QVariant &value) const {
  const auto constraints = constraintsFor(row.kind, row.constraints);
  if (row.kind == QLatin1String("boolean")) {
    if (value.typeId() != QMetaType::Bool)
      return {};
    return value.toBool() ? QStringLiteral("true") : QStringLiteral("false");
  }
  if (row.kind == QLatin1String("integer")) {
    qlonglong number = 0;
    if (value.typeId() == QMetaType::Double) {
      const auto asDouble = value.toDouble();
      if (!std::isfinite(asDouble) || asDouble != std::trunc(asDouble))
        return {};
      if (asDouble < static_cast<double>(std::numeric_limits<qlonglong>::min()) ||
          asDouble > static_cast<double>(std::numeric_limits<qlonglong>::max()))
        return {};
      number = static_cast<qlonglong>(asDouble);
    } else {
      bool ok = false;
      number = value.toLongLong(&ok);
      if (!ok)
        return {};
    }
    if (constraints.contains(QStringLiteral("min")) &&
        number < constraints.value(QStringLiteral("min")).toLongLong())
      return {};
    if (constraints.contains(QStringLiteral("max")) &&
        number > constraints.value(QStringLiteral("max")).toLongLong())
      return {};
    return QString::number(number);
  }
  if (row.kind == QLatin1String("path") &&
      constraints.value(QStringLiteral("multiple")).toBool()) {
    QStringList paths;
    if (value.typeId() == QMetaType::QStringList)
      paths = value.toStringList();
    else if (value.canConvert<QVariantList>()) {
      for (const auto &item : value.toList())
        paths.append(item.toString());
    } else
      return {};
    const auto maxItems = constraints.contains(QStringLiteral("maxItems"))
                              ? constraints.value(QStringLiteral("maxItems")).toLongLong()
                              : 64;
    if (paths.size() > maxItems)
      return {};
    QJsonArray array;
    for (const auto &entry : paths) {
      if (entry.isEmpty())
        return {};
      array.append(entry);
    }
    return QString::fromUtf8(QJsonDocument(array).toJson(QJsonDocument::Compact));
  }
  if (row.kind != QLatin1String("string") && row.kind != QLatin1String("enum") &&
      row.kind != QLatin1String("path"))
    return {};
  if (!value.canConvert<QString>())
    return {};
  const auto text = value.toString();
  if (row.kind == QLatin1String("string")) {
    const auto maxLength = constraints.contains(QStringLiteral("maxLength"))
                               ? constraints.value(QStringLiteral("maxLength")).toLongLong()
                               : 1024;
    if (scalarCount(text) > maxLength || controlCharacter(text))
      return {};
  } else if (row.kind == QLatin1String("enum")) {
    bool found = false;
    const auto options = constraints.value(QStringLiteral("options")).toList();
    for (const auto &option : options)
      if (option.toMap().value(QStringLiteral("value")).toString() == text)
        found = true;
    if (!found)
      return {};
  }
  return jsonString(text);
}
void SettingsCatalog::setValue(const QString &providerId, const QString &settingId,
                               const QVariant &value) {
  if (!m_loaded || m_unreachable)
    return;
  const ProviderRecord *provider = nullptr;
  for (const auto &candidate : m_providers) {
    if (candidate.ok && candidate.id == providerId) {
      provider = &candidate;
      break;
    }
  }
  const auto *row = find(providerId, settingId);
  if (!provider || !row || !row->enabled)
    return;
  const auto key = keyFor(providerId, settingId);
  if (m_pending.contains(key))
    return;
  const auto json = encodeValue(*provider, *row, value);
  if (json.isEmpty()) {
    m_errors.insert(key, QStringLiteral("InvalidArgument"));
    publish();
    return;
  }
  m_errors.remove(key);
  m_pending.insert(key);
  const auto ticket = ++m_tickets[key];
  const auto epoch = m_epoch;
  const auto generation = provider->generation;
  m_saveGeneration.insert(key, generation);
  publish();
  auto msg = settingsCall("SetValue");
  msg << providerId << generation << settingId << json
      << QUuid::createUuid().toString(QUuid::WithoutBraces);
  auto pending = new QDBusPendingCallWatcher(
      QDBusConnection::sessionBus().asyncCall(msg, callMs), this);
  connect(pending, &QDBusPendingCallWatcher::finished, this,
          [this, key, ticket, epoch](auto *call) {
            const auto reply = call->reply();
            call->deleteLater();
            if (ticket != m_tickets.value(key) || epoch != m_epoch)
              return;
            if (reply.type() == QDBusMessage::ErrorMessage ||
                reply.signature() != QLatin1String("o") ||
                reply.arguments().isEmpty()) {
              settle(key, ticket, epoch,
                     reply.type() == QDBusMessage::ErrorMessage
                         ? errorCode(reply)
                         : QStringLiteral("Unavailable"));
              return;
            }
            watchOperation(
                key, qvariant_cast<QDBusObjectPath>(reply.arguments().at(0)).path(),
                ticket, epoch);
          });
}
void SettingsCatalog::dismissError(const QString &providerId,
                                   const QString &settingId) {
  const auto key = keyFor(providerId, settingId);
  if (!m_errors.remove(key))
    return;
  publish();
}
void SettingsCatalog::syncDeferred() {
  QStringList drop;
  for (const auto &key : m_deferred) {
    const auto providerId = key.section(QLatin1Char('\n'), 0, 0);
    const QString *generation = nullptr;
    for (const auto &provider : m_providers) {
      if (provider.ok && provider.id == providerId) {
        generation = &provider.generation;
        break;
      }
    }
    if (!generation || *generation != m_deferredGeneration.value(key))
      drop.append(key);
  }
  for (const auto &key : drop) {
    m_deferred.remove(key);
    m_deferredGeneration.remove(key);
  }
}
void SettingsCatalog::settle(const QString &key, quint64 ticket, quint64 epoch,
                             const QString &error, bool deferred) {
  if (ticket != m_tickets.value(key) || epoch != m_epoch)
    return;
  m_pending.remove(key);
  if (deferred && error.isEmpty()) {
    // Provider answered Unsupported: the value is stored and applies at its
    // next start. The note stays until that provider's generation changes.
    m_errors.remove(key);
    m_deferred.insert(key);
    m_deferredGeneration.insert(key, m_saveGeneration.value(key));
  } else if (error.isEmpty()) {
    m_errors.remove(key);
    m_deferred.remove(key);
    m_deferredGeneration.remove(key);
  } else {
    m_errors.insert(key, error);
  }
  m_saveGeneration.remove(key);
  publish();
  if (error.isEmpty())
    refresh();
}
void SettingsCatalog::watchOperation(const QString &key, const QString &objectPath,
                                     quint64 ticket, quint64 epoch) {
  auto timer = new QTimer(this);
  timer->setInterval(100);
  const auto deadline = QDeadlineTimer(operationWaitMs);
  connect(timer, &QTimer::timeout, this,
          [this, timer, key, objectPath, ticket, epoch, deadline] {
            timer->stop();
            if (ticket != m_tickets.value(key) || epoch != m_epoch) {
              timer->deleteLater();
              return;
            }
            if (deadline.hasExpired()) {
              settle(key, ticket, epoch, QStringLiteral("Timeout"));
              timer->deleteLater();
              return;
            }
            auto msg = QDBusMessage::createMethodCall(
                QString::fromLatin1(service), objectPath,
                QStringLiteral("org.arinn.Operation1"), QStringLiteral("Get"));
            auto pending = new QDBusPendingCallWatcher(
                QDBusConnection::sessionBus().asyncCall(msg, callMs), this);
            connect(pending, &QDBusPendingCallWatcher::finished, this,
                    [this, timer, key, ticket, epoch, deadline](auto *call) {
                      const auto reply = call->reply();
                      call->deleteLater();
                      if (ticket != m_tickets.value(key) || epoch != m_epoch) {
                        timer->deleteLater();
                        return;
                      }
                      if (reply.type() == QDBusMessage::ErrorMessage ||
                          reply.signature() != QLatin1String("(ssbtta(sss)b)") ||
                          reply.arguments().isEmpty()) {
                        settle(key, ticket, epoch, QStringLiteral("Unavailable"));
                        timer->deleteLater();
                        return;
                      }
                      const auto arg =
                          qvariant_cast<QDBusArgument>(reply.arguments().at(0));
                      QString state, error;
                      bool retryable = false, cancellable = false;
                      qulonglong started = 0, ended = 0;
                      arg.beginStructure();
                      arg >> state >> error >> retryable >> started >> ended;
                      arg.beginArray();
                      while (!arg.atEnd()) {
                        QString provider, capability, entity;
                        arg.beginStructure();
                        arg >> provider >> capability >> entity;
                        arg.endStructure();
                      }
                      arg.endArray();
                      arg >> cancellable;
                      arg.endStructure();
                      if (state == QLatin1String("running") ||
                          state == QLatin1String("queued")) {
                        if (deadline.hasExpired()) {
                          settle(key, ticket, epoch, QStringLiteral("Timeout"));
                          timer->deleteLater();
                          return;
                        }
                        timer->start();
                        return;
                      }
                      if (error == QLatin1String("Unsupported")) {
                        // The provider stored the value and applies it at its
                        // next start. Operation1 puts an error only on a
                        // failed operation, so this code can arrive as either
                        // succeeded or failed. Both are the next-start note,
                        // not a rejection.
                        settle(key, ticket, epoch, QString(), true);
                      } else if (state == QLatin1String("succeeded")) {
                        settle(key, ticket, epoch, QString());
                      } else {
                        settle(key, ticket, epoch,
                               error.isEmpty() ? state : error);
                      }
                      timer->deleteLater();
                    });
          });
  timer->start();
}
