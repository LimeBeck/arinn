// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

#include "language_change.h"
#include "localization_manager.h"
#include "local_translations.h"
#include <QFile>
#include <QProcess>
#include <QQmlComponent>
#include <QQmlEngine>
#include <QQuickItem>
#include <QQuickWindow>
#include <QSignalSpy>
#include <QTemporaryDir>
#include <QtTest>

namespace {
bool catalogue(const QString &path, const QString &translation, const QString &language = QStringLiteral("ru")) {
  QDir().mkpath(QFileInfo(path).absolutePath());
  QFile source(path + QStringLiteral(".ts"));
  if (!source.open(QIODevice::WriteOnly)) return false;
  source.write(QStringLiteral("<?xml version=\"1.0\"?><TS version=\"2.1\" language=\"%1\" sourcelanguage=\"en\">"
      "<context><name>HomeView</name><message><source>Continue</source><translation>%2</translation>"
      "</message></context></TS>").arg(language, translation.toHtmlEscaped()).toUtf8());
  source.close();
  QProcess process;
  process.start(QStringLiteral(ARINN_LRELEASE_BINARY), {source.fileName(), QStringLiteral("-qm"), path});
  return process.waitForFinished(10000) && process.exitCode() == 0;
}
QString caption() { return QCoreApplication::translate("HomeView", "Continue"); }
QQuickItem *namedItem(QQuickItem *root, const QString &name) {
  if (root->objectName() == name) return root;
  for (auto *child : root->childItems())
    if (auto *found = namedItem(child, name)) return found;
  return nullptr;
}
}

class LocalizationManagerTest : public QObject {
  Q_OBJECT
  QLocale m_previous;
private slots:
  void init() { m_previous = QLocale(); }
  void cleanup() { QLocale::setDefault(m_previous); }

  void switchesBindingsWithoutReplacingTheirState() {
    LocalizationManager manager({}, {}, QLocale(QStringLiteral("en_GB")));
    QQmlEngine engine;
    manager.attachEngine(&engine);
    QQmlComponent component(&engine);
    component.setData("import QtQml\nQtObject { property string caption: qsTr(\"Continue\"); property string draft: \"\" }",
                      QUrl(QStringLiteral("qrc:/ui/HomeView.qml")));
    QScopedPointer<QObject> object(component.create());
    QVERIFY2(object, qPrintable(component.errorString()));
    object->setProperty("draft", QStringLiteral("unsaved input"));
    QObject model;
    int updates = 0;
    new LanguageChangeObserver(&model, [&] { ++updates; });
    manager.setLanguage(QStringLiteral("ru"));
    QTRY_COMPARE(object->property("caption").toString(), QStringLiteral("Продолжить"));
    QTRY_COMPARE(updates, 1);
    manager.setLanguage(QStringLiteral("en"));
    QCOMPARE(object->property("caption").toString(), QStringLiteral("Continue"));
    QCOMPARE(object->property("draft").toString(), QStringLiteral("unsaved input"));
    QCOMPARE(manager.clockText(QDateTime(QDate(2026, 1, 1), QTime(13, 4))), QStringLiteral("13:04"));
    manager.setLanguage(QStringLiteral("en-US"));
    QCOMPARE(manager.clockText(QDateTime(QDate(2026, 1, 1), QTime(13, 4))),
             QLocale(QStringLiteral("en_US")).toString(QTime(13, 4), QLocale::ShortFormat));
    manager.setLanguage(QStringLiteral("system"));
    QCOMPARE(QLocale(), QLocale(QStringLiteral("en_GB")));
  }

