// SPDX-FileCopyrightText: 2026 Arinn contributors
//
// SPDX-License-Identifier: GPL-3.0-or-later

#include "asset_images.h"
#include "backend.h"
#include "sessionlifecycle.h"
#include "shell_window.h"
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
    configureShellWindows();
    QQmlApplicationEngine engine;
    auto *sessionGuard = createSessionLifecycle(&engine);
    engine.rootContext()->setContextProperty("backend", &backend);
    engine.rootContext()->setContextProperty("sessionGuard", sessionGuard);
    installAssetImageProvider(&engine);
    engine.load(QUrl("qrc:/Main.qml"));
    QVERIFY(!engine.rootObjects().isEmpty());
    auto window = qobject_cast<QQuickWindow *>(engine.rootObjects().first());
    QVERIFY(window);
    // The transparent playback layer needs an alpha channel (#236).
    QVERIFY(window->requestedFormat().alphaBufferSize() >= 8);
    window->show();
    window->requestActivate();
    QTest::qWait(100);
    QTest::keyClick(window, Qt::Key_Backspace);
    QTRY_VERIFY(window->property("navOpen").toBool());
    QTest::keyClick(window, Qt::Key_Escape);
    QTRY_VERIFY(!window->property("navOpen").toBool());
    QTRY_COMPARE_WITH_TIMEOUT(backend.applications().size(), 1, 5000);
    QTRY_COMPARE(backend.homeState(), QString("ready"));
    const auto appId =
        backend.applications().at(0).toMap().value("id").toString();
    const auto favorites = backend.favorites();
    // The integration fixture persists one application via Favorites1.
    QCOMPARE(favorites.size(), 1);
    QCOMPARE(favorites.first().toMap().value("id").toString(), appId);
    auto home = window->findChild<QQuickItem *>("productionHome");
    QVERIFY(home);
    QCOMPARE(home->property("screen").toString(), QString("home"));
    const auto homeFavorites = home->property("favoriteEntries").toList();
    QCOMPARE(homeFavorites.size(), favorites.size() + 1);
    QCOMPARE(homeFavorites.first().toMap().value("id").toString(),
             QString("arinn-browse"));
    QCOMPARE(homeFavorites.mid(1), favorites);
    bool onApplicationsRoute = false;
    for (const auto &entry : home->property("applicationEntries").toList())
      if (entry.toMap().value("id").toString() == appId)
        onApplicationsRoute = true;
    QVERIFY(onApplicationsRoute);
    auto first = findItem(window->contentItem(), "menuButton");
    QQuickItem *second = nullptr;
    QTRY_VERIFY(second = findItem(window->contentItem(), "entry-" + appId));
    QVERIFY(first);
    QVERIFY(second->isVisible());
    bool inFavorites = false;
    for (auto *item = second; item; item = item->parentItem())
      if (item->objectName() == QStringLiteral("favorites"))
        inFavorites = true;
    QVERIFY(inFavorites);
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
