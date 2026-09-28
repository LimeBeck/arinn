// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

#include "backend.h"
#include "dbussessionlifecycle.h"
#include <QDBusArgument>
#include <QDBusContext>
#include <QDBusError>
#include <QDBusMessage>
#include <QDBusPendingReply>
#include <QDBusVariant>
#include <QDBusVirtualObject>
#include <QQmlApplicationEngine>
#include <QQmlContext>
#include <QQuickWindow>
#include <QtTest>
class LockService : public QObject, protected QDBusContext {
  Q_OBJECT
  Q_CLASSINFO("D-Bus Interface", "org.freedesktop.ScreenSaver")
public:
  bool active = false;
  bool delay = false;
  QList<QDBusMessage> pending;
  void reply(bool value) {
    connectionForReplies.send(pending.takeFirst().createReply(value));
  }
  QDBusConnection connectionForReplies = QDBusConnection::sessionBus();
public slots:
  bool GetActive() {
    if (delay) {
      setDelayedReply(true);
      pending.append(message());
      connectionForReplies = connection();
    }
    return active;
  }
signals:
  void ActiveChanged(bool active);
};
class SleepService : public QObject {
  Q_OBJECT
  Q_CLASSINFO("D-Bus Interface", "org.freedesktop.login1.Manager")
signals:
  void PrepareForSleep(bool sleeping);
};
// Supplies an initial Home state, then simulates a stalled daemon.
class StalledDaemon : public QDBusVirtualObject {
public:
  int requests = 0;
  bool stalled = false;
  QString introspect(const QString &) const override { return {}; }
  bool handleMessage(const QDBusMessage &message,
                     const QDBusConnection &bus) override {
    ++requests;
    if (stalled) {
      message.setDelayedReply(true);
      return true;
    }
    if (message.member() == "Status") {
      QDBusArgument state;
      state.beginStructure();
      state << QString("epoch") << false << QString("generation")
            << qulonglong(1) << true << QString() << QString() << QString()
            << false << true << QString() << QString();
      state.endStructure();
      bus.send(message.createReply(QVariant::fromValue(state)));
    } else {
      bus.send(message.createErrorReply(QDBusError::NotSupported, "fixture"));
    }
    return true;
  }
};
class SessionLifecycleTest : public QObject {
  Q_OBJECT
private slots:
  void cleanup() {
    for (const auto &name : {"lifecycle-services", "delayed-lock"})
      QDBusConnection::disconnectFromBus(name);
    auto bus = QDBusConnection::sessionBus();
    bus.unregisterService("org.freedesktop.ScreenSaver");
    bus.unregisterService("org.freedesktop.login1");
    bus.unregisterObject("/ScreenSaver");
    bus.unregisterObject("/org/freedesktop/login1");
  }
  void daemonIndependentVisibility_data() {
    QTest::addColumn<bool>("stalled");
    QTest::newRow("daemon-absent") << false;
    QTest::newRow("daemon-unresponsive") << true;
  }
  void daemonIndependentVisibility() {
    QFETCH(bool, stalled);
    auto bus = QDBusConnection::sessionBus();
    auto server = QDBusConnection::connectToBus(QDBusConnection::SessionBus,
                                                "lifecycle-services");
    LockService lock;
    SleepService sleep;
    StalledDaemon daemon;
    QVERIFY(server.registerService("org.freedesktop.ScreenSaver"));
    QVERIFY(server.registerService("org.freedesktop.login1"));
    QVERIFY(server.registerObject("/ScreenSaver", &lock,
                                  QDBusConnection::ExportAllSlots |
                                      QDBusConnection::ExportAllSignals));
    QVERIFY(server.registerObject("/org/freedesktop/login1", &sleep,
                                  QDBusConnection::ExportAllSignals));
    QVERIFY(server.registerService("org.arinn.Daemon1"));
    QVERIFY(server.registerVirtualObject("/org/arinn/Daemon1", &daemon));
    Backend backend;
    DbusSessionLifecycle guard(nullptr, bus, bus);
    QQmlApplicationEngine engine;
    engine.rootContext()->setContextProperty("backend", &backend);
    engine.rootContext()->setContextProperty("sessionGuard", &guard);
    engine.load(QUrl("qrc:/Main.qml"));
    QVERIFY(!engine.rootObjects().isEmpty());
    auto window = qobject_cast<QQuickWindow *>(engine.rootObjects().first());
    QVERIFY(window);
    QTRY_VERIFY(window->isVisible());
    QTRY_VERIFY(backend.connected());
    if (stalled) {
      daemon.stalled = true;
      const auto previous = daemon.requests;
      backend.refresh();
      QTRY_VERIFY(daemon.requests > previous);
    } else {
      QVERIFY(server.unregisterService("org.arinn.Daemon1"));
      QTRY_VERIFY(!backend.connected());
    }
    emit lock.ActiveChanged(true);
    QTRY_VERIFY_WITH_TIMEOUT(!window->isVisible(), 500);
    emit lock.ActiveChanged(false);
    QTRY_VERIFY_WITH_TIMEOUT(window->isVisible(), 500);
    emit sleep.PrepareForSleep(true);
    QTRY_VERIFY_WITH_TIMEOUT(!window->isVisible(), 500);
    lock.delay = true;
    emit sleep.PrepareForSleep(false);
    QTRY_COMPARE(lock.pending.size(), 1);
    QVERIFY(!window->isVisible());
    lock.reply(false);
    QTRY_VERIFY_WITH_TIMEOUT(window->isVisible(), 500);
    server.unregisterService("org.freedesktop.ScreenSaver");
    QTRY_VERIFY_WITH_TIMEOUT(!window->isVisible(), 500);
    server.unregisterObject("/ScreenSaver");
    server.unregisterObject("/org/freedesktop/login1");
    server.unregisterService("org.freedesktop.login1");
    server.unregisterObject("/org/arinn/Daemon1");
    if (stalled)
      server.unregisterService("org.arinn.Daemon1");
    QDBusConnection::disconnectFromBus("lifecycle-services");
  }
  void staleReplies_data() {
    QTest::addColumn<int>("event");
    QTest::newRow("lock-signal") << 0;
    QTest::newRow("owner-loss") << 1;
    QTest::newRow("sleep-wake") << 2;
  }
  void staleReplies() {
    QFETCH(int, event);
    auto client = QDBusConnection::sessionBus();
    auto server = QDBusConnection::connectToBus(QDBusConnection::SessionBus,
                                                "delayed-lock");
    LockService lock;
    lock.delay = true;
    SleepService sleep;
    QVERIFY(server.registerService("org.freedesktop.ScreenSaver"));
    QVERIFY(server.registerService("org.freedesktop.login1"));
    QVERIFY(server.registerObject("/ScreenSaver", &lock,
                                  QDBusConnection::ExportAllSlots |
                                      QDBusConnection::ExportAllSignals));
    QVERIFY(server.registerObject("/org/freedesktop/login1", &sleep,
                                  QDBusConnection::ExportAllSignals));
    DbusSessionLifecycle guard(nullptr, client, client);
    QSignalSpy changed(&guard, &SessionLifecycle::changed);
    QVERIFY(guard.blocked());
    QTRY_COMPARE(lock.pending.size(), 1);
    if (event == 0) {
      emit lock.ActiveChanged(true);
      QTRY_VERIFY(changed.count() > 0);
    } else if (event == 1) {
      QVERIFY(server.unregisterService("org.freedesktop.ScreenSaver"));
      QTRY_VERIFY(changed.count() > 0);
    } else {
      emit sleep.PrepareForSleep(true);
      QTRY_VERIFY(changed.count() > 0);
      changed.clear();
      emit sleep.PrepareForSleep(false);
      QTRY_COMPARE(lock.pending.size(), 2);
    }
    // This old unlocked answer must not undo a newer signal/owner/wake epoch.
    lock.reply(false);
    QTest::qWait(100);
    QVERIFY(guard.blocked());
    if (event == 1) {
      QVERIFY(server.registerService("org.freedesktop.ScreenSaver"));
      QTRY_COMPARE(lock.pending.size(), 1);
      lock.reply(false);
    } else if (event == 2) {
      lock.reply(false);
    } else {
      emit lock.ActiveChanged(false);
    }
    QTRY_VERIFY(!guard.blocked());
    server.unregisterObject("/ScreenSaver");
    server.unregisterObject("/org/freedesktop/login1");
    server.unregisterService("org.freedesktop.ScreenSaver");
    server.unregisterService("org.freedesktop.login1");
    QDBusConnection::disconnectFromBus("delayed-lock");
  }
  void unavailableLockState() {
    auto bus = QDBusConnection::sessionBus();
    DbusSessionLifecycle guard(nullptr, bus, bus);
    QSignalSpy changed(&guard, &SessionLifecycle::changed);
    QVERIFY(guard.blocked());
    QTRY_VERIFY(changed.count() > 0);
    QVERIFY(guard.blocked());
    // An owned service without GetActive is also unknown, not unlocked.
    changed.clear();
    QVERIFY(bus.registerService("org.freedesktop.ScreenSaver"));
    QTRY_VERIFY(changed.count() >= 2);
    QVERIFY(guard.blocked());
    bus.unregisterService("org.freedesktop.ScreenSaver");
  }
  void sessionSignals() {
    auto bus = QDBusConnection::sessionBus();
    LockService lock;
    SleepService sleep;
    QVERIFY(bus.registerService("org.freedesktop.ScreenSaver"));
    QVERIFY(bus.registerService("org.freedesktop.login1"));
    QVERIFY(bus.registerObject("/ScreenSaver", &lock,
                               QDBusConnection::ExportAllSlots |
                                   QDBusConnection::ExportAllSignals));
    QVERIFY(bus.registerObject("/org/freedesktop/login1", &sleep,
                               QDBusConnection::ExportAllSignals));
    DbusSessionLifecycle guard(nullptr, bus, bus);
    QVERIFY(bus.registerObject("/org/arinn/Session1", &guard,
                               QDBusConnection::ExportAllProperties));
    QTRY_VERIFY(!guard.blocked());
    auto request = QDBusMessage::createMethodCall(
        bus.baseService(), "/org/arinn/Session1",
        "org.freedesktop.DBus.Properties", "Get");
    request << "org.arinn.Session1" << "blocked";
    auto pending = bus.asyncCall(request);
    QTRY_VERIFY(pending.isFinished());
    QDBusPendingReply<QDBusVariant> reply = pending;
    QVERIFY(!reply.isError());
    QCOMPARE(reply.value().variant().toBool(), false);
    bus.unregisterObject("/org/arinn/Session1");
    lock.active = true;
    emit lock.ActiveChanged(true);
    QTRY_VERIFY(guard.blocked());
    lock.active = false;
    emit lock.ActiveChanged(false);
    QTRY_VERIFY(!guard.blocked());
    emit sleep.PrepareForSleep(true);
    QTRY_VERIFY(guard.blocked());
    emit lock.ActiveChanged(false);
    QTest::qWait(50);
    QVERIFY(guard.blocked());
    emit sleep.PrepareForSleep(false);
    QTRY_VERIFY(!guard.blocked());
    bus.unregisterService("org.freedesktop.ScreenSaver");
    QTRY_VERIFY(guard.blocked());
    QVERIFY(bus.registerService("org.freedesktop.ScreenSaver"));
    QTRY_VERIFY(!guard.blocked());
    bus.unregisterObject("/ScreenSaver");
    bus.unregisterObject("/org/freedesktop/login1");
    bus.unregisterService("org.freedesktop.ScreenSaver");
    bus.unregisterService("org.freedesktop.login1");
  }
};
QTEST_MAIN(SessionLifecycleTest)
#include "sessionlifecycle_test.moc"
