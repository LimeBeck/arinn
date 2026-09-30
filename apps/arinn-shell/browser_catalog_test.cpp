// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

#include "browser_catalog.h"
#include <fcntl.h>
#include <unistd.h>
#include <QElapsedTimer>
#include <QGuiApplication>
#include <QProcess>
#include <QQmlApplicationEngine>
#include <QQmlContext>
#include <QQuickItem>
#include <QQuickWindow>
#include <QSocketNotifier>
#include <QTimer>
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
      openArgCount = 0, providerReads = 0, restarts = 0, cancels = 0, adds = 0,
      lists = 0;
  bool scripted = false, holdOperation = false, rejectAdd = false,
       holdAdd = false, holdList = false, recordAdded = true;
  QString rejectCode = QStringLiteral("InvalidArgument");
  QString addedTitle, addedUrl, addedIcon, addedRequest;
  QList<QDBusMessage> heldAdds, heldLists, staleLists;
  QDBusConnection replyBus = QDBusConnection::sessionBus();
  void replyLists(const QList<QDBusMessage> &messages) {
    auto bus = QDBusConnection::sessionBus();
    for (const auto &held : messages) {
      QDBusArgument body;
      body.beginStructure();
      body << qulonglong(1) << scriptedShortcuts;
      body.endStructure();
      bus.send(held.createReply(QVariant::fromValue(body)));
    }
  }
  void releaseLists() {
    const auto pending = heldLists;
    heldLists.clear();
    replyLists(pending);
  }
  void releaseStaleLists() {
    const auto pending = staleLists;
    staleLists.clear();
    replyLists(pending);
  }
  void captureStaleLists() {
    if (holdList && staleLists.isEmpty() && !heldLists.isEmpty()) {
      staleLists = heldLists;
      heldLists.clear();
    }
  }
  void releaseAdds() {
    auto bus = QDBusConnection::sessionBus();
    const auto pending = heldAdds;
    heldAdds.clear();
    for (const auto &held : pending) {
      if (rejectAdd) {
        bus.send(held.createErrorReply(
            QStringLiteral("org.arinn.Error1.") + rejectCode, rejectCode));
        continue;
      }
      if (scripted && recordAdded)
        scriptedShortcuts.append(
            ShortcutWire{QStringLiteral("added-%1").arg(adds), addedTitle,
                         addedUrl, {}});
      bus.send(held.createReply(QStringLiteral("new-shortcut")));
    }
  }
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
      ++lists;
      if (holdList) {
        message.setDelayedReply(true);
        heldLists.append(message);
        replyBus = bus;
        fprintf(stdout, "held-list\n");
        fflush(stdout);
        return true;
      }
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
      if (holdAdd) {
        message.setDelayedReply(true);
        heldAdds.append(message);
        replyBus = bus;
        return true;
      }
      captureStaleLists();
      if (rejectAdd) {
        bus.send(message.createErrorReply(
            QStringLiteral("org.arinn.Error1.") + rejectCode, rejectCode));
        return true;
      }
      if (scripted && recordAdded)
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

