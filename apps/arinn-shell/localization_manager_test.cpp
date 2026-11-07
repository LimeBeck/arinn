// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

#include "language_change.h"
#include "localization_manager.h"
#include "local_translations.h"
#include "provider_messages.h"
#include <QFile>
#include <QProcess>
#include <QQmlComponent>
#include <QQmlEngine>
#include <QQuickItem>
#include <QQuickWindow>
#include <QSignalSpy>
#include <QTemporaryDir>
#include <QtTest>
#include <QXmlStreamReader>

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
QByteArray providerQm(const QString &messages) {
  QTemporaryDir directory;
  QFile source(directory.filePath(QStringLiteral("messages.ts")));
  if (!source.open(QIODevice::WriteOnly)) return {};
  source.write((QStringLiteral("<TS version=\"2.1\" language=\"de\" sourcelanguage=\"en\"><context><name></name>") +
      messages + QStringLiteral("</context></TS>")).toUtf8());
  source.close();
  QProcess process;
  const auto output = directory.filePath(QStringLiteral("messages.qm"));
  process.start(QStringLiteral(ARINN_LRELEASE_BINARY), {QStringLiteral("-idbased"), QStringLiteral("-nounfinished"), source.fileName(), QStringLiteral("-qm"), output});
  if (!process.waitForFinished(10000) || process.exitCode() != 0) return {};
  QFile compiled(output);
  return compiled.open(QIODevice::ReadOnly) ? compiled.readAll() : QByteArray{};
}
QByteArray providerCatalog() {
  return R"JSON({"formatVersion":1,"pluginId":"radio","sourceLocale":"en","locales":["de"],"messages":{
    "hello":{"revision":1,"source":"Hello %1; %L2","arguments":{"name":{"position":1,"type":"text"},"amount":{"position":2,"type":"integer"}}},
    "count":{"revision":2,"source":"%n station(s)","numerus":true},
    "plain":{"revision":1,"source":"My station"}
  }})JSON";
}
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

  void providerCatalogAndReferencesRequireCurrentTypedIdentity() {
    const auto catalog = ProviderMessages::catalog(providerCatalog(), QStringLiteral("radio"));
    QVERIFY(catalog);
    QVERIFY(!ProviderMessages::catalog(providerCatalog(), QStringLiteral("other")));
    auto bad = providerCatalog();
    bad.replace("\"type\":\"integer\"", "\"type\":\"text\"");
    QVERIFY(!ProviderMessages::catalog(bad, QStringLiteral("radio"))); // %L2 cannot be text
    bad = providerCatalog();
    bad.replace("\"position\":2", "\"position\":1");
    QVERIFY(!ProviderMessages::catalog(bad, QStringLiteral("radio")));
    QJsonObject object{{"messageKey", "hello"}, {"messageRevision", 1},
                       {"arguments", QJsonObject{{"name", "User"}, {"amount", std::numeric_limits<qint64>::max()}}}};
    QVERIFY(ProviderMessages::reference(object, *catalog));
    object.insert("messageRevision", 2);
    QVERIFY(!ProviderMessages::reference(object, *catalog));
    object.insert("messageRevision", 1);
    object.insert("arguments", QJsonObject{{"name", "User"}, {"amount", true}});
    QVERIFY(!ProviderMessages::reference(object, *catalog));
    object.insert("arguments", QJsonObject{{"name", "User"}, {"amount", 9223372036854775808.0}});
    QVERIFY(!ProviderMessages::reference(object, *catalog));
    object = {{"messageKey", "count"}, {"messageRevision", 2}, {"count", 0}};
    QVERIFY(ProviderMessages::reference(object, *catalog));
    object.insert("count", -1);
    QVERIFY(!ProviderMessages::reference(object, *catalog));
    object.remove("count");
    QVERIFY(!ProviderMessages::reference(object, *catalog));
  }

  void builtInProvidersShipEditableSourcesAndWorkingPrivateRussianDictionaries() {
    for (const auto &plugin : {QStringLiteral("music-local"), QStringLiteral("yandex-music")}) {
      const auto base = QStringLiteral(ARINN_SHELL_SOURCE) + QStringLiteral("/../../plugins/") + plugin + QStringLiteral("/i18n/ru");
      QFile qm(base + QStringLiteral(".qm")), ts(base + QStringLiteral(".ts"));
      QVERIFY(qm.open(QIODevice::ReadOnly) && ts.open(QIODevice::ReadOnly));
      ProviderMessages::Dictionary dictionary(plugin);
      QVERIFY(dictionary.load(qm.readAll()));
      QXmlStreamReader xml(ts.readAll());
      QString id, source, translated;
      int messages = 0;
      while (!xml.atEnd()) {
        xml.readNext();
        if (xml.isStartElement()) {
          if (xml.name() == QLatin1String("message")) {
            id = xml.attributes().value("id").toString();
            source.clear();
            translated.clear();
          } else if (xml.name() == QLatin1String("source")) source = xml.readElementText();
          else if (xml.name() == QLatin1String("translation")) translated = xml.readElementText();
        } else if (xml.isEndElement() && xml.name() == QLatin1String("message")) {
          QVERIFY(!id.isEmpty() && !source.isEmpty() && !translated.isEmpty());
          QCOMPARE(dictionary.lookup(id, -1), translated);
          QVERIFY(dictionary.lookup(source, -1).isEmpty());
          ++messages;
        }
      }
      QVERIFY(!xml.hasError());
      QVERIFY(messages >= 2);
    }
  }

  void providerDictionariesArePrivateLayeredAndFormatArgumentsOnce() {
    const auto catalog = ProviderMessages::catalog(providerCatalog(), QStringLiteral("radio"));
    QVERIFY(catalog);
    ProviderMessages::Dictionary lower("radio"), personal("radio"), other("different-plugin");
    const auto lowerBytes = providerQm(QStringLiteral(
        "<message id=\"hello@1\"><source>Hello %1; %L2</source><translation>Hallo %1; %L2</translation></message>"
        "<message id=\"plain@1\"><source>My station</source><translation>Mein Sender</translation></message>"
        "<message id=\"count@2\" numerus=\"yes\"><source>%n station(s)</source><translation><numerusform>%n Sender</numerusform><numerusform>%n Sender zusammen</numerusform></translation></message>"));
    QVERIFY(lower.load(lowerBytes));
    QVERIFY(personal.load(providerQm(QStringLiteral("<message id=\"hello@1\"><source>Hello %1; %L2</source><translation>Privat %L2: %1</translation></message>"))));
    QVERIFY(other.load(providerQm(QStringLiteral("<message id=\"plain@1\"><source>Other plugin's station</source><translation>Fremd</translation></message>"))));
    const auto reference = ProviderMessages::reference({{"messageKey", "hello"}, {"messageRevision", 1},
        {"arguments", QJsonObject{{"name", "%2"}, {"amount", 1234}}}}, *catalog);
    QVERIFY(reference);
    QCOMPARE(personal.lookup("hello@1", -1), QStringLiteral("Privat %L2: %1"));
    QVERIFY(ProviderMessages::validPattern(personal.lookup("hello@1", -1), catalog->messages.value("hello")));
    QCOMPARE(ProviderMessages::render(*catalog, *reference, {&personal, &lower}, QLocale("de"), "complete fallback"), QStringLiteral("Privat 1.234: %2"));
    const ProviderMessages::Reference plain{QStringLiteral("plain"), 1, {}, {}};
    QCOMPARE(ProviderMessages::render(*catalog, plain, {&personal, &lower}, QLocale("de"), "fallback"), QStringLiteral("Mein Sender"));
    QCOMPARE(ProviderMessages::render(*catalog, plain, {&other}, QLocale("de"), "fallback"), QStringLiteral("fallback"));
    QCOMPARE(QCoreApplication::translate(nullptr, "plain@1"), QStringLiteral("plain@1"));
    QCOMPARE(ProviderMessages::render(*catalog, {"count", 2, {}, 1}, {&lower}, QLocale("de"), "one station"), QStringLiteral("1 Sender"));
    QCOMPARE(ProviderMessages::render(*catalog, {"count", 2, {}, 2}, {&lower}, QLocale("de"), "two stations"), QStringLiteral("2 Sender zusammen"));
    QCOMPARE(ProviderMessages::render(*catalog, {"count", 2, {}, 2}, {}, QLocale("ru"), "two stations"), QStringLiteral("two stations"));
    QVERIFY(!lower.load(QByteArray("corrupt replacement")));
    QCOMPARE(ProviderMessages::render(*catalog, plain, {&lower}, QLocale("de"), "fallback"), QStringLiteral("Mein Sender"));
    auto external = std::make_unique<char[]>(size_t(lowerBytes.size()));
    std::copy(lowerBytes.begin(), lowerBytes.end(), external.get());
    const auto borrowed = QByteArray::fromRawData(external.get(), lowerBytes.size());
    ProviderMessages::Dictionary owned("radio");
    QVERIFY(owned.load(borrowed));
    std::fill_n(external.get(), lowerBytes.size(), '\0');
    external.reset();
    QCOMPARE(ProviderMessages::render(*catalog, plain, {&owned}, QLocale("de"), "fallback"), QStringLiteral("Mein Sender"));
  }

  void invalidProviderTemplatesFallThroughWithoutLeakingIdsOrExpandingData() {
    const auto catalog = ProviderMessages::catalog(providerCatalog(), QStringLiteral("radio"));
    QVERIFY(catalog);
    const ProviderMessages::Reference reference{"hello", 1, {{"name", "data %L2"}, {"amount", std::numeric_limits<qint64>::min()}}, {}};
    ProviderMessages::Dictionary bad("radio"), rawId("radio");
    QVERIFY(bad.load(providerQm(QStringLiteral("<message id=\"hello@1\"><source>Hello %1; %L2</source><translation>Missing %1</translation></message>"))));
    QVERIFY(rawId.load(providerQm(QStringLiteral("<message id=\"plain@1\"><source>My station</source><translation>plain@1</translation></message>"))));
    QCOMPARE(ProviderMessages::render(*catalog, reference, {&bad}, QLocale("de"), "already complete fallback"), QStringLiteral("already complete fallback"));
    QCOMPARE(ProviderMessages::render(*catalog, {"plain", 1, {}, {}}, {&rawId}, QLocale("de"), "My station"), QStringLiteral("My station"));
    const auto pattern = ProviderMessages::format("%1: %2", catalog->messages.value("hello"), reference, QLocale("de"));
    QVERIFY(pattern);
    QCOMPARE(*pattern, QStringLiteral("data %L2: -9223372036854775808"));
    QVERIFY(!ProviderMessages::validPattern("%L1 %2", catalog->messages.value("hello")));
    QVERIFY(!ProviderMessages::validPattern("%1 %02", catalog->messages.value("hello")));
    QVERIFY(!ProviderMessages::validPattern("%1 %2 %L", catalog->messages.value("hello")));
    auto huge = reference;
    huge.arguments.insert("name", QString(8190, QLatin1Char('x')));
    QCOMPARE(ProviderMessages::render(*catalog, huge, {}, QLocale("de"), "fallback"), QStringLiteral("fallback"));
    QVERIFY(!ProviderMessages::format("%1 %1 %2", catalog->messages.value("hello"), huge, QLocale("de")));
    auto stale = reference;
    stale.revision = 2;
    QCOMPARE(ProviderMessages::render(*catalog, stale, {&bad}, QLocale("de"), "current plaintext"), QStringLiteral("current plaintext"));
  }

  void personalParentWinsOverCommunityExactLocaleInTheSameRoot() {
    QTemporaryDir root;
    QVERIFY(root.isValid());
    const auto personal = root.filePath("overrides/de/shell/arinn.qm");
    const auto community = root.filePath("arinn_de_DE.qm");
    QVERIFY(catalogue(personal, "Persoenlich", "de"));
    QVERIFY(catalogue(community, "Gemeinschaft", "de-DE"));
    LocalizationManager manager({root.path()}, {}, QLocale("en_US"));
    manager.setLanguage("de-DE");
    QCOMPARE(caption(), QStringLiteral("Persoenlich"));
    const auto providerPersonal = root.filePath("overrides/de/providers/radio/messages.qm");
    const auto providerCommunity = root.filePath("providers/radio/messages_de_DE.qm");
    QDir().mkpath(QFileInfo(providerPersonal).absolutePath());
    QDir().mkpath(QFileInfo(providerCommunity).absolutePath());
    QVERIFY(QFile::copy(personal, providerPersonal));
    QVERIFY(QFile::copy(community, providerCommunity));
    QCOMPARE(LocalTranslations::providerCatalogPaths({root.path()}, "radio", "de-DE"), QStringList({providerPersonal, providerCommunity}));
    QVERIFY(LocalTranslations::providerCatalogPaths({root.path()}, "other", "de-DE").isEmpty());
    QVERIFY(LocalTranslations::providerCatalogPaths({root.path()}, "radio", "../de").isEmpty());
    const auto personalSettings = root.filePath("overrides/de/providers/radio/settings.json");
    const auto communitySettings = root.filePath("providers/radio/settings_de_DE.json");
    const auto writeSettings = [](const QString &path, const QString &locale, const QString &label) {
      QFile file(path);
      if (!file.open(QIODevice::WriteOnly)) return false;
      return file.write(QJsonDocument(QJsonObject{{"formatVersion", 1}, {"pluginId", "radio"}, {"locale", locale},
          {"settings", QJsonObject{{"scan", QJsonObject{{"title", QJsonObject{{"source", "Interval"}, {"translation", label}}}}}}}}).toJson()) > 0;
    };
    QVERIFY(writeSettings(personalSettings, "de", "Persoenlich"));
    QVERIFY(writeSettings(communitySettings, "de-DE", "Gemeinschaft"));
    QString translated;
    QVERIFY(LocalTranslations::settingText({root.path()}, "de-DE", "radio", "scan", "title", "Interval", {}, &translated));
    QCOMPARE(translated, QStringLiteral("Persoenlich"));
    QVERIFY(QFile::remove(personalSettings));
    QVERIFY(LocalTranslations::settingText({root.path()}, "de-DE", "radio", "scan", "title", "Interval", {}, &translated));
    QCOMPARE(translated, QStringLiteral("Gemeinschaft"));
    QVERIFY(!LocalTranslations::settingText({root.path()}, "de-DE", "radio", "scan", "title", "Changed source", {}, &translated));
  }

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

  void unsafeActiveReplacementsKeepTheirLastGoodBytes_data() {
    QTest::addColumn<QString>("replacement");
    for (const auto &kind : {"directory", "outside-link", "dangling-link", "unreadable"})
      QTest::newRow(kind) << QString::fromLatin1(kind);
  }

  void unsafeActiveReplacementsKeepTheirLastGoodBytes() {
    QFETCH(QString, replacement);
    QTemporaryDir root, outside;
    QVERIFY(root.isValid() && outside.isValid());
    const auto local = root.filePath("overrides/ru/shell/arinn.qm");
    QVERIFY(catalogue(local, QStringLiteral("Личный перевод")));
    LocalizationManager manager({root.path()}, {}, QLocale(QStringLiteral("en_US")));
    manager.setLanguage("ru");
    QCOMPARE(caption(), QStringLiteral("Личный перевод"));
    const auto before = manager.revision();
    if (replacement == QLatin1String("unreadable")) {
      QVERIFY(QFile::setPermissions(local, QFileDevice::WriteOwner));
      if (QFileInfo(local).isReadable()) QSKIP("This user can still read a file with no read permissions");
    } else {
      QVERIFY(QFile::remove(local));
      if (replacement == QLatin1String("directory")) QVERIFY(QDir().mkpath(local));
      else {
        const auto target = outside.filePath("arinn.qm");
        if (replacement == QLatin1String("outside-link"))
          QVERIFY(catalogue(target, QStringLiteral("Внешний перевод")));
        QVERIFY(QFile::link(target, local));
        QVERIFY(QFileInfo(local).isSymLink());
      }
    }
    manager.reload();
    QVERIFY(manager.reloadFailed());
    QCOMPARE(manager.revision(), before);
    QCOMPARE(caption(), QStringLiteral("Личный перевод"));
    if (replacement == QLatin1String("directory")) QVERIFY(QDir(local).removeRecursively());
    else QVERIFY(QFile::remove(local));
    manager.reload();
    QVERIFY(!manager.reloadFailed());
    QCOMPARE(caption(), QStringLiteral("Продолжить"));
    QVERIFY(catalogue(local, QStringLiteral("Исправлено")));
    manager.reload();
    QVERIFY(!manager.reloadFailed());
    QCOMPARE(caption(), QStringLiteral("Исправлено"));
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
