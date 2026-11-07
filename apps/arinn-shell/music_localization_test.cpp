// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "music_room.h"
#include "provider_text_client.h"
#include <QFile>
#include <QLibraryInfo>
#include <QProcess>
#include <QTemporaryDir>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QQmlComponent>
#include <QQmlContext>
#include <QQmlEngine>
#include <QQuickItem>
#include <QQuickWindow>
#include <QTranslator>
#include <QtDBus>
#include <QtTest>

namespace {
class SourceList : public QObject {
  Q_OBJECT
  Q_PROPERTY(QVariantList sources MEMBER rows NOTIFY sourcesChanged)
public:
  QVariantList rows;
signals:
  void sourcesChanged();
};
class Script : public QDBusVirtualObject {
public:
  int reads = 0, commands = 0;
  int entryCount = 1;
  bool continuation = false;
  bool retainedFacets = false;
  QString introspect(const QString &) const override { return {}; }
  bool handleMessage(const QDBusMessage &message, const QDBusConnection &bus) override {
    if (message.member() == QLatin1String("Command")) {
      ++commands;
      bus.send(message.createErrorReply(QStringLiteral("org.arinn.Error1.Timeout"), QString()));
      return true;
    }
    QJsonObject result;
    if (message.member() == QLatin1String("GetState")) {
      ++reads;
      result = {{"revision", 1}, {"catalogue_revision", 1}, {"backend", "available"},
                {"account", "local"}, {"actions", QJsonArray{"scan"}},
                {"facets", QJsonArray{"albums", "artists", "tracks"}}};
      if (retainedFacets) { result.insert("catalogue_revision", 12); result.insert("facets", QJsonArray{"selections", "albums", "artists", "tracks"}); }
    } else if (message.member() == QLatin1String("GetPage")) {
      ++reads;
      const auto query = QJsonDocument::fromJson(message.arguments().at(1).toString().toUtf8()).object();
      const auto facet = query.value("facet").toString();
      const auto kind = facet == QLatin1String("albums") ? "album"
                      : facet == QLatin1String("artists") ? "artist" : "track";
      QJsonArray entries;
      if (retainedFacets && facet == QLatin1String("selections"))
        entries.append(QJsonObject{{"id", "fixed-wave"}, {"kind", "station"}, {"title", "My Wave"}, {"available", true}});
      if (facet != QLatin1String("selections"))
        for (int index = 0; index < entryCount; ++index)
          entries.append(QJsonObject{{"id", facet + ":" + QString::number(index + 1)}, {"kind", kind},
                                    {"title", retainedFacets ? "My Wave" : "Content title"}, {"available", true}});
      result = {{"generation", "generation"}, {"revision", 1},
                {"entries", entries}, {"next_token", continuation ? "next" : ""}};
      if (retainedFacets) result.insert("revision", facet == QLatin1String("selections") ? 10 : 12);
    } else {
      return false;
    }
    bus.send(message.createReply(QString::fromUtf8(QJsonDocument(result).toJson(QJsonDocument::Compact))));
    return true;
  }
};
struct RegisteredSource {};
QDBusArgument &operator<<(QDBusArgument &arg, const RegisteredSource &) {
  arg.beginStructure();
  arg << QStringLiteral("source") << QStringLiteral("fixture") << QString() << QStringLiteral("Provider content name")
      << QStringLiteral("1") << QDBusObjectPath("/synthetic/music") << QStringList{"music"}
      << QStringLiteral("ready") << QStringLiteral("available") << QStringLiteral("generation")
      << qulonglong(1) << QString() << qulonglong(0) << qulonglong(0) << qulonglong(0) << qulonglong(0);
  arg.endStructure();
  return arg;
}
const QDBusArgument &operator>>(const QDBusArgument &arg, RegisteredSource &) { return arg; }
class TextRoot : public QDBusVirtualObject {
public:
  QMap<QString, QByteArray> translations;
  int resolves = 0;
  QString introspect(const QString &) const override { return {}; }
  bool handleMessage(const QDBusMessage &message, const QDBusConnection &bus) override {
    if (message.member() == QLatin1String("Providers")) {
      QDBusArgument body;
      body.beginStructure();
      body << QStringLiteral("registry");
      body.beginArray(QMetaType::fromType<RegisteredSource>());
      body << RegisteredSource{};
      body.endArray();
      body << QString();
      body.endStructure();
      bus.send(message.createReply(QVariant::fromValue(body)));
    } else if (message.member() == QLatin1String("GetCatalog")) {
      bus.send(message.createReply(QVariantList{QStringLiteral("generation"), QStringLiteral(R"JSON({"formatVersion":1,"pluginId":"fixture","sourceLocale":"en","locales":["de","ru"],"messages":{"wave":{"revision":1,"source":"My Wave"}}})JSON")}));
    } else if (message.member() == QLatin1String("GetTranslation")) {
      bus.send(message.createReply(translations.value(message.arguments().value(2).toString())));
    } else if (message.member() == QLatin1String("Resolve")) {
      ++resolves;
      QJsonArray resolved;
      const auto requests = QJsonDocument::fromJson(message.arguments().value(2).toString().toUtf8()).array();
      for (qsizetype i = 0; i < requests.size(); ++i) {
        const auto request = requests.at(i).toObject();
        if (request.value("objectId") == QJsonValue(QStringLiteral("fixed-wave")) && request.value("revision").toInteger() == 10)
          resolved.append(QJsonObject{{"requestIndex", i}, {"fallbackText", "My Wave"}, {"message", QJsonObject{{"messageKey", "wave"}, {"messageRevision", 1}}}});
      }
      const auto reply = message.createReply(QString::fromUtf8(QJsonDocument(resolved).toJson(QJsonDocument::Compact)));
      QTimer::singleShot(80, [bus, reply] { bus.send(reply); });
    } else return false;
    return true;
  }
};
QByteArray waveQm(const QString &locale, const QString &translation) {
  QTemporaryDir directory;
  QFile source(directory.filePath("messages.ts"));
  if (!source.open(QIODevice::WriteOnly)) return {};
  source.write(QStringLiteral("<TS version=\"2.1\" language=\"%1\"><context><name></name><message id=\"wave@1\"><source>My Wave</source><translation>%2</translation></message></context></TS>").arg(locale, translation.toHtmlEscaped()).toUtf8());
  source.close();
  const auto output = directory.filePath("messages.qm");
  QProcess compiler;
  compiler.start(QLibraryInfo::path(QLibraryInfo::BinariesPath) + QStringLiteral("/lrelease"),
      {QStringLiteral("-idbased"), source.fileName(), QStringLiteral("-qm"), output});
  if (!compiler.waitForFinished(10000) || compiler.exitCode() != 0) return {};
  QFile qm(output);
  return qm.open(QIODevice::ReadOnly) ? qm.readAll() : QByteArray{};
}
class Translation : public QTranslator {
public:
  bool isEmpty() const override { return false; }
  QString translate(const char *context, const char *source, const char *, int) const override {
    return qstrcmp(context, "MusicRoom") == 0 || qstrcmp(context, "MusicAllView") == 0
        ? QStringLiteral("Translated: ") + QString::fromUtf8(source) : QString();
  }
};
// Arrange a near-end list before making the provider's real continuation
// eligible. The view and the language observer still use the actual MusicRoom.
class PagingRoom : public MusicRoom {
  Q_OBJECT
public:
  bool allowPaging = false;
  Q_INVOKABLE bool more(const QString &shelf, const QString &source = QString()) const {
    return allowPaging && MusicRoom::more(shelf, source);
  }
};
QQuickItem *namedItem(QQuickItem *parent, const QString &name) {
  if (parent->objectName() == name)
    return parent;
  for (auto *child : parent->childItems())
    if (auto *found = namedItem(child, name))
      return found;
  return nullptr;
}
}
class MusicLocalizationTest : public QObject {
  Q_OBJECT
private slots:
  void retainedFacetRevisionsTranslateTogetherWithoutReadingPagesOnALanguageSwitch() {
    delete ProviderTextClient::instance();
    qDBusRegisterMetaType<RegisteredSource>();
    auto bus = QDBusConnection::sessionBus();
    QVERIFY(bus.registerService(QStringLiteral("org.arinn.Daemon1")));
    Script script;
    script.retainedFacets = true;
    TextRoot root;
    root.translations.insert("de", waveQm("de", "Meine Welle"));
    root.translations.insert("ru", waveQm("ru", "Моя волна"));
    QVERIFY(!root.translations.value("de").isEmpty() && !root.translations.value("ru").isEmpty());
    QVERIFY(bus.registerVirtualObject("/synthetic/music", &script));
    QVERIFY(bus.registerVirtualObject("/org/arinn/Daemon1", &root));
    const auto oldTag = qApp->property("arinn.translationLocale");
    const auto oldLocale = QLocale();
    const auto cleanup = qScopeGuard([&] {
      bus.unregisterObject("/synthetic/music");
      bus.unregisterObject("/org/arinn/Daemon1");
      bus.unregisterService("org.arinn.Daemon1");
      QLocale::setDefault(oldLocale);
      qApp->setProperty("arinn.translationLocale", oldTag);
    });
    qApp->setProperty("arinn.translationLocale", QStringLiteral("de"));
    QLocale::setDefault(QLocale("de"));
    SourceList catalog;
    catalog.rows = {QVariantMap{{"id", "source"}, {"name", "Provider content name"},
        {"path", "/synthetic/music"}, {"generation", "generation"}, {"ready", true}}};
    MusicRoom room;
    room.setActive(false);
    room.setCatalog(&catalog);
    QTRY_VERIFY(!room.loading());
    QTRY_COMPARE(room.list("recent").size(), 1);
    QTRY_COMPARE(room.list("stations").size(), 1);
    QTRY_COMPARE(room.list("stations").first().toMap().value("title").toString(), QStringLiteral("Meine Welle"));
    QCOMPARE(room.list("recent").first().toMap().value("title").toString(), QStringLiteral("My Wave"));
    QTest::qWait(120);
    const auto reads = script.reads, resolves = root.resolves;
    qApp->setProperty("arinn.translationLocale", QStringLiteral("ru"));
    QLocale::setDefault(QLocale("ru"));
    QEvent event(QEvent::LanguageChange);
    QCoreApplication::sendEvent(qApp, &event);
    QTRY_COMPARE(room.list("stations").first().toMap().value("title").toString(), QStringLiteral("Моя волна"));
    QCOMPARE(room.list("recent").first().toMap().value("title").toString(), QStringLiteral("My Wave"));
    QCOMPARE(script.reads, reads);
    QCOMPARE(root.resolves, resolves);
    QCOMPARE(script.commands, 0);
  }
  void openAllListRetranslatesAndKeepsItsCardAndTabWithoutPaging() {
    auto bus = QDBusConnection::sessionBus();
    QVERIFY(bus.registerService(QStringLiteral("org.arinn.Daemon1")));
    Script script;
    script.entryCount = 36;
    script.continuation = true;
    QVERIFY(bus.registerVirtualObject(QStringLiteral("/synthetic/music"), &script));
    const auto cleanup = qScopeGuard([&] {
      bus.unregisterObject(QStringLiteral("/synthetic/music"));
      bus.unregisterService(QStringLiteral("org.arinn.Daemon1"));
    });
    SourceList catalog;
    catalog.rows = {QVariantMap{{"id", "source"}, {"name", "Provider content name"},
                               {"path", "/synthetic/music"}, {"generation", "generation"}, {"ready", true}}};
    PagingRoom room;
    room.setActive(false);
    room.setCatalog(&catalog);
    QTRY_VERIFY(!room.loading());
    QTRY_COMPARE(room.list(QStringLiteral("albums")).size(), 36);
    QVERIFY(room.MusicRoom::more(QStringLiteral("albums"), QString()));
    QQmlEngine engine;
    QQuickWindow window;
    engine.rootContext()->setContextProperty(QStringLiteral("testRoom"), &room);
    QQmlComponent component(&engine);
    component.setData(R"(
      import QtQuick
      import "ui/screens/music" as Music
      Music.MusicAllView { room: testRoom }
    )", QUrl::fromLocalFile(QStringLiteral(ARINN_SHELL_SOURCE) + QStringLiteral("/music-all-language-fixture.qml")));
    QScopedPointer<QObject> object(component.create());
    QVERIFY2(object, qPrintable(component.errorString()));
    auto *item = qobject_cast<QQuickItem *>(object.data());
    QVERIFY(item);
    item->setParentItem(window.contentItem());
    window.resize(1920, 1080);
    window.show();
    window.requestActivate();
    QVERIFY(QMetaObject::invokeMethod(object.data(), "open", Q_ARG(QVariant, QStringLiteral("albums")), Q_ARG(QVariant, QStringLiteral("Albums"))));
    auto *grid = namedItem(item, QStringLiteral("musicAllGrid"));
    auto *title = namedItem(item, QStringLiteral("musicAllTitle"));
    QVERIFY(grid && title);
    QTRY_VERIFY(grid->hasActiveFocus());
    QVERIFY(QMetaObject::invokeMethod(object.data(), "focusKey", Q_ARG(QVariant, QStringLiteral("source\nalbums:36"))));
    QTest::qWait(100);
    QCOMPARE(grid->property("currentIndex").toInt(), 35);
    QCOMPARE(title->property("text").toString(), QStringLiteral("Albums"));
    const auto focused = window.activeFocusItem();
    const int reads = script.reads;
    room.allowPaging = true;
    QVERIFY(room.more(QStringLiteral("albums"), QString()));
    QSignalSpy pages(&room, &MusicRoom::listChanged);
    Translation translator;
    QVERIFY(QCoreApplication::installTranslator(&translator));
    const auto remove = qScopeGuard([&] { QCoreApplication::removeTranslator(&translator); });
    engine.retranslate();
    QTest::qWait(100);
    QCOMPARE(script.reads, reads);
    QCOMPARE(pages.count(), 0);
    QTRY_COMPARE(title->property("text").toString(), QStringLiteral("Translated: Albums"));
    QTRY_VERIFY(namedItem(item, QStringLiteral("musicAllTab-1")) && namedItem(item, QStringLiteral("musicAllTab-1"))->property("text").toString().contains(QStringLiteral("Translated: This computer")));
    QTest::qWait(100);
    QCOMPARE(window.activeFocusItem(), focused);
    QCOMPARE(grid->property("currentIndex").toInt(), 35);
    QCOMPARE(object->property("source").toString(), QString());

    auto *tab = namedItem(item, QStringLiteral("musicAllTab-1"));
    QVERIFY(tab);
    tab->forceActiveFocus();
    QCoreApplication::removeTranslator(&translator);
    engine.retranslate();
    QTRY_COMPARE(title->property("text").toString(), QStringLiteral("Albums"));
    QTRY_VERIFY(window.activeFocusItem() && window.activeFocusItem()->objectName() == QStringLiteral("musicAllTab-1"));
    QTRY_COMPARE(window.activeFocusItem()->property("text").toString(), QStringLiteral("This computer"));
    QCOMPARE(object->property("source").toString(), QString());
    QCOMPARE(script.reads, reads);
    room.allowPaging = false;
    QVERIFY(object->setProperty("source", QStringLiteral("source")));
    QVERIFY(object->setProperty("alphabetical", true));
    QTest::qWait(100);
    room.allowPaging = true;
    QQuickItem other(window.contentItem());
    other.forceActiveFocus();
    QVERIFY(QCoreApplication::installTranslator(&translator));
    engine.retranslate();
    QTest::qWait(100);
    QCOMPARE(window.activeFocusItem(), &other);
    QCOMPARE(object->property("source").toString(), QStringLiteral("source"));
    QVERIFY(object->property("alphabetical").toBool());
    QCOMPARE(object->property("focusedEntry").toMap().value("key").toString(), QStringLiteral("source\nalbums:36"));
    QCOMPARE(script.reads, reads);
  }