class ShellStandIn : public QObject {
  Q_OBJECT
  Q_PROPERTY(bool homeVisible MEMBER m_home CONSTANT)
  Q_PROPERTY(QString returnWindow MEMBER m_return CONSTANT)
  Q_PROPERTY(QVariantList items MEMBER m_items CONSTANT)
  Q_PROPERTY(QVariantList continueEntries MEMBER m_continue CONSTANT)
  Q_PROPERTY(QVariantList applications MEMBER m_applications CONSTANT)
  Q_PROPERTY(QVariantList favorites MEMBER m_favorites CONSTANT)
  Q_PROPERTY(QString homeState MEMBER m_homeState CONSTANT)
  Q_PROPERTY(QStringList busyEntryIds MEMBER m_busy CONSTANT)
  Q_PROPERTY(QString message MEMBER m_message CONSTANT)
public:
  bool m_home = true;
  QString m_return;
  QString m_homeState = QStringLiteral("empty");
  QString m_message;
  QVariantList m_items, m_continue, m_applications, m_favorites;
  QStringList m_busy;
  Q_INVOKABLE void back() {}
  Q_INVOKABLE void launch(const QString &) {}
  Q_INVOKABLE void activate(const QString &) {}
  Q_INVOKABLE void refresh() {}
};
class SessionGuardStandIn : public QObject {
  Q_OBJECT
  Q_PROPERTY(bool blocked MEMBER m_blocked CONSTANT)
public:
  bool m_blocked = false;
};
// Drives Main.qml the way the daemon's Home report does: the shell stays
// hidden until Home is visible, then showHome focuses returnWindow.
class ExternalHomeBackend : public QObject {
  Q_OBJECT
  Q_PROPERTY(bool homeVisible READ homeVisible NOTIFY changed)
  Q_PROPERTY(QString returnWindow READ returnWindow NOTIFY changed)
  Q_PROPERTY(QVariantList items MEMBER m_items CONSTANT)
  Q_PROPERTY(QVariantList continueEntries MEMBER m_continue CONSTANT)
  Q_PROPERTY(QVariantList applications MEMBER m_applications CONSTANT)
  Q_PROPERTY(QVariantList favorites MEMBER m_favorites CONSTANT)
  Q_PROPERTY(QString homeState MEMBER m_homeState CONSTANT)
  Q_PROPERTY(QStringList busyEntryIds MEMBER m_busy CONSTANT)
  Q_PROPERTY(QString message MEMBER m_message CONSTANT)
public:
  bool homeVisible() const { return m_home; }
  QString returnWindow() const { return m_return; }
  void openOverlay(const QString &activity) {
    m_return = activity;
    m_home = true;
    emit changed();
  }
  Q_INVOKABLE void back() { ++backs; }
  Q_INVOKABLE void launch(const QString &) {}
  Q_INVOKABLE void activate(const QString &) {}
  Q_INVOKABLE void refresh() {}
  int backs = 0;
  bool m_home = false;
  QString m_return;
  QString m_homeState = QStringLiteral("ready");
  QString m_message;
  QVariantList m_items, m_continue, m_applications, m_favorites;
  QStringList m_busy;
signals:
  void changed();
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
    QTRY_COMPARE(findTab(QStringLiteral("Page")).value("saved").toBool(), true);
    QCOMPARE(catalog.operationState(), QString());
    QCOMPARE(catalog.saveError(), QString());
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
    QTRY_COMPARE(catalog.saveError(), QString("InvalidArgument"));
    QCOMPARE(catalog.operationState(), QString());
    QCOMPARE(findTab(QStringLiteral("Bad")).value("saved").toBool(), false);
    catalog.dismissSaveError();
    QCOMPARE(catalog.saveError(), QString());
    daemon.rejectCode = QStringLiteral("Conflict");
    catalog.addShortcut(bad.value("id").toString());
    QTRY_COMPARE(catalog.saveError(), QString("Conflict"));
    QCOMPARE(catalog.operationState(), QString());
    catalog.addShortcut(QStringLiteral("missing"));
    QCOMPARE(catalog.saveError(), QString("NotFound"));
    QCOMPARE(catalog.operationState(), QString());

