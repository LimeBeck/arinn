// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

#include "browser_catalog.h"
#include <QtDBus>
#include <QtTest>

struct WindowWire {
  QString id, token;
  bool focused = false;
};
struct TabWire {
  QString id, windowId, title, url;
  bool active = false, audible = false;
};
struct EffectWire {
  QString provider, capability, id;
};
struct ProviderWire {
  QString backend = QStringLiteral("available");
  QString runtime = QStringLiteral("ready");
  QString id = QStringLiteral("browser");
};
struct ShortcutWire {
  QString id, title, url, icon;
};
Q_DECLARE_METATYPE(WindowWire)
Q_DECLARE_METATYPE(TabWire)
Q_DECLARE_METATYPE(EffectWire)
Q_DECLARE_METATYPE(ProviderWire)
Q_DECLARE_METATYPE(ShortcutWire)
QDBusArgument &operator<<(QDBusArgument &argument, const WindowWire &window) {
  argument.beginStructure();
  argument << window.id << window.token << window.focused;
  argument.endStructure();
  return argument;
}
QDBusArgument &operator<<(QDBusArgument &argument, const TabWire &tab) {
  argument.beginStructure();
  argument << tab.id << tab.windowId << tab.title << tab.url << tab.active
           << tab.audible;
  argument.endStructure();
  return argument;
}
QDBusArgument &operator<<(QDBusArgument &argument, const EffectWire &effect) {
  argument.beginStructure();
  argument << effect.provider << effect.capability << effect.id;
  argument.endStructure();
  return argument;
}
const QDBusArgument &operator>>(const QDBusArgument &argument, WindowWire &) {
  return argument;
}
const QDBusArgument &operator>>(const QDBusArgument &argument, TabWire &) {
  return argument;
}
const QDBusArgument &operator>>(const QDBusArgument &argument, EffectWire &) {
  return argument;
}
QDBusArgument &operator<<(QDBusArgument &argument, const ProviderWire &provider) {
  argument.beginStructure();
  argument << provider.id << QString("plugin") << QString() << QString("Browser")
           << QString("1")
           << QDBusObjectPath("/org/arinn/Daemon1/providers/p_browser")
           << QStringList{"browser"} << provider.runtime << provider.backend
           << QString("11111111-1111-4111-8111-111111111111") << qulonglong(1)
           << QString() << qulonglong(0) << qulonglong(0) << qulonglong(0)
           << qulonglong(0);
  argument.endStructure();
  return argument;
}
QDBusArgument &operator<<(QDBusArgument &argument, const ShortcutWire &shortcut) {
  argument.beginStructure();
  argument << shortcut.id << shortcut.title << shortcut.url << shortcut.icon;
  argument.endStructure();
  return argument;
}
const QDBusArgument &operator>>(const QDBusArgument &argument, ProviderWire &) {
  return argument;
}
const QDBusArgument &operator>>(const QDBusArgument &argument, ShortcutWire &) {
  return argument;
}

