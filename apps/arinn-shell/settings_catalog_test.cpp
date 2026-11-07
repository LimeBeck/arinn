// SPDX-FileCopyrightText: 2026 Arinn contributors
//
// SPDX-License-Identifier: GPL-3.0-or-later

#include "settings_catalog.h"
#include <QInputDevice>
#include <QKeyEvent>
#include <QtDBus>
#include <QTranslator>
#include <QtTest>
namespace {
// A fake daemon on its own connection, as the real one is, so the catalog's
// calls go through the bus and a reply can be held back (#558). The other
// tests keep the daemon on the catalog's connection.
QDBusConnection ownBus() {
  return QDBusConnection::connectToBus(QDBusConnection::SessionBus,
                                       QStringLiteral("arinn-fake-daemon"));
}
struct ProviderWire {
  QString id;
  QString name;
};
struct SettingWire {
  QString id;
  QString category;
  QString title;
  QString description;
  QString kind;
  QString value;
  QString constraints;
  bool enabled = true;
};
struct EntityWire {
  QString provider;
  QString capability;
  QString entity;
};
// One row of Daemon1.Providers and one Library1.GetRoots root (ADR-0071).
struct DaemonProviderWire {
  QString id;
  QDBusObjectPath endpoint;
  QStringList capabilities;
  QString generation;
  QString plugin;
};
struct RootWire {
  QString path;
  bool available = true;
};
QDBusArgument &operator<<(QDBusArgument &argument, const DaemonProviderWire &row) {
  argument.beginStructure();
  argument << row.id
           << (row.plugin.isEmpty() ? QStringLiteral("plugin") : row.plugin)
           << QStringLiteral("system")
           << row.id << QStringLiteral("1") << row.endpoint << row.capabilities
           << QStringLiteral("ready") << QStringLiteral("ready")
           << (row.generation.isEmpty() ? QStringLiteral("g-") + row.id
                                        : row.generation)
           << qulonglong(1) << QString()
           << qulonglong(0) << qulonglong(0) << qulonglong(0) << qulonglong(0);
  argument.endStructure();
  return argument;
}
const QDBusArgument &operator>>(const QDBusArgument &argument,
                                DaemonProviderWire &) {
  return argument;
}
QDBusArgument &operator<<(QDBusArgument &argument, const RootWire &row) {
  argument.beginStructure();
  argument << row.path << row.available;
  argument.endStructure();
  return argument;
}
const QDBusArgument &operator>>(const QDBusArgument &argument, RootWire &) {
  return argument;
}
QDBusArgument &operator<<(QDBusArgument &argument, const ProviderWire &row) {
  argument.beginStructure();
  argument << row.id << row.name;
  argument.endStructure();
  return argument;
}
const QDBusArgument &operator>>(const QDBusArgument &argument, ProviderWire &) {
  return argument;
}
QDBusArgument &operator<<(QDBusArgument &argument, const SettingWire &row) {
  argument.beginStructure();
  argument << row.id << row.category << row.title << row.description << row.kind
           << row.value << row.constraints << row.enabled;
  argument.endStructure();
  return argument;
}
const QDBusArgument &operator>>(const QDBusArgument &argument, SettingWire &) {
  return argument;
}
QDBusArgument &operator<<(QDBusArgument &argument, const EntityWire &row) {
  argument.beginStructure();
  argument << row.provider << row.capability << row.entity;
  argument.endStructure();
  return argument;
}
const QDBusArgument &operator>>(const QDBusArgument &argument, EntityWire &) {
  return argument;
}
class SettingsDaemon : public QDBusVirtualObject {
public:
  QList<ProviderWire> providers;
  QHash<QString, QList<SettingWire>> settings;
  // GetTranslations tables (ADR-0129); a provider without one gets the
  // NotSupported reply of an older daemon.
  QHash<QString, QString> translations;
  bool rejectList = false;
  bool holdOperation = false;
  QString operationState = QStringLiteral("succeeded");
  QString operationError;
  QHash<QString, QString> generations;
  QString generationOf(const QString &id) const {
    return generations.contains(id) ? generations.value(id)
                                    : QStringLiteral("g-") + id;
  }
  int operationReads = 0;
  int settingsReads = 0;
  // Library providers on Daemon1.Providers, their Features and roots.
  QList<DaemonProviderWire> libraryProviders;
  // Daemon1.Providers answers with an error.
  bool providersFail = false;
  QStringList libraryFeatures;
  QList<RootWire> roots;
  QString rootsError;
  int rootsReads = 0;
  int featuresReads = 0;
  // True when the daemon runs on ownBus().
  bool ownConnection = false;
  QDBusConnection connection() const {
    return ownConnection ? ownBus() : QDBusConnection::sessionBus();
  }
  // While set, GetRoots calls wait in `heldRoots` for releaseRoots().
  bool holdRoots = false;
  QList<QDBusMessage> heldRoots;
  QDBusMessage rootsReply(const QDBusMessage &message) const {
    if (!rootsError.isEmpty())
      return message.createErrorReply(
          QStringLiteral("org.arinn.Error1.") + rootsError, rootsError);
    QDBusArgument body;
    body.beginStructure();
    body << QStringLiteral("00000000-0000-0000-0000-000000000001")
         << qulonglong(rootsReads);
    body.beginArray(QMetaType::fromType<RootWire>());
    for (const auto &root : roots)
      body << root;
    body.endArray();
    body.endStructure();
    return message.createReply(QVariant::fromValue(body));
  }
  void releaseRoots() {
    holdRoots = false;
    const auto held = heldRoots;
    heldRoots.clear();
    for (const auto &message : held)
      connection().send(rootsReply(message));
  }
  struct Call {
    QString provider;
    QString generation;
    QString setting;
    QString value;
    QString request;
  };
  QList<Call> sets;
  QString introspect(const QString &) const override { return {}; }
  bool handleMessage(const QDBusMessage &message,
                     const QDBusConnection &bus) override {
    if (message.member() == QLatin1String("ListProviders")) {
      if (rejectList) {
        bus.send(message.createErrorReply(
            QStringLiteral("org.arinn.Error1.Unavailable"),
            QStringLiteral("Unavailable")));
        return true;
      }
      QDBusArgument body;
      body.beginArray(QMetaType::fromType<ProviderWire>());
      for (const auto &provider : providers)
        body << provider;
      body.endArray();
      bus.send(message.createReply(QVariant::fromValue(body)));
      return true;
    }
    if (message.member() == QLatin1String("GetSettings")) {
      ++settingsReads;
      const auto id = message.arguments().value(0).toString();
      if (!settings.contains(id)) {
        bus.send(message.createErrorReply(
            QStringLiteral("org.arinn.Error1.NotFound"), QStringLiteral("NotFound")));
        return true;
      }
      QDBusArgument body;
      body.beginArray(QMetaType::fromType<SettingWire>());
      for (const auto &row : settings.value(id))
        body << row;
      body.endArray();
      auto reply = message.createReply();
      reply << generationOf(id) << QVariant::fromValue(body);
      bus.send(reply);
      return true;
    }
    if (message.member() == QLatin1String("GetTranslations")) {
      const auto id = message.arguments().value(0).toString();
      if (translations.contains(id)) {
        auto reply = message.createReply();
        reply << generationOf(id) << translations.value(id);
        bus.send(reply);
        return true;
      }
    }
    if (message.member() == QLatin1String("SetValue")) {
      Call call;
      call.provider = message.arguments().value(0).toString();
      call.generation = message.arguments().value(1).toString();
      call.setting = message.arguments().value(2).toString();
      call.value = message.arguments().value(3).toString();
      call.request = message.arguments().value(4).toString();
      sets.append(call);
      auto rows = settings.value(call.provider);
      for (auto &row : rows) {
        if (row.id == call.setting)
          row.value = call.value;
      }
      settings.insert(call.provider, rows);
      bus.send(message.createReply(QVariant::fromValue(
          QDBusObjectPath(QStringLiteral("/org/arinn/Daemon1/ops/settings")))));
      return true;
    }
    if (message.member() == QLatin1String("Providers") &&
        message.interface() == QLatin1String("org.arinn.Daemon1") && providersFail) {
      bus.send(message.createErrorReply(QStringLiteral("org.arinn.Error1.Unavailable"),
                                        QStringLiteral("fixture")));
      return true;
    }
    if (message.member() == QLatin1String("Providers") &&
        message.interface() == QLatin1String("org.arinn.Daemon1")) {
      QDBusArgument body;
      body.beginStructure();
      body << QStringLiteral("snapshot");
      body.beginArray(QMetaType::fromType<DaemonProviderWire>());
      for (const auto &provider : libraryProviders)
        body << provider;
      body.endArray();
      body << QString();
      body.endStructure();
      bus.send(message.createReply(QVariant::fromValue(body)));
      return true;
    }
    if (message.interface() == QLatin1String("org.arinn.Library1") &&
        message.member() == QLatin1String("Features")) {
      ++featuresReads;
      bus.send(message.createReply(libraryFeatures));
      return true;
    }
    if (message.interface() == QLatin1String("org.arinn.Library1") &&
        message.member() == QLatin1String("GetRoots")) {
      ++rootsReads;
      message.setDelayedReply(true);
      if (holdRoots)
        heldRoots.append(message);
      else
        bus.send(rootsReply(message));
      return true;
    }
    if (message.member() == QLatin1String("Get") &&
        message.interface() == QLatin1String("org.arinn.Operation1")) {
      ++operationReads;
      QDBusArgument body;
      body.beginStructure();
      body << QString(holdOperation ? QStringLiteral("running") : operationState)
           << operationError << false << qulonglong(1) << qulonglong(2);
      body.beginArray(QMetaType::fromType<EntityWire>());
      body.endArray();
      body << false;
      body.endStructure();
      bus.send(message.createReply(QVariant::fromValue(body)));
      return true;
    }
    bus.send(message.createErrorReply(QDBusError::NotSupported,
                                      QStringLiteral("fixture")));
    return true;
  }
};
bool registerBus(SettingsDaemon &daemon) {
  auto bus = daemon.connection();
  return bus.registerService(QStringLiteral("org.arinn.Daemon1")) &&
         bus.registerVirtualObject(QStringLiteral("/org/arinn"), &daemon,
                                   QDBusConnection::SubPath);
}
SettingWire setting(const QString &id, const QString &category, const QString &kind,
                    const QString &value, const QString &constraints, bool enabled) {
  SettingWire row;
  row.id = id;
  row.category = category;
  row.title = id;
  row.description = QStringLiteral("detail");
  row.kind = kind;
  row.value = value;
  row.constraints = constraints;
  row.enabled = enabled;
  return row;
}
void emitProvidersChanged(QDBusConnection bus = QDBusConnection::sessionBus()) {
  bus.send(QDBusMessage::createSignal(
      QStringLiteral("/org/arinn/Daemon1"), QStringLiteral("org.arinn.Daemon1"),
      QStringLiteral("ProvidersChanged")));
}
void emitStateChanged(const QString &provider, const QString &capability,
                      QDBusConnection bus = QDBusConnection::sessionBus()) {
  auto signal = QDBusMessage::createSignal(QStringLiteral("/org/arinn/Daemon1"),
                                           QStringLiteral("org.arinn.Daemon1"),
                                           QStringLiteral("StateChanged"));
  signal << provider << capability << QStringLiteral("g-") + provider
         << qulonglong(2);
  bus.send(signal);
}
QVariantMap rowAt(const SettingsCatalog &catalog, const QString &provider,
                  const QString &id) {
  for (const auto &value : catalog.rows()) {
    const auto row = value.toMap();
    if (row.value(QStringLiteral("providerId")).toString() == provider &&
        row.value(QStringLiteral("id")).toString() == id)
      return row;
  }
  return {};
}
} // namespace