    daemon.mode = "unavailable";
    catalog.refresh();
    QTRY_COMPARE(catalog.browserState(), QString("unavailable"));
    const auto adds = daemon.adds;
    catalog.addShortcut(QStringLiteral("anything"));
    QTRY_COMPARE(catalog.saveError(), QString("Unavailable"));
    QCOMPARE(catalog.operationState(), QString());
    QCOMPARE(daemon.adds, adds);
  }
  void saveSurvivesStaleListUntilRemoved() {
    // A delayed reply and the next call cannot share this process: Qt delivers
    // local virtual-object calls directly and rejects the second one. The
    // fixture process owns the daemon so List can stay in flight while Add
    // completes. Wait with qWait: QProcess::waitForReadyRead does not dispatch
    // the session bus, so the catalog would never send the held List.
    QProcess fixture;
    fixture.setProcessChannelMode(QProcess::SeparateChannels);
    fixture.start(QCoreApplication::applicationFilePath(),
                  {QStringLiteral("--shortcut-fixture")});
    QVERIFY2(fixture.waitForStarted(), qPrintable(fixture.errorString()));
    struct Stop {
      QProcess *process;
      ~Stop() {
        if (process->state() == QProcess::NotRunning)
          return;
        process->write("quit\n");
        process->waitForBytesWritten(200);
        if (!process->waitForFinished(1000)) {
          process->kill();
          process->waitForFinished(1000);
        }
      }
    } stop{&fixture};
    QByteArray output;
    auto waitOutput = [&](const QByteArray &needle) {
      QElapsedTimer timer;
      timer.start();
      while (timer.elapsed() < 5000) {
        QTest::qWait(20);
        output += fixture.readAllStandardOutput();
        if (output.contains(needle))
          return true;
      }
      return false;
    };
    auto dumped = [&](const char *what) {
      QTest::qWait(20);
      output += fixture.readAllStandardOutput();
      return QStringLiteral("%1 stdout=%2 stderr=%3 state=%4")
          .arg(QString::fromUtf8(what), QString::fromUtf8(output),
               QString::fromUtf8(fixture.readAllStandardError()))
          .arg(int(fixture.state()));
    };
    if (!waitOutput("ready"))
      QFAIL(qPrintable(dumped("ready")));
    BrowserCatalog catalog;
    QTRY_COMPARE(catalog.browserState(), QString("populated"));
    QString pageId;
    for (const auto &entry : catalog.tabs()) {
      const auto map = entry.toMap();
      if (map.value(QStringLiteral("title")).toString() ==
          QStringLiteral("Page")) {
        pageId = map.value(QStringLiteral("id")).toString();
        QCOMPARE(map.value(QStringLiteral("saved")).toBool(), false);
      }
    }
    QVERIFY(!pageId.isEmpty());
    fixture.write("hold-all\n");
    QVERIFY(fixture.waitForBytesWritten());
    if (!waitOutput("holding"))
      QFAIL(qPrintable(dumped("holding")));
    catalog.refresh();
    if (!waitOutput("held-list"))
      QFAIL(qPrintable(dumped("held-list")));
    catalog.addShortcut(pageId);
    QTRY_COMPARE(catalog.tabs().at(0).toMap().value(QStringLiteral("saved")).toBool(),
                 true);
    fixture.write("seed-other\n");
    fixture.write("release-stale\n");
    QVERIFY(fixture.waitForBytesWritten());
    auto hasOther = [&] {
      for (const auto &entry : catalog.shortcuts())
        if (entry.toMap().value(QStringLiteral("id")).toString() ==
            QStringLiteral("other"))
          return true;
      return false;
    };
    QTRY_VERIFY(hasOther());
    QCOMPARE(catalog.tabs().at(0).toMap().value(QStringLiteral("saved")).toBool(),
             true);
    fixture.write("clear-and-release\n");
    QVERIFY(fixture.waitForBytesWritten());
    catalog.refresh();
    QTRY_COMPARE(catalog.tabs().at(0).toMap().value(QStringLiteral("saved")).toBool(),
                 false);
    QTRY_VERIFY(catalog.shortcuts().isEmpty());
    fixture.write("quit\n");
    QVERIFY(fixture.waitForFinished(2000));
  }
  void saveErrorTimesOut() {
    auto bus = QDBusConnection::sessionBus();
    BrowserDaemon daemon;
    daemon.scripted = true;
    daemon.rejectAdd = true;
    daemon.scriptedWindows = {WindowWire{"w1", {}, true}};
    daemon.scriptedTabs = {
        TabWire{"page", "w1", "Page", "https://pages.example/watch", true,
                false}};
    QVERIFY(bus.registerService("org.arinn.Daemon1"));
    QVERIFY(bus.registerVirtualObject("/org/arinn", &daemon,
                                      QDBusConnection::SubPath));
    BrowserCatalog catalog;
    QTRY_COMPARE(catalog.browserState(), QString("populated"));
    const auto id = catalog.tabs().at(0).toMap().value("id").toString();
    catalog.addShortcut(id);
    QTRY_COMPARE(catalog.saveError(), QString("InvalidArgument"));
    QCOMPARE(catalog.operationState(), QString());
    QTRY_COMPARE_WITH_TIMEOUT(catalog.saveError(), QString(), 6000);
  }
  void externalActivityOpensRememberedHome() {
    auto bus = QDBusConnection::sessionBus();
    BrowserDaemon daemon;
    QVERIFY(bus.registerService("org.arinn.Daemon1"));
    QVERIFY(bus.registerVirtualObject("/org/arinn", &daemon,
                                      QDBusConnection::SubPath));
    ExternalHomeBackend backend;
    backend.m_continue = {
        QVariantMap{{QStringLiteral("id"), QStringLiteral("a")},
                    {QStringLiteral("title"), QStringLiteral("Earlier")}},
        QVariantMap{{QStringLiteral("id"), QStringLiteral("b")},
                    {QStringLiteral("title"), QStringLiteral("Activity")},
                    {QStringLiteral("subtitle"), QStringLiteral("Return")},
                    {QStringLiteral("running"), true}}};
    SessionGuardStandIn guard;
    QQmlApplicationEngine engine;
    QStringList problems;
    QObject::connect(
        &engine, &QQmlEngine::warnings, &engine,
        [&](const QList<QQmlError> &warnings) {
          for (const auto &warning : warnings) {
            const auto text = warning.toString();
            if (text.contains(QStringLiteral("qrc:/brand/")))
              continue;
            problems.append(text);
          }
        });
    engine.rootContext()->setContextProperty(QStringLiteral("backend"),
                                             &backend);
    engine.rootContext()->setContextProperty(QStringLiteral("sessionGuard"),
                                             &guard);
    engine.load(QUrl::fromLocalFile(QStringLiteral(ARINN_MAIN_QML)));
    QVERIFY2(problems.isEmpty(), qPrintable(problems.join('\n')));
    QVERIFY(!engine.rootObjects().isEmpty());
    auto window = qobject_cast<QQuickWindow *>(engine.rootObjects().first());
    QVERIFY(window);
    QVERIFY(!window->isVisible());
    QCOMPARE(backend.backs, 0);
    backend.openOverlay(QStringLiteral("b"));
    QTRY_VERIFY(window->isVisible());
    window->requestActivate();
    auto home =
        window->findChild<QQuickItem *>(QStringLiteral("productionHome"));
    QVERIFY(home);
    QTRY_COMPARE(home->property("returnActivityId").toString(),
                 QStringLiteral("b"));
    QCOMPARE(home->property("screen").toString(), QStringLiteral("home"));
    QVERIFY(!home->property("navOpen").toBool());
    // Loader delegates are visual children. QObject::findChild misses them.
    const auto findVisual = [](QQuickItem *root, const QString &name,
                               auto &&self) -> QQuickItem * {
      if (!root)
        return nullptr;
      if (root->objectName() == name)
        return root;
      for (auto *child : root->childItems())
        if (auto *found = self(child, name, self))
          return found;
      return nullptr;
    };
    QQuickItem *entry = nullptr;
    QTRY_VERIFY(entry = findVisual(window->contentItem(),
                                   QStringLiteral("entry-b"), findVisual));
    QTRY_VERIFY(entry->hasActiveFocus());
    QTest::keyClick(window, Qt::Key_Backspace);
    QTRY_COMPARE(backend.backs, 1);
    QVERIFY(!home->property("navOpen").toBool());
  }
  void productionSaveWiring() {
    auto bus = QDBusConnection::sessionBus();
    BrowserDaemon daemon;
    daemon.scripted = true;
    daemon.scriptedWindows = {WindowWire{"w1", {}, true}};
    daemon.scriptedTabs = {
        TabWire{"page", "w1", "Page", "https://pages.example/watch", true,
                false},
        TabWire{"bad", "w1", "Bad", "http://public.example/a", false, false}};
    QVERIFY(bus.registerService("org.arinn.Daemon1"));
    QVERIFY(bus.registerVirtualObject("/org/arinn", &daemon,
                                      QDBusConnection::SubPath));
    ShellStandIn backend;
    SessionGuardStandIn guard;
    QQmlApplicationEngine engine;
    QStringList problems;
    QObject::connect(
        &engine, &QQmlEngine::warnings, &engine,
        [&](const QList<QQmlError> &warnings) {
          for (const auto &warning : warnings) {
            const auto text = warning.toString();
            if (text.contains(QStringLiteral("qrc:/brand/")))
              continue;
            problems.append(text);
          }
        });
    engine.rootContext()->setContextProperty(QStringLiteral("backend"),
                                             &backend);
    engine.rootContext()->setContextProperty(QStringLiteral("sessionGuard"),
                                             &guard);
    engine.load(QUrl::fromLocalFile(QStringLiteral(ARINN_MAIN_QML)));
    QVERIFY2(problems.isEmpty(), qPrintable(problems.join('\n')));
    QVERIFY(!engine.rootObjects().isEmpty());
    auto window = qobject_cast<QQuickWindow *>(engine.rootObjects().first());
    QVERIFY(window);
    window->show();
    window->requestActivate();
    window->setProperty("browserOpen", true);
    auto browser =
        window->findChild<QQuickItem *>(QStringLiteral("productionBrowser"));
    QVERIFY(browser);
    QTRY_COMPARE(browser->property("browserState").toString(),
                 QString("populated"));
    QString pageId;
    QString badId;
    const auto tabs = browser->property("orderedTabs").toList();
    for (const auto &entry : tabs) {
      const auto map = entry.toMap();
      if (map.value(QStringLiteral("title")).toString() ==
          QStringLiteral("Page"))
        pageId = map.value(QStringLiteral("id")).toString();
      if (map.value(QStringLiteral("title")).toString() == QStringLiteral("Bad"))
        badId = map.value(QStringLiteral("id")).toString();
    }
    QVERIFY(!pageId.isEmpty());
    QVERIFY(!badId.isEmpty());
    QVERIFY(QMetaObject::invokeMethod(browser, "requestSave",
                                      Q_ARG(QVariant, QVariant(pageId))));
    QTRY_COMPARE(daemon.adds, 1);
    QCOMPARE(daemon.addedTitle, QString("Page"));
    QCOMPARE(daemon.addedUrl, QString("https://pages.example/watch"));
    QCOMPARE(daemon.addedIcon, QString());
    QVERIFY(QMetaObject::invokeMethod(browser, "requestSave",
                                      Q_ARG(QVariant, QVariant(pageId))));
    QTest::qWait(200);
    QCOMPARE(daemon.adds, 1);

    daemon.rejectAdd = true;
    QVERIFY(QMetaObject::invokeMethod(browser, "requestSave",
                                      Q_ARG(QVariant, QVariant(badId))));
    QTRY_COMPARE(daemon.adds, 2);
    auto error = window->findChild<QQuickItem *>(QStringLiteral("saveError"));
    QVERIFY(error);
    QTRY_COMPARE(error->property("text").toString(),
                 QString("This address cannot be saved."));
    QVERIFY(error->property("visible").toBool());
    QVERIFY(QMetaObject::invokeMethod(
        browser, "noteRowFocus", Q_ARG(QVariant, QVariant(0)),
        Q_ARG(QVariant, QVariant(QStringLiteral("bookmark")))));
    QTRY_COMPARE(error->property("text").toString(), QString());
  }
  void homeDuringSlowOpenDoesNotCancel() {
    auto bus = QDBusConnection::sessionBus();
    BrowserDaemon daemon;
    daemon.holdOperation = true;
    QVERIFY(bus.registerService("org.arinn.Daemon1"));
    QVERIFY(bus.registerVirtualObject("/org/arinn", &daemon,
                                      QDBusConnection::SubPath));
    ShellStandIn backend;
    SessionGuardStandIn guard;
    QQmlApplicationEngine engine;
    QStringList problems;
    QObject::connect(
        &engine, &QQmlEngine::warnings, &engine,
        [&](const QList<QQmlError> &warnings) {
          for (const auto &warning : warnings) {
            const auto text = warning.toString();
            if (text.contains(QStringLiteral("qrc:/brand/")))
              continue;
            problems.append(text);
          }
        });
    engine.rootContext()->setContextProperty(QStringLiteral("backend"),
                                             &backend);
    engine.rootContext()->setContextProperty(QStringLiteral("sessionGuard"),
                                             &guard);
    engine.load(QUrl::fromLocalFile(QStringLiteral(ARINN_MAIN_QML)));
    QVERIFY2(problems.isEmpty(), qPrintable(problems.join('\n')));
    QVERIFY(!engine.rootObjects().isEmpty());
    auto window = qobject_cast<QQuickWindow *>(engine.rootObjects().first());
    QVERIFY(window);
    window->show();
    window->requestActivate();
    auto catalog = window->findChild<BrowserCatalog *>();
    QVERIFY(catalog);
    QTRY_VERIFY(!catalog->shortcuts().isEmpty());
    auto cancelButton =
        window->findChild<QQuickItem *>(QStringLiteral("launchCancel"));
    QVERIFY(cancelButton);
    QQuickItem *overlay = cancelButton;
    while (overlay && overlay->metaObject()->indexOfProperty("revealDelay") < 0)
      overlay = overlay->parentItem();
    QVERIFY(overlay);
    overlay->setProperty("revealDelay", 20);
    overlay->setProperty("slowDelay", 40);
    overlay->setProperty("timeoutDelay", 5000);
    catalog->openShortcut(QStringLiteral("known"));
    QTRY_VERIFY(catalog->opening());
    QTRY_VERIFY(overlay->property("slow").toBool());
    QVERIFY(overlay->isVisible());
    cancelButton->forceActiveFocus();
    QTRY_VERIFY(cancelButton->hasActiveFocus());
    QCOMPARE(daemon.cancels, 0);
    QTest::keyClick(window, Qt::Key_Home);
    QTRY_VERIFY(!overlay->isVisible());
    QCOMPARE(daemon.cancels, 0);
    QVERIFY(catalog->opening());
    QCOMPARE(catalog->openingError(), QString());
    QTest::qWait(200);
    QCOMPARE(daemon.cancels, 0);
    QVERIFY(catalog->opening());
  }
};