class BrowserDaemon : public QDBusVirtualObject {
public:
  QString mode = "populated";
  QString runtime = "ready";
  QString activatedId, closedId, openedId, activateRequest, restartedId;
  int tabReads = 0, activates = 0, closes = 0, opens = 0, operationReads = 0,
      openArgCount = 0, providerReads = 0, restarts = 0, cancels = 0, adds = 0;
  bool scripted = false, holdOperation = false, rejectAdd = false;
  QString addedTitle, addedUrl, addedIcon, addedRequest;
  QList<WindowWire> scriptedWindows;
  QList<TabWire> scriptedTabs;
  QList<ShortcutWire> scriptedShortcuts;
  QString introspect(const QString &) const override { return {}; }
  bool handleMessage(const QDBusMessage &message,
                     const QDBusConnection &bus) override {
    QDBusArgument body;
    if (message.member() == "Status") {
      body.beginStructure();
      body << QString("epoch") << true << QString("generation") << qulonglong(1)
           << true << QString() << QString() << QString() << false << true
           << QString() << QString();
      body.endStructure();
    } else if (message.member() == "Providers") {
      ++providerReads;
      const auto failed = mode == "failed" || runtime == "failed";
      const auto backend = mode == "unavailable" || failed
                               ? QString("unavailable")
                               : QString("available");
      const auto state = failed ? QString("failed") : runtime;
      body.beginStructure();
      body << QString("snapshot")
           << QList<ProviderWire>{{backend, state, QStringLiteral("browser")}}
           << QString();
      body.endStructure();
    } else if (message.member() == "Restart") {
      ++restarts;
      restartedId = message.arguments().at(0).toString();
      bus.send(message.createReply(
          QVariant::fromValue(QDBusObjectPath("/org/arinn/Daemon1/ops/restart"))));
      return true;
    } else if (message.member() == "Cancel") {
      ++cancels;
      holdOperation = false;
      bus.send(message.createReply());
      return true;
    } else if (message.member() == "GetTabs") {
      ++tabReads;
      if (mode == "error") {
        bus.send(message.createErrorReply(QDBusError::Failed, "Unavailable"));
        return true;
      }
      body.beginStructure();
      body << QString("11111111-1111-4111-8111-111111111111") << qulonglong(1);
      body.beginArray(QMetaType::fromType<WindowWire>());
      if (scripted) {
        for (const auto &window : scriptedWindows)
          body << window;
      } else if (mode == "populated") {
        body << WindowWire{"w1", {}, true};
        body << WindowWire{"w2", {}, false};
      }
      body.endArray();
      body.beginArray(QMetaType::fromType<TabWire>());
      if (scripted) {
        for (const auto &tab : scriptedTabs)
          body << tab;
      } else if (mode == "populated") {
        body << TabWire{"t1", "w1", "First", "https://example.test/1", true,
                        false};
        body << TabWire{"t2", "w2", "Second", "https://example.test/2", true,
                        true};
      }
      body.endArray();
      body << QString();
      body.endStructure();
    } else if (message.member() == "ActivateTab" ||
               message.member() == "CloseTab") {
      if (message.member() == "ActivateTab") {
        ++activates;
        activatedId = message.arguments().at(1).toString();
        activateRequest = message.arguments().at(2).toString();
      } else {
        ++closes;
        closedId = message.arguments().at(1).toString();
      }
      bus.send(message.createReply(
          QVariant::fromValue(QDBusObjectPath("/org/arinn/Daemon1/ops/tab"))));
      return true;
    } else if (message.member() == "List") {
      body.beginStructure();
      body << qulonglong(1)
           << (scripted ? scriptedShortcuts
                        : QList<ShortcutWire>{
                              {"known", "Known", "https://example.test/s", {}}});
      body.endStructure();
    } else if (message.member() == "Add") {
      ++adds;
      addedTitle = message.arguments().at(0).toString();
      addedUrl = message.arguments().at(1).toString();
      addedIcon = message.arguments().at(2).toString();
      addedRequest = message.arguments().at(3).toString();
      if (rejectAdd) {
        bus.send(message.createErrorReply("org.arinn.Error1.InvalidArgument",
                                          "InvalidArgument"));
        return true;
      }
      if (scripted)
        scriptedShortcuts.append(
            ShortcutWire{QStringLiteral("added-%1").arg(adds), addedTitle,
                         addedUrl, {}});
      bus.send(message.createReply(QStringLiteral("new-shortcut")));
      return true;
    } else if (message.member() == "Open") {
      ++opens;
      openArgCount = message.arguments().size();
      openedId = message.arguments().at(0).toString();
      bus.send(message.createReply(
          QVariant::fromValue(QDBusObjectPath("/org/arinn/Daemon1/ops/open"))));
      return true;
    } else if (message.member() == "Get") {
      ++operationReads;
      body.beginStructure();
      body << QString(holdOperation ? "running" : "succeeded") << QString()
           << false << qulonglong(1) << qulonglong(2);
      body.beginArray(QMetaType::fromType<EffectWire>());
      body.endArray();
      body << false;
      body.endStructure();
    } else {
      bus.send(message.createErrorReply(QDBusError::NotSupported, "fixture"));
      return true;
    }
    bus.send(message.createReply(QVariant::fromValue(body)));
    return true;
  }
};

