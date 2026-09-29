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
  QString backend;
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
  argument << QString("browser") << QString("plugin") << QString()
           << QString("Browser") << QString("1")
           << QDBusObjectPath("/org/arinn/Daemon1/providers/p_browser")
           << QStringList{"browser"} << QString("ready") << provider.backend
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
  QString activatedId, closedId, openedId, activateRequest;
  int tabReads = 0, activates = 0, closes = 0, opens = 0, operationReads = 0,
      openArgCount = 0;
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
      const auto backend =
          mode == "unavailable" ? QString("unavailable") : QString("available");
      body.beginStructure();
      body << QString("snapshot")
           << QList<ProviderWire>{{backend}} << QString();
      body.endStructure();
    } else if (message.member() == "GetTabs") {
      ++tabReads;
      if (mode == "error") {
        bus.send(message.createErrorReply(QDBusError::Failed, "Unavailable"));
        return true;
      }
      body.beginStructure();
      body << QString("11111111-1111-4111-8111-111111111111") << qulonglong(1);
      body.beginArray(QMetaType::fromType<WindowWire>());
      if (mode == "populated") {
        body << WindowWire{"w1", {}, true};
        body << WindowWire{"w2", {}, false};
      }
      body.endArray();
      body.beginArray(QMetaType::fromType<TabWire>());
      if (mode == "populated") {
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
           << QList<ShortcutWire>{
                  {"known", "Known", "https://example.test/s", {}}};
      body.endStructure();
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
      body << QString("succeeded") << QString() << false << qulonglong(1)
           << qulonglong(2);
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
};

QTEST_GUILESS_MAIN(BrowserCatalogTest)
#include "browser_catalog_test.moc"
