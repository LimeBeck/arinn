// SPDX-FileCopyrightText: 2026 Arinn contributors
//
// SPDX-License-Identifier: GPL-3.0-or-later

#include "backend.h"
#include "sessionguard.h"
#include <QQmlApplicationEngine>
#include <QQmlContext>
#include <QQuickItem>
#include <QQuickWindow>
#include <QtTest>
class LockService : public QObject {
  Q_OBJECT
  Q_CLASSINFO("D-Bus Interface", "org.freedesktop.ScreenSaver")
public:
  bool active = false;
public slots:
  bool GetActive() const { return active; }
signals:
  void ActiveChanged(bool active);
};
class SleepService : public QObject {
  Q_OBJECT
  Q_CLASSINFO("D-Bus Interface", "org.freedesktop.login1.Manager")
signals:
  void PrepareForSleep(bool sleeping);
};
class BackendTest : public QObject {
  Q_OBJECT
private slots:
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
    SessionGuard guard(nullptr, bus, bus);
    QTRY_VERIFY(!guard.blocked());
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
  void typedWireAndLocalFocus() {
    Backend backend;
    SessionGuard sessionGuard;
    QTRY_VERIFY_WITH_TIMEOUT(backend.connected(), 5000);
    QTRY_VERIFY_WITH_TIMEOUT(backend.available(), 5000);
    QTRY_COMPARE_WITH_TIMEOUT(backend.items().size(), 1, 5000);
    QCOMPARE(backend.items().at(0).toMap().value("localId").toString(),
             QString("demo.activity"));
    QQmlApplicationEngine engine;
    engine.rootContext()->setContextProperty("backend", &backend);
    engine.rootContext()->setContextProperty("sessionGuard", &sessionGuard);
    engine.load(QUrl("qrc:/Main.qml"));
    QVERIFY(!engine.rootObjects().isEmpty());
    auto window = qobject_cast<QQuickWindow *>(engine.rootObjects().first());
    QVERIFY(window);
    window->show();
    window->requestActivate();
    QTest::qWait(100);
    QTest::keyClick(window, Qt::Key_Backspace);
    QTRY_VERIFY(window->property("navOpen").toBool());
    QTest::keyClick(window, Qt::Key_Escape);
    QTRY_VERIFY(!window->property("navOpen").toBool());
    auto first = window->findChild<QQuickItem *>("returnButton");
    auto second = window->findChild<QQuickItem *>("demoButton");
    QVERIFY(first && second);
    first->forceActiveFocus();
    QTRY_VERIFY(first->hasActiveFocus());
    backend.activate(backend.items().at(0).toMap().value("id").toString()); // fixture deliberately takes 7 s
    QTest::keyClick(window, Qt::Key_Right);
    QTRY_VERIFY_WITH_TIMEOUT(second->hasActiveFocus(), 500);
    QTest::keyClick(window, Qt::Key_Left);
    QTRY_VERIFY_WITH_TIMEOUT(first->hasActiveFocus(), 500);
    QTRY_COMPARE_WITH_TIMEOUT(backend.message(), QString("Timeout"), 7000);
  }
};
QTEST_MAIN(BackendTest)
#include "backend_test.moc"
