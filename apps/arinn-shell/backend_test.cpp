// SPDX-FileCopyrightText: 2026 Arinn contributors
//
// SPDX-License-Identifier: GPL-3.0-or-later

#include "asset_images.h"
#include "main_qml_test_support.h"
#include "backend.h"
#include "localization_manager.h"
#include "language_change.h"
#include "settings_catalog.h"
#include "sessionlifecycle.h"
#include "shell_window.h"
#include <QKeyEvent>
#include <QQmlApplicationEngine>
#include <QQmlContext>
#include <QQuickItem>
#include <QQuickWindow>
#include <QScopeGuard>
#include <QtTest>
namespace {
class RejectRussianReload final : public QObject {
protected:
  bool eventFilter(QObject *, QEvent *event) override {
    if (event->type() == TranslationReloadCheck::typeId()) {
      auto *check = static_cast<TranslationReloadCheck *>(event);
      if (check->locale == QLatin1String("ru")) check->failed = true;
    }
    return false;
  }
};
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
    LocalizationManager localization({}, {}, QLocale(QStringLiteral("en_US")));
    Backend backend;
    QTRY_VERIFY_WITH_TIMEOUT(backend.connected(), 5000);
    QTRY_VERIFY_WITH_TIMEOUT(backend.available(), 5000);
    QTRY_COMPARE_WITH_TIMEOUT(backend.items().size(), 1, 5000);
    QCOMPARE(backend.items().at(0).toMap().value("localId").toString(),
             QString("demo.activity"));
    configureShellWindows();
    QQmlApplicationEngine engine;
    localization.attachEngine(&engine);
    engine.rootContext()->setContextProperty(QStringLiteral("shellLocalization"), &localization);
    auto *sessionGuard = createSessionLifecycle(&engine);
    InterfaceSounds interfaceSounds;

