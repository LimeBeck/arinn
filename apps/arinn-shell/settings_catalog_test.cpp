// SPDX-FileCopyrightText: 2026 Arinn contributors
//
// SPDX-License-Identifier: GPL-3.0-or-later

#include "settings_catalog.h"
#include <QInputDevice>
#include <QKeyEvent>
#include <QtDBus>
#include <QtTest>
namespace {
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
  auto bus = QDBusConnection::sessionBus();
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
  }
  void cleanup() {
    auto bus = QDBusConnection::sessionBus();
    bus.unregisterObject(QStringLiteral("/org/arinn"),
                         QDBusConnection::UnregisterTree);
    bus.unregisterService(QStringLiteral("org.arinn.Daemon1"));
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
    QCOMPARE(catalog.categories().size(), 7);
    QCOMPARE(catalog.categories().at(0).toMap().value(QStringLiteral("id")).toString(),
             QStringLiteral("media"));
    QCOMPARE(catalog.categories().at(6).toMap().value(QStringLiteral("id")).toString(),
             QStringLiteral("system"));
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
  // ADR-0112: the light field and the ambient timeout come from the daemon's
  // own settings; the same ids from another provider do not count.
  void shellPreferencesFromTheCoreSource() {
    SettingsDaemon daemon;
    daemon.providers = {{QStringLiteral("alpha"), QStringLiteral("Alpha")},
                        {QStringLiteral("arinn"), QStringLiteral("Arinn")}};
    const auto timeouts = QStringLiteral(
        R"({"options":[{"value":"5","title":"5 min"},{"value":"10","title":"10 min"},{"value":"15","title":"15 min"},{"value":"30","title":"30 min"},{"value":"60","title":"60 min"}]})");
    daemon.settings.insert(
        QStringLiteral("alpha"),
        {setting(QStringLiteral("light_field"), QStringLiteral("appearance"),
                 QStringLiteral("boolean"), QStringLiteral("true"), QString(),
                 true)});
    daemon.settings.insert(
        QStringLiteral("arinn"),
        {setting(QStringLiteral("light_field"), QStringLiteral("appearance"),
                 QStringLiteral("boolean"), QStringLiteral("false"),
                 QStringLiteral("{}"), true),
         setting(QStringLiteral("ambient_timeout"), QStringLiteral("appearance"),
                 QStringLiteral("enum"), QStringLiteral("\"30\""), timeouts,
                 true)});
    SettingsCatalog catalog;
    // Defaults until the daemon has answered.
    QCOMPARE(catalog.lightField(), true);
    QCOMPARE(catalog.ambientTimeoutMinutes(), 10);
    QSignalSpy changes(&catalog, &SettingsCatalog::preferencesChanged);
    QVERIFY(registerBus(daemon));
    catalog.refresh();
    QTRY_COMPARE(catalog.screenState(), QStringLiteral("ready"));
    QTRY_COMPARE(catalog.lightField(), false);
    QCOMPARE(catalog.ambientTimeoutMinutes(), 30);
    const auto row = rowAt(catalog, QStringLiteral("arinn"),
                           QStringLiteral("ambient_timeout"));
    QCOMPARE(row.value(QStringLiteral("constraints")).toMap()
                 .value(QStringLiteral("options")).toList().size(), 5);
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
  void missingServiceIsUnavailable() {
    SettingsCatalog catalog;
    QTRY_COMPARE(catalog.screenState(), QStringLiteral("unavailable"));
    QCOMPARE(catalog.rows().size(), 0);
    QCOMPARE(catalog.categories().size(), 0);
  }
};

int main(int argc, char **argv) {
  QCoreApplication app(argc, argv);
  SettingsCatalogTest test;
  return QTest::qExec(&test, argc, argv);
}
#include "settings_catalog_test.moc"
