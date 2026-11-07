// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "music_room.h"
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
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
    } else if (message.member() == QLatin1String("GetPage")) {
      ++reads;
      const auto query = QJsonDocument::fromJson(message.arguments().at(1).toString().toUtf8()).object();
      const auto facet = query.value("facet").toString();
      const auto kind = facet == QLatin1String("albums") ? "album"
                      : facet == QLatin1String("artists") ? "artist" : "track";
      const QJsonArray entries = facet == QLatin1String("selections") ? QJsonArray()
          : QJsonArray{QJsonObject{{"id", facet + ":1"}, {"kind", kind},
                                  {"title", "Content title"}, {"available", true}}};
      result = {{"generation", "generation"}, {"revision", 1},
                {"entries", entries}, {"next_token", ""}};
    } else {
      return false;
    }
    bus.send(message.createReply(QString::fromUtf8(QJsonDocument(result).toJson(QJsonDocument::Compact))));
    return true;
  }
};
class Translation : public QTranslator {
public:
  bool isEmpty() const override { return false; }
  QString translate(const char *context, const char *source, const char *, int) const override {
    return qstrcmp(context, "MusicRoom") == 0 ? QStringLiteral("Translated: ") + QString::fromUtf8(source) : QString();
  }
};
}
class MusicLocalizationTest : public QObject {
  Q_OBJECT
private slots:
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