    installMainQmlContextProperties(&engine, &backend, sessionGuard, &interfaceSounds);
    installAssetImageProvider(&engine);
    engine.load(QUrl("qrc:/Main.qml"));
    QVERIFY(!engine.rootObjects().isEmpty());
    auto window = qobject_cast<QQuickWindow *>(engine.rootObjects().first());
    QVERIFY(window);
    // The transparent playback layer needs an alpha channel (#236).
    QVERIFY(window->requestedFormat().alphaBufferSize() >= 8);
    // The hidden shell owns a native window before its first show, so the
    // EWMH adapter can find it for the first Home (#379).
    QVERIFY(!window->handle());
    createHiddenShellWindow(window);
    QVERIFY(window->handle());
    QVERIFY(!window->isVisible());
    window->show();
    window->requestActivate();
    QTest::qWait(100);
    QTest::keyClick(window, Qt::Key_Backspace);
    QTRY_VERIFY(window->property("navOpen").toBool());
    QTest::keyClick(window, Qt::Key_Escape);
    QTRY_VERIFY(!window->property("navOpen").toBool());
    QTRY_COMPARE_WITH_TIMEOUT(backend.applications().size(), 1, 5000);
    const auto appId =
        backend.applications().at(0).toMap().value("id").toString();
    QTRY_VERIFY(backend.favoritesReady());
    QVERIFY(backend.favorites().isEmpty());
    backend.setFavorite(appId, true);
    QTRY_COMPARE(backend.favorites().size(), 1);
    QTRY_COMPARE(backend.homeState(), QString("ready"));
    const auto favorites = backend.favorites();
    // Pin explicitly through the same durable API used by the tile sheet.
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
    auto *settings = window->findChild<SettingsCatalog *>(QStringLiteral("settingsCatalog"));
    QVERIFY(settings);
    QTRY_VERIFY(settings->uiLanguageKnown());
    auto *continueRow = findItem(window->contentItem(), QStringLiteral("continue"));
    QVERIFY(continueRow);
    localization.setLanguage(QStringLiteral("ru"));
    QTRY_COMPARE(continueRow->property("title").toString(), QStringLiteral("Продолжить"));
    QTRY_VERIFY(second->hasActiveFocus());
    QCOMPARE(home->property("screen").toString(), QStringLiteral("home"));
    localization.setLanguage(QStringLiteral("en"));
    QTRY_COMPARE(continueRow->property("title").toString(), QStringLiteral("Continue"));
    QTRY_VERIFY(second->hasActiveFocus());
    QVERIFY(QMetaObject::invokeMethod(home, "openNavigation"));
    QTRY_VERIFY(window->activeFocusItem() && window->activeFocusItem()->objectName().startsWith(QStringLiteral("nav-")));
    QTest::keyClick(window, Qt::Key_Down);
    const auto navigationId = window->activeFocusItem()->objectName();
    const auto navigationText = window->activeFocusItem()->property("text").toString();
    localization.setLanguage(QStringLiteral("ru"));
    QTRY_VERIFY(window->activeFocusItem() && window->activeFocusItem()->objectName() == navigationId);
    QTRY_VERIFY(window->activeFocusItem()->property("text").toString() != navigationText);
    localization.setLanguage(QStringLiteral("en"));
    QTRY_VERIFY(window->activeFocusItem() && window->activeFocusItem()->objectName() == navigationId);
    QTRY_COMPARE(window->activeFocusItem()->property("text").toString(), navigationText);
    QTest::keyClick(window, Qt::Key_Escape);
    QTRY_VERIFY(second->hasActiveFocus());
    // A confirmed language preference can be valid while its local files
    // exceed the client's cache budget. Exercise actual Main/Settings wiring:
    // the desired preference stays saved, and the failed application is shown.
    auto *settingsView = window->findChild<QQuickItem *>("productionSettings");
    QVERIFY(settingsView);
    auto *toastTimer = settingsView->findChild<QObject *>("settingsToastTimer");
    QVERIFY(toastTimer);
    QVariantMap languageRow;
    for (const auto &value : settings->rows()) {
      const auto row = value.toMap();
      if (row.value("providerId").toString() == QLatin1String("arinn") &&
          row.value("id").toString() == QLatin1String("ui_language")) languageRow = row;
    }
    QVERIFY(!languageRow.isEmpty());
    const auto originalLanguage = settings->uiLanguage();
    RejectRussianReload reject;
    qApp->installEventFilter(&reject);
    const auto restoreLanguage = qScopeGuard([&] {
      qApp->removeEventFilter(&reject);
      settings->setValue("arinn", "ui_language", originalLanguage);
    });
    QVERIFY(home->setProperty("screen", "settings"));
    QTRY_VERIFY(settingsView->isVisible());
    QVERIFY(QMetaObject::invokeMethod(settingsView, "beginSave", Q_ARG(QVariant, languageRow),
        Q_ARG(QVariant, QStringLiteral("ru")), Q_ARG(QVariant, QStringLiteral("Русский"))));
    QTRY_COMPARE(settings->uiLanguage(), QStringLiteral("ru"));
    QTRY_VERIFY(settingsView->property("awaitingKey").toString().isEmpty());
    QVERIFY(localization.reloadFailed());
    QCOMPARE(localization.language(), QStringLiteral("ru"));
    QCOMPARE(continueRow->property("title").toString(), QStringLiteral("Continue"));
    QTRY_COMPARE(settingsView->property("toastTitle").toString(), QStringLiteral("Unable to reload translations"));
    QVERIFY(settingsView->property("toastIsError").toBool());
    QVERIFY(toastTimer->setProperty("running", false));
    QVERIFY(settingsView->setProperty("toastTitle", QString()));
    QSignalSpy toastStarts(toastTimer, SIGNAL(runningChanged()));
    QVERIFY(toastStarts.isValid());
    QVERIFY(QMetaObject::invokeMethod(settingsView, "translationsReloadRequested"));
    QTRY_COMPARE(settingsView->property("toastTitle").toString(), QStringLiteral("Unable to reload translations"));
    QTest::qWait(50);
    QCOMPARE(toastStarts.count(), 1);
    qApp->removeEventFilter(&reject);
    settings->setValue("arinn", "ui_language", originalLanguage);
    QTRY_COMPARE(settings->uiLanguage(), originalLanguage);
    QTRY_VERIFY(!localization.reloadFailed());
    QVERIFY(home->setProperty("screen", "home"));
    second->forceActiveFocus();
    QTRY_VERIFY(second->hasActiveFocus());
    // Autorepeat releases while holding Select must not activate the tile.
    QTest::keyPress(window, Qt::Key_Return);
    QTest::qWait(100);
    QKeyEvent repeatRelease(QEvent::KeyRelease, Qt::Key_Return, Qt::NoModifier, QString(), true);
    QCoreApplication::sendEvent(window, &repeatRelease);
    QTest::qWait(650);
    QTest::keyRelease(window, Qt::Key_Return);
    QTRY_VERIFY(home->property("tileActionsOpen").toBool());
    QVERIFY(backend.busyEntryIds().isEmpty());
    QTest::keyClick(window, Qt::Key_Back);
    QTRY_VERIFY(!home->property("tileActionsOpen").toBool());
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
    // #759: the close flow learns how an application operation ended.
    QSignalSpy finished(&backend, &Backend::operationFinished);
    // The queued Terminate below replaces the message as it starts, so read
    // the Activate's message when it ends.
    QString firstMessage;
    connect(&backend, &Backend::operationFinished, &backend, [&] {
      if (firstMessage.isEmpty())
        firstMessage = backend.message();
    });
    const auto activityId = backend.items().at(0).toMap().value("id").toString();
    backend.activate(backend.items()
                       .at(0)
                       .toMap()
                       .value("id")
                       .toString()); // fixture deliberately takes 7 s
    QVERIFY(!backend.busyEntryIds().isEmpty());
    // Review of #777: a close asked for meanwhile waits for the Activate
    // instead of being dropped.
    backend.terminate(activityId);
    QTest::keyClick(window, Qt::Key_Down);
    QTRY_VERIFY_WITH_TIMEOUT(second->hasActiveFocus(), 500);
    QTest::keyClick(window, Qt::Key_Up);
    QTRY_VERIFY_WITH_TIMEOUT(first->hasActiveFocus(), 500);
    QTRY_VERIFY_WITH_TIMEOUT(finished.count() >= 1, 7000);
    QCOMPARE(firstMessage, QString("Arinn did not reply. Try again."));
    QCOMPARE(finished.at(0).at(0).toString(), activityId);
    QCOMPARE(finished.at(0).at(1).toString(), QStringLiteral("Activate"));
    QCOMPARE(finished.at(0).at(2).toString(), QStringLiteral("failed"));
    // Then the queued Terminate starts: pending again, or already ended.
    QTRY_VERIFY(backend.busyEntryIds().contains(activityId) ||
                (finished.count() >= 2 &&
                 finished.at(1).at(1).toString() == QLatin1String("Terminate")));
  }
};
QTEST_MAIN(BackendTest)
#include "backend_test.moc"