  void partialOverlaysReloadAndKeepTheirLastGoodBytes() {
    QTemporaryDir personal, shared;
    QVERIFY(personal.isValid() && shared.isValid());
    const auto local = personal.filePath(QStringLiteral("overrides/ru/shell/arinn.qm"));
    const auto packaged = shared.filePath(QStringLiteral("arinn_ru_RU.qm"));
    QVERIFY(catalogue(local, QStringLiteral("Мой перевод")));
    QVERIFY(catalogue(packaged, QStringLiteral("Другой перевод")));
    LocalizationManager manager({personal.path(), shared.path()}, {}, QLocale(QStringLiteral("en_US")));
    manager.setLanguage(QStringLiteral("ru-RU"));
    QCOMPARE(caption(), QStringLiteral("Мой перевод"));
    QCOMPARE(QCoreApplication::translate("HomeView", "Favorites"), QStringLiteral("Избранное"));
    const auto before = manager.revision();
    QFile corrupt(local);
    QVERIFY(corrupt.open(QIODevice::WriteOnly | QIODevice::Truncate));
    corrupt.write("broken catalogue");
    corrupt.close();
    manager.reload();
    QVERIFY(manager.reloadFailed());
    QCOMPARE(manager.revision(), before);
    QCOMPARE(caption(), QStringLiteral("Мой перевод"));
    QVERIFY(catalogue(local, QStringLiteral("Исправлено")));
    manager.reload();
    QVERIFY(!manager.reloadFailed());
    QCOMPARE(caption(), QStringLiteral("Исправлено"));
    QVERIFY(QFile::remove(local));
    manager.reload();
    QCOMPARE(caption(), QStringLiteral("Другой перевод"));
    manager.setLanguage(QStringLiteral("en"));
    QCOMPARE(caption(), QStringLiteral("Continue"));
  }

  void partialNewLanguageNeedsNoApplicationRebuild() {
    QTemporaryDir folder;
    QVERIFY(folder.isValid());
    QVERIFY(catalogue(folder.filePath(QStringLiteral("arinn_de.qm")), QStringLiteral("Weiter"), QStringLiteral("de")));
    LocalizationManager manager({folder.path()}, {}, QLocale(QStringLiteral("en_US")));
    manager.setLanguage(QStringLiteral("de"));
    QCOMPARE(caption(), QStringLiteral("Weiter"));
    QCOMPARE(QCoreApplication::translate("HomeView", "Favorites"), QStringLiteral("Favorites"));
    QVERIFY(catalogue(folder.filePath(QStringLiteral("arinn_qaa.qm")), QStringLiteral("Private language label"), QStringLiteral("qaa")));
    manager.setLanguage(QStringLiteral("qaa"));
    QCOMPARE(manager.translationLocale(), QStringLiteral("qaa"));
    QCOMPARE(caption(), QStringLiteral("Private language label"));
  }

  void confirmedPreferenceCacheStartsBeforeTheDaemonAnswers() {
    QTemporaryDir folder;
    const auto cache = folder.filePath(QStringLiteral("ui-language.json"));
    {
      LocalizationManager manager({}, cache, QLocale(QStringLiteral("en_US")));
      manager.setLanguage(QStringLiteral("ru"));
    }
    {
      LocalizationManager manager({}, cache, QLocale(QStringLiteral("en_US")));
      QCOMPARE(manager.language(), QStringLiteral("ru"));
      QCOMPARE(caption(), QStringLiteral("Продолжить"));
      manager.setLanguage(QStringLiteral("../en"));
      QCOMPARE(manager.language(), QStringLiteral("ru"));
    }
  }

  void localeFallbackDoesNotCrossScripts() {
    QVERIFY(!LocalTranslations::locales(QStringLiteral("zh-TW")).contains(QStringLiteral("zh")));
    QVERIFY(LocalTranslations::locales(QStringLiteral("zh-TW")).contains(QStringLiteral("zh-Hant")));
    QVERIFY(!LocalTranslations::locales(QStringLiteral("sr-Latn-RS")).contains(QStringLiteral("sr")));
    QVERIFY(LocalTranslations::locales(QStringLiteral("de-DE")).contains(QStringLiteral("de")));
    QVERIFY(!LocalTranslations::locales(QStringLiteral("sr-Abcd-RS")).contains(QStringLiteral("sr")));
  }

  void qmDependenciesCannotLoadUndeclaredFiles() {
    QTemporaryDir folder;
    const auto file = folder.filePath(QStringLiteral("arinn_de.qm"));
    QVERIFY(catalogue(file, QStringLiteral("Weiter"), QStringLiteral("de")));
    QFile input(file);
    QVERIFY(input.open(QIODevice::ReadOnly));
    auto bytes = input.readAll();
    QVERIFY(LocalTranslations::selfContainedQm(bytes));
    // A valid block envelope for an empty dependency list is still forbidden.
    bytes.append(QByteArray::fromHex("960000000400000000"));
    QVERIFY(!LocalTranslations::selfContainedQm(bytes));
    bytes.chop(3);
    QVERIFY(!LocalTranslations::selfContainedQm(bytes));
  }

