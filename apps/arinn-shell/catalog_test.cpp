// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

#include "backend.h"
#include <QUuid>
#include <QtDBus>
#include <QtTest>

struct ProviderWire {
  QString id;
};
struct AppWire {
  QString id;
  bool enabled = true;
  QString group = "games";
};
struct RunningWire {
  QString id;
};
struct HomeWire {};
struct ShortcutWire {
  QString id, title, url, icon;
};
struct TargetWire {
  QString provider, capability, id;
};
Q_DECLARE_METATYPE(ProviderWire)
Q_DECLARE_METATYPE(AppWire)
Q_DECLARE_METATYPE(RunningWire)
Q_DECLARE_METATYPE(HomeWire)
Q_DECLARE_METATYPE(ShortcutWire)
Q_DECLARE_METATYPE(TargetWire)
QDBusArgument &operator<<(QDBusArgument &a, const ProviderWire &p) {
  a.beginStructure();
  a << p.id << QString("plugin") << QString() << QString("Provider")
    << QString("1") << QDBusObjectPath("/org/arinn/Provider/" + p.id)
    << QStringList{"application"} << QString("ready") << QString("available")
    << QString("generation") << qulonglong(1) << QString() << qulonglong(0)
    << qulonglong(0) << qulonglong(0) << qulonglong(0);
  a.endStructure();
  return a;
}
QDBusArgument &operator<<(QDBusArgument &a, const AppWire &p) {
  a.beginStructure();
  a << p.id << QString("Application") << p.enabled << p.group;
  a.endStructure();
  return a;
}
QDBusArgument &operator<<(QDBusArgument &a, const RunningWire &p) {
  a.beginStructure();
  a << p.id << QString("application") << QString("Window")
    << QString("window-token") << true << false;
  a.endStructure();
  return a;
}
QDBusArgument &operator<<(QDBusArgument &a, const HomeWire &) {
  a.beginStructure();
  a << QString() << QString() << QString() << QString() << QString();
  a.endStructure();
  return a;
}
// These fixture-only records are sent, never received.
const QDBusArgument &operator>>(const QDBusArgument &a, ProviderWire &) {
  return a;
}
const QDBusArgument &operator>>(const QDBusArgument &a, AppWire &) { return a; }
const QDBusArgument &operator>>(const QDBusArgument &a, RunningWire &) {
  return a;
}
const QDBusArgument &operator>>(const QDBusArgument &a, HomeWire &) {
  return a;
}
QDBusArgument &operator<<(QDBusArgument &a, const ShortcutWire &p) {
  a.beginStructure();
  a << p.id << p.title << p.url << p.icon;
  a.endStructure();
  return a;
}
QDBusArgument &operator<<(QDBusArgument &a, const TargetWire &p) {
  a.beginStructure();
  a << p.provider << p.capability << p.id;
  a.endStructure();
  return a;
}
const QDBusArgument &operator>>(const QDBusArgument &a, ShortcutWire &) {
  return a;
}
const QDBusArgument &operator>>(const QDBusArgument &a, TargetWire &) {
  return a;
}

