// SPDX-FileCopyrightText: 2026 Arinn contributors
//
// SPDX-License-Identifier: GPL-3.0-or-later

#include "backend.h"
#include "sessionlifecycle.h"
#include <QQmlApplicationEngine>
#include <QQmlContext>
#include <QQuickItem>
#include <QQuickWindow>
#include <QtTest>
class BackendTest : public QObject {
  Q_OBJECT
private slots:
  void typedWireAndLocalFocus() {
    Backend backend;
    QTRY_VERIFY_WITH_TIMEOUT(backend.connected(), 5000);
    QTRY_VERIFY_WITH_TIMEOUT(backend.available(), 5000);
    QTRY_COMPARE_WITH_TIMEOUT(backend.items().size(), 1, 5000);
    QCOMPARE(backend.items().at(0).toMap().value("localId").toString(),
             QString("demo.activity"));
    QQmlApplicationEngine engine;
    auto *sessionGuard = createSessionLifecycle(&engine);
    engine.rootContext()->setContextProperty("backend", &backend);
    engine.rootContext()->setContextProperty("sessionGuard", sessionGuard);
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