  void openIntegerNoteRetranslatesWithoutLosingItsDraftOrFocus() {
    LocalizationManager manager({}, {}, QLocale(QStringLiteral("en_US")));
    QQmlEngine engine;
    manager.attachEngine(&engine);
    QQuickWindow window;
    QQmlComponent component(&engine);
    component.setData(R"(
      import QtQuick
      import "ui/screens/settings" as Settings
      Settings.SettingsView {
        categories: [{id: "system", title: "System"}]
        rows: [{providerId: "local", id: "limit", categoryId: "system", title: "Limit", type: "integer",
                currentValue: 4, constraints: {min: 0, max: 5}, enabled: true}]
        function beginEditor() { openInteger(rows[0]); nudgeInteger(1, false); }
      }
    )", QUrl::fromLocalFile(QStringLiteral(ARINN_SHELL_SOURCE) + QStringLiteral("/language-fixture.qml")));
    QScopedPointer<QObject> object(component.create());
    QVERIFY2(object, qPrintable(component.errorString()));
    auto *item = qobject_cast<QQuickItem *>(object.data());
    QVERIFY(item);
    item->setParentItem(window.contentItem());
    window.resize(1920, 1080);
    window.show();
    window.requestActivate();
    QTRY_VERIFY(window.activeFocusItem() && window.activeFocusItem()->objectName() == QStringLiteral("category-system"));
    QVERIFY(QMetaObject::invokeMethod(object.data(), "beginEditor"));
    QCOMPARE(object->property("integerNote").toString(), QStringLiteral("This is the maximum"));
    QTRY_VERIFY(window.activeFocusItem() && window.activeFocusItem()->objectName() == QStringLiteral("integerStepper"));
    const QPointer<QQuickItem> focused = window.activeFocusItem();
    const auto draft = object->property("editingDraft");
    manager.setLanguage(QStringLiteral("ru"));
    QTRY_COMPARE(object->property("integerNote").toString(), QStringLiteral("Это максимум"));
    QCOMPARE(object->property("editingDraft"), draft);
    QCOMPARE(object->property("integerDraft").toInt(), 5);
    QCOMPARE(window.activeFocusItem(), focused.data());
  }

  void openFolderRefusalRetranslatesAndKeepsTheDirectory() {
    LocalizationManager manager({}, {}, QLocale(QStringLiteral("en_US")));
    QQmlEngine engine;
    manager.attachEngine(&engine);
    QQmlComponent component(&engine);
    component.setData(R"(
      import QtQuick
      import "ui/screens/settings" as Settings
      Settings.LibraryFoldersView {
        row: ({providerId: "library", id: "roots", libraryFolders: true, currentValue: ["/example/library"], enabled: true,
               constraints: {directory: true, multiple: true, maxItems: 8}})
        browser: QtObject {
          property var places: []
          property var folders: []
          property string path: "/example/library"
          property string state: "ready"
          function refreshPlaces() {}
          function open(value) { path = value; }
          function up() {}
        }
        Component.onCompleted: { add(); chosen("/example/library"); }
      }
    )", QUrl::fromLocalFile(QStringLiteral(ARINN_SHELL_SOURCE) + QStringLiteral("/folder-language-fixture.qml")));
    QScopedPointer<QObject> object(component.create());
    QVERIFY2(object, qPrintable(component.errorString()));
    auto *picker = object->findChild<QObject *>(QStringLiteral("folderPicker"));
    QVERIFY(picker);
    QCOMPARE(picker->property("errorText").toString(), QStringLiteral("This folder is already in the library"));
    manager.setLanguage(QStringLiteral("ru"));
    QTRY_COMPARE(picker->property("errorText").toString(), QStringLiteral("Эта папка уже в медиатеке"));
    QCOMPARE(picker->property("path").toString(), QStringLiteral("/example/library"));
    QVERIFY(picker->property("visible").toBool());
  }