class CatalogDaemon : public QDBusVirtualObject {
public:
  QString mode;
  QString returnedWindow;
  QString invokedMethod, invokedId, openInterface, openRequestId;
  QList<ShortcutWire> shortcuts;
  qulonglong shortcutRevision = 1;
  bool listError = false;
  bool operationDone = true;
  bool operationSuccess = false;
  int pages = 0;
  int launches = 0;
  int opens = 0;
  int lists = 0;
  int providerCalls = 0;
  QString introspect(const QString &) const override { return {}; }
  bool handleMessage(const QDBusMessage &m,
                     const QDBusConnection &bus) override {
    QDBusArgument a;
    if (m.member() == "Status") {
      a.beginStructure();
      a << QString("epoch") << true << QString("generation") << qulonglong(1)
        << true << returnedWindow << QString() << QString() << false << true
        << QString() << QString();
      a.endStructure();
    } else if (m.member() == "HomeItems") {
      a << QList<HomeWire>{};
    } else if (m.member() == "Providers") {
      ++providerCalls;
      if (mode == "error") {
        bus.send(m.createErrorReply(QDBusError::Failed, "Unavailable"));
        return true;
      }
      a.beginStructure();
      a << QString("snapshot")
        << (mode == "empty"
                ? QList<ProviderWire>{}
                : (mode == "partial" ? QList<ProviderWire>{{"broken"}, {"test"}}
                                     : QList<ProviderWire>{{"test"}}))
        << QString();
      a.endStructure();
    } else if (m.member() == "GetApplications") {
      if (m.path().endsWith("/broken")) {
        bus.send(m.createErrorReply(QDBusError::Failed, "Unavailable"));
        return true;
      }
      ++pages;
      bool first = m.arguments().at(0).toString().isEmpty();
      a.beginStructure();
      a << (mode == "generation" && !first ? QString("stale")
                                           : QString("generation"))
        << qulonglong(mode == "revision" && !first ? 2 : 1)
        << QList<AppWire>{{first ? "first" : "second", first,
                           first ? "games" : "future-group"}}
        << QList<RunningWire>{{first ? "window-first" : "window-second"}}
        << (first || mode == "cycle" ? QString("next") : QString());
      a.endStructure();
    } else if (m.member() == "Launch") {
      ++launches;
      invokedMethod = m.member();
      invokedId = m.arguments().at(1).toString();
      bus.send(
          m.createErrorReply(QDBusError::Failed, "fixture operation failure"));
      return true;
    } else if (m.member() == "List") {
      ++lists;
      if (listError) {
        bus.send(m.createErrorReply(QDBusError::Failed, "list failed"));
        return true;
      }
      a.beginStructure();
      a << shortcutRevision << shortcuts;
      a.endStructure();
    } else if (m.member() == "Open") {
      ++opens;
      openInterface = m.interface();
      invokedMethod = m.member();
      invokedId = m.arguments().at(0).toString();
      openRequestId = m.arguments().at(1).toString();
      bus.send(m.createReply(QVariant::fromValue(
          QDBusObjectPath("/org/arinn/Daemon1/operations/1"))));
      return true;
    } else if (m.member() == "Get") {
      const bool done = operationDone;
      a.beginStructure();
      a << (done ? (operationSuccess ? QString("succeeded") : QString("failed"))
                 : QString("running"))
        << (done && !operationSuccess ? QString("Unavailable") : QString())
        << (done && !operationSuccess) << qulonglong(1)
        << qulonglong(done ? 2 : 0) << QList<TargetWire>{} << false;
      a.endStructure();
    } else {
      bus.send(m.createErrorReply(QDBusError::NotSupported, "fixture"));
      return true;
    }
    bus.send(m.createReply(QVariant::fromValue(a)));
    return true;
  }
};
class CatalogTest : public QObject {
  Q_OBJECT
private slots:
  void initTestCase() {
    qDBusRegisterMetaType<ProviderWire>();
    qDBusRegisterMetaType<AppWire>();
    qDBusRegisterMetaType<RunningWire>();
    qDBusRegisterMetaType<HomeWire>();
    qDBusRegisterMetaType<ShortcutWire>();
    qDBusRegisterMetaType<TargetWire>();
  }
  void catalog_data() {
    QTest::addColumn<QString>("mode");
    for (const auto &mode : {"populated", "empty", "error", "generation",
                             "revision", "cycle", "partial"})
      QTest::newRow(mode) << QString(mode);
  }
  void catalog() {
    QFETCH(QString, mode);
    auto bus = QDBusConnection::sessionBus();
    CatalogDaemon daemon;
    daemon.mode = mode;
    QVERIFY(bus.registerService("org.arinn.Daemon1"));
    QVERIFY(bus.registerVirtualObject("/org/arinn", &daemon,
                                      QDBusConnection::SubPath));
    {
      Backend backend;
      QTRY_VERIFY(backend.connected());
      const auto state = (mode == "populated" || mode == "partial") ? "ready"
                         : mode == "empty"                          ? "empty"
                                                                    : "error";
      const auto applicationsState = mode == "populated" ? "ready"
                                     : mode == "empty"   ? "empty"
                                                         : "error";
      QTRY_COMPARE(backend.homeState(), QString(state));
      QTRY_COMPARE(backend.applicationsState(), QString(applicationsState));
      if (mode == "populated" || mode == "partial") {
        QTRY_COMPARE(backend.applications().size(), 2);
        QCOMPARE(backend.items().size(), 2);
        QCOMPARE(backend.items()[1].toMap()["localId"].toString(),
                 QString("window-second"));
        QCOMPARE(daemon.pages, 2);
        QCOMPARE(backend.applications()[1].toMap()["enabled"].toBool(), false);
        // Provider groups pass through; unknown groups display as "other".
        QCOMPARE(backend.applications()[0].toMap()["group"].toString(),
                 QString("games"));
        QCOMPARE(backend.applications()[1].toMap()["group"].toString(),
                 QString("other"));
        // Shortcuts lead the Home row; application tiles follow them.
        const auto shortcutCount = backend.webShortcuts().size();
        QCOMPARE(backend.favorites().size(),
                 shortcutCount + backend.applications().size());
        QCOMPARE(backend.favorites()
                     .at(shortcutCount)
                     .toMap()
                     .value("id")
                     .toString(),
                 backend.applications().at(0).toMap().value("id").toString());
        QSignalSpy changes(&backend, &Backend::applicationsChanged);
        backend.refresh();
        QTest::qWait(200);
        QCOMPARE(changes.count(), 0);
        backend.launch(backend.applications()[1].toMap()["id"].toString());
        QCOMPARE(daemon.launches, 0);
        backend.launch(backend.applications()[0].toMap()["id"].toString());
        backend.launch(backend.applications()[0].toMap()["id"].toString());
        QTRY_COMPARE(daemon.invokedId, QString("first"));
        QCOMPARE(daemon.launches, 1);
        QCOMPARE(daemon.opens, 0);
        QTRY_COMPARE(backend.message(), QString("fixture operation failure"));
        daemon.returnedWindow = "opaque-window";
        backend.refresh();
        QTRY_COMPARE(backend.continueEntries().size(), 1);
        daemon.returnedWindow.clear();
        backend.refresh();
        QTRY_VERIFY(backend.continueEntries().isEmpty());
      } else
        QVERIFY(backend.applications().isEmpty());
      if (mode == "error") {
        // A failed catalog stays an application error while shortcuts load.
        // It must not be reported as an empty shortcut list.
        QCOMPARE(backend.homeState(), QString("error"));
        QCOMPARE(backend.applicationsState(), QString("error"));
        QVERIFY(backend.webShortcuts().isEmpty());
        daemon.shortcuts = {{QStringLiteral("keep"), QStringLiteral("Keep"),
                             QStringLiteral("https://example.test/keep"),
                             QString()}};
        daemon.shortcutRevision = 3;
        const int providersBefore = daemon.providerCalls;
        auto signal = QDBusMessage::createSignal(
            "/org/arinn/Daemon1", "org.arinn.WebShortcuts1", "Changed");
        signal << qulonglong(3);
        QVERIFY(bus.send(signal));
        QTRY_COMPARE(backend.webShortcuts().size(), 1);
        QCOMPARE(backend.homeState(), QString("ready"));
        QCOMPARE(backend.applicationsState(), QString("error"));
        QVERIFY(backend.applications().isEmpty());
        QCOMPARE(backend.favorites().at(0).toMap().value("id").toString(),
                 QString("keep"));
        QCOMPARE(daemon.providerCalls, providersBefore);
        daemon.shortcuts.clear();
        daemon.shortcutRevision = 4;
        auto cleared = QDBusMessage::createSignal(
            "/org/arinn/Daemon1", "org.arinn.WebShortcuts1", "Changed");
        cleared << qulonglong(4);
        QVERIFY(bus.send(cleared));
        QTRY_VERIFY(backend.webShortcuts().isEmpty());
        QCOMPARE(backend.homeState(), QString("error"));
      }
      daemon.mode = "empty";
      backend.refresh();
      QTRY_COMPARE(backend.homeState(), QString("empty"));
      bus.unregisterService("org.arinn.Daemon1");
      QTRY_COMPARE(backend.homeState(), QString("unavailable"));
      QVERIFY(backend.applications().isEmpty());
      QVERIFY(backend.returnWindow().isEmpty());
      QVERIFY(bus.registerService("org.arinn.Daemon1"));
      QTRY_COMPARE(backend.homeState(), QString("empty"));
    }
    bus.unregisterObject("/org/arinn", QDBusConnection::UnregisterTree);
    bus.unregisterService("org.arinn.Daemon1");
  }
  void webShortcuts() {
    auto bus = QDBusConnection::sessionBus();
    CatalogDaemon daemon;
    daemon.mode = "empty";
    {
      Backend backend;
      QTRY_COMPARE(backend.homeState(), QString("unavailable"));
      QStringList states;
      QObject::connect(&backend, &Backend::changed, &backend,
                       [&] { states << backend.homeState(); });
      QVERIFY(bus.registerVirtualObject("/org/arinn", &daemon,
                                        QDBusConnection::SubPath));
      QVERIFY(bus.registerService("org.arinn.Daemon1"));
      QTRY_COMPARE(backend.homeState(), QString("empty"));
      QVERIFY(states.contains(QString("loading")));
      QVERIFY(backend.webShortcuts().isEmpty());

      const auto north = QStringLiteral("https://example.test/north");
      const QString emoji = QString(QChar(0xD83C)) + QChar(0xDFAE);
      const QString broken(QChar(0xD800));
      daemon.shortcuts = {{QStringLiteral("north"), QStringLiteral("north"),
                           north, QStringLiteral("packaged-mark")},
                          {QStringLiteral("south"), QStringLiteral("South"),
                           QStringLiteral("https://example.test/south"),
                           QString()},
                          {QStringLiteral("emoji"), emoji,
                           QStringLiteral("https://example.test/emoji"),
                           QString()},
                          {QStringLiteral("broken"), broken,
                           QStringLiteral("https://example.test/broken"),
                           QString()}};
      daemon.shortcutRevision = 2;
      const int listsBefore = daemon.lists;
      const int providersBefore = daemon.providerCalls;
      auto signal = QDBusMessage::createSignal(
          "/org/arinn/Daemon1", "org.arinn.WebShortcuts1", "Changed");
      signal << qulonglong(2);
      QVERIFY(bus.send(signal));
      QTRY_COMPARE(backend.webShortcuts().size(), 4);
      QCOMPARE(daemon.lists, listsBefore + 1);
      QCOMPARE(daemon.providerCalls, providersBefore);
      const auto first = backend.webShortcuts().at(0).toMap();
      QCOMPARE(first.value("id").toString(), QString("north"));
      QCOMPARE(first.value("title").toString(), QString("north"));
      QCOMPARE(first.value("monogram").toString(), QString("N"));
      QCOMPARE(
          backend.webShortcuts().at(1).toMap().value("monogram").toString(),
          QString("S"));
      const auto emojiEntry = backend.webShortcuts().at(2).toMap();
      QCOMPARE(emojiEntry.value("monogram").toString(), emoji.toUpper());
      QVERIFY(emojiEntry.value("monogram").toString() !=
              QString(emoji.at(0)));
      QCOMPARE(
          backend.webShortcuts().at(3).toMap().value("monogram").toString(),
          QString());
      for (const auto &entry : backend.webShortcuts()) {
        const auto map = entry.toMap();
        QVERIFY(!map.contains("url"));
        QVERIFY(!map.contains("icon"));
        for (const auto &value : map)
          QVERIFY(!value.toString().contains(QStringLiteral("example.test")));
      }
      QCOMPARE(backend.homeState(), QString("ready"));

      QSignalSpy same(&backend, &Backend::webShortcutsChanged);
      backend.refresh();
      QTest::qWait(200);
      QCOMPARE(same.count(), 0);

      daemon.operationDone = false;
      const auto id = first.value("id").toString();
      backend.launch(id);
      backend.launch(id);
      QTRY_COMPARE(daemon.opens, 1);
      QCOMPARE(daemon.launches, 0);
      QCOMPARE(daemon.invokedMethod, QString("Open"));
      QCOMPARE(daemon.openInterface, QString("org.arinn.WebShortcuts1"));
      QCOMPARE(daemon.invokedId, id);
      QVERIFY(!QUuid::fromString(daemon.openRequestId).isNull());
      QTRY_VERIFY(backend.busyEntryIds().contains(id));
      daemon.operationDone = true;
      QTRY_COMPARE(backend.message(), QString("Unavailable"));
      QTRY_VERIFY(backend.busyEntryIds().isEmpty());

      daemon.operationSuccess = true;
      backend.launch(
          backend.webShortcuts().at(1).toMap().value("id").toString());
      QTRY_COMPARE(backend.message(), QString("Done"));
      QCOMPARE(daemon.opens, 2);
      QCOMPARE(daemon.launches, 0);

      backend.launch(QStringLiteral("missing"));
      QCOMPARE(backend.message(), QString("Shortcut is no longer available"));
      QCOMPARE(daemon.opens, 2);

      const int listsAfterOpen = daemon.lists;
      auto focus = QDBusMessage::createSignal(
          "/org/arinn/Daemon1", "org.arinn.Daemon1", "StateChanged");
      focus << QStringLiteral("test") << QStringLiteral("browser")
            << QStringLiteral("generation") << qulonglong(1);
      QVERIFY(bus.send(focus));
      QTest::qWait(300);
      QCOMPARE(daemon.lists, listsAfterOpen);

      daemon.listError = true;
      backend.refresh();
      QTRY_COMPARE(backend.homeState(), QString("error"));
      QCOMPARE(backend.applicationsState(), QString("empty"));
      QVERIFY(backend.webShortcuts().isEmpty());

      bus.unregisterService("org.arinn.Daemon1");
      QTRY_COMPARE(backend.homeState(), QString("unavailable"));
      QVERIFY(backend.webShortcuts().isEmpty());
    }
    bus.unregisterObject("/org/arinn", QDBusConnection::UnregisterTree);
    bus.unregisterService("org.arinn.Daemon1");
  }
};
QTEST_GUILESS_MAIN(CatalogTest)
#include "catalog_test.moc"
