// SPDX-FileCopyrightText: 2026 Arinn contributors
//
// SPDX-License-Identifier: GPL-3.0-or-later

#include "settings_catalog.h"
#include <QDeadlineTimer>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QJsonValue>
#include <QUuid>
#include <QtDBus>
#include <QtQml>
#include <cmath>
#include <limits>
namespace {
constexpr auto service = "org.arinn.Daemon1";
constexpr auto path = "/org/arinn/Daemon1";
constexpr auto settingsInterface = "org.arinn.Settings1";
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
    {"media", QT_TR_NOOP("Media"), "video_library"},
    {"playback", QT_TR_NOOP("Playback"), "play_circle"},
    {"sound", QT_TR_NOOP("Sound"), "volume_up"},
    {"network", QT_TR_NOOP("Network"), "wifi"},
    {"applications", QT_TR_NOOP("Applications"), "apps"},
    {"appearance", QT_TR_NOOP("Appearance"), "palette"},
    {"system", QT_TR_NOOP("System"), "memory"},
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
bool categoryKnown(const QString &id) {
  for (const auto &category : kCategories)
    if (id == QLatin1String(category.id))
      return true;
  return false;
}
void registerSettingsCatalog() {
  qmlRegisterType<SettingsCatalog>("Arinn.Shell", 1, 0, "SettingsCatalog");
}
Q_COREAPP_STARTUP_FUNCTION(registerSettingsCatalog)
} // namespace

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
  auto watcher = new QDBusServiceWatcher(
      QString::fromLatin1(service), bus, QDBusServiceWatcher::WatchForOwnerChange,
      this);
  connect(watcher, &QDBusServiceWatcher::serviceOwnerChanged, this,
          [this](const QString &, const QString &, const QString &) {
            ++m_epoch;
            m_pending.clear();
            m_errors.clear();
            m_tickets.clear();
            m_providers.clear();
            m_loaded = false;
            m_error = false;
            m_unreachable = false;
            // Drop the in-flight load. Its callbacks see the new epoch and
            // must not clear the refresh this call starts.
            m_refreshing = false;
            m_dirty = false;
            publish();
            refresh();
          });
  m_timer.setInterval(fallbackPollMs);
  connect(&m_timer, &QTimer::timeout, this, &SettingsCatalog::refresh);
  m_timer.start();
  refresh();
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
void SettingsCatalog::publish() {
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
          const auto constraints = constraintsFor(setting.kind, setting.constraints);
          const auto multiple =
              setting.kind == QLatin1String("path") &&
              constraints.value(QStringLiteral("multiple")).toBool();
          const auto key = keyFor(provider.id, setting.id);
          live.insert(key);
          rows.append(QVariantMap{
              {QStringLiteral("providerId"), provider.id},
              {QStringLiteral("providerName"), provider.name},
              {QStringLiteral("id"), setting.id},
              {QStringLiteral("categoryId"), setting.category},
              {QStringLiteral("title"), setting.title},
              {QStringLiteral("description"), setting.description},
              {QStringLiteral("type"), setting.kind},
              {QStringLiteral("currentValue"),
               valueFor(setting.kind, setting.valueJson, multiple)},
              {QStringLiteral("constraints"), constraints},
              {QStringLiteral("enabled"), setting.enabled},
              {QStringLiteral("pending"), m_pending.contains(key)},
              {QStringLiteral("error"), m_errors.value(key)}});
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
  m_categories = categories;
  m_rows = rows;
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
void SettingsCatalog::settle(const QString &key, quint64 ticket, quint64 epoch,
                             const QString &error) {
  if (ticket != m_tickets.value(key) || epoch != m_epoch)
    return;
  m_pending.remove(key);
  if (error.isEmpty())
    m_errors.remove(key);
  else
    m_errors.insert(key, error);
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
                      if (state == QLatin1String("succeeded"))
                        settle(key, ticket, epoch, QString());
                      else
                        settle(key, ticket, epoch,
                               error.isEmpty() ? state : error);
                      timer->deleteLater();
                    });
          });
  timer->start();
}