  void cachedRoomAndSearchLabelsChangeWithoutProviderReads() {
    auto bus = QDBusConnection::sessionBus();
    QVERIFY(bus.registerService(QStringLiteral("org.arinn.Daemon1")));
    Script script;
    QVERIFY(bus.registerVirtualObject(QStringLiteral("/synthetic/music"), &script));
    const auto cleanup = qScopeGuard([&] {
      bus.unregisterObject(QStringLiteral("/synthetic/music"));
      bus.unregisterService(QStringLiteral("org.arinn.Daemon1"));
    });
    SourceList catalog;
    catalog.rows = {QVariantMap{{"id", "source"}, {"name", "Provider content name"},
                               {"path", "/synthetic/music"}, {"generation", "generation"}, {"ready", true}}};
    MusicRoom room;
    room.setActive(false);
    room.setCatalog(&catalog);
    QTRY_VERIFY(!room.shelves().isEmpty());
    QTRY_VERIFY(!room.loading());
    room.search(QStringLiteral("query"));
    QTRY_VERIFY(!room.searchShelves().isEmpty());
    QTRY_VERIFY(room.searchPending().isEmpty());
    room.sourceCommand(QStringLiteral("source"), QStringLiteral("scan"));
    QTRY_VERIFY(!room.sources().first().toMap().value("error").toString().isEmpty());
    QTest::qWait(100);
    const auto shelves = room.shelves();
    const auto search = room.searchShelves();
    const int reads = script.reads, commands = script.commands;
    Translation translator;
    QVERIFY(QCoreApplication::installTranslator(&translator));
    const auto remove = qScopeGuard([&] { QCoreApplication::removeTranslator(&translator); });
    QTRY_VERIFY(room.shelves().first().toMap().value("title").toString().startsWith("Translated: "));
    QTRY_VERIFY(room.searchShelves().first().toMap().value("title").toString().startsWith("Translated: "));
    QTRY_VERIFY(room.sources().first().toMap().value("error").toString().startsWith("Translated: "));
    QCOMPARE(room.sources().first().toMap().value("name").toString(), QStringLiteral("Provider content name"));
    for (qsizetype i = 0; i < shelves.size(); ++i) {
      QCOMPARE(room.shelves().at(i).toMap().value("id"), shelves.at(i).toMap().value("id"));
      QCOMPARE(room.shelves().at(i).toMap().value("entries"), shelves.at(i).toMap().value("entries"));
    }
    for (qsizetype i = 0; i < search.size(); ++i)
      QCOMPARE(room.searchShelves().at(i).toMap().value("entries"), search.at(i).toMap().value("entries"));
    QCOMPARE(room.searchQuery(), QStringLiteral("query"));
    QCOMPARE(script.reads, reads);
    QCOMPARE(script.commands, commands);
  }
};
QTEST_MAIN(MusicLocalizationTest)
#include "music_localization_test.moc"