class SettingsCatalogTest : public QObject {
  Q_OBJECT
private slots:
  void initTestCase() {
    qDBusRegisterMetaType<ProviderWire>();
    qDBusRegisterMetaType<SettingWire>();
    qDBusRegisterMetaType<EntityWire>();
    qDBusRegisterMetaType<DaemonProviderWire>();
    qDBusRegisterMetaType<RootWire>();
  }
  void cleanup() {
    for (auto bus : {QDBusConnection::sessionBus(), ownBus()}) {
      bus.unregisterObject(QStringLiteral("/org/arinn"),
                           QDBusConnection::UnregisterTree);
      bus.unregisterService(QStringLiteral("org.arinn.Daemon1"));
    }
  }
  // Review of #779: a packaged provider's name is translated only for its
  // own id; a third-party provider with the same name keeps it.
  void packagedProviderNamesAreMatchedById() {
    class Translator : public QTranslator {
    public:
      QString translate(const char *context, const char *source, const char *,
                        int) const override {
        if (QByteArray(context) == "SettingsCatalog" && QByteArray(source) == "Playback")
          return QStringLiteral("Wiedergabe");
        return {};
      }
    } translator;
    QCoreApplication::installTranslator(&translator);
    SettingsDaemon daemon;
    daemon.providers = {{QStringLiteral("mpv"), QStringLiteral("Playback")},
                        {QStringLiteral("thirdparty"), QStringLiteral("Playback")}};
    for (const auto &id : {QStringLiteral("mpv"), QStringLiteral("thirdparty")})
      daemon.settings.insert(id, {setting(QStringLiteral("x"), QStringLiteral("playback"),
                                          QStringLiteral("boolean"), QStringLiteral("false"),
                                          QString(), true)});
    QVERIFY(registerBus(daemon));
    SettingsCatalog catalog;
    QTRY_COMPARE(catalog.screenState(), QStringLiteral("ready"));
    QCOMPARE(rowAt(catalog, QStringLiteral("mpv"), QStringLiteral("x"))
                 .value(QStringLiteral("providerName")).toString(),
             QStringLiteral("Wiedergabe"));
    QCOMPARE(rowAt(catalog, QStringLiteral("thirdparty"), QStringLiteral("x"))
                 .value(QStringLiteral("providerName")).toString(),
             QStringLiteral("Playback"));
    QCoreApplication::removeTranslator(&translator);
  }
  // Review of #779: a packaged plugin configured under another instance id
  // keeps its translated name; Daemon1.Providers names the plugin.
  void packagedProviderNamesFollowThePluginId() {
    class Translator : public QTranslator {
    public:
      QString translate(const char *context, const char *source, const char *,
                        int) const override {
        if (QByteArray(context) == "SettingsCatalog" && QByteArray(source) == "Playback")
          return QStringLiteral("Wiedergabe");
        return {};
      }
    } translator;
    QCoreApplication::installTranslator(&translator);
    SettingsDaemon daemon;
    daemon.providers = {{QStringLiteral("living-room-player"), QStringLiteral("Playback")},
                        {QStringLiteral("mpv"), QStringLiteral("Playback")}};
    for (const auto &id : {QStringLiteral("living-room-player"), QStringLiteral("mpv")})
      daemon.settings.insert(id, {setting(QStringLiteral("x"), QStringLiteral("playback"),
                                          QStringLiteral("boolean"), QStringLiteral("false"),
                                          QString(), true)});
    // The instance "living-room-player" runs the packaged mpv plugin; the
    // instance "mpv" runs a third-party plugin.
    daemon.libraryProviders = {
        {QStringLiteral("living-room-player"), QDBusObjectPath(QStringLiteral("/p1")),
         {QStringLiteral("playback")}, QString(), QStringLiteral("mpv")},
        {QStringLiteral("mpv"), QDBusObjectPath(QStringLiteral("/p2")),
         {QStringLiteral("playback")}, QString(), QStringLiteral("vendor-player")}};
    QVERIFY(registerBus(daemon));
    SettingsCatalog catalog;
    QTRY_COMPARE(rowAt(catalog, QStringLiteral("living-room-player"), QStringLiteral("x"))
                     .value(QStringLiteral("providerName")).toString(),
                 QStringLiteral("Wiedergabe"));
    QTRY_COMPARE(rowAt(catalog, QStringLiteral("mpv"), QStringLiteral("x"))
                     .value(QStringLiteral("providerName")).toString(),
                 QStringLiteral("Playback"));
    QCoreApplication::removeTranslator(&translator);
  }
  // Review of #779: a new daemon drops the old plugin identities, also when
  // its own listing fails.
  void pluginIdsAreForgottenWithTheDaemon() {
    class Translator : public QTranslator {
    public:
      QString translate(const char *context, const char *source, const char *,
                        int) const override {
        if (QByteArray(context) == "SettingsCatalog" && QByteArray(source) == "Playback")
          return QStringLiteral("Wiedergabe");
        return {};
      }
    } translator;
    QCoreApplication::installTranslator(&translator);
    const auto instance = QStringLiteral("living-room-player");
    SettingsDaemon first;
    first.ownConnection = true;
    first.providers = {{instance, QStringLiteral("Playback")}};
    first.settings.insert(instance, {setting(QStringLiteral("x"), QStringLiteral("playback"),
                                             QStringLiteral("boolean"), QStringLiteral("false"),
                                             QString(), true)});
    first.libraryProviders = {{instance, QDBusObjectPath(QStringLiteral("/p1")),
                               {QStringLiteral("playback")}, QString(), QStringLiteral("mpv")}};
    QVERIFY(registerBus(first));
    SettingsCatalog catalog;
    QTRY_COMPARE(rowAt(catalog, instance, QStringLiteral("x"))
                     .value(QStringLiteral("providerName")).toString(),
                 QStringLiteral("Wiedergabe"));
    // The next daemon runs a third-party plugin under the same instance id,
    // and its provider listing fails.
    auto own = ownBus();
    own.unregisterObject(QStringLiteral("/org/arinn"), QDBusConnection::UnregisterTree);
    own.unregisterService(QStringLiteral("org.arinn.Daemon1"));
    SettingsDaemon second;
    second.providers = first.providers;
    second.settings = first.settings;
    second.providersFail = true;
    QVERIFY(registerBus(second));
    QTRY_COMPARE(rowAt(catalog, instance, QStringLiteral("x"))
                     .value(QStringLiteral("providerName")).toString(),
                 QStringLiteral("Playback"));
    QCoreApplication::removeTranslator(&translator);
  }
  // Review of #779: a reload under the same daemon whose provider listing
  // fails does not keep the old plugin identity either.
  void pluginIdsAreForgottenWhenARescanFails() {
    class Translator : public QTranslator {
    public:
      QString translate(const char *context, const char *source, const char *,
                        int) const override {
        if (QByteArray(context) == "SettingsCatalog" && QByteArray(source) == "Playback")
          return QStringLiteral("Wiedergabe");
        return {};
      }
    } translator;
    QCoreApplication::installTranslator(&translator);
    const auto instance = QStringLiteral("living-room-player");
    SettingsDaemon daemon;
    daemon.providers = {{instance, QStringLiteral("Playback")}};
    daemon.settings.insert(instance, {setting(QStringLiteral("x"), QStringLiteral("playback"),
                                              QStringLiteral("boolean"), QStringLiteral("false"),
                                              QString(), true)});
    daemon.libraryProviders = {{instance, QDBusObjectPath(QStringLiteral("/p1")),
                                {QStringLiteral("playback")}, QString(), QStringLiteral("mpv")}};
    QVERIFY(registerBus(daemon));
    SettingsCatalog catalog;
    QTRY_COMPARE(rowAt(catalog, instance, QStringLiteral("x"))
                     .value(QStringLiteral("providerName")).toString(),
                 QStringLiteral("Wiedergabe"));
    daemon.providersFail = true;
    emitProvidersChanged();
    QTRY_COMPARE(rowAt(catalog, instance, QStringLiteral("x"))
                     .value(QStringLiteral("providerName")).toString(),
                 QStringLiteral("Playback"));
    QCoreApplication::removeTranslator(&translator);
  }
  // Review of #780: only a library provider's folder list is marked for
  // the library wording; another provider's folder list is not.
  void folderListsOfLibraryProvidersAreMarked() {
    SettingsDaemon daemon;
    daemon.providers = {{QStringLiteral("library"), QStringLiteral("Library")},
                        {QStringLiteral("backup"), QStringLiteral("Backup")}};
    for (const auto &id : {QStringLiteral("library"), QStringLiteral("backup")})
      daemon.settings.insert(id, {setting(QStringLiteral("dirs"), QStringLiteral("media"),
                                          QStringLiteral("path"), QStringLiteral(R"(["/a"])"),
                                          QStringLiteral(R"({"directory":true,"multiple":true})"),
                                          true)});
    daemon.libraryProviders = {
        {QStringLiteral("library"),
         QDBusObjectPath(QStringLiteral("/org/arinn/Daemon1/providers/library")),
         {QStringLiteral("library")}},
        {QStringLiteral("backup"),
         QDBusObjectPath(QStringLiteral("/org/arinn/Daemon1/providers/backup")),
         {QStringLiteral("storage")}}};
    QVERIFY(registerBus(daemon));
    SettingsCatalog catalog;
    QTRY_VERIFY(rowAt(catalog, QStringLiteral("library"), QStringLiteral("dirs"))
                    .value(QStringLiteral("libraryFolders")).toBool());
    QVERIFY(!rowAt(catalog, QStringLiteral("backup"), QStringLiteral("dirs"))
                 .value(QStringLiteral("libraryFolders")).toBool());
  }
  void groupsProvidersByCategory() {
    SettingsDaemon daemon;
    daemon.providers = {{QStringLiteral("beta"), QStringLiteral("Beta")},
                        {QStringLiteral("alpha"), QStringLiteral("Alpha")}};
    daemon.settings.insert(
        QStringLiteral("beta"),
        {setting(QStringLiteral("locked"), QStringLiteral("applications"),
                 QStringLiteral("string"), QStringLiteral("\"fixed\""),
                 QStringLiteral(R"({"max_length":40})"), false),
         setting(QStringLiteral("scan"), QStringLiteral("media"),
                 QStringLiteral("boolean"), QStringLiteral("false"),
                 QString(), true),
         setting(QStringLiteral("other"), QStringLiteral("custom"),
                 QStringLiteral("boolean"), QStringLiteral("true"), QString(),
                 true)});
    daemon.settings.insert(
        QStringLiteral("alpha"),
        {setting(QStringLiteral("roots"), QStringLiteral("media"),
                 QStringLiteral("path"), QStringLiteral(R"(["/videos","/music"])"),
                 QStringLiteral(R"({"directory":true,"multiple":true,"max_items":4})"),
                 true),
         setting(QStringLiteral("output"), QStringLiteral("sound"),
                 QStringLiteral("enum"), QStringLiteral("\"stereo\""),
                 QStringLiteral(
                     R"({"options":[{"value":"stereo","title":"Stereo"},{"value":"surround","title":"Surround"}]})"),
                 true),
         setting(QStringLiteral("idle"), QStringLiteral("system"),
                 QStringLiteral("integer"), QStringLiteral("10"),
                 QStringLiteral(R"({"min":1,"max":60})"), true)});
    QVERIFY(registerBus(daemon));
    SettingsCatalog catalog;
    QTRY_COMPARE(catalog.screenState(), QStringLiteral("ready"));
    // #756: only categories with rows, in the fixed order; Playback,
    // Network and Appearance have none here.
    QStringList shownCategories;
    for (const auto &category : catalog.categories())
      shownCategories.append(category.toMap().value(QStringLiteral("id")).toString());
    QCOMPARE(shownCategories, QStringList({QStringLiteral("media"), QStringLiteral("sound"),
                                           QStringLiteral("applications"),
                                           QStringLiteral("system")}));
    QStringList order;
    for (const auto &value : catalog.rows()) {
      const auto row = value.toMap();
      order.append(row.value(QStringLiteral("categoryId")).toString() +
                   QLatin1Char('/') +
                   row.value(QStringLiteral("providerId")).toString() +
                   QLatin1Char('/') + row.value(QStringLiteral("id")).toString());
    }
    QCOMPARE(order,
             QStringList({QStringLiteral("media/beta/scan"),
                          QStringLiteral("media/alpha/roots"),
                          QStringLiteral("sound/alpha/output"),
                          QStringLiteral("applications/beta/locked"),
                          QStringLiteral("system/alpha/idle")}));
    const auto roots = rowAt(catalog, QStringLiteral("alpha"), QStringLiteral("roots"));
    QCOMPARE(roots.value(QStringLiteral("currentValue")).toStringList(),
             QStringList({QStringLiteral("/videos"), QStringLiteral("/music")}));
    QCOMPARE(roots.value(QStringLiteral("constraints")).toMap().value(QStringLiteral("multiple")).toBool(),
             true);
    QCOMPARE(roots.value(QStringLiteral("providerName")).toString(),
             QStringLiteral("Alpha"));

    const auto output = rowAt(catalog, QStringLiteral("alpha"), QStringLiteral("output"));
    QCOMPARE(output.value(QStringLiteral("currentValue")).toString(),
             QStringLiteral("stereo"));
    QCOMPARE(output.value(QStringLiteral("constraints"))
                 .toMap()
                 .value(QStringLiteral("options"))
                 .toList()
                 .size(),
             2);
    const auto idle = rowAt(catalog, QStringLiteral("alpha"), QStringLiteral("idle"));
    QCOMPARE(idle.value(QStringLiteral("currentValue")).toLongLong(), 10);
    QCOMPARE(idle.value(QStringLiteral("constraints")).toMap().value(QStringLiteral("max")).toInt(),
             60);
    const auto locked =
        rowAt(catalog, QStringLiteral("beta"), QStringLiteral("locked"));
    QCOMPARE(locked.value(QStringLiteral("enabled")).toBool(), false);
    QCOMPARE(locked.value(QStringLiteral("currentValue")).toString(),
             QStringLiteral("fixed"));
  }
  void setValuePendingThenApplied() {
    SettingsDaemon daemon;
    daemon.providers = {{QStringLiteral("alpha"), QStringLiteral("Alpha")}};
    daemon.settings.insert(
        QStringLiteral("alpha"),
        {setting(QStringLiteral("scan"), QStringLiteral("media"),
                 QStringLiteral("boolean"), QStringLiteral("false"), QString(),
                 true)});
    daemon.holdOperation = true;
    QVERIFY(registerBus(daemon));
    SettingsCatalog catalog;
    QTRY_COMPARE(catalog.screenState(), QStringLiteral("ready"));
    catalog.setValue(QStringLiteral("alpha"), QStringLiteral("scan"), true);
    QTRY_COMPARE(rowAt(catalog, QStringLiteral("alpha"), QStringLiteral("scan"))
                     .value(QStringLiteral("pending"))
                     .toBool(),
                 true);
    QTRY_COMPARE(daemon.sets.size(), 1);
    QCOMPARE(daemon.sets.at(0).provider, QStringLiteral("alpha"));
    QCOMPARE(daemon.sets.at(0).generation, QStringLiteral("g-alpha"));
    QCOMPARE(daemon.sets.at(0).setting, QStringLiteral("scan"));
    QCOMPARE(daemon.sets.at(0).value, QStringLiteral("true"));
    QVERIFY(!daemon.sets.at(0).request.isEmpty());
    daemon.holdOperation = false;
    QTRY_COMPARE(rowAt(catalog, QStringLiteral("alpha"), QStringLiteral("scan"))
                     .value(QStringLiteral("pending"))
                     .toBool(),
                 false);
    QTRY_COMPARE(rowAt(catalog, QStringLiteral("alpha"), QStringLiteral("scan"))
                     .value(QStringLiteral("currentValue"))
                     .toBool(),
                 true);
    QCOMPARE(rowAt(catalog, QStringLiteral("alpha"), QStringLiteral("scan"))
                 .value(QStringLiteral("error"))
                 .toString(),
             QString());
  }
  void secondSetValueWhilePendingIsIgnored() {
    SettingsDaemon daemon;
    daemon.providers = {{QStringLiteral("alpha"), QStringLiteral("Alpha")}};
    daemon.settings.insert(
        QStringLiteral("alpha"),
        {setting(QStringLiteral("scan"), QStringLiteral("media"),
                 QStringLiteral("boolean"), QStringLiteral("false"), QString(),
                 true)});
    daemon.holdOperation = true;
    QVERIFY(registerBus(daemon));
    SettingsCatalog catalog;
    QTRY_COMPARE(catalog.screenState(), QStringLiteral("ready"));
    catalog.setValue(QStringLiteral("alpha"), QStringLiteral("scan"), true);
    QTRY_COMPARE(daemon.sets.size(), 1);
    QTRY_COMPARE(rowAt(catalog, QStringLiteral("alpha"), QStringLiteral("scan"))
                     .value(QStringLiteral("pending"))
                     .toBool(),
                 true);
    QTRY_VERIFY(daemon.operationReads > 0);
    catalog.setValue(QStringLiteral("alpha"), QStringLiteral("scan"), false);
    QTest::qWait(200);
    QCOMPARE(daemon.sets.size(), 1);
    QCOMPARE(daemon.sets.at(0).value, QStringLiteral("true"));
  }
  void setValueFailureKeepsTheValue() {
    SettingsDaemon daemon;
    daemon.providers = {{QStringLiteral("alpha"), QStringLiteral("Alpha")}};
    daemon.settings.insert(
        QStringLiteral("alpha"),
        {setting(QStringLiteral("idle"), QStringLiteral("system"),
                 QStringLiteral("integer"), QStringLiteral("10"),
                 QStringLiteral(R"({"min":1,"max":60})"), true)});
    daemon.operationState = QStringLiteral("failed");
    daemon.operationError = QStringLiteral("InvalidArgument");
    QVERIFY(registerBus(daemon));
    SettingsCatalog catalog;
    QTRY_COMPARE(catalog.screenState(), QStringLiteral("ready"));
    catalog.setValue(QStringLiteral("alpha"), QStringLiteral("idle"), 11);
    QTRY_COMPARE(rowAt(catalog, QStringLiteral("alpha"), QStringLiteral("idle"))
                     .value(QStringLiteral("error"))
                     .toString(),
                 QStringLiteral("InvalidArgument"));
    QCOMPARE(rowAt(catalog, QStringLiteral("alpha"), QStringLiteral("idle"))
                 .value(QStringLiteral("pending"))
                 .toBool(),
             false);
    QCOMPARE(rowAt(catalog, QStringLiteral("alpha"), QStringLiteral("idle"))
                 .value(QStringLiteral("currentValue"))
                 .toLongLong(),
             10);
    QCOMPARE(daemon.sets.at(0).value, QStringLiteral("11"));
  }
  void disabledSettingIsNotWritten() {
    SettingsDaemon daemon;
    daemon.providers = {{QStringLiteral("alpha"), QStringLiteral("Alpha")}};
    daemon.settings.insert(
        QStringLiteral("alpha"),
        {setting(QStringLiteral("locked"), QStringLiteral("system"),
                 QStringLiteral("string"), QStringLiteral("\"fixed\""),
                 QStringLiteral(R"({"max_length":40})"), false)});
    QVERIFY(registerBus(daemon));
    SettingsCatalog catalog;
    QTRY_COMPARE(catalog.screenState(), QStringLiteral("ready"));
    catalog.setValue(QStringLiteral("alpha"), QStringLiteral("locked"),
                     QStringLiteral("other"));
    QTest::qWait(200);
    QCOMPARE(daemon.sets.size(), 0);
    QCOMPARE(rowAt(catalog, QStringLiteral("alpha"), QStringLiteral("locked"))
                 .value(QStringLiteral("enabled"))
                 .toBool(),
             false);
  }
  void setValueEncodesStringAndPathList() {
    SettingsDaemon daemon;
    daemon.providers = {{QStringLiteral("alpha"), QStringLiteral("Alpha")}};
    daemon.settings.insert(
        QStringLiteral("alpha"),
        {setting(QStringLiteral("label"), QStringLiteral("applications"),
                 QStringLiteral("string"), QStringLiteral("\"\""),
                 QStringLiteral(R"({"max_length":40})"), true),
         setting(QStringLiteral("roots"), QStringLiteral("media"),
                 QStringLiteral("path"), QStringLiteral("[]"),
                 QStringLiteral(R"({"multiple":true,"max_items":4})"), true)});
    QVERIFY(registerBus(daemon));
    SettingsCatalog catalog;
    QTRY_COMPARE(catalog.screenState(), QStringLiteral("ready"));
    catalog.setValue(QStringLiteral("alpha"), QStringLiteral("label"),
                     QStringLiteral("a\"b"));
    catalog.setValue(QStringLiteral("alpha"), QStringLiteral("roots"),
                     QStringList({QStringLiteral("/videos"), QStringLiteral("/music")}));
    QTRY_COMPARE(daemon.sets.size(), 2);
    QCOMPARE(daemon.sets.at(0).value, QStringLiteral("\"a\\\"b\""));
    QCOMPARE(daemon.sets.at(1).value, QStringLiteral("[\"/videos\",\"/music\"]"));
  }
  // #842: the music sources turned off in the Sources sheet, a JSON array of
  // provider ids in the daemon's `music_disabled_sources`. A change shows at
  // once; one made while a write is in flight is written after it, and a
  // written list is not sent twice. The Settings screen leaves the row out.
  void musicSourcesTurnedOffAreWrittenInTurn() {
    SettingsDaemon daemon;
    daemon.providers = {{QStringLiteral("arinn"), QStringLiteral("Arinn")}};
    daemon.settings.insert(
        QStringLiteral("arinn"),
        {setting(QStringLiteral("music_disabled_sources"), QStringLiteral("media"),
                 QStringLiteral("string"), QStringLiteral("\"[\\\"a\\\"]\""),
                 QStringLiteral("{\"max_length\":4096}"), true),
         setting(QStringLiteral("light_field"), QStringLiteral("appearance"),
                 QStringLiteral("boolean"), QStringLiteral("true"),
                 QStringLiteral("{}"), true)});
    daemon.holdOperation = true;
    // Codex review of #851: before the daemon answers, a change could not be
    // stored and would overwrite a list not read yet: nothing changes.
    {
      SettingsCatalog early;
      QVERIFY(!early.musicSourcesWritable());
      early.setMusicSourceEnabled(QStringLiteral("b"), false);
      QVERIFY(early.musicDisabledSources().isEmpty());
    }
    QVERIFY(registerBus(daemon));
    SettingsCatalog catalog;
    QTRY_COMPARE(catalog.screenState(), QStringLiteral("ready"));
    QTRY_COMPARE(catalog.musicDisabledSources(), QStringList{QStringLiteral("a")});
    QVERIFY(catalog.musicSourcesWritable());
    for (const auto &value : catalog.rows())
      QVERIFY(value.toMap().value(QStringLiteral("id")).toString() !=
              QStringLiteral("music_disabled_sources"));
    QVERIFY(!rowAt(catalog, QStringLiteral("arinn"), QStringLiteral("light_field")).isEmpty());
    catalog.setMusicSourceEnabled(QStringLiteral("b"), false);
    QCOMPARE(catalog.musicDisabledSources(), (QStringList{QStringLiteral("a"), QStringLiteral("b")}));
    QTRY_COMPARE(daemon.sets.size(), 1);
    QCOMPARE(daemon.sets.at(0).setting, QStringLiteral("music_disabled_sources"));
    QCOMPARE(daemon.sets.at(0).value, QStringLiteral("\"[\\\"a\\\",\\\"b\\\"]\""));
    // A second change while the first is in flight shows at once and is
    // written when the first ends.
    catalog.setMusicSourceEnabled(QStringLiteral("a"), true);
    QCOMPARE(catalog.musicDisabledSources(), QStringList{QStringLiteral("b")});
    QTest::qWait(200);
    QCOMPARE(daemon.sets.size(), 1);
    daemon.holdOperation = false;
    QTRY_COMPARE(daemon.sets.size(), 2);
    QCOMPARE(daemon.sets.at(1).value, QStringLiteral("\"[\\\"b\\\"]\""));
    QTRY_COMPARE(catalog.musicDisabledSources(), QStringList{QStringLiteral("b")});
    QTest::qWait(500);
    QCOMPARE(daemon.sets.size(), 2);
    // Every source on again: an empty string.
    catalog.setMusicSourceEnabled(QStringLiteral("b"), true);
    QTRY_COMPARE(daemon.sets.size(), 3);
    QCOMPARE(daemon.sets.at(2).value, QStringLiteral("\"\""));
    QTRY_VERIFY(catalog.musicDisabledSources().isEmpty());
  }
  // Codex review of #851: a toggle reversed while its write is in flight is
  // written once the stored list shows the first write, not dropped against
  // the list from before it.
  void aReversedToggleIsWrittenAfterTheFirst() {
    SettingsDaemon daemon;
    daemon.providers = {{QStringLiteral("arinn"), QStringLiteral("Arinn")}};
    daemon.settings.insert(
        QStringLiteral("arinn"),
        {setting(QStringLiteral("music_disabled_sources"), QStringLiteral("media"),
                 QStringLiteral("string"), QStringLiteral("\"\""),
                 QStringLiteral("{\"max_length\":4096}"), true)});
    daemon.holdOperation = true;
    QVERIFY(registerBus(daemon));
    SettingsCatalog catalog;
    QTRY_COMPARE(catalog.screenState(), QStringLiteral("ready"));
    catalog.setMusicSourceEnabled(QStringLiteral("b"), false);
    QTRY_COMPARE(daemon.sets.size(), 1);
    catalog.setMusicSourceEnabled(QStringLiteral("b"), true);
    QVERIFY(catalog.musicDisabledSources().isEmpty());
    daemon.holdOperation = false;
    QTRY_COMPARE(daemon.sets.size(), 2);
    QCOMPARE(daemon.sets.at(1).value, QStringLiteral("\"\""));
    QTRY_VERIFY(catalog.musicDisabledSources().isEmpty());
    QTest::qWait(500);
    QCOMPARE(daemon.sets.size(), 2);
    QVERIFY(catalog.musicDisabledSources().isEmpty());
  }
  // Codex review of #851: a daemon that changes owner during a write: the
  // change stays shown and is written to the new daemon once it has loaded.
  void musicSourcesAreWrittenAgainAfterADaemonRestart() {
    const auto rows = QList<SettingWire>{
        setting(QStringLiteral("music_disabled_sources"), QStringLiteral("media"),
                QStringLiteral("string"), QStringLiteral("\"\""),
                QStringLiteral("{\"max_length\":4096}"), true)};
    SettingsDaemon first;
    first.ownConnection = true;
    first.providers = {{QStringLiteral("arinn"), QStringLiteral("Arinn")}};
    first.settings.insert(QStringLiteral("arinn"), rows);
    first.holdOperation = true;
    QVERIFY(registerBus(first));
    SettingsCatalog catalog;
    QTRY_COMPARE(catalog.screenState(), QStringLiteral("ready"));
    catalog.setMusicSourceEnabled(QStringLiteral("b"), false);
    QTRY_COMPARE(first.sets.size(), 1);
    // The daemon goes away mid-write and another one takes the name.
    auto own = ownBus();
    own.unregisterObject(QStringLiteral("/org/arinn"), QDBusConnection::UnregisterTree);
    own.unregisterService(QStringLiteral("org.arinn.Daemon1"));
    SettingsDaemon second;
    second.providers = first.providers;
    second.settings.insert(QStringLiteral("arinn"), rows);
    QVERIFY(registerBus(second));
    QTRY_COMPARE(second.sets.size(), 1);
    QCOMPARE(second.sets.at(0).value, QStringLiteral("\"[\\\"b\\\"]\""));
    QCOMPARE(catalog.musicDisabledSources(), QStringList{QStringLiteral("b")});
    QTRY_COMPARE(catalog.musicDisabledSources(), QStringList{QStringLiteral("b")});
  }
  // Codex review of #851: a toggle reversed during a write survives a
  // daemon change after the first write was stored: the new daemon's list is
  // read first, and the reversal is written to it.
  void aReversalSurvivesADaemonRestart() {
    const auto rows = [](const QString &value) {
      return QList<SettingWire>{
          setting(QStringLiteral("music_disabled_sources"), QStringLiteral("media"),
                  QStringLiteral("string"), value,
                  QStringLiteral("{\"max_length\":4096}"), true)};
    };
    SettingsDaemon first;
    first.ownConnection = true;
    first.providers = {{QStringLiteral("arinn"), QStringLiteral("Arinn")}};
    first.settings.insert(QStringLiteral("arinn"), rows(QStringLiteral("\"\"")));
    first.holdOperation = true;
    QVERIFY(registerBus(first));
    SettingsCatalog catalog;
    QTRY_COMPARE(catalog.screenState(), QStringLiteral("ready"));
    catalog.setMusicSourceEnabled(QStringLiteral("b"), false);
    QTRY_COMPARE(first.sets.size(), 1);
    catalog.setMusicSourceEnabled(QStringLiteral("b"), true);
    // The first write was stored; then the daemon is replaced.
    auto own = ownBus();
    own.unregisterObject(QStringLiteral("/org/arinn"), QDBusConnection::UnregisterTree);
    own.unregisterService(QStringLiteral("org.arinn.Daemon1"));
    SettingsDaemon second;
    second.providers = first.providers;
    second.settings.insert(QStringLiteral("arinn"),
                           rows(QStringLiteral("\"[\\\"b\\\"]\"")));
    QVERIFY(registerBus(second));
    QTRY_COMPARE(second.sets.size(), 1);
    QCOMPARE(second.sets.at(0).value, QStringLiteral("\"\""));
    QTRY_VERIFY(catalog.musicDisabledSources().isEmpty());
  }
  // Codex review of #851: a daemon that does not list the setting cannot
  // store a turned-off source, so nothing changes and nothing is written.
  void musicSourcesNeedTheCoreSetting() {
    SettingsDaemon daemon;
    daemon.providers = {{QStringLiteral("arinn"), QStringLiteral("Arinn")}};
    daemon.settings.insert(
        QStringLiteral("arinn"),
        {setting(QStringLiteral("light_field"), QStringLiteral("appearance"),
                 QStringLiteral("boolean"), QStringLiteral("true"),
                 QStringLiteral("{}"), true)});
    QVERIFY(registerBus(daemon));
    SettingsCatalog catalog;
    QTRY_COMPARE(catalog.screenState(), QStringLiteral("ready"));
    QVERIFY(!catalog.musicSourcesWritable());
    catalog.setMusicSourceEnabled(QStringLiteral("b"), false);
    QVERIFY(catalog.musicDisabledSources().isEmpty());
    QTest::qWait(200);
    QVERIFY(daemon.sets.isEmpty());
  }
  // ADR-0112: the light field and the ambient timeout come from the daemon's
  // own settings; the same ids from another provider do not count.
  void shellPreferencesFromTheCoreSource() {
    SettingsDaemon daemon;
    // `alpha` comes after `arinn`: without the provider filter its values
    // would be read last and win.
    daemon.providers = {{QStringLiteral("arinn"), QStringLiteral("Arinn")},
                        {QStringLiteral("alpha"), QStringLiteral("Alpha")}};
    const auto timeouts = QStringLiteral(
        R"({"options":[{"value":"5","title":"5 min"},{"value":"10","title":"10 min"},{"value":"15","title":"15 min"},{"value":"30","title":"30 min"},{"value":"60","title":"60 min"}]})");
    const auto soundLevels = QStringLiteral(
        R"({"options":[{"value":"off","title":"Off"},{"value":"quiet","title":"Quiet"},{"value":"normal","title":"Normal"}]})");
    daemon.settings.insert(
        QStringLiteral("alpha"),
        {setting(QStringLiteral("light_field"), QStringLiteral("appearance"),
                 QStringLiteral("boolean"), QStringLiteral("true"), QString(),
                 true),
         setting(QStringLiteral("ambient_timeout"), QStringLiteral("appearance"),
                 QStringLiteral("enum"), QStringLiteral("\"60\""), timeouts,
                 true)});
    daemon.settings.insert(
        QStringLiteral("arinn"),
        {setting(QStringLiteral("light_field"), QStringLiteral("appearance"),
                 QStringLiteral("boolean"), QStringLiteral("false"),
                 QStringLiteral("{}"), true),
         setting(QStringLiteral("ambient_timeout"), QStringLiteral("appearance"),
                 QStringLiteral("enum"), QStringLiteral("\"30\""), timeouts,
                 true),
         setting(QStringLiteral("interface_sounds"), QStringLiteral("sound"),
                 QStringLiteral("enum"), QStringLiteral("\"normal\""),
                 soundLevels, true)});
    SettingsCatalog catalog;
    // Defaults until the daemon has answered.
    QCOMPARE(catalog.lightField(), true);
    QCOMPARE(catalog.ambientTimeoutMinutes(), 10);
    QVERIFY(!catalog.interfaceSoundsLevelKnown());
    QVERIFY(catalog.interfaceSoundsLevel().isEmpty());
    QSignalSpy changes(&catalog, &SettingsCatalog::preferencesChanged);
    QVERIFY(registerBus(daemon));
    catalog.refresh();
    QTRY_COMPARE(catalog.screenState(), QStringLiteral("ready"));
    QTRY_COMPARE(catalog.lightField(), false);
    QCOMPARE(catalog.ambientTimeoutMinutes(), 30);
    QVERIFY(catalog.interfaceSoundsLevelKnown());
    QCOMPARE(catalog.interfaceSoundsLevel(), QStringLiteral("normal"));
    const auto row = rowAt(catalog, QStringLiteral("arinn"),
                           QStringLiteral("ambient_timeout"));
    QCOMPARE(row.value(QStringLiteral("constraints")).toMap()
                 .value(QStringLiteral("options")).toList().size(), 5);
    // #756 B16: Arinn's own settings carry no source line; a provider's do.
    QCOMPARE(row.value(QStringLiteral("providerName")).toString(), QString());
    QCOMPARE(rowAt(catalog, QStringLiteral("alpha"), QStringLiteral("light_field"))
                 .value(QStringLiteral("providerName")).toString(),
             QStringLiteral("Alpha"));
    // A value the daemon does not offer keeps the last one.
    daemon.settings[QStringLiteral("arinn")][1].value = QStringLiteral("\"7\"");
    auto signal = QDBusMessage::createSignal(
        QStringLiteral("/org/arinn/Daemon1"), QStringLiteral("org.arinn.Settings1"),
        QStringLiteral("SettingsChanged"));
    signal << QStringLiteral("arinn");
    QVERIFY(QDBusConnection::sessionBus().send(signal));
    QTest::qWait(300);
    QCOMPARE(catalog.ambientTimeoutMinutes(), 30);
    daemon.settings[QStringLiteral("arinn")][0].value = QStringLiteral("true");
    daemon.settings[QStringLiteral("arinn")][1].value = QStringLiteral("\"15\"");
    const int before = changes.count();
    QVERIFY(QDBusConnection::sessionBus().send(signal));
    QTRY_COMPARE(catalog.ambientTimeoutMinutes(), 15);
    QCOMPARE(catalog.lightField(), true);
    QVERIFY(changes.count() > before);
  }
  // ADR-0136: Light follows the time of day comes from the `arinn` source.
  // The same id on another provider does not count, and a missing key keeps
  // the default.
  void lightFollowsTimeComesFromTheCoreSource() {
    SettingsDaemon daemon;
    daemon.providers = {{QStringLiteral("arinn"), QStringLiteral("Arinn")},
                        {QStringLiteral("alpha"), QStringLiteral("Alpha")}};
    daemon.settings.insert(
        QStringLiteral("alpha"),
        {setting(QStringLiteral("light_follows_time"), QStringLiteral("appearance"),
                 QStringLiteral("boolean"), QStringLiteral("false"), QString(),
                 true)});
    auto core = setting(QStringLiteral("light_follows_time"), QStringLiteral("appearance"),
                        QStringLiteral("boolean"), QStringLiteral("false"),
                        QStringLiteral("{}"), true);
    core.title = QStringLiteral("Light follows the time of day");
    core.description = QStringLiteral(
        "In the evening and at night the glow is dimmer and the screen a little warmer. Off keeps the daytime look");
    daemon.settings.insert(QStringLiteral("arinn"), {core});
    SettingsCatalog catalog;
    QCOMPARE(catalog.lightFollowsTime(), true);
    QVERIFY(registerBus(daemon));
    catalog.refresh();
    QTRY_COMPARE(catalog.screenState(), QStringLiteral("ready"));
    QTRY_COMPARE(catalog.lightFollowsTime(), false);
    const auto row = rowAt(catalog, QStringLiteral("arinn"),
                           QStringLiteral("light_follows_time"));
    QCOMPARE(row.value(QStringLiteral("title")).toString(),
             QStringLiteral("Light follows the time of day"));
    QCOMPARE(row.value(QStringLiteral("categoryId")).toString(),
             QStringLiteral("appearance"));
  }
  void settingsChangedRefreshes() {
    SettingsDaemon daemon;
    daemon.providers = {{QStringLiteral("alpha"), QStringLiteral("Alpha")}};
    daemon.settings.insert(
        QStringLiteral("alpha"),
        {setting(QStringLiteral("scan"), QStringLiteral("media"),
                 QStringLiteral("boolean"), QStringLiteral("false"), QString(),
                 true)});
    QVERIFY(registerBus(daemon));
    SettingsCatalog catalog;
    QTRY_COMPARE(catalog.screenState(), QStringLiteral("ready"));
    daemon.settings[QStringLiteral("alpha")][0].value = QStringLiteral("true");
    auto signal = QDBusMessage::createSignal(
        QStringLiteral("/org/arinn/Daemon1"), QStringLiteral("org.arinn.Settings1"),
        QStringLiteral("SettingsChanged"));
    signal << QStringLiteral("alpha");
    QVERIFY(QDBusConnection::sessionBus().send(signal));
    QTRY_COMPARE(rowAt(catalog, QStringLiteral("alpha"), QStringLiteral("scan"))
                     .value(QStringLiteral("currentValue"))
                     .toBool(),
                 true);
  }
  // ADR-0129: provider texts in the shell's language, field by field, from
  // the full locale, then the language, else the source text.
  void providerTextsFollowTheLocale() {
    SettingsDaemon daemon;
    daemon.providers = {{QStringLiteral("alpha"), QStringLiteral("Alpha")},
                        {QStringLiteral("beta"), QStringLiteral("Beta")}};
    daemon.settings.insert(
        QStringLiteral("alpha"),
        {setting(QStringLiteral("decoding"), QStringLiteral("playback"),
                 QStringLiteral("enum"), QStringLiteral("\"no\""),
                 QStringLiteral(
                     R"({"options":[{"value":"auto","title":"Automatic"},{"value":"no","title":"Software"}]})"),
                 true),
         setting(QStringLiteral("delay"), QStringLiteral("playback"),
                 QStringLiteral("integer"), QStringLiteral("5"),
                 QStringLiteral(R"({"min":0,"max":60,"unit":"s"})"), true),
         setting(QStringLiteral("scan"), QStringLiteral("media"),
                 QStringLiteral("boolean"), QStringLiteral("true"), QString(),
                 true)});
    daemon.settings.insert(
        QStringLiteral("beta"),
        {setting(QStringLiteral("other"), QStringLiteral("media"),
                 QStringLiteral("boolean"), QStringLiteral("true"), QString(),
                 true)});
    daemon.translations.insert(
        QStringLiteral("alpha"),
        QStringLiteral(
            R"({"ru":{"decoding":{"title":"Декодирование","options":{"auto":"Автоматически"}},"delay":{"unit":"с","description":""}},"ru_RU":{"scan":{"title":"Сканировать","description":"Искать новые файлы"}}})"));
    QVERIFY(registerBus(daemon));
    const auto previous = QLocale();
    const auto options = [](const QVariantMap &row) {
      QStringList titles;
      for (const auto &option : row.value(QStringLiteral("constraints"))
                                    .toMap()
                                    .value(QStringLiteral("options"))
                                    .toList())
        titles.append(option.toMap().value(QStringLiteral("title")).toString());
      return titles;
    };
    QLocale::setDefault(QLocale(QStringLiteral("ru_RU")));
    {
      SettingsCatalog catalog;
      QTRY_COMPARE(catalog.screenState(), QStringLiteral("ready"));
      const auto decoding =
          rowAt(catalog, QStringLiteral("alpha"), QStringLiteral("decoding"));
      QCOMPARE(decoding.value(QStringLiteral("title")).toString(),
               QStringLiteral("Декодирование"));
      // No translated description: the source text.
      QCOMPARE(decoding.value(QStringLiteral("description")).toString(),
               QStringLiteral("detail"));
      QCOMPARE(options(decoding),
               QStringList({QStringLiteral("Автоматически"),
                            QStringLiteral("Software")}));
      QCOMPARE(rowAt(catalog, QStringLiteral("alpha"), QStringLiteral("delay"))
                   .value(QStringLiteral("constraints"))
                   .toMap()
                   .value(QStringLiteral("unit"))
                   .toString(),
               QStringLiteral("с"));
      // A description translated to an empty text stays empty.
      QCOMPARE(rowAt(catalog, QStringLiteral("alpha"), QStringLiteral("delay"))
                   .value(QStringLiteral("description"))
                   .toString(),
               QString());
      const auto scan = rowAt(catalog, QStringLiteral("alpha"), QStringLiteral("scan"));
      QCOMPARE(scan.value(QStringLiteral("title")).toString(),
               QStringLiteral("Сканировать"));
      QCOMPARE(scan.value(QStringLiteral("description")).toString(),
               QStringLiteral("Искать новые файлы"));
      // A provider without translations keeps its texts.
      QCOMPARE(rowAt(catalog, QStringLiteral("beta"), QStringLiteral("other"))
                   .value(QStringLiteral("title"))
                   .toString(),
               QStringLiteral("other"));
    }
    QLocale::setDefault(QLocale(QStringLiteral("en_US")));
    {
      SettingsCatalog catalog;
      QTRY_COMPARE(catalog.screenState(), QStringLiteral("ready"));
      const auto decoding =
          rowAt(catalog, QStringLiteral("alpha"), QStringLiteral("decoding"));
      QCOMPARE(decoding.value(QStringLiteral("title")).toString(),
               QStringLiteral("decoding"));
      QCOMPARE(options(decoding),
               QStringList({QStringLiteral("Automatic"), QStringLiteral("Software")}));
      QCOMPARE(rowAt(catalog, QStringLiteral("alpha"), QStringLiteral("delay"))
                   .value(QStringLiteral("constraints"))
                   .toMap()
                   .value(QStringLiteral("unit"))
                   .toString(),
               QStringLiteral("s"));
    }
    QLocale::setDefault(previous);
  }
  void unchangedReloadDoesNotNotify() {
    // Every ProvidersChanged reloads the catalog. An unchanged reload must not
    // announce catalogChanged, or the open screen rebuilds its rows (#441).
    SettingsDaemon daemon;
    daemon.providers = {{QStringLiteral("alpha"), QStringLiteral("Alpha")}};
    daemon.settings.insert(
        QStringLiteral("alpha"),
        {setting(QStringLiteral("scan"), QStringLiteral("media"),
                 QStringLiteral("boolean"), QStringLiteral("false"), QString(),
                 true)});
    QVERIFY(registerBus(daemon));
    SettingsCatalog catalog;
    QTRY_COMPARE(catalog.screenState(), QStringLiteral("ready"));
    QTest::qWait(50);
    QSignalSpy changes(&catalog, &SettingsCatalog::catalogChanged);
    const auto providersChanged = [] {
      return QDBusConnection::sessionBus().send(QDBusMessage::createSignal(
          QStringLiteral("/org/arinn/Daemon1"), QStringLiteral("org.arinn.Daemon1"),
          QStringLiteral("ProvidersChanged")));
    };
    for (int round = 0; round < 3; ++round) {
      const auto reads = daemon.settingsReads;
      QVERIFY(providersChanged());
      QTRY_VERIFY(daemon.settingsReads > reads);
      QTest::qWait(50);
    }
    QCOMPARE(changes.count(), 0);
    daemon.settings[QStringLiteral("alpha")][0].value = QStringLiteral("true");
    QVERIFY(providersChanged());
    QTRY_COMPARE(changes.count(), 1);
    QCOMPARE(rowAt(catalog, QStringLiteral("alpha"), QStringLiteral("scan"))
                 .value(QStringLiteral("currentValue"))
                 .toBool(),
             true);
  }
  void eachKindAndSavingOutcome() {
    SettingsDaemon daemon;
    daemon.providers = {{QStringLiteral("alpha"), QStringLiteral("Alpha")}};
    daemon.settings.insert(
        QStringLiteral("alpha"),
        {setting(QStringLiteral("scan"), QStringLiteral("media"),
                 QStringLiteral("boolean"), QStringLiteral("false"), QString(),
                 true),
         setting(QStringLiteral("output"), QStringLiteral("sound"),
                 QStringLiteral("enum"), QStringLiteral("\"stereo\""),
                 QStringLiteral(
                     R"({"options":[{"value":"stereo","title":"Stereo"},{"value":"surround","title":"Surround"}]})"),
                 true),
         setting(QStringLiteral("idle"), QStringLiteral("system"),
                 QStringLiteral("integer"), QStringLiteral("10"),
                 QStringLiteral(R"({"min":1,"max":60,"unit":"min","step":5})"),
                 true),
         setting(QStringLiteral("label"), QStringLiteral("applications"),
                 QStringLiteral("string"), QStringLiteral("\"\""),
                 QStringLiteral(R"({"max_length":40})"), true)});
    QVERIFY(registerBus(daemon));
    SettingsCatalog catalog;
    QTRY_COMPARE(catalog.screenState(), QStringLiteral("ready"));
    const auto idle = rowAt(catalog, QStringLiteral("alpha"), QStringLiteral("idle"));
    QCOMPARE(idle.value(QStringLiteral("constraints")).toMap().value(QStringLiteral("unit")).toString(),
             QStringLiteral("min"));
    QCOMPARE(idle.value(QStringLiteral("constraints")).toMap().value(QStringLiteral("step")).toInt(),
             5);
    QCOMPARE(idle.value(QStringLiteral("deferred")).toBool(), false);

    catalog.setValue(QStringLiteral("alpha"), QStringLiteral("output"),
                     QStringLiteral("surround"));
    QTRY_COMPARE(daemon.sets.size(), 1);
    QCOMPARE(daemon.sets.at(0).value, QStringLiteral("\"surround\""));
    QTRY_COMPARE(rowAt(catalog, QStringLiteral("alpha"), QStringLiteral("output"))
                     .value(QStringLiteral("pending"))
                     .toBool(),
                 false);
    QCOMPARE(rowAt(catalog, QStringLiteral("alpha"), QStringLiteral("output"))
                 .value(QStringLiteral("error"))
                 .toString(),
             QString());
    QCOMPARE(rowAt(catalog, QStringLiteral("alpha"), QStringLiteral("output"))
                 .value(QStringLiteral("deferred"))
                 .toBool(),
             false);

    daemon.operationError = QStringLiteral("Unsupported");
    catalog.setValue(QStringLiteral("alpha"), QStringLiteral("idle"), 15);
    QTRY_COMPARE(rowAt(catalog, QStringLiteral("alpha"), QStringLiteral("idle"))
                     .value(QStringLiteral("deferred"))
                     .toBool(),
                 true);
    QCOMPARE(rowAt(catalog, QStringLiteral("alpha"), QStringLiteral("idle"))
                 .value(QStringLiteral("error"))
                 .toString(),
             QString());
    QCOMPARE(rowAt(catalog, QStringLiteral("alpha"), QStringLiteral("idle"))
                 .value(QStringLiteral("currentValue"))
                 .toLongLong(),
             15);
    QCOMPARE(daemon.sets.at(1).value, QStringLiteral("15"));
    daemon.generations.insert(QStringLiteral("alpha"), QStringLiteral("g-next"));
    auto signal = QDBusMessage::createSignal(
        QStringLiteral("/org/arinn/Daemon1"), QStringLiteral("org.arinn.Settings1"),
        QStringLiteral("SettingsChanged"));
    signal << QStringLiteral("alpha");
    QVERIFY(QDBusConnection::sessionBus().send(signal));
    QTRY_COMPARE(rowAt(catalog, QStringLiteral("alpha"), QStringLiteral("idle"))
                     .value(QStringLiteral("deferred"))
                     .toBool(),
                 false);

    daemon.operationState = QStringLiteral("failed");
    daemon.operationError = QStringLiteral("HDMI 2 is not connected.");
    catalog.setValue(QStringLiteral("alpha"), QStringLiteral("label"),
                     QStringLiteral("room"));
    QTRY_COMPARE(rowAt(catalog, QStringLiteral("alpha"), QStringLiteral("label"))
                     .value(QStringLiteral("error"))
                     .toString(),
                 QStringLiteral("HDMI 2 is not connected."));
    QCOMPARE(rowAt(catalog, QStringLiteral("alpha"), QStringLiteral("label"))
                 .value(QStringLiteral("currentValue"))
                 .toString(),
             QString());
    catalog.dismissError(QStringLiteral("alpha"), QStringLiteral("label"));
    QTRY_COMPARE(rowAt(catalog, QStringLiteral("alpha"), QStringLiteral("label"))
                     .value(QStringLiteral("error"))
                     .toString(),
                 QString());

    daemon.operationState = QStringLiteral("succeeded");
    daemon.operationError.clear();
    catalog.setValue(QStringLiteral("alpha"), QStringLiteral("scan"), true);
    QTRY_COMPARE(rowAt(catalog, QStringLiteral("alpha"), QStringLiteral("scan"))
                     .value(QStringLiteral("currentValue"))
                     .toBool(),
                 true);
    QCOMPARE(daemon.sets.last().value, QStringLiteral("true"));
    catalog.setValue(QStringLiteral("alpha"), QStringLiteral("idle"), 99);
    QTRY_COMPARE(rowAt(catalog, QStringLiteral("alpha"), QStringLiteral("idle"))
                     .value(QStringLiteral("error"))
                     .toString(),
                 QStringLiteral("InvalidArgument"));
    QCOMPARE(daemon.sets.size(), 4);
  }
  void unsupportedDefersOnSucceededAndFailed() {
    SettingsDaemon daemon;
    daemon.providers = {{QStringLiteral("alpha"), QStringLiteral("Alpha")}};
    daemon.settings.insert(
        QStringLiteral("alpha"),
        {setting(QStringLiteral("idle"), QStringLiteral("system"),
                 QStringLiteral("integer"), QStringLiteral("10"),
                 QStringLiteral(R"({"min":1,"max":60})"), true),
         setting(QStringLiteral("delay"), QStringLiteral("system"),
                 QStringLiteral("integer"), QStringLiteral("2"),
                 QStringLiteral(R"({"min":1,"max":30})"), true)});
    QVERIFY(registerBus(daemon));
    SettingsCatalog catalog;
    QTRY_COMPARE(catalog.screenState(), QStringLiteral("ready"));

    daemon.operationState = QStringLiteral("succeeded");
    daemon.operationError = QStringLiteral("Unsupported");
    catalog.setValue(QStringLiteral("alpha"), QStringLiteral("idle"), 15);
    QTRY_COMPARE(rowAt(catalog, QStringLiteral("alpha"), QStringLiteral("idle"))
                     .value(QStringLiteral("deferred"))
                     .toBool(),
                 true);
    QCOMPARE(rowAt(catalog, QStringLiteral("alpha"), QStringLiteral("idle"))
                 .value(QStringLiteral("error"))
                 .toString(),
             QString());
    QCOMPARE(rowAt(catalog, QStringLiteral("alpha"), QStringLiteral("idle"))
                 .value(QStringLiteral("currentValue"))
                 .toLongLong(),
             15);

    daemon.operationState = QStringLiteral("failed");
    daemon.operationError = QStringLiteral("Unsupported");
    catalog.setValue(QStringLiteral("alpha"), QStringLiteral("delay"), 8);
    QTRY_COMPARE(rowAt(catalog, QStringLiteral("alpha"), QStringLiteral("delay"))
                     .value(QStringLiteral("deferred"))
                     .toBool(),
                 true);
    QCOMPARE(rowAt(catalog, QStringLiteral("alpha"), QStringLiteral("delay"))
                 .value(QStringLiteral("error"))
                 .toString(),
             QString());
    QCOMPARE(rowAt(catalog, QStringLiteral("alpha"), QStringLiteral("delay"))
                 .value(QStringLiteral("currentValue"))
                 .toLongLong(),
             8);
    QCOMPARE(rowAt(catalog, QStringLiteral("alpha"), QStringLiteral("idle"))
                 .value(QStringLiteral("deferred"))
                 .toBool(),
             true);
  }
  void navigationKeysDoNotMarkAKeyboard() {
    for (const auto *device : QInputDevice::devices()) {
      if (device && device->type() == QInputDevice::DeviceType::Keyboard)
        QSKIP("This process already has a keyboard device");
    }
    KeyboardMonitor monitor;
    QCOMPARE(monitor.available(), false);
    const Qt::Key keys[] = {
        Qt::Key_Left,     Qt::Key_Right,      Qt::Key_Up,       Qt::Key_Down,
        Qt::Key_Return,   Qt::Key_Enter,      Qt::Key_Backspace, Qt::Key_Escape,
        Qt::Key_Back,     Qt::Key_Select,     Qt::Key_Home,     Qt::Key_PageUp,
        Qt::Key_PageDown, Qt::Key_ChannelUp,  Qt::Key_ChannelDown};
    for (const auto key : keys) {
      QKeyEvent event(QEvent::KeyPress, key, Qt::NoModifier);
      QCoreApplication::sendEvent(QCoreApplication::instance(), &event);
      QCOMPARE(monitor.available(), false);
    }
  }
  // #540 (spec §5.6, ADR-0071): a folder is unavailable only when the
  // library provider reports it through Library1.GetRoots. The list follows
  // the provider's library StateChanged; Unavailable (not read yet) and a
  // provider without LibraryRoots1 show nothing.
  void libraryRootsMarkUnavailableFolders() {
    SettingsDaemon daemon;
    daemon.providers = {{QStringLiteral("library"), QStringLiteral("Library")}};
    daemon.settings.insert(
        QStringLiteral("library"),
        {setting(QStringLiteral("roots"), QStringLiteral("media"),
                 QStringLiteral("path"),
                 QStringLiteral(R"(["/srv/films","/mnt/share","/srv/series"])"),
                 QStringLiteral(R"({"directory":true,"multiple":true})"), true),
         setting(QStringLiteral("download"), QStringLiteral("media"),
                 QStringLiteral("path"), QStringLiteral("\"/mnt/share\""),
                 QStringLiteral(R"({"directory":true})"), true)});
    daemon.libraryProviders = {
        {QStringLiteral("library"),
         QDBusObjectPath(QStringLiteral("/org/arinn/Daemon1/providers/library")),
         {QStringLiteral("library")}}};
    daemon.libraryFeatures = {QStringLiteral("LibraryResume1"),
                              QStringLiteral("LibraryRoots1")};
    daemon.rootsError = QStringLiteral("Unavailable");
    daemon.roots = {{QStringLiteral("/srv/films"), true},
                    {QStringLiteral("/mnt/share"), false},
                    {QStringLiteral("/srv/series"), true}};
    QVERIFY(registerBus(daemon));
    SettingsCatalog catalog;
    QTRY_COMPARE(catalog.screenState(), QStringLiteral("ready"));
    // Not read yet: no folder is unavailable.
    QTRY_COMPARE(daemon.rootsReads, 1);
    QTest::qWait(100);
    auto roots = rowAt(catalog, QStringLiteral("library"), QStringLiteral("roots"));
    QCOMPARE(roots.value(QStringLiteral("unavailablePaths")).toStringList(),
             QStringList());
    // The daemon has read the roots and announces them.
    daemon.rootsError.clear();
    emitStateChanged(QStringLiteral("library"), QStringLiteral("library"));
    QTRY_COMPARE(rowAt(catalog, QStringLiteral("library"), QStringLiteral("roots"))
                     .value(QStringLiteral("unavailablePaths"))
                     .toStringList(),
                 QStringList({QStringLiteral("/mnt/share")}));
    roots = rowAt(catalog, QStringLiteral("library"), QStringLiteral("roots"));
    // The value itself is untouched: the folder stays in the list.
    QCOMPARE(roots.value(QStringLiteral("currentValue")).toStringList().size(), 3);
    // Only a folder list carries the state.
    QVERIFY(!rowAt(catalog, QStringLiteral("library"), QStringLiteral("download"))
                 .contains(QStringLiteral("unavailablePaths")));
    // Another capability's StateChanged reads nothing.
    const auto reads = daemon.rootsReads;
    emitStateChanged(QStringLiteral("library"), QStringLiteral("media"));
    QTest::qWait(200);
    QCOMPARE(daemon.rootsReads, reads);
    // The share is back.
    daemon.roots[1].available = true;
    emitStateChanged(QStringLiteral("library"), QStringLiteral("library"));
    QTRY_COMPARE(rowAt(catalog, QStringLiteral("library"), QStringLiteral("roots"))
                     .value(QStringLiteral("unavailablePaths"))
                     .toStringList(),
                 QStringList());
    QCOMPARE(daemon.sets.size(), 0);
  }
  void libraryWithoutRootsShowsNoUnavailableFolder() {
    SettingsDaemon daemon;
    daemon.providers = {{QStringLiteral("library"), QStringLiteral("Library")}};
    daemon.settings.insert(
        QStringLiteral("library"),
        {setting(QStringLiteral("roots"), QStringLiteral("media"),
                 QStringLiteral("path"), QStringLiteral(R"(["/mnt/share"])"),
                 QStringLiteral(R"({"directory":true,"multiple":true})"), true)});
    daemon.libraryProviders = {
        {QStringLiteral("library"),
         QDBusObjectPath(QStringLiteral("/org/arinn/Daemon1/providers/library")),
         {QStringLiteral("library")}}};
    daemon.roots = {{QStringLiteral("/mnt/share"), false}};
    QVERIFY(registerBus(daemon));
    SettingsCatalog catalog;
    QTRY_COMPARE(catalog.screenState(), QStringLiteral("ready"));
    emitStateChanged(QStringLiteral("library"), QStringLiteral("library"));
    QTest::qWait(300);
    // Without LibraryRoots1 in Features, GetRoots is never called.
    QCOMPARE(daemon.rootsReads, 0);
    QCOMPARE(rowAt(catalog, QStringLiteral("library"), QStringLiteral("roots"))
                 .value(QStringLiteral("unavailablePaths"))
                 .toStringList(),
             QStringList());
    // Features can change in the same generation. Unsupported is no state.
    daemon.libraryFeatures = {QStringLiteral("LibraryRoots1")};
    daemon.rootsError = QStringLiteral("Unsupported");
    emitStateChanged(QStringLiteral("library"), QStringLiteral("library"));
    QTRY_COMPARE(daemon.rootsReads, 1);
    QTest::qWait(100);
    QCOMPARE(rowAt(catalog, QStringLiteral("library"), QStringLiteral("roots"))
                 .value(QStringLiteral("unavailablePaths"))
                 .toStringList(),
             QStringList());
  }
  void libraryFeaturesChangeWithoutRestart() {
    SettingsDaemon daemon;
    daemon.providers = {{QStringLiteral("library"), QStringLiteral("Library")}};
    daemon.settings.insert(
        QStringLiteral("library"),
        {setting(QStringLiteral("roots"), QStringLiteral("media"),
                 QStringLiteral("path"), QStringLiteral(R"(["/mnt/share"])"),
                 QStringLiteral(R"({"directory":true,"multiple":true})"), true)});
    daemon.libraryProviders = {
        {QStringLiteral("library"),
         QDBusObjectPath(QStringLiteral("/org/arinn/Daemon1/providers/library")),
         {QStringLiteral("library")}}};
    daemon.roots = {{QStringLiteral("/mnt/share"), false}};
    QVERIFY(registerBus(daemon));
    SettingsCatalog catalog;
    QTRY_COMPARE(catalog.screenState(), QStringLiteral("ready"));
    QTRY_VERIFY(daemon.featuresReads > 0);
    const auto unavailable = [&catalog] {
      return rowAt(catalog, QStringLiteral("library"), QStringLiteral("roots"))
          .value(QStringLiteral("unavailablePaths")).toStringList();
    };
    QCOMPARE(unavailable(), QStringList());
    QCOMPARE(daemon.rootsReads, 0);
    daemon.libraryFeatures = {QStringLiteral("LibraryRoots1")};
    emitStateChanged(QStringLiteral("library"), QStringLiteral("library"));
    QTRY_COMPARE(unavailable(), QStringList({QStringLiteral("/mnt/share")}));
    const auto reads = daemon.rootsReads;
    daemon.libraryFeatures.clear();
    emitStateChanged(QStringLiteral("library"), QStringLiteral("library"));
    QTRY_COMPARE(unavailable(), QStringList());
    QCOMPARE(daemon.rootsReads, reads);
  }
  // #558: one library change brings StateChanged and then ProvidersChanged.
  // Root reads are coalesced per provider (one at a time, once more if asked
  // meanwhile), and Features stays fresh without a new generation. Only a
  // folder list is matched, not a list of files.
  void rootReadsAreCoalesced() {
    SettingsDaemon daemon;
    daemon.ownConnection = true;
    daemon.providers = {{QStringLiteral("library"), QStringLiteral("Library")}};
    daemon.settings.insert(
        QStringLiteral("library"),
        {setting(QStringLiteral("roots"), QStringLiteral("media"),
                 QStringLiteral("path"), QStringLiteral(R"(["/mnt/share"])"),
                 QStringLiteral(R"({"directory":true,"multiple":true})"), true),
         setting(QStringLiteral("playlists"), QStringLiteral("media"),
                 QStringLiteral("path"), QStringLiteral(R"(["/mnt/share"])"),
                 QStringLiteral(R"({"multiple":true})"), true)});
    daemon.libraryProviders = {
        {QStringLiteral("library"),
         QDBusObjectPath(QStringLiteral("/org/arinn/Daemon1/providers/library")),
         {QStringLiteral("library")}}};
    daemon.libraryFeatures = {QStringLiteral("LibraryRoots1")};
    daemon.roots = {{QStringLiteral("/mnt/share"), false}};
    QVERIFY(registerBus(daemon));
    SettingsCatalog catalog;
    QTRY_COMPARE(rowAt(catalog, QStringLiteral("library"), QStringLiteral("roots"))
                     .value(QStringLiteral("unavailablePaths"))
                     .toStringList(),
                 QStringList({QStringLiteral("/mnt/share")}));
    QTest::qWait(200);
    QCOMPARE(daemon.rootsReads, 1);
    QCOMPARE(daemon.featuresReads, 1);
    // A burst of triggers while a read is on its way: one more read.
    daemon.holdRoots = true;
    emitStateChanged(QStringLiteral("library"), QStringLiteral("library"), ownBus());
    QTRY_COMPARE(daemon.heldRoots.size(), 1);
    emitStateChanged(QStringLiteral("library"), QStringLiteral("library"), ownBus());
    emitStateChanged(QStringLiteral("library"), QStringLiteral("library"), ownBus());
    emitProvidersChanged(ownBus());
    QTest::qWait(300);
    QCOMPARE(daemon.rootsReads, 2);
    daemon.roots[0].available = true;
    daemon.releaseRoots();
    QTRY_COMPARE(rowAt(catalog, QStringLiteral("library"), QStringLiteral("roots"))
                     .value(QStringLiteral("unavailablePaths"))
                     .toStringList(),
                 QStringList());
    QTest::qWait(300);
    QCOMPARE(daemon.rootsReads, 3);
    // Each coalesced read refreshes Features, even in the same generation.
    QCOMPARE(daemon.featuresReads, 3);
    // A new generation reads it again.
    daemon.libraryProviders[0].generation = QStringLiteral("g-restarted");
    emitProvidersChanged(ownBus());
    QTRY_COMPARE(daemon.featuresReads, 4);
    // A list of files is not a list of library folders.
    QVERIFY(!rowAt(catalog, QStringLiteral("library"), QStringLiteral("playlists"))
                 .contains(QStringLiteral("unavailablePaths")));
  }
  void missingServiceIsUnavailable() {
    SettingsCatalog catalog;
    QTRY_COMPARE(catalog.screenState(), QStringLiteral("unavailable"));
    QCOMPARE(catalog.rows().size(), 0);
    // #742: the categories stay so System → About can be reached. The view
    // covers every section except System.
    QStringList shown;
    for (const auto &category : catalog.categories())
      shown.append(category.toMap().value(QStringLiteral("id")).toString());
    QCOMPARE(shown, QStringList({QStringLiteral("media"), QStringLiteral("playback"),
                                 QStringLiteral("sound"), QStringLiteral("network"),
                                 QStringLiteral("applications"), QStringLiteral("appearance"),
                                 QStringLiteral("system")}));
  }
  void systemStaysListedForAbout() {
    SettingsDaemon daemon;
    daemon.providers = {{QStringLiteral("library"), QStringLiteral("Library")}};
    daemon.settings.insert(
        QStringLiteral("library"),
        {setting(QStringLiteral("scan"), QStringLiteral("media"),
                 QStringLiteral("boolean"), QStringLiteral("false"), QString(),
                 true)});
    QVERIFY(registerBus(daemon));
    SettingsCatalog catalog;
    QTRY_COMPARE(catalog.screenState(), QStringLiteral("ready"));
    QStringList shown;
    for (const auto &category : catalog.categories())
      shown.append(category.toMap().value(QStringLiteral("id")).toString());
    QCOMPARE(shown, QStringList({QStringLiteral("media"), QStringLiteral("system")}));
    QCOMPARE(catalog.rows().size(), 1);
  }
};

int main(int argc, char **argv) {
  QCoreApplication app(argc, argv);
  SettingsCatalogTest test;
  return QTest::qExec(&test, argc, argv);
}
#include "settings_catalog_test.moc"
