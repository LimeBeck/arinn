// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

#include "backend.h"
#include <QtDBus>
#include <QtTest>

struct ProviderWire {
  QString id;
};
struct AppWire {
  QString id;
  bool enabled = true;
};
struct RunningWire {
  QString id;
};
struct HomeWire {};
Q_DECLARE_METATYPE(ProviderWire)
Q_DECLARE_METATYPE(AppWire)
Q_DECLARE_METATYPE(RunningWire)
Q_DECLARE_METATYPE(HomeWire)
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
  a << p.id << QString("Application") << p.enabled;
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

class CatalogDaemon : public QDBusVirtualObject {
public:
  QString mode;
  QString returnedWindow;
  QString invokedMethod, invokedId;
  int pages = 0;
  int launches = 0;
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
        << QList<AppWire>{{first ? "first" : "second", first}}
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
      QTRY_COMPARE(backend.homeState(), QString(state));
      if (mode == "populated" || mode == "partial") {
        QTRY_COMPARE(backend.applications().size(), 2);
        QCOMPARE(backend.items().size(), 2);
        QCOMPARE(backend.items()[1].toMap()["localId"].toString(),
                 QString("window-second"));
        QCOMPARE(daemon.pages, 2);
        QCOMPARE(backend.applications()[1].toMap()["enabled"].toBool(), false);
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
        QTRY_COMPARE(backend.message(), QString("fixture operation failure"));
        daemon.returnedWindow = "opaque-window";
        backend.refresh();
        QTRY_COMPARE(backend.continueEntries().size(), 1);
        daemon.returnedWindow.clear();
        backend.refresh();
        QTRY_VERIFY(backend.continueEntries().isEmpty());
      } else
        QVERIFY(backend.applications().isEmpty());
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
};
QTEST_GUILESS_MAIN(CatalogTest)
#include "catalog_test.moc"
