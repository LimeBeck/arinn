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
#include <QKeyEvent>
#include <QQuickItem>
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
// Repeater delegates are in the item tree, not always QObject children.
QQuickItem *findItem(QQuickItem *root, const QString &name) {
  if (root->objectName() == name)
    return root;
  for (auto *child : root->childItems())
    if (auto *found = findItem(child, name))
      return found;
  return nullptr;
}
// Serves Home like StalledDaemon and accepts SystemControl1 power requests
// (ADR-0108). It never suspends anything.
class PowerDaemon : public StalledDaemon {
public:
  QStringList calls;
  bool handleMessage(const QDBusMessage &message,
                     const QDBusConnection &bus) override {
    if (message.interface() != "org.arinn.SystemControl1")
      return StalledDaemon::handleMessage(message, bus);
    calls.append(message.member());
    if (message.member() == "Suspend")
      bus.send(message.createReply());
    else
      bus.send(message.createErrorReply(QDBusError::NotSupported, "fixture"));
    return true;
  }
};
class EndpointLock : public QDBusVirtualObject {
public:
  bool active = false;
  QString introspect(const QString &) const override { return {}; }
  bool handleMessage(const QDBusMessage &message,
                     const QDBusConnection &bus) override {
    if (message.member() != "GetActive")
      return false;
    bus.send(message.createReply(active));
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
    bus.unregisterService("org.xfce.ScreenSaver");
    bus.unregisterObject("/org/freedesktop/ScreenSaver");
    bus.unregisterObject("/org/xfce/ScreenSaver");
    bus.unregisterService("org.freedesktop.login1");
    bus.unregisterObject("/ScreenSaver");
    bus.unregisterObject("/org/freedesktop/login1");
  }
  void supportedEndpoints_data() {
    QTest::addColumn<QString>("service");
    QTest::addColumn<QString>("path");
    QTest::newRow("canonical") << QString("org.freedesktop.ScreenSaver")
                               << QString("/org/freedesktop/ScreenSaver");
    QTest::newRow("legacy")
        << QString("org.freedesktop.ScreenSaver") << QString("/ScreenSaver");
    QTest::newRow("xfce") << QString("org.xfce.ScreenSaver")
                          << QString("/org/xfce/ScreenSaver");
  }
  void supportedEndpoints() {
    QFETCH(QString, service);
    QFETCH(QString, path);
    auto bus = QDBusConnection::sessionBus();
    EndpointLock lock;
    QVERIFY(bus.registerService(service));
    QVERIFY(bus.registerVirtualObject(path, &lock));
    DbusSessionLifecycle guard(nullptr, bus, bus);
    QTRY_VERIFY(!guard.blocked());
    auto signal = QDBusMessage::createSignal(path, service, "ActiveChanged");
    signal << true;
    QVERIFY(bus.send(signal));
    QTRY_VERIFY(guard.blocked());
    signal.setArguments({false});
    QVERIFY(bus.send(signal));
    QTRY_VERIFY(!guard.blocked());
    QVERIFY(bus.unregisterService(service));
    QTRY_VERIFY(guard.blocked());
    bus.unregisterObject(path);
  }
  void primaryEndpointWins() {
    auto bus = QDBusConnection::sessionBus();
    EndpointLock primary, fallback;
    primary.active = true;
    QVERIFY(bus.registerService("org.freedesktop.ScreenSaver"));
    QVERIFY(bus.registerService("org.xfce.ScreenSaver"));
    QVERIFY(
        bus.registerVirtualObject("/org/freedesktop/ScreenSaver", &primary));
    QVERIFY(bus.registerVirtualObject("/org/xfce/ScreenSaver", &fallback));
    DbusSessionLifecycle guard(nullptr, bus, bus);
    QTest::qWait(100);
    QVERIFY(guard.blocked());
    auto signal = QDBusMessage::createSignal(
        "/org/xfce/ScreenSaver", "org.xfce.ScreenSaver", "ActiveChanged");
    signal << false;
    QVERIFY(bus.send(signal));
    QTest::qWait(50);
    QVERIFY(guard.blocked());
    primary.active = false;
    signal = QDBusMessage::createSignal("/org/freedesktop/ScreenSaver",
                                        "org.freedesktop.ScreenSaver",
                                        "ActiveChanged");
    signal << false;
    QVERIFY(bus.send(signal));
    QTRY_VERIFY(!guard.blocked());
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
  // #148, §5.7: the power menu opened from navigation is the power scope;
  // an accepted Sleep shows only a waiting state, and the lifecycle guard,
  // not a timer, hides the shell. After wake the screen beneath is back.
  void powerMenuAcrossSleep() {
    auto bus = QDBusConnection::sessionBus();
    auto server = QDBusConnection::connectToBus(QDBusConnection::SessionBus,
                                                "lifecycle-services");
    LockService lock;
    SleepService sleep;
    PowerDaemon daemon;
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
    window->requestActivate();
    auto home = window->findChild<QQuickItem *>("productionHome");
    auto overlay = window->findChild<QQuickItem *>("powerOverlay");
    QVERIFY(home);
    QVERIFY(overlay);
    QVERIFY(QMetaObject::invokeMethod(home, "powerRequested"));
    QTRY_COMPARE(window->property("focusedScreen").toString(),
                 QString("power"));
    QTRY_VERIFY(daemon.calls.contains("GetPower"));
    auto sleepRow = findItem(window->contentItem(), "sleepButton");
    QVERIFY(sleepRow);
    QTRY_VERIFY(sleepRow->hasActiveFocus());
    // Home beneath is inert while the power scope is open.
    QVERIFY(!home->isEnabled());
    // A held Back is one step (§6.3): its repeats neither close the
    // confirmation nor unwind the scope beneath.
    auto menu = findItem(window->contentItem(), "powerMenu");
    QVERIFY(menu);
    QTest::keyClick(window, Qt::Key_Down);
    QTest::keyClick(window, Qt::Key_Return);
    QTRY_COMPARE(menu->property("step").toString(), QString("confirm"));
    for (int i = 0; i < 3; ++i) {
      QKeyEvent repeat(QEvent::KeyPress, Qt::Key_Escape, Qt::NoModifier,
                       QString(), true);
      QCoreApplication::sendEvent(window, &repeat);
    }
    QTest::qWait(100);
    QCOMPARE(menu->property("step").toString(), QString("confirm"));
    QTest::keyClick(window, Qt::Key_Escape);
    QTRY_COMPARE(menu->property("step").toString(), QString("menu"));
    QTest::keyClick(window, Qt::Key_Escape);
    QTRY_COMPARE(window->property("focusedScreen").toString(), QString("home"));
    for (int i = 0; i < 3; ++i) {
      QKeyEvent repeat(QEvent::KeyPress, Qt::Key_Escape, Qt::NoModifier,
                       QString(), true);
      QCoreApplication::sendEvent(window, &repeat);
    }
    QTest::qWait(100);
    QVERIFY(!window->property("navOpen").toBool());
    QVERIFY(QMetaObject::invokeMethod(home, "powerRequested"));
    // A new opening starts on the first row, not the last one used.
    QTRY_VERIFY(sleepRow->hasActiveFocus());
    QTest::keyClick(window, Qt::Key_Return);
    QTRY_VERIFY(daemon.calls.contains("Suspend"));
    auto accepted = findItem(window->contentItem(), "powerAccepted");
    QVERIFY(accepted);
    QTRY_VERIFY(accepted->isVisible());
    QVERIFY(overlay->property("open").toBool());
    emit sleep.PrepareForSleep(true);
    QTRY_VERIFY_WITH_TIMEOUT(!window->isVisible(), 500);
    QTRY_VERIFY(!overlay->property("open").toBool());
    emit sleep.PrepareForSleep(false);
    QTRY_VERIFY_WITH_TIMEOUT(window->isVisible(), 1000);
    QTRY_COMPARE(window->property("focusedScreen").toString(),
                 QString("home"));
    QCOMPARE(daemon.calls.count("Suspend"), 1);
    server.unregisterObject("/ScreenSaver");
    server.unregisterObject("/org/freedesktop/login1");
    server.unregisterService("org.freedesktop.login1");
    server.unregisterObject("/org/arinn/Daemon1");
    server.unregisterService("org.arinn.Daemon1");
    QDBusConnection::disconnectFromBus("lifecycle-services");
  }
  // #307, §5.7: idle on Home starts ambient; the first key is consumed and
  // Home's focus is back; a lock ends ambient without a wake.
  void ambientAfterIdle() {
    auto bus = QDBusConnection::sessionBus();
    auto server = QDBusConnection::connectToBus(QDBusConnection::SessionBus,
                                                "lifecycle-services");
    LockService lock;
    SleepService sleep;
    PowerDaemon daemon;
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
    window->requestActivate();
    QTRY_VERIFY(window->isActive());
    auto idle = window->findChild<QObject *>("ambientIdle");
    auto ambient = findItem(window->contentItem(), "ambientScreen");
    QVERIFY(idle);
    QVERIFY(ambient);
    QCOMPARE(idle->property("timeout").toInt(), 600000);
    QTRY_VERIFY(window->activeFocusItem());
    QTRY_COMPARE(window->property("focusedScreen").toString(),
                 QString("home"));
    // Explicit start from the power menu: wake returns to the menu row.
    auto home = findItem(window->contentItem(), "productionHome");
    QVERIFY(QMetaObject::invokeMethod(home, "powerRequested"));
    auto ambientRow = findItem(window->contentItem(), "ambientButton");
    QVERIFY(ambientRow);
    QTRY_VERIFY(findItem(window->contentItem(), "sleepButton")->hasActiveFocus());
    for (int i = 0; i < 3; ++i)
      QTest::keyClick(window, Qt::Key_Down);
    QTRY_VERIFY(ambientRow->hasActiveFocus());
    QTest::keyClick(window, Qt::Key_Return);
    QTRY_VERIFY(ambient->property("active").toBool());
    QCOMPARE(window->property("focusedScreen").toString(), QString("ambient"));
    QTRY_VERIFY(ambient->property("armed").toBool());
    QTest::keyClick(window, Qt::Key_Return);
    QTRY_VERIFY(!ambient->property("active").toBool());
    QTRY_VERIFY(ambientRow->hasActiveFocus());
    QCOMPARE(window->property("focusedScreen").toString(), QString("power"));
    QTest::keyClick(window, Qt::Key_Escape);
    QTRY_COMPARE(window->property("focusedScreen").toString(),
                 QString("home"));
    QTRY_VERIFY(window->activeFocusItem());
    auto focused = window->activeFocusItem();
    idle->setProperty("timeout", 300);
    QTRY_VERIFY(idle->property("counting").toBool());
    QTRY_VERIFY_WITH_TIMEOUT(ambient->property("active").toBool(), 3000);
    QCOMPARE(window->property("focusedScreen").toString(), QString("ambient"));
    QTRY_VERIFY(ambient->property("armed").toBool());
    // The wake key does nothing else: Home's focus stays where it was.
    QTest::keyClick(window, Qt::Key_Down);
    QTRY_VERIFY(!ambient->property("active").toBool());
    QTRY_COMPARE(window->property("focusedScreen").toString(),
                 QString("home"));
    QTRY_COMPARE(window->activeFocusItem(), focused);
    // Ambient never asked the platform for anything.
    QVERIFY(!daemon.calls.contains("Suspend"));
    // A lock ends ambient without a wake event.
    QTRY_VERIFY_WITH_TIMEOUT(ambient->property("active").toBool(), 3000);
    lock.active = true;
    emit lock.ActiveChanged(true);
    QTRY_VERIFY_WITH_TIMEOUT(!window->isVisible(), 500);
    QVERIFY(!ambient->property("active").toBool());
    QVERIFY(!idle->property("counting").toBool());
    server.unregisterObject("/ScreenSaver");
    server.unregisterObject("/org/freedesktop/login1");
    server.unregisterService("org.freedesktop.login1");
    server.unregisterObject("/org/arinn/Daemon1");
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
