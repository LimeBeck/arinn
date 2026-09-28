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
namespace {
QQuickItem *findItem(QQuickItem *root, const QString &name) {
  if (root->objectName() == name)
    return root;
  for (auto *child : root->childItems())
    if (auto *found = findItem(child, name))
      return found;
  return nullptr;
}
} // namespace
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
    QTRY_COMPARE_WITH_TIMEOUT(backend.applications().size(), 1, 5000);
    QTRY_VERIFY(backend.homeState() != QString("loading"));
    const auto appId =
        backend.applications().at(0).toMap().value("id").toString();
    // Home favorites are the daemon web-shortcut list. An application does
    // not make Home ready and is not a favorite tile.
    for (const auto &shortcut : backend.webShortcuts())
      QVERIFY(shortcut.toMap().value("id").toString() != appId);
    QCOMPARE(backend.homeState(),
             QString(backend.returnWindow().isEmpty() &&
                             backend.webShortcuts().isEmpty()
                         ? "empty"
                         : "ready"));
    auto home = window->findChild<QQuickItem *>("productionHome");
    QVERIFY(home);
    QCOMPARE(home->property("favoriteEntries").toList().size(),
             backend.webShortcuts().size());
    if (auto *onHome = findItem(window->contentItem(), "entry-" + appId))
      QVERIFY(!onHome->isVisible());
    QTest::keyClick(window, Qt::Key_Backspace);
    QTRY_VERIFY(window->property("navOpen").toBool());
    for (int step = 1; step <= 3; ++step) {
      QTest::keyClick(window, Qt::Key_Down);
      auto *nav = findItem(window->contentItem(),
                           QStringLiteral("nav-%1").arg(step));
      QVERIFY(nav);
      QTRY_VERIFY(nav->hasActiveFocus());
    }
    QTest::keyClick(window, Qt::Key_Return);
    QTRY_COMPARE(home->property("screen").toString(), QString("applications"));
    QTRY_VERIFY(!window->property("navOpen").toBool());
    auto first = findItem(window->contentItem(), "menuButton");
    QQuickItem *second = nullptr;
    QTRY_VERIFY(second = findItem(window->contentItem(), "entry-" + appId));
    QVERIFY(first);
    QVERIFY(second->isVisible());
    bool inFavorites = false;
    for (auto *item = second; item; item = item->parentItem())
      if (item->objectName() == QStringLiteral("favorites"))
        inFavorites = true;
    QVERIFY(!inFavorites);
    second->forceActiveFocus();
    QTRY_VERIFY(second->hasActiveFocus());
    QSignalSpy updates(&backend, &Backend::applicationsChanged);
    backend.refresh();
    QTest::qWait(1200);
    QCOMPARE(updates.count(), 0);
    QVERIFY(second->hasActiveFocus());
    const auto capture = qEnvironmentVariable("ARINN_UI_CAPTURE");
    if (!capture.isEmpty()) {
      QTest::qWait(300);
      QVERIFY(window->grabWindow().save(capture));
    }
    first->forceActiveFocus();
    QTRY_VERIFY(first->hasActiveFocus());
    backend.activate(backend.items()
                       .at(0)
                       .toMap()
                       .value("id")
                       .toString()); // fixture deliberately takes 7 s
    QVERIFY(!backend.busyEntryIds().isEmpty());
    QTest::keyClick(window, Qt::Key_Down);
    QTRY_VERIFY_WITH_TIMEOUT(second->hasActiveFocus(), 500);
    QTest::keyClick(window, Qt::Key_Up);
    QTRY_VERIFY_WITH_TIMEOUT(first->hasActiveFocus(), 500);
    QTRY_COMPARE_WITH_TIMEOUT(backend.message(), QString("Timeout"), 7000);
    QVERIFY(backend.busyEntryIds().isEmpty());
  }
};
QTEST_MAIN(BackendTest)
#include "backend_test.moc"