  void libraryTabsSeasonsAndEpisodesKeepFocusAcrossRetranslation() {
    LocalizationManager manager({}, {}, QLocale(QStringLiteral("en_US")));
    QQmlEngine engine;
    manager.attachEngine(&engine);
    QQuickWindow window;
    QQmlComponent component(&engine);
    component.setData(R"(
      import QtQuick
      import "ui/screens/library" as Library
      Library.LibraryView {
        tabs: [{id: "films", title: qsTranslate("LibraryCatalog", "Films")},
               {id: "series", title: qsTranslate("LibraryCatalog", "Series")}]
        entries: [{id: "show", tabId: "series", title: "Content title", kind: "series",
          seasons: [{id: "s1", title: qsTranslate("LibraryCatalog", "Season %1").arg(1)},
                    {id: "s2", title: qsTranslate("LibraryCatalog", "Season %1").arg(2)}],
          episodes: [{id: "e1", seasonId: "s1", title: "Episode one", duration: "40 min", number: 1},
                     {id: "e2", seasonId: "s2", title: "Episode two", duration: "40 min", number: 1}]}]
      }
    )", QUrl::fromLocalFile(QStringLiteral(ARINN_SHELL_SOURCE) + QStringLiteral("/library-language-fixture.qml")));
    QScopedPointer<QObject> object(component.create());
    QVERIFY2(object, qPrintable(component.errorString()));
    auto *item = qobject_cast<QQuickItem *>(object.data());
    QVERIFY(item);
    item->setParentItem(window.contentItem());
    window.resize(1920, 1080);
    window.show();
    window.requestActivate();
    const auto focused = [&] { return window.activeFocusItem() ? window.activeFocusItem()->objectName() : QString(); };
    QTRY_COMPARE(focused(), QStringLiteral("tab-films"));
    QSignalSpy tabRequests(object.data(), SIGNAL(tabRequested(QString)));
    QVERIFY(tabRequests.isValid());
    manager.setLanguage(QStringLiteral("ru"));
    QTRY_COMPARE(focused(), QStringLiteral("tab-films"));
    QCOMPARE(window.activeFocusItem()->property("text").toString(), QStringLiteral("Фильмы"));
    auto *hoveredTab = namedItem(item, QStringLiteral("tab-series"));
    QVERIFY(hoveredTab);
    hoveredTab->forceActiveFocus();
    QCOMPARE(object->property("currentTabId").toString(), QStringLiteral("films"));
    manager.setLanguage(QStringLiteral("en"));
    QTRY_COMPARE(focused(), QStringLiteral("tab-series"));
    QCOMPARE(object->property("currentTabId").toString(), QStringLiteral("films"));
    QCOMPARE(tabRequests.count(), 0);
    QVERIFY(QMetaObject::invokeMethod(object.data(), "focusTab", Q_ARG(QVariant, 1)));
    QVERIFY(QMetaObject::invokeMethod(object.data(), "openEntry", Q_ARG(QVariant, QStringLiteral("show"))));
    QTRY_COMPARE(focused(), QStringLiteral("detailPlay"));
    QVERIFY(QMetaObject::invokeMethod(object.data(), "focusSeason", Q_ARG(QVariant, 1)));
    QTRY_COMPARE(focused(), QStringLiteral("season-s2"));
    manager.setLanguage(QStringLiteral("ru"));
    QTRY_COMPARE(focused(), QStringLiteral("season-s2"));
    QCOMPARE(object->property("currentSeasonId").toString(), QStringLiteral("s2"));
    auto *hoveredSeason = namedItem(item, QStringLiteral("season-s1"));
    QVERIFY(hoveredSeason);
    hoveredSeason->forceActiveFocus();
    QCOMPARE(object->property("currentSeasonId").toString(), QStringLiteral("s2"));
    manager.setLanguage(QStringLiteral("en"));
    QTRY_COMPARE(focused(), QStringLiteral("season-s1"));
    QCOMPARE(object->property("currentSeasonId").toString(), QStringLiteral("s2"));
    QVERIFY(QMetaObject::invokeMethod(object.data(), "focusEpisode", Q_ARG(QVariant, 0)));
    QTRY_COMPARE(focused(), QStringLiteral("episode-e2"));
    manager.setLanguage(QStringLiteral("ru"));
    QTRY_COMPARE(focused(), QStringLiteral("episode-e2"));
    QCOMPARE(object->property("detailEntryId").toString(), QStringLiteral("show"));
    QCOMPARE(object->property("focusedEpisodeId").toString(), QStringLiteral("e2"));
    QQuickItem other(window.contentItem());
    other.setSize(QSizeF(10, 10));
    object->setProperty("ownsFocus", false);
    other.forceActiveFocus();
    manager.setLanguage(QStringLiteral("en"));
    QCoreApplication::processEvents();
    QCOMPARE(window.activeFocusItem(), &other);
  }
};
QTEST_MAIN(LocalizationManagerTest)
#include "localization_manager_test.moc"