int runShortcutFixture(int argc, char **argv) {
  QCoreApplication app(argc, argv);
  BrowserDaemon daemon;
  daemon.scripted = true;
  daemon.recordAdded = false;
  daemon.scriptedWindows = {WindowWire{"w1", {}, true}};
  daemon.scriptedTabs = {
      TabWire{"page", "w1", "Page", "https://pages.example/watch", true, false}};
  auto bus = QDBusConnection::sessionBus();
  qDBusRegisterMetaType<WindowWire>();
  qDBusRegisterMetaType<TabWire>();
  qDBusRegisterMetaType<EffectWire>();
  qDBusRegisterMetaType<ProviderWire>();
  qDBusRegisterMetaType<ShortcutWire>();
  if (!bus.registerService("org.arinn.Daemon1") ||
      !bus.registerVirtualObject("/org/arinn", &daemon,
                                QDBusConnection::SubPath))
    return 1;
  const int fd = fileno(stdin);
  const int flags = fcntl(fd, F_GETFL, 0);
  if (flags >= 0)
    fcntl(fd, F_SETFL, flags | O_NONBLOCK);
  QByteArray pending;
  auto drain = [&] {
    char buf[256];
    while (true) {
      const auto n = ::read(fd, buf, sizeof buf);
      if (n <= 0)
        break;
      pending.append(buf, int(n));
    }
    while (true) {
      const auto nl = pending.indexOf('\n');
      if (nl < 0)
        break;
      const auto line = QString::fromUtf8(pending.left(nl)).trimmed();
      pending.remove(0, nl + 1);
      if (line == QLatin1String("hold-all")) {
        daemon.holdList = true;
        fprintf(stdout, "holding\n");
        fflush(stdout);
      } else if (line == QLatin1String("seed-other"))
        daemon.scriptedShortcuts = {
            ShortcutWire{"other", "Other", "https://other.example/a", {}}};
      else if (line == QLatin1String("release-stale"))
        daemon.releaseStaleLists();
      else if (line == QLatin1String("clear-and-release")) {
        daemon.holdList = false;
        daemon.scriptedShortcuts.clear();
        daemon.releaseLists();
        daemon.releaseStaleLists();
      } else if (line == QLatin1String("quit"))
        app.quit();
    }
  };
  QSocketNotifier notifier(fd, QSocketNotifier::Read);
  QObject::connect(&notifier, &QSocketNotifier::activated, &app,
                   [&](QSocketDescriptor, QSocketNotifier::Type) { drain(); });
  QTimer poll;
  poll.setInterval(20);
  QObject::connect(&poll, &QTimer::timeout, &app, drain);
  poll.start();
  fprintf(stdout, "ready\n");
  fflush(stdout);
  return app.exec();
}

int main(int argc, char **argv) {
  for (int i = 1; i < argc; ++i) {
    if (qstrcmp(argv[i], "--shortcut-fixture") == 0)
      return runShortcutFixture(argc, argv);
  }
  QGuiApplication app(argc, argv);
  BrowserCatalogTest test;
  return QTest::qExec(&test, argc, argv);
}
#include "browser_catalog_test.moc"