class BrowserCatalogTest : public QObject {
  Q_OBJECT
private slots:
  void initTestCase() {
    qDBusRegisterMetaType<WindowWire>();
    qDBusRegisterMetaType<TabWire>();
    qDBusRegisterMetaType<EffectWire>();
    qDBusRegisterMetaType<ProviderWire>();
    qDBusRegisterMetaType<ShortcutWire>();
  }
  void cleanup() {
    auto bus = QDBusConnection::sessionBus();
    bus.unregisterObject("/org/arinn", QDBusConnection::UnregisterTree);
    bus.unregisterService("org.arinn.Daemon1");
  }
  void snapshotActivateAndClose() {
    auto bus = QDBusConnection::sessionBus();
    BrowserDaemon daemon;
    QVERIFY(bus.registerService("org.arinn.Daemon1"));
    QVERIFY(bus.registerVirtualObject("/org/arinn", &daemon,
                                      QDBusConnection::SubPath));
    BrowserCatalog catalog;
    QTRY_COMPARE(catalog.browserState(), QString("populated"));
    QCOMPARE(catalog.tabs().size(), 2);
    QCOMPARE(catalog.windows().size(), 2);
    int focused = 0;
    QString secondId;
    for (const auto &entry : catalog.windows())
      focused += entry.toMap().value("focused").toBool() ? 1 : 0;
    QCOMPARE(focused, 1);
    QVERIFY(catalog.windows().at(0).toMap().value("focused").toBool());
    for (const auto &entry : catalog.tabs()) {
      const auto map = entry.toMap();
      if (map.value("title").toString() == "Second") {
        secondId = map.value("id").toString();
        QCOMPARE(map.value("url").toString(), QString("https://example.test/2"));
        QCOMPARE(map.value("active").toBool(), true);
        QCOMPARE(map.value("audible").toBool(), true);
        QVERIFY(!map.contains("favicon"));
      }
    }
    QVERIFY(!secondId.isEmpty());
    QCOMPARE(catalog.shortcuts().size(), 1);
    QCOMPARE(catalog.shortcuts().at(0).toMap().value("id").toString(),
             QString("known"));
    QVERIFY(!catalog.shortcuts().at(0).toMap().contains("url"));
    catalog.activateTab(secondId);
    catalog.activateTab(secondId);
    QTRY_COMPARE(daemon.activatedId, QString("t2"));
    QCOMPARE(daemon.activates, 1);
    QCOMPARE(daemon.activateRequest.isEmpty(), false);
    QVERIFY(!daemon.activateRequest.contains("example.test"));
    QTRY_COMPARE(catalog.operationState(), QString("succeeded"));
    QVERIFY(daemon.operationReads >= 1);
    catalog.closeTab(secondId);
    QTRY_COMPARE(daemon.closedId, QString("t2"));
    QCOMPARE(daemon.closes, 1);
    QTRY_COMPARE(catalog.operationState(), QString("succeeded"));
    catalog.openShortcut("known");
    QTRY_COMPARE(daemon.openedId, QString("known"));
    QCOMPARE(daemon.opens, 1);
    QCOMPARE(daemon.openArgCount, 2);
    QTRY_COMPARE(catalog.operationState(), QString("succeeded"));
    catalog.openShortcut("missing");
    QTRY_COMPARE(catalog.operationState(), QString("NotFound"));
    QCOMPARE(daemon.opens, 1);
    const auto reads = daemon.tabReads;
    daemon.mode = "empty";
    catalog.refresh();
    QTRY_COMPARE(catalog.browserState(), QString("empty"));
    QVERIFY(catalog.tabs().isEmpty());
    QVERIFY(daemon.tabReads > reads);
    daemon.mode = "error";
    catalog.refresh();
    QTRY_COMPARE(catalog.browserState(), QString("error"));
    daemon.mode = "unavailable";
    const auto before = daemon.tabReads;
    catalog.refresh();
    QTRY_COMPARE(catalog.browserState(), QString("unavailable"));
    QTest::qWait(200);
    QCOMPARE(daemon.tabReads, before);
    bus.unregisterService("org.arinn.Daemon1");
    QTRY_COMPARE(catalog.browserState(), QString("unavailable"));
    QVERIFY(catalog.tabs().isEmpty());
  }
  void orderOriginAndRetry() {
    auto bus = QDBusConnection::sessionBus();
    BrowserDaemon daemon;
    daemon.scripted = true;
    daemon.scriptedWindows = {WindowWire{"w1", {}, true},
                              WindowWire{"w2", {}, false},
                              WindowWire{"w3", {}, false}};
    daemon.scriptedTabs = {
        TabWire{"t1", "w1", "First", "https://www.example.test:8443/a", true,
                false},
        TabWire{"t2", "w2", "Second", "https://example.test/b", true, false},
        TabWire{"idle", "w2", "Idle", "https://hd.videos.example/watch", false,
                false},
        TabWire{"scheme", "w3", "Scheme", "http://example.test:8443/c", true,
                false}};
    daemon.scriptedShortcuts = {
        ShortcutWire{"port", "Port", "https://example.test:8443/s",
                     "packaged:mark"},
        ShortcutWire{"plain", "Plain", "http://example.test/s", {}}};
    QVERIFY(bus.registerService("org.arinn.Daemon1"));
    QVERIFY(bus.registerVirtualObject("/org/arinn", &daemon,
                                      QDBusConnection::SubPath));
    BrowserCatalog catalog;
    QTRY_COMPARE(catalog.browserState(), QString("populated"));
    QTRY_COMPARE(catalog.tabs().at(0).toMap().value("title").toString(),
                 QString("First"));
    QCOMPARE(catalog.tabs().at(0).toMap().value("letter").toString(),
             QString("E"));
    QCOMPARE(catalog.tabs().at(0).toMap().value("icon").toString(),
             QString("packaged:mark"));
    QCOMPARE(catalog.tabs().at(0).toMap().value("saved").toBool(), true);
    QCOMPARE(catalog.openShortcutIds(), QStringList{"port"});
    QCOMPARE(catalog.currentService().value("title").toString(),
             QString("First"));
    QCOMPARE(catalog.currentService().value("subtitle").toString(),
             QString("www.example.test:8443 · Return"));
    QVariantMap idle;
    for (const auto &entry : catalog.tabs()) {
      if (entry.toMap().value("title").toString() == QLatin1String("Idle"))
        idle = entry.toMap();
    }
    QCOMPARE(idle.value("letter").toString(), QString("V"));
    QCOMPARE(idle.value("saved").toBool(), false);
    QVERIFY(!idle.contains(QStringLiteral("icon")));
    for (const auto &entry : catalog.shortcuts()) {
      const auto map = entry.toMap();
      QVERIFY(!map.contains("url"));
      QVERIFY(!map.contains("icon"));
    }
    daemon.scriptedWindows = {WindowWire{"w1", {}, false},
                              WindowWire{"w2", {}, true},
                              WindowWire{"w3", {}, false}};
    catalog.refresh();
    QTRY_COMPARE(catalog.tabs().at(0).toMap().value("title").toString(),
                 QString("Second"));
    QCOMPARE(catalog.tabs().at(1).toMap().value("title").toString(),
             QString("Idle"));
    QCOMPARE(catalog.currentService().value("title").toString(),
             QString("Second"));
    QVERIFY(!catalog.openShortcutIds().contains(QString("plain")));

    daemon.holdOperation = true;
    catalog.openShortcut("port");
    QTRY_VERIFY(catalog.opening());
    QCOMPARE(catalog.openingTitle(), QString("Port"));
    catalog.cancelOpen();
    QTRY_COMPARE(daemon.cancels, 1);
    QCOMPARE(catalog.opening(), false);
    QCOMPARE(catalog.openingError(), QString());

    daemon.mode = "unavailable";
    daemon.runtime = "ready";
    daemon.scripted = false;
    catalog.refresh();
    QTRY_COMPARE(catalog.browserState(), QString("unavailable"));
    QCOMPARE(catalog.retryAction(), QString("refresh"));
    const auto reads = daemon.tabReads;
    const auto providers = daemon.providerReads;
    catalog.retry();
    QTRY_VERIFY(daemon.providerReads > providers);
    QCOMPARE(daemon.restarts, 0);
    QCOMPARE(daemon.tabReads, reads);

    daemon.runtime = "failed";
    catalog.refresh();
    QTRY_COMPARE(catalog.retryAction(), QString("restart"));
    catalog.retry();
    QTRY_COMPARE(daemon.restartedId, QString("browser"));
    QCOMPARE(daemon.restarts, 1);
    QCOMPARE(daemon.tabReads, reads);
  }
  void saveOpenPage() {
    auto bus = QDBusConnection::sessionBus();
    BrowserDaemon daemon;
    daemon.scripted = true;
    daemon.scriptedWindows = {WindowWire{"w1", {}, true}};
    daemon.scriptedTabs = {
        TabWire{"page", "w1", "Page", "https://pages.example/watch", true,
                false},
        TabWire{"kept", "w1", "Kept", "https://www.kept.example/a", false,
                false},
        TabWire{"blank", "w1", "", "https://blank.example/x", false, false},
        TabWire{"bad", "w1", "Bad", "http://public.example/a", false, false}};
    daemon.scriptedShortcuts = {
        ShortcutWire{"kept-id", "Kept", "https://kept.example/home", {}}};
    QVERIFY(bus.registerService("org.arinn.Daemon1"));
    QVERIFY(bus.registerVirtualObject("/org/arinn", &daemon,
                                      QDBusConnection::SubPath));
    BrowserCatalog catalog;
    QTRY_COMPARE(catalog.browserState(), QString("populated"));
    auto findTab = [&](const QString &title) {
      for (const auto &entry : catalog.tabs()) {
        if (entry.toMap().value("title").toString() == title)
          return entry.toMap();
      }
      return QVariantMap{};
    };
    const auto kept = findTab(QStringLiteral("Kept"));
    const auto page = findTab(QStringLiteral("Page"));
    QVERIFY(!kept.isEmpty());
    QVERIFY(!page.isEmpty());
    QCOMPARE(kept.value("saved").toBool(), true);
    QCOMPARE(page.value("saved").toBool(), false);
    for (const auto &entry : catalog.shortcuts())
      QVERIFY(!entry.toMap().contains("url"));
    catalog.addShortcut(kept.value("id").toString());
    QTest::qWait(200);
    QCOMPARE(daemon.adds, 0);

    catalog.addShortcut(page.value("id").toString());
    QTRY_COMPARE(daemon.adds, 1);
    QCOMPARE(daemon.addedTitle, QString("Page"));
    QCOMPARE(daemon.addedUrl, QString("https://pages.example/watch"));
    QCOMPARE(daemon.addedIcon, QString());
    QVERIFY(!QUuid::fromString(daemon.addedRequest).isNull());
    QTRY_COMPARE(catalog.operationState(), QString("succeeded"));
    QCOMPARE(findTab(QStringLiteral("Page")).value("saved").toBool(), true);
    catalog.addShortcut(page.value("id").toString());
    QTest::qWait(200);
    QCOMPARE(daemon.adds, 1);

    const auto blank = findTab(QString());
    catalog.addShortcut(blank.value("id").toString());
    QTRY_COMPARE(daemon.adds, 2);
    QCOMPARE(daemon.addedTitle, QString("blank.example"));
    QCOMPARE(daemon.addedUrl, QString("https://blank.example/x"));

    daemon.rejectAdd = true;
    const auto bad = findTab(QStringLiteral("Bad"));
    QCOMPARE(bad.value("saved").toBool(), false);
    catalog.addShortcut(bad.value("id").toString());
    QTRY_COMPARE(daemon.adds, 3);
    QTRY_COMPARE(catalog.operationState(), QString("InvalidArgument"));
    QCOMPARE(findTab(QStringLiteral("Bad")).value("saved").toBool(), false);

    daemon.mode = "unavailable";
    catalog.refresh();
    QTRY_COMPARE(catalog.browserState(), QString("unavailable"));
    const auto adds = daemon.adds;
    catalog.addShortcut(QStringLiteral("anything"));
    QTRY_COMPARE(catalog.operationState(), QString("Unavailable"));
    QCOMPARE(daemon.adds, adds);
  }
};

QTEST_GUILESS_MAIN(BrowserCatalogTest)
#include "browser_catalog_test.moc"
