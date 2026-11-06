// SPDX-FileCopyrightText: 2026 Arinn contributors
//
// SPDX-License-Identifier: GPL-3.0-or-later

// MusicCatalog against a scripted daemon on the session bus (ADR-0134).
// Synthetic providers and tracks only.
#include "asset_images.h"
#include "music_catalog.h"
#include "music_room.h"
#include <QFile>
#include <QQmlComponent>
#include <QQmlContext>
#include <QQmlEngine>
#include <QQmlPropertyMap>
#include <QRegularExpression>
#include <QXmlStreamReader>
#include <memory>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QtDBus>
#include <QtTest>
namespace {
struct ProviderRow {
  QString id;
  QString name;
  QString generation;
};
QDBusArgument &operator<<(QDBusArgument &argument, const ProviderRow &row) {
  argument.beginStructure();
  argument << row.id << QStringLiteral("plugin") << QStringLiteral("system")
           << row.name << QStringLiteral("1")
           << QDBusObjectPath(QStringLiteral("/org/arinn/Daemon1/providers/p_") + row.id)
           << QStringList{QStringLiteral("music")} << QStringLiteral("ready")
           << QStringLiteral("available") << row.generation << qulonglong(1) << QString()
           << qulonglong(0) << qulonglong(0) << qulonglong(0) << qulonglong(0);
  argument.endStructure();
  return argument;
}
const QDBusArgument &operator>>(const QDBusArgument &argument, ProviderRow &) {
  return argument;
}
class MusicDaemon : public QDBusVirtualObject {
public:
  QList<ProviderRow> providers;
  // GetState JSON per provider, or an error code name.
  QHash<QString, QJsonObject> states;
  QHash<QString, QString> stateErrors;
  QHash<QString, int> pageReads;
  // Entries per "provider/facet"; a facet without its own uses one track.
  QHash<QString, QJsonArray> pages;
  // With a page size, those entries come in pages with tokens "p<n>"; the
  // next `conflicts` continuations fail as an expired lease does.
  int pageSize = 0;
  int conflicts = 0;
  // A continuation token that fails as Unavailable this many times, and one
  // whose page names itself as the next token, as a looping provider does.
  QHash<QString, int> tokenFailures;
  QString stuckToken;
  QStringList commands;
  QString commandError;
  QString pageError;
  QSet<QString> registryMethods;
  QString active = QStringLiteral("null");
  QString introspect(const QString &) const override { return {}; }
  bool handleMessage(const QDBusMessage &message, const QDBusConnection &bus) override {
    const auto member = message.member();
    // The fixture may not invent a daemon method absent from the wire contract.
    if (message.interface() == QLatin1String("org.arinn.Daemon1")
        && !registryMethods.contains(member))
      return false;
    if (member == QLatin1String("Providers")) {
      QDBusArgument body;
      body.beginStructure();
      body << QStringLiteral("snapshot");
      body.beginArray(QMetaType::fromType<ProviderRow>());
      for (const auto &row : providers)
        body << row;
      body.endArray();
      body << QString();
      body.endStructure();
      bus.send(message.createReply(QVariant::fromValue(body)));
      return true;
    }
    if (member == QLatin1String("GetActive")) {
      bus.send(message.createReply(active));
      return true;
    }
    const auto id = message.path().section(QStringLiteral("p_"), 1);
    if(member == QLatin1String("Command")) {
      const auto command=QJsonDocument::fromJson(message.arguments().at(1).toString().toUtf8()).object();
      commands.append(command.value("action").toString());
      if(!commandError.isEmpty()) {bus.send(message.createErrorReply("org.arinn.Error1."+commandError,commandError));return true;}
      bus.send(message.createReply(QString::fromUtf8(QJsonDocument(states.value(id)).toJson(QJsonDocument::Compact))));return true;
    }
    if (member == QLatin1String("GetState")) {
      if (stateErrors.contains(id)) {
        const auto code = stateErrors.value(id);
        bus.send(message.createErrorReply(QStringLiteral("org.arinn.Error1.") + code, code));
        return true;
      }
      bus.send(message.createReply(
          QString::fromUtf8(QJsonDocument(states.value(id)).toJson(QJsonDocument::Compact))));
      return true;
    }
    if (member == QLatin1String("GetPage")) {
      ++pageReads[id];
      if (!pageError.isEmpty()) {
        bus.send(message.createErrorReply(QStringLiteral("org.arinn.Error1.") + pageError, pageError));
        return true;
      }
      // As a provider without a catalogue answers.
      if (states.value(id).value("facets").toArray().isEmpty()) {
        bus.send(message.createErrorReply(QStringLiteral("org.arinn.Error1.Unsupported"),
                                          QStringLiteral("Unsupported")));
        return true;
      }
      const auto facet = QJsonDocument::fromJson(message.arguments().value(1).toString().toUtf8())
                             .object().value("facet").toString();
      const auto token = QJsonDocument::fromJson(message.arguments().value(1).toString().toUtf8())
                             .object().value("token").toString();
      if (!token.isEmpty() && conflicts > 0) {
        --conflicts;
        bus.send(message.createErrorReply(QStringLiteral("org.arinn.Error1.Conflict"),
                                          QStringLiteral("Conflict")));
        return true;
      }
      if (!token.isEmpty() && tokenFailures.value(token) > 0) {
        --tokenFailures[token];
        bus.send(message.createErrorReply(QStringLiteral("org.arinn.Error1.Unavailable"),
                                          QStringLiteral("Unavailable")));
        return true;
      }
      const auto own = pages.find(id + QLatin1Char('/') + facet);
      auto entries = own != pages.end() ? *own
                     : QJsonArray{QJsonObject{{"id", "t1"},
                                              {"kind", "track"},
                                              {"title", "Synthetic"},
                                              {"available", true}}};
      QString next;
      if (pageSize > 0 && own != pages.end()) {
        const int offset = token.isEmpty() ? 0 : token.mid(1).toInt() * pageSize;
        QJsonArray slice;
        for (int i = offset; i < std::min<int>(offset + pageSize, entries.size()); ++i)
          slice.append(entries.at(i));
        if (offset + pageSize < entries.size())
          next = QStringLiteral("p%1").arg(offset / pageSize + 1);
        entries = slice;
        if (!stuckToken.isEmpty() && token == stuckToken)
          next = stuckToken;
      }
      QJsonObject page{{"generation", message.arguments().value(0).toString()},
                       {"revision", 1},
                       {"entries", entries},
                       {"next_token", next}};
      bus.send(message.createReply(
          QString::fromUtf8(QJsonDocument(page).toJson(QJsonDocument::Compact))));
      return true;
    }
    return false;
  }
};
QJsonObject state(const QJsonArray &facets, const QString &account = QStringLiteral("local")) {
  return QJsonObject{{"revision", 1},     {"catalogue_revision", 1},
                     {"backend", "available"}, {"account", account},
                     {"facets", facets},  {"actions", QJsonArray{}}};
}
} // namespace

class MusicCatalogTest : public QObject {
  Q_OBJECT
  MusicDaemon daemon;
private slots:
  void initTestCase() {
    qDBusRegisterMetaType<ProviderRow>();
    QFile contract(QString::fromUtf8(ARINN_DAEMON_XML));
    QVERIFY(contract.open(QIODevice::ReadOnly));
    QXmlStreamReader xml(contract.readAll());
    QString interface;
    while (!xml.atEnd()) {
      xml.readNext();
      if (xml.isStartElement() && xml.name() == QLatin1String("interface"))
        interface = xml.attributes().value("name").toString();
      if (xml.isStartElement() && xml.name() == QLatin1String("method")
          && interface == QLatin1String("org.arinn.Daemon1"))
        daemon.registryMethods.insert(xml.attributes().value("name").toString());
    }
    QVERIFY(!xml.hasError());
    QVERIFY(daemon.registryMethods.contains(QStringLiteral("Providers")));
    QVERIFY(!daemon.registryMethods.contains(QStringLiteral("GetProviders")));
    auto bus = QDBusConnection::sessionBus();
    QVERIFY(bus.registerService(QStringLiteral("org.arinn.Daemon1")));
    QVERIFY(bus.registerVirtualObject(QStringLiteral("/org/arinn"), &daemon,
                                      QDBusConnection::SubPath));
  }
  void init() {
    daemon.commandError.clear();
    daemon.pageError.clear();
    daemon.providers.clear();
    daemon.states.clear();
    daemon.stateErrors.clear();
    daemon.pageReads.clear();
    daemon.pages.clear();
    daemon.pageSize = 0;
    daemon.conflicts = 0;
    daemon.tokenFailures.clear();
    daemon.stuckToken.clear();
    daemon.commands.clear();
    daemon.active = QStringLiteral("null");
  }
  // #800, ADR-0142 MH-9: personal entries first, providers in Sources
  // order; dated ordinary entries newest first; undated ones alternate.
  void roomOrdersPersonalThenDatedThenAlternating() {
    auto entry = [](const char *key, qulonglong at = 0) {
      QVariantMap value{{"key", QString::fromLatin1(key)}};
      if (at)
        value.insert("added_at", at);
      return QVariant(value);
    };
    auto keys = [](const QVariantList &list) {
      QStringList out;
      for (const auto &value : list)
        out.append(value.toMap().value("key").toString());
      return out.join(',');
    };
    // Without dates: each provider's own order, one from each in turn.
    QCOMPARE(keys(MusicRoom::order({{entry("a-wave")}, {}},
                                   {{entry("a1"), entry("a2"), entry("a3")},
                                    {entry("b1"), entry("b2")}})),
             QStringLiteral("a-wave,a1,b1,a2,b2,a3"));
    // Dated entries newest first, undated ones after them.
    QCOMPARE(keys(MusicRoom::order({}, {{entry("a1", 100), entry("a2", 300)},
                                        {entry("b1"), entry("b2", 200)}})),
             QStringLiteral("a2,b2,a1,b1"));
    // Personal entries of a later provider still precede ordinary ones.
    QCOMPARE(keys(MusicRoom::order({{}, {entry("b-mix")}}, {{entry("a1", 5)}, {}})),
             QStringLiteral("b-mix,a1"));
  }
  // #800: late entries never move focus. The shown cards keep their places,
  // new ones join at the end and gone ones leave.
  void roomKeepsShownEntriesInPlace() {
    auto list = [](const QStringList &keys) {
      QVariantList out;
      for (const auto &key : keys)
        out.append(QVariantMap{{"key", key}});
      return out;
    };
    auto keys = [](const QVariantList &values) {
      QStringList out;
      for (const auto &value : values)
        out.append(value.toMap().value("key").toString());
      return out;
    };
    QCOMPARE(keys(MusicRoom::stable(list({"a1", "a2"}), list({"b1", "a1", "b2", "a2"}))),
             (QStringList{"a1", "a2", "b1", "b2"}));
    QCOMPARE(keys(MusicRoom::stable(list({"a1", "b1", "a2"}), list({"a1", "a2"}))),
             (QStringList{"a1", "a2"}));
  }
  // #800 (MH-1, MH-5, MH-10): one room over every source. Each source is
  // paged on its own, shelves gather them with source marks, identical ids
  // stay two cards, a shelf no source supplies is absent, the Sources sheet
  // carries each state and its housekeeping, and a source that leaves takes
  // its entries along.
  void roomGathersShelvesFromEverySource() {
    daemon.providers = {{"local", "Fixture music", "g1"}, {"account", "Fixture account", "g1"}};
    auto local = state({"albums", "tracks", "playlists"});
    local["actions"] = QJsonArray{"play", "scan"};
    auto account = state({"selections", "albums"}, "connected");
    account["actions"] = QJsonArray{"play", "disconnect"};
    daemon.states["local"] = local;
    daemon.states["account"] = account;
    auto album = [](const char *id, const char *title, qint64 at = 0) {
      QJsonObject value{{"id", id}, {"kind", "album"}, {"title", title}, {"available", true}};
      if (at)
        value.insert("added_at", at);
      return value;
    };
    daemon.pages["local/albums"] = QJsonArray{album("album:1", "Old", 100), album("album:2", "New", 900)};
    daemon.pages["account/albums"] = QJsonArray{album("album:1", "Same id"), album("album:9", "Remote")};
    daemon.pages["account/selections"] = QJsonArray{
        QJsonObject{{"id", "station:mix"}, {"kind", "station"}, {"title", "Mix"}, {"available", true}}};
    daemon.pages["local/tracks"] = QJsonArray{};
    daemon.pages["local/playlists"] = QJsonArray{};
    MusicCatalog catalog;
    MusicRoom room;
    room.setCatalog(&catalog);
    QTRY_COMPARE(room.sources().size(), 2);
    QTRY_VERIFY(room.connected());
    QTRY_VERIFY(!room.loading());
    QStringList ids;
    QVariantList albums;
    for (const auto &value : room.shelves()) {
      const auto shelf = value.toMap();
      ids.append(shelf.value("id").toString());
      if (shelf.value("id") == "albums")
        albums = shelf.value("entries").toList();
    }
    // Stations from the personal facet, Albums from both; no source offers
    // playlists, tracks or artists here, so those shelves are absent.
    QCOMPARE(ids, (QStringList{"stations", "albums"}));
    QStringList keys;
    for (const auto &value : albums)
      keys.append(value.toMap().value("key").toString());
    QCOMPARE(keys, (QStringList{"local\nalbum:2", "local\nalbum:1", "account\nalbum:1",
                                "account\nalbum:9"}));
    QCOMPARE(albums.first().toMap().value("local").toBool(), true);
    QCOMPARE(albums.at(2).toMap().value("providerName").toString(), QStringLiteral("Fixture account"));
    QVERIFY(room.dated());
    // The All list filters by source and sorts by title on request.
    QCOMPARE(room.list("albums", "account").size(), 2);
    QCOMPARE(room.list("albums", QString(), true).first().toMap().value("title").toString(),
             QStringLiteral("New"));
    const auto sources = room.sources();
    QCOMPARE(sources.at(0).toMap().value("status").toString(), QStringLiteral("connected"));
    QCOMPARE(sources.at(0).toMap().value("actions").toStringList(), QStringList{"scan"});
    QCOMPARE(sources.at(1).toMap().value("actions").toStringList(), QStringList{"disconnect"});
    // Housekeeping goes to the source it belongs to, by capability only.
    room.sourceCommand("account", "scan");
    room.sourceCommand("account", "disconnect");
    QTRY_COMPARE(daemon.commands, QStringList{"disconnect"});
    QTRY_VERIFY(!room.sources().at(1).toMap().value("busy").toBool());
    QVERIFY(room.sources().at(1).toMap().value("error").toString().isEmpty());
    // Codex review of #812: a refused command says so in the sheet.
    daemon.commandError = QStringLiteral("Unavailable");
    room.sourceCommand("account", "disconnect");
    QTRY_VERIFY(!room.sources().at(1).toMap().value("error").toString().isEmpty());
    daemon.commandError.clear();
    // Review of #812: a new catalogue revision keeps the source's cards
    // until its new first pages replace them, so none leave and return.
    {
      bool emptied = false;
      const auto watch = connect(&room, &MusicRoom::listChanged, this, [&] {
        if (room.list("albums", "local").isEmpty())
          emptied = true;
      });
      const auto reads = daemon.pageReads.value("local");
      local["catalogue_revision"] = 2;
      daemon.states["local"] = local;
      room.refresh();
      QTRY_VERIFY(daemon.pageReads.value("local") >= reads + 3);
      QTRY_VERIFY(!room.loading());
      disconnect(watch);
      QVERIFY(!emptied);
      QCOMPARE(room.list("albums", "local").size(), 2);
    }
    // A source that stops answering leaves the shelves, and the sheet says why.
    daemon.stateErrors["account"] = QStringLiteral("Timeout");
    room.refresh();
    QTRY_COMPARE(room.sources().at(1).toMap().value("status").toString(), QStringLiteral("unavailable"));
    QTRY_COMPARE(room.list("albums").size(), 2);
    QVERIFY(!room.sources().at(1).toMap().value("reason").toString().isEmpty());
    // A source that leaves the registry leaves the room.
    daemon.providers = {{"local", "Fixture music", "g1"}};
    catalog.refresh();
    QTRY_COMPARE(room.sources().size(), 1);
  }
  // Review of #812: a shelf's All list pages its personal entries in
  // `selections` as well as its own facet, and a continuation that fails (a
  // lease expires after 30 s) reads again from the start and pages on
  // instead of stopping at the first page.
  void roomPagesSelectionsAndResumesAfterAnExpiredLease() {
    daemon.providers = {{"account", "Fixture account", "g1"}};
    auto account = state({"selections", "albums"}, "connected");
    account["actions"] = QJsonArray{"play"};
    daemon.states["account"] = account;
    QJsonArray mine, albums;
    for (int i = 0; i < 5; ++i) {
      mine.append(QJsonObject{{"id", QStringLiteral("album:mine%1").arg(i)}, {"kind", "album"},
                              {"title", QStringLiteral("Mine %1").arg(i)}, {"available", true}});
      albums.append(QJsonObject{{"id", QStringLiteral("album:%1").arg(i)}, {"kind", "album"},
                                {"title", QStringLiteral("Album %1").arg(i)}, {"available", true}});
    }
    daemon.pages["account/selections"] = mine;
    daemon.pages["account/albums"] = albums;
    daemon.pageSize = 2;
    MusicCatalog catalog;
    MusicRoom room;
    room.setCatalog(&catalog);
    QTRY_VERIFY(room.connected());
    QTRY_VERIFY(!room.loading());
    QCOMPARE(room.list("albums").size(), 4);
    QVERIFY(room.more("albums"));
    // The next continuation fails once: the facet starts over and pages on.
    daemon.conflicts = 1;
    for (int i = 0; i < 6 && room.more("albums"); ++i) {
      const auto before = room.list("albums").size();
      room.loadMore("albums");
      QTRY_VERIFY(room.list("albums").size() > before || !room.more("albums"));
    }
    QTRY_COMPARE(room.list("albums").size(), 10);
    QVERIFY(!room.more("albums"));
    // Personal entries still lead the full list.
    const auto list = room.list("albums");
    for (int i = 0; i < 5; ++i)
      QVERIFY(list.at(i).toMap().value("id").toString().startsWith("album:mine"));
  }
  // Codex review of #812: a first page that fails (a timeout) is read again
  // on a later state poll instead of leaving its shelf out until the next
  // catalogue revision.
  void roomRetriesAFailedFirstPage() {
    daemon.providers = {{"local", "Fixture music", "g1"}};
    daemon.states["local"] = state({"albums"});
    daemon.pages["local/albums"] = QJsonArray{
        QJsonObject{{"id", "album:1"}, {"kind", "album"}, {"title", "One"}, {"available", true}}};
    daemon.pageError = QStringLiteral("Timeout");
    MusicCatalog catalog;
    MusicRoom room;
    QList<bool> loads;
    connect(&room, &MusicRoom::loadingChanged, this, [&] { loads.append(room.loading()); });
    room.setCatalog(&catalog);
    QTRY_VERIFY(room.connected());
    QTRY_VERIFY(!room.loading());
    // The last notification carries false, with the shelves still empty.
    QTRY_VERIFY(!loads.isEmpty() && !loads.last());
    QVERIFY(loads.contains(true));
    QVERIFY(room.list("albums").isEmpty());
    // Review of #812: QML hears `loading` turn false although the shelves
    // stay empty, and retries do not raise it again.
    QCOMPARE(room.property("loading").toBool(), false);
    QList<bool> seen;
    const auto watch = connect(&room, &MusicRoom::loadingChanged, this,
                               [&] { seen.append(room.loading()); });
    QTest::qWait(4500);
    disconnect(watch);
    QVERIFY2(!seen.contains(true), "a retry raised loading");
    daemon.pageError.clear();
    QTRY_COMPARE_WITH_TIMEOUT(room.list("albums").size(), 1, 20000);
    // A refresh for a new revision that fails keeps the old entries shown
    // and is read again, not left stale until the next revision.
    daemon.pages["local/albums"].append(
        QJsonObject{{"id", "album:2"}, {"kind", "album"}, {"title", "Two"}, {"available", true}});
    auto local = state({"albums"});
    local["catalogue_revision"] = 2;
    daemon.states["local"] = local;
    daemon.pageError = QStringLiteral("Timeout");
    const auto reads = daemon.pageReads.value("local");
    room.refresh();
    QTRY_VERIFY(daemon.pageReads.value("local") > reads);
    QTest::qWait(200);
    QCOMPARE(room.list("albums").size(), 1);
    daemon.pageError.clear();
    QTRY_COMPARE_WITH_TIMEOUT(room.list("albums").size(), 2, 20000);
  }
  // Codex review of #812: a source that has not answered its state counts
  // as loading, and `dated` sees dates past a shelf's preview.
  void roomLoadsUntilStatesAnswerAndSeesDatesBeyondThePreview() {
    daemon.providers = {{"local", "Fixture music", "g1"}};
    daemon.states["local"] = state({"selections", "albums"});
    QJsonArray mine;
    for (int i = 0; i < 30; ++i)
      mine.append(QJsonObject{{"id", QStringLiteral("album:mine%1").arg(i)}, {"kind", "album"},
                              {"title", QStringLiteral("Mine %1").arg(i)}, {"available", true}});
    daemon.pages["local/selections"] = mine;
    daemon.pages["local/albums"] = QJsonArray{QJsonObject{
        {"id", "album:dated"}, {"kind", "album"}, {"title", "Dated"}, {"available", true},
        {"added_at", 1700000000}}};
    MusicCatalog catalog;
    MusicRoom room;
    // The moment the source is known, before its state answers, the room is
    // loading.
    int seen = -1;
    connect(&room, &MusicRoom::sourcesChanged, this, [&] {
      if (seen < 0 && !room.sources().isEmpty())
        seen = room.loading() ? 1 : 0;
    });
    room.setCatalog(&catalog);
    QTRY_VERIFY(seen >= 0);
    QCOMPARE(seen, 1);
    QTRY_VERIFY(room.connected());
    QTRY_VERIFY(!room.loading());
    QVERIFY(room.dated());
  }
  // #801 (MH-10): Find searches every browsable source; each shelf takes
  // the sources' results in turn, in their own order, unmerged; the sources
  // still answering are named; an empty query clears the results.
  void roomSearchesEverySource() {
    daemon.providers = {{"local", "Fixture music", "g1"}, {"account", "Fixture account", "g1"}};
    daemon.states["local"] = state({"albums", "tracks"});
    daemon.states["account"] = state({"albums", "artists"}, "connected");
    auto album = [](const char *id) {
      return QJsonObject{{"id", id}, {"kind", "album"}, {"title", id}, {"available", true},
                         {"added_at", 1700000000}};
    };
    daemon.pages["local/albums"] = QJsonArray{album("album:a"), album("album:b")};
    daemon.pages["account/albums"] = QJsonArray{album("album:a"), album("album:z")};
    daemon.pages["local/tracks"] = QJsonArray{};
    daemon.pages["account/artists"] = QJsonArray{};
    MusicCatalog catalog;
    MusicRoom room;
    room.setCatalog(&catalog);
    QTRY_VERIFY(room.connected());
    QTRY_VERIFY(!room.loading());
    room.search(QStringLiteral("fixture"));
    QCOMPARE(room.searchQuery(), QStringLiteral("fixture"));
    QVERIFY(!room.searchPending().isEmpty());
    QTRY_VERIFY(room.searchPending().isEmpty());
    const auto shelves = room.searchShelves();
    QCOMPARE(shelves.size(), 1);
    QStringList keys;
    for (const auto &entry : shelves.first().toMap().value("entries").toList())
      keys.append(entry.toMap().value("key").toString());
    QCOMPARE(keys, (QStringList{"local\nalbum:a", "account\nalbum:a", "local\nalbum:b",
                                "account\nalbum:z"}));
    // Codex review of #819: a source that leaves during a search takes its
    // results along; one that joins is searched too. The results of the
    // source that stays never leave meanwhile, so focus on them stays.
    qsizetype least = 4;
    const auto watch = connect(&room, &MusicRoom::searchChanged, this, [&] {
      const auto now = room.searchShelves();
      least = std::min<qsizetype>(
          least, now.isEmpty() ? 0 : now.first().toMap().value("entries").toList().size());
    });
    daemon.providers = {{"local", "Fixture music", "g1"}};
    catalog.refresh();
    QTRY_COMPARE(room.searchShelves().first().toMap().value("entries").toList().size(), 2);
    daemon.providers = {{"local", "Fixture music", "g1"}, {"account", "Fixture account", "g1"}};
    catalog.refresh();
    QTRY_COMPARE(room.searchShelves().first().toMap().value("entries").toList().size(), 4);
    QTRY_VERIFY(room.searchPending().isEmpty());
    disconnect(watch);
    QCOMPARE(least, 2);
    // A query beyond the protocol's 1024 bytes finds nothing: the results
    // of the shorter query do not stay (Codex review of #819).
    const QString tooLong(1025, QLatin1Char('x'));
    room.search(tooLong);
    QCOMPARE(room.searchQuery(), tooLong);
    QVERIFY(room.searchShelves().isEmpty());
    QVERIFY(room.searchPending().isEmpty());
    room.search(QString());
    QVERIFY(room.searchShelves().isEmpty());
  }
  // Codex review of #819: a search page refused while the provider's slots
  // are busy is read again, so the source still contributes its results.
  void roomRetriesARefusedSearchPage() {
    daemon.providers = {{"account", "Fixture account", "g1"}};
    daemon.states["account"] = state({"albums"}, "connected");
    daemon.pages["account/albums"] =
        QJsonArray{QJsonObject{{"id", "album:a"}, {"kind", "album"}, {"title", "A"},
                               {"available", true}}};
    MusicCatalog catalog;
    MusicRoom room;
    room.setCatalog(&catalog);
    QTRY_VERIFY(room.connected());
    QTRY_VERIFY(!room.loading());
    const auto before = daemon.pageReads.value("account");
    daemon.pageError = QStringLiteral("Unavailable");
    room.search(QStringLiteral("fixture"));
    QTRY_VERIFY(daemon.pageReads.value("account") > before);
    QTest::qWait(100);
    daemon.pageError.clear();
    QVERIFY(!room.searchPending().isEmpty());
    QTRY_COMPARE_WITH_TIMEOUT(room.searchShelves().size(), 1, 10000);
    QTRY_VERIFY(room.searchPending().isEmpty());
  }
  // Follow-up of #812 (in #801): a resume whose catch-up page fails keeps
  // the shown entries and its target, and the retry resumes to them instead
  // of showing only the first page.
  void roomKeepsAListsPlaceWhenACatchUpFails() {
    daemon.providers = {{"account", "Fixture account", "g1"}};
    auto account = state({"albums"}, "connected");
    account["actions"] = QJsonArray{"play"};
    daemon.states["account"] = account;
    QJsonArray albums;
    for (int i = 0; i < 9; ++i)
      albums.append(QJsonObject{{"id", QStringLiteral("album:%1").arg(i)}, {"kind", "album"},
                                {"title", QStringLiteral("Album %1").arg(i)}, {"available", true}});
    daemon.pages["account/albums"] = albums;
    daemon.pageSize = 3;
    MusicCatalog catalog;
    MusicRoom room;
    room.setCatalog(&catalog);
    QTRY_VERIFY(room.connected());
    QTRY_VERIFY(!room.loading());
    room.loadMore("albums");
    QTRY_COMPARE(room.list("albums").size(), 6);
    int least = 6;
    const auto watch = connect(&room, &MusicRoom::listChanged, this, [&] {
      least = std::min<int>(least, room.list("albums").size());
    });
    // The continuation fails, then the resume's catch-up page fails too.
    daemon.conflicts = 2;
    room.loadMore("albums");
    QTRY_COMPARE_WITH_TIMEOUT(room.list("albums").size(), 9, 20000);
    disconnect(watch);
    QCOMPARE(least, 6);
  }
  // Review of #819: a resume retried from the source's chain of first
  // pages goes on with the chain, so a second failed facet is read again too.
  void roomRetriesEveryFacetAfterFailedResumes() {
    daemon.providers = {{"account", "Fixture account", "g1"}};
    daemon.states["account"] = state({"albums", "tracks"}, "connected");
    QJsonArray albums, tracks;
    for (int i = 0; i < 9; ++i) {
      albums.append(QJsonObject{{"id", QStringLiteral("album:%1").arg(i)}, {"kind", "album"},
                                {"title", QStringLiteral("Album %1").arg(i)}, {"available", true}});
      tracks.append(QJsonObject{{"id", QStringLiteral("track:%1").arg(i)}, {"kind", "track"},
                                {"title", QStringLiteral("Track %1").arg(i)}, {"available", true}});
    }
    daemon.pages["account/albums"] = albums;
    daemon.pages["account/tracks"] = tracks;
    daemon.pageSize = 3;
    MusicCatalog catalog;
    MusicRoom room;
    room.setCatalog(&catalog);
    QTRY_VERIFY(room.connected());
    QTRY_VERIFY(!room.loading());
    room.loadMore("albums");
    room.loadMore("recent");
    QTRY_COMPARE(room.list("albums").size(), 6);
    QTRY_COMPARE(room.list("recent").size(), 6);
    // Both continuations fail, and so do both resumes.
    daemon.pageError = QStringLiteral("Unavailable");
    room.loadMore("albums");
    room.loadMore("recent");
    QTRY_VERIFY(daemon.pageReads.value("account") >= 8);
    QTest::qWait(200);
    daemon.pageError.clear();
    QTRY_COMPARE_WITH_TIMEOUT(room.list("albums").size(), 9, 20000);
    QTRY_COMPARE_WITH_TIMEOUT(room.list("recent").size(), 9, 20000);
  }
  // Review of #819: a retried resume reads its tokens afresh, so a token
  // read before the failure does not end it at the first page.
  void roomRetriedResumeReadsItsTokensAgain() {
    daemon.providers = {{"account", "Fixture account", "g1"}};
    daemon.states["account"] = state({"albums"}, "connected");
    QJsonArray albums;
    for (int i = 0; i < 9; ++i)
      albums.append(QJsonObject{{"id", QStringLiteral("album:%1").arg(i)}, {"kind", "album"},
                                {"title", QStringLiteral("Album %1").arg(i)}, {"available", true}});
    daemon.pages["account/albums"] = albums;
    daemon.pageSize = 3;
    MusicCatalog catalog;
    MusicRoom room;
    room.setCatalog(&catalog);
    QTRY_VERIFY(room.connected());
    QTRY_VERIFY(!room.loading());
    room.loadMore("albums");
    QTRY_COMPARE(room.list("albums").size(), 6);
    int least = 6;
    const auto watch = connect(&room, &MusicRoom::listChanged, this, [&] {
      least = std::min<int>(least, room.list("albums").size());
    });
    // The continuation fails; the resume reads p1, then fails on p2.
    daemon.tokenFailures["p2"] = 2;
    room.loadMore("albums");
    QTRY_COMPARE_WITH_TIMEOUT(room.list("albums").size(), 9, 20000);
    disconnect(watch);
    QCOMPARE(least, 6);
  }
  // Follow-up of #812, review of #819: a provider that repeats a token
  // ends the resume, and a shorter read never replaces what is shown.
  void roomEndsAResumeOnARepeatedToken() {
    daemon.providers = {{"account", "Fixture account", "g1"}};
    daemon.states["account"] = state({"albums"}, "connected");
    QJsonArray albums;
    for (int i = 0; i < 12; ++i)
      albums.append(QJsonObject{{"id", QStringLiteral("album:%1").arg(i)}, {"kind", "album"},
                                {"title", QStringLiteral("Album %1").arg(i)}, {"available", true}});
    daemon.pages["account/albums"] = albums;
    daemon.pageSize = 3;
    MusicCatalog catalog;
    MusicRoom room;
    room.setCatalog(&catalog);
    QTRY_VERIFY(room.connected());
    QTRY_VERIFY(!room.loading());
    room.loadMore("albums");
    QTRY_COMPARE(room.list("albums").size(), 6);
    room.loadMore("albums");
    QTRY_COMPARE(room.list("albums").size(), 9);
    int least = 9;
    const auto watch = connect(&room, &MusicRoom::listChanged, this, [&] {
      least = std::min<int>(least, room.list("albums").size());
    });
    // The lease expires, and on the resume the provider loops on p1.
    daemon.conflicts = 1;
    daemon.stuckToken = QStringLiteral("p1");
    const auto before = daemon.pageReads.value("account");
    room.loadMore("albums");
    QTRY_VERIFY(!room.more("albums"));
    const auto reads = daemon.pageReads.value("account");
    QVERIFY(reads > before);
    QTest::qWait(500);
    QCOMPARE(daemon.pageReads.value("account"), reads);
    disconnect(watch);
    QCOMPARE(least, 9);
    QCOMPARE(room.list("albums").size(), 9);
  }
  // #800 first use: without a connected source the room is not connected,
  // so the hero offers Connect and no shelf shows.
  void roomWithoutAConnectedSourceIsTheFirstUse() {
    daemon.providers = {{"account", "Fixture account", "g1"}};
    auto account = state({}, "disconnected");
    account["actions"] = QJsonArray{"connect"};
    daemon.states["account"] = account;
    // Codex review of #812: before discovery knows any source, the room is
    // not settled, so it never claims to be the first use.
    MusicRoom unknown;
    QVERIFY(!unknown.settled());
    MusicCatalog catalog;
    MusicRoom room;
    room.setCatalog(&catalog);
    QTRY_COMPARE(room.sources().size(), 1);
    QTRY_COMPARE(room.sources().first().toMap().value("status").toString(), QStringLiteral("signin"));
    QTRY_VERIFY(room.settled());
    QVERIFY(!room.connected());
    QVERIFY(room.shelves().isEmpty());
    QCOMPARE(room.sources().first().toMap().value("actions").toStringList(), QStringList{"connect"});
  }
  void timedOutStartClearsErrorOnlyAfterItsNewSessionIsConfirmed() {
    daemon.providers={{"account","Fixture account","g1"}};
    auto initial=state({"tracks"},"connected");initial["session"]="old";initial["actions"]=QJsonArray{"radio"};
    daemon.states["account"]=initial;
    MusicCatalog catalog;QTRY_COMPARE(catalog.sources().size(),1);QTRY_COMPARE(catalog.state().value("session").toString(),QString("old"));
    daemon.commandError="Timeout";catalog.command("radio","station:fixture");
    QTRY_VERIFY(!catalog.errorText().isEmpty());QTRY_VERIFY(!catalog.busy());
    auto confirmed=initial;confirmed["playback"]="playing";
    auto active=[&](const QString &generation) {return QString::fromUtf8(QJsonDocument(QJsonObject{{"provider_id","account"},{"generation",generation},{"state",confirmed}}).toJson(QJsonDocument::Compact));};
    daemon.active=active("g1");catalog.refresh();QTest::qWait(50);QVERIFY(!catalog.errorText().isEmpty());
    confirmed["session"]="new";daemon.active=active("wrong-generation");catalog.refresh();QTest::qWait(50);QVERIFY(!catalog.errorText().isEmpty());
    daemon.active=active("g1");catalog.refresh();QTRY_VERIFY(catalog.errorText().isEmpty());
  }
  void registeredSourcesExposeMusicNavigationAndRecover() {
    MusicCatalog catalog;
    QQmlEngine engine;
    engine.rootContext()->setContextProperty("musicCatalog", &catalog);
    QFile main(QString::fromUtf8(ARINN_MAIN_QML));
    QFile home(QString::fromUtf8(ARINN_HOME_QML));
    QVERIFY(main.open(QIODevice::ReadOnly));
    QVERIFY(home.open(QIODevice::ReadOnly));
    // Evaluate the production entry-point bindings, rather than duplicating
    // their source-list condition in a test-only navigation model.
    const auto mainText = QString::fromUtf8(main.readAll());
    const auto homeText = QString::fromUtf8(home.readAll());
    const auto title = QRegularExpression(QStringLiteral(R"(\n\s*musicTitle: ([^\n]+))")).match(mainText);
    const auto sections = QRegularExpression(QStringLiteral(R"(readonly property var sections: ([^\n]+))")).match(homeText);
    const auto routes = QRegularExpression(QStringLiteral(R"(readonly property var routes: ([^\n]+))")).match(homeText);
    QVERIFY(title.hasMatch() && sections.hasMatch() && routes.hasMatch());
    QQmlComponent component(&engine);
    component.setData((QStringLiteral("import QtQml\nQtObject { id: root; property string musicTitle: ")
                       + title.captured(1) + QStringLiteral("\nreadonly property var sections: ")
                       + sections.captured(1) + QStringLiteral("\nreadonly property var routes: ")
                       + routes.captured(1) + QStringLiteral("\n}")).toUtf8(), QUrl());
    std::unique_ptr<QObject> navigation(component.create());
    QVERIFY2(navigation != nullptr, qPrintable(component.errorString()));
    auto navigationRoutes = [&] {
      return qvariant_cast<QJSValue>(navigation->property("routes")).toVariant().toStringList();
    };
    QVERIFY(!navigationRoutes().contains(QStringLiteral("music")));
    daemon.providers = {{"local", "Fixture music", "g1"}, {"account", "Fixture account", "g1"}};
    daemon.states.insert("local", state({"tracks"}));
    daemon.states.insert("account", state({}, "disconnected"));
    catalog.refresh();
    QTRY_COMPARE(catalog.sources().size(), 2);
    QTRY_VERIFY(navigationRoutes().contains(QStringLiteral("music")));
    QCOMPARE(navigationRoutes().indexOf(QStringLiteral("music")),
             navigationRoutes().indexOf(QStringLiteral("library")) + 1);
    const auto labels = qvariant_cast<QJSValue>(navigation->property("sections")).toVariant().toStringList();
    QCOMPARE(labels.at(navigationRoutes().indexOf(QStringLiteral("music"))), catalog.sectionTitle());

    // A source generation change must be discovered without restarting shell.
    daemon.providers[0].generation = QStringLiteral("g2");
    catalog.refresh();
    QTRY_COMPARE(catalog.sources().first().toMap().value("generation").toString(), QStringLiteral("g2"));
    QVERIFY(navigationRoutes().contains(QStringLiteral("music")));

    auto bus = QDBusConnection::sessionBus();
    QVERIFY(bus.unregisterService(QStringLiteral("org.arinn.Daemon1")));
    QTRY_VERIFY(catalog.sources().isEmpty());
    QTRY_VERIFY(!navigationRoutes().contains(QStringLiteral("music")));
    QVERIFY(bus.registerService(QStringLiteral("org.arinn.Daemon1")));
    QTRY_COMPARE(catalog.sources().size(), 2);
    QTRY_VERIFY(navigationRoutes().contains(QStringLiteral("music")));
    QTRY_COMPARE(catalog.state().value("account").toString(), QStringLiteral("local"));
  }
  // Review of #710: a source without facets (an account that is not
  // connected) was asked for a page, so "This action is not available"
  // stayed under its Connect prompt.
  void aSourceWithoutFacetsIsNotPaged() {
    daemon.providers = {{"account", "Account music", "g1"}};
    daemon.states.insert("account", state({}, "disconnected"));
    MusicCatalog catalog;
    QTRY_COMPARE(catalog.state().value("account").toString(), QStringLiteral("disconnected"));
    QTest::qWait(1500);
    QCOMPARE(daemon.pageReads.value("account"), 0);
    QCOMPARE(catalog.errorText(), QString());
    QVERIFY(catalog.entries().isEmpty());
  }
  // Review of #710: a transient state error stayed on screen after the
  // state read again, because only a page load or command cleared it, and a
  // source without facets loads no page.
  void aTransientStateErrorClears() {
    daemon.providers = {{"account", "Account music", "g1"}};
    daemon.stateErrors.insert("account", "Timeout");
    MusicCatalog catalog;
    QTRY_VERIFY(!catalog.errorText().isEmpty());
    daemon.stateErrors.clear();
    daemon.states.insert("account", state({}, "disconnected"));
    QTRY_COMPARE_WITH_TIMEOUT(catalog.errorText(), QString(), 3000);
  }
  // Review of #710: a selected source that disappeared stayed selected with
  // no endpoint, and the view showed an empty account prompt.
  void aDisappearedSourceFallsBackToAnAvailableOne() {
    daemon.providers = {{"first", "First music", "g1"}, {"second", "Second music", "g1"}};
    daemon.states.insert("first", state({"tracks"}));
    daemon.states.insert("second", state({"tracks"}));
    MusicCatalog catalog;
    QTRY_COMPARE(catalog.sourceId(), QStringLiteral("first"));
    catalog.choose(QStringLiteral("second"));
    QCOMPARE(catalog.sourceId(), QStringLiteral("second"));
    QSignalSpy resets(&catalog, &MusicCatalog::browseReset);
    daemon.providers = {{"first", "First music", "g1"}};
    QTRY_COMPARE_WITH_TIMEOUT(catalog.sourceId(), QStringLiteral("first"), 3000);
    QVERIFY(resets.count() >= 1);
    QTRY_COMPARE(catalog.state().value("account").toString(), QStringLiteral("local"));
  }
  void freshStationCanStartBeforeAnyTrackIsPlaying() {
    daemon.providers={{"local","Fixture station source","g1"}};
    auto initial=state({"selections"});initial["account"]="connected";initial["actions"]=QJsonArray{"play","radio"};
    daemon.states.insert("local",initial);MusicCatalog catalog;
    QTRY_COMPARE(catalog.state().value("account").toString(),QStringLiteral("connected"));
    const auto before=daemon.commands.size();catalog.command(QStringLiteral("radio"),QStringLiteral("station:wave"));
    QTRY_COMPARE(daemon.commands.size(),before+1);QCOMPARE(daemon.commands.last(),QStringLiteral("radio"));
  }
  void nativeMusicControlsTheProductionInterfaceSoundBinding() {
    daemon.providers={{"local","Fixture music","g1"}};
    daemon.states.insert("local",state({"tracks"}));
    MusicCatalog catalog;
    QQmlEngine engine;
    QQmlPropertyMap media, browser, player;
    media.insert("continueEntries",QVariantList{});browser.insert("tabs",QVariantList{});player.insert("playing",false);
    engine.rootContext()->setContextProperty("musicCatalog",&catalog);
    engine.rootContext()->setContextProperty("mediaCatalog",&media);
    engine.rootContext()->setContextProperty("browserCatalog",&browser);
    engine.rootContext()->setContextProperty("playerView",&player);
    engine.rootContext()->setContextProperty("playerOpen",false);
    QQmlComponent ambient(&engine);
    ambient.setData("import QtQml\nQtObject { function playing(media,browser,localPlayer) {return false;} }",QUrl());
    std::unique_ptr<QObject> idle(ambient.create());QVERIFY2(idle!=nullptr,qPrintable(ambient.errorString()));
    engine.rootContext()->setContextProperty("ambientIdle",idle.get());
    QFile main(QString::fromUtf8(ARINN_MAIN_QML));QVERIFY(main.open(QIODevice::ReadOnly));
    const QRegularExpression pattern(QStringLiteral(R"(readonly property bool interfaceMediaActive:\s*([\s\S]*?)\n\s*Binding\s*\{)"));
    const auto binding=pattern.match(QString::fromUtf8(main.readAll()));QVERIFY(binding.hasMatch());
    QQmlComponent component(&engine);
    component.setData(("import QtQml\nQtObject { readonly property bool mediaActive: "+binding.captured(1)+"\n}").toUtf8(),QUrl());
    std::unique_ptr<QObject> policy(component.create());QVERIFY2(policy!=nullptr,qPrintable(component.errorString()));
    QVERIFY(!policy->property("mediaActive").toBool());
    auto publish=[&](const QString &playback) {
      auto current=state({"tracks"});current["playback"]=playback;current["session"]="fixture-session";
      daemon.active=QString::fromUtf8(QJsonDocument(QJsonObject{{"provider_id","local"},{"generation","g1"},{"state",current}}).toJson(QJsonDocument::Compact));
      catalog.refresh();
    };
    publish("playing");QTRY_VERIFY(policy->property("mediaActive").toBool());
    publish("paused");QTRY_VERIFY(!policy->property("mediaActive").toBool());
    // A track loading is about to sound: no interface sound over its first
    // note (#756 M26).
    publish("loading");QTRY_VERIFY(policy->property("mediaActive").toBool());
    publish("paused");QTRY_VERIFY(!policy->property("mediaActive").toBool());
    publish("playing");QTRY_VERIFY(policy->property("mediaActive").toBool());
    daemon.active="null";catalog.refresh();QTRY_VERIFY(!policy->property("mediaActive").toBool());
  }
  // The Home card names the source of the active session.
  void theHomeCardNamesTheActiveSource() {
    daemon.providers = {{"local", "Local music", "g1"}};
    daemon.states.insert("local", state({"tracks"}));
    daemon.active = QString::fromUtf8(QJsonDocument(QJsonObject{
        {"provider_id", "local"},
        {"generation", "g1"},
        {"state", QJsonObject{{"revision", 1},
                              {"session", "s"},
                              {"playback", "playing"},
                              {"actions", QJsonArray{"pause"}},
                              {"current", QJsonObject{{"occurrence", "o"},
                                                      {"track", QJsonObject{{"id", "t1"},
                                                                            {"kind", "track"},
                                                                            {"title", "Synthetic"}}}}}}}})
                                          .toJson(QJsonDocument::Compact));
    MusicCatalog catalog;
    QTRY_COMPARE(catalog.homeEntries().size(), 1);
    QTRY_COMPARE(catalog.homeEntries().first().toMap().value("statusText").toString(),
                 QStringLiteral("Local music · Playing"));
    QVERIFY(catalog.homeEntries().first().toMap().value("live").toBool());
  }
  // A station tells a continuous station from an album on the Home card
  // (#756 M24).
  void theHomeCardNamesTheStation() {
    daemon.providers = {{"local", "Local music", "g1"}};
    daemon.states.insert("local", state({"tracks"}));
    daemon.active = QString::fromUtf8(QJsonDocument(QJsonObject{
        {"provider_id", "local"},
        {"generation", "g1"},
        {"state", QJsonObject{{"revision", 1},
                              {"session", "s"},
                              {"playback", "paused"},
                              {"continuous", true},
                              {"station", QJsonObject{{"id", "station:wave"},
                                                      {"kind", "station"},
                                                      {"title", "Synthetic wave"}}},
                              {"actions", QJsonArray{"resume"}},
                              {"current", QJsonObject{{"occurrence", "o"},
                                                      {"track", QJsonObject{{"id", "t1"},
                                                                            {"kind", "track"},
                                                                            {"title", "Synthetic"}}}}}}}})
                                          .toJson(QJsonDocument::Compact));
    MusicCatalog catalog;
    QTRY_COMPARE(catalog.homeEntries().value(0).toMap().value("statusText").toString(),
                 QStringLiteral("Local music · Synthetic wave · Paused"));
  }
  // The stale "Unavailable" under every music screen (#756 M5, M8): a
  // provider keeps its last error after it skipped to the next track. The
  // reason shows only while the session stays stopped and clears once it
  // plays again. The Quick menu reads the same session (#756 M2).
  void playbackProblemHasAReasonAndClearsOnRecovery() {
    daemon.providers = {{"local", "Local music", "g1"}};
    daemon.states.insert("local", state({"tracks"}));
    auto publish = [&](const QString &playback, const QString &error) {
      QJsonObject current{{"revision", 1}, {"session", "s"}, {"playback", playback},
                          {"actions", QJsonArray{playback == "playing" ? "pause" : "resume"}},
                          {"current", QJsonObject{{"occurrence", "o"},
                                                  {"track", QJsonObject{{"id", "t1"},
                                                                        {"title", "Synthetic"},
                                                                        {"artist", "Artist"}}}}}};
      if (!error.isEmpty())
        current["error"] = error;
      daemon.active = QString::fromUtf8(
          QJsonDocument(QJsonObject{{"provider_id", "local"}, {"generation", "g1"},
                                    {"state", current}})
              .toJson(QJsonDocument::Compact));
    };
    publish("stopped", "NotFound");
    MusicCatalog catalog;
    QTRY_COMPARE(catalog.playbackProblem(), QStringLiteral("The track was not found in this source."));
    QTRY_COMPARE(catalog.notice(),
                 QStringLiteral("Playback stopped. The track was not found in this source."));
    QVERIFY(catalog.nowPlaying().isEmpty());
    // An unknown code reads as a generic failure, never as the code.
    publish("stopped", "Internal");
    catalog.refresh();
    QTRY_COMPARE(catalog.playbackProblem(), QStringLiteral("The track could not be played."));
    // A queue that ended naturally after a skipped failure has no problem.
    publish("ended", "Unavailable");
    catalog.refresh();
    QTRY_VERIFY(catalog.playbackProblem().isEmpty());
    QVERIFY(catalog.notice().isEmpty());
    QSignalSpy changed(&catalog, &MusicCatalog::problemsChanged);
    publish("playing", "Unavailable");
    catalog.refresh();
    QTRY_COMPARE(catalog.nowPlaying().value("state").toString(), QStringLiteral("playing"));
    QVERIFY(catalog.playbackProblem().isEmpty());
    QVERIFY(catalog.notice().isEmpty());
    QVERIFY(changed.count() > 0);
    // The one-second poll alone does not announce an unchanged session.
    const auto announced = changed.count();
    catalog.refresh();
    QTest::qWait(300);
    QCOMPARE(changed.count(), announced);
    const auto now = catalog.nowPlaying();
    QCOMPARE(now.value("id").toString(), MusicCatalog::sessionEntryId());
    QCOMPARE(now.value("title").toString(), QStringLiteral("Synthetic"));
    QCOMPARE(now.value("subtitle").toString(), QStringLiteral("Artist"));
    QCOMPARE(now.value("state").toString(), QStringLiteral("playing"));
    QVERIFY(now.value("playPause").toBool());
    // Play/Pause from the sheet and Home's card pauses a playing session.
    QTRY_VERIFY(!catalog.sources().isEmpty());
    const auto before = daemon.commands.size();
    catalog.togglePlayback();
    QTRY_COMPARE(daemon.commands.size(), before + 1);
    QCOMPARE(daemon.commands.last(), QStringLiteral("pause"));
    publish("loading", QString());
    catalog.refresh();
    QTRY_COMPARE(catalog.activeState().value("playback").toString(), QStringLiteral("loading"));
    QVERIFY(catalog.nowPlaying().isEmpty());
  }
  // A failed page read is told apart from a failed command: the view ends
  // its search for a detail's opener only on the former (review of #798).
  void aFailedPageIsSignalled() {
    daemon.providers = {{"local", "Local music", "g1"}};
    auto scannable = state({"tracks"});
    scannable["actions"] = QJsonArray{"scan"};
    daemon.states.insert("local", scannable);
    MusicCatalog catalog;
    QTRY_VERIFY(!catalog.entries().isEmpty());
    QSignalSpy failed(&catalog, &MusicCatalog::pageFailed);
    daemon.commandError = "Unsupported";
    const auto sent = daemon.commands.size();
    catalog.command(QStringLiteral("scan"));
    QTRY_COMPARE(daemon.commands.size(), sent + 1);
    QTRY_VERIFY(!catalog.errorText().isEmpty());
    QCOMPARE(failed.count(), 0);
    daemon.commandError.clear();
    QTRY_VERIFY(!catalog.busy());
    daemon.pageError = "Timeout";
    catalog.browse(QStringLiteral("tracks"), QString(), QStringLiteral("other"));
    QTRY_COMPARE(failed.count(), 1);
    QVERIFY(!catalog.errorText().isEmpty());
  }
  // A degraded source says that part of it is unavailable, with no code.
  void aDegradedSourceExplainsItself() {
    daemon.providers = {{"local", "Local music", "g1"}};
    auto degraded = state({"tracks"});
    degraded["backend"] = "degraded";
    degraded["error"] = "Unavailable";
    daemon.states.insert("local", degraded);
    MusicCatalog catalog;
    QTRY_COMPARE(catalog.notice(),
                 QStringLiteral("Part of this source is unavailable. Refresh to try again."));
  }
  // #804: a stored station the provider answers `radio` with NotFound or
  // Unavailable for is unresolved, so Home marks its pin unavailable;
  // starting it again, or a restart of the provider, clears that.
  void aStationTheProviderNoLongerKnowsIsUnresolved() {
    daemon.providers = {{"local", "Local music", "g1"}};
    auto local = state({"selections"});
    local["actions"] = QJsonArray{"radio"};
    daemon.states.insert("local", local);
    MusicCatalog catalog;
    QTRY_COMPARE(catalog.state().value("actions").toList().size(), 1);
    QSignalSpy changed(&catalog, &MusicCatalog::unresolvedChanged);
    QVERIFY(catalog.stationResolved("local", "station:gone"));
    daemon.commandError = "NotFound";
    catalog.command("radio", "station:gone");
    QTRY_VERIFY(!catalog.stationResolved("local", "station:gone"));
    QCOMPARE(catalog.unresolvedStations(),
             QVariantList{QVariant(QStringList{"local", "station:gone"})});
    QCOMPARE(changed.count(), 1);
    QVERIFY(catalog.stationResolved("local", "station:other"));
    // Another failure is not "no longer known".
    daemon.commandError = "Timeout";
    catalog.command("radio", "station:slow");
    QTRY_VERIFY(!catalog.errorText().isEmpty());
    QTRY_VERIFY(!catalog.busy());
    QVERIFY(catalog.stationResolved("local", "station:slow"));
    daemon.commandError.clear();
    catalog.command("radio", "station:gone");
    QTRY_VERIFY(catalog.stationResolved("local", "station:gone"));
    QTRY_VERIFY(!catalog.busy());
    // A provider that cannot tell answers Unavailable (music.md).
    daemon.commandError = "Unavailable";
    catalog.command("radio", "station:unknown");
    QTRY_VERIFY(!catalog.stationResolved("local", "station:unknown"));
    QTRY_VERIFY(!catalog.busy());
    // The same generation keeps it; a restarted provider is asked again.
    catalog.refresh();
    QTest::qWait(50);
    QVERIFY(!catalog.stationResolved("local", "station:unknown"));
    daemon.providers[0].generation = QStringLiteral("g2");
    catalog.refresh();
    QTRY_VERIFY(catalog.stationResolved("local", "station:unknown"));
    QVERIFY(catalog.unresolvedStations().isEmpty());
    daemon.commandError.clear();
  }
  // #804: Home marks a pin unavailable, with its reason, while its source
  // or its account is not connected, or the source no longer knows the
  // station; Main.musicPinRows.
  void pinsOfAbsentSourcesOrUnknownStationsAreUnavailable() {
    QFile main(QString::fromUtf8(ARINN_MAIN_QML));
    QVERIFY(main.open(QIODevice::ReadOnly));
    const auto text = QString::fromUtf8(main.readAll());
    auto extract = [&](const QString &name) {
      const auto start = text.indexOf(QStringLiteral("    function ") + name + '(');
      const auto end = text.indexOf(QStringLiteral("\n    }\n"), start);
      return start >= 0 && end > start ? text.mid(start, end + 6 - start) : QString();
    };
    const auto rowsFunction = extract(QStringLiteral("musicPinRows"));
    const auto followsFunction = extract(QStringLiteral("musicRoomFollows"));
    const auto pairsFunction = extract(QStringLiteral("pinnedMusicStations"));
    QVERIFY(!pairsFunction.isEmpty());
    QVERIFY(!rowsFunction.isEmpty());
    QVERIFY(!followsFunction.isEmpty());
    QQmlEngine engine;
    QQmlComponent component(&engine);
    component.setData(("import QtQml\nQtObject {\n" + rowsFunction + followsFunction + pairsFunction +
                       "    function rows(f, s, u, a) {\n"
                       "        return musicPinRows(f, s, u, a, (p, id) => \"image://fixture/\" + p + \"/\" + id);\n"
                       "    }\n}")
                          .toUtf8(),
                      QUrl());
    std::unique_ptr<QObject> root(component.create());
    QVERIFY2(root != nullptr, qPrintable(component.errorString()));
    auto pin = [](const QString &provider, const QString &station) {
      return QVariantMap{{"id", provider + station}, {"musicStation", true},
                         {"providerId", provider}, {"stationId", station},
                         {"launchable", true}, {"title", "Fixture"}};
    };
    auto withArtwork = pin("local", "station:library");
    withArtwork.insert("artworkId", "cover:library");
    auto goneWithArtwork = pin("away", "station:x");
    goneWithArtwork.insert("artworkId", "cover:x");
    const QVariantList favorites{QVariantMap{{"id", "app"}, {"title", "App"}},
                                 withArtwork,
                                 goneWithArtwork, pin("local", "station:gone"),
                                 pin("cloud", "station:wave"), pin("down", "station:wave"),
                                 pin("local", "station:a\nb")};
    const QVariantList sources{
        QVariantMap{{"id", "local"}, {"name", "Local music"}, {"ready", true}},
        QVariantMap{{"id", "local\nstation:a"}, {"name", "Other"}, {"ready", true}},
        QVariantMap{{"id", "cloud"}, {"name", "Cloud"}, {"ready", true}},
        QVariantMap{{"id", "down"}, {"name", "Down"}, {"ready", true}}};
    // The music room's rows: the cloud account expired, the other service
    // stopped answering, while both provider processes run.
    const QVariantList accounts{QVariantMap{{"id", "local"}, {"status", "connected"}},
                                QVariantMap{{"id", "cloud"}, {"status", "signin"}},
                                QVariantMap{{"id", "down"}, {"status", "unavailable"}}};
    // IDs may hold a newline: an unresolved station of another provider
    // never matches a pin by a joined key.
    const QVariantList unresolved{QVariant(QStringList{"local", "station:gone"}),
                                  QVariant(QStringList{"local\nstation:a", "b"})};
    QVariant result;
    QVERIFY(QMetaObject::invokeMethod(root.get(), "rows", Q_RETURN_ARG(QVariant, result),
                                      Q_ARG(QVariant, favorites), Q_ARG(QVariant, sources),
                                      Q_ARG(QVariant, unresolved),
                                      Q_ARG(QVariant, accounts)));
    const auto rows = qvariant_cast<QJSValue>(result).toVariant().toList();
    QCOMPARE(rows.size(), 7);
    QVERIFY(!rows[6].toMap().value("unavailable").toBool());
    QCOMPARE(rows[0].toMap().value("title").toString(), QStringLiteral("App"));
    QVERIFY(!rows[1].toMap().value("unavailable").toBool());
    QCOMPARE(rows[1].toMap().value("subtitle").toString(), QStringLiteral("Local music"));
    // The stored artwork ID shows through the source's image route.
    QCOMPARE(rows[1].toMap().value("artwork").toString(),
             QStringLiteral("image://fixture/local/cover:library"));
    QVERIFY(rows[2].toMap().value("artwork").toString().isEmpty());
    QVERIFY(rows[2].toMap().value("unavailable").toBool());
    QVERIFY(!rows[2].toMap().value("launchable").toBool());
    QCOMPARE(rows[2].toMap().value("subtitle").toString(),
             QStringLiteral("Music source not connected"));
    QCOMPARE(rows[3].toMap().value("subtitle").toString(),
             QStringLiteral("This station is no longer available"));
    for (int i : {4, 5}) {
      QVERIFY(rows[i].toMap().value("unavailable").toBool());
      QCOMPARE(rows[i].toMap().value("subtitle").toString(),
               QStringLiteral("Music source not connected"));
    }
    // The room keeps the account rows live while Home shows a pin, not only
    // on the music screen.
    auto follows = [&](bool music, bool home, const QVariantList &shelf) {
      QVariant value;
      [&] {
        QVERIFY(QMetaObject::invokeMethod(root.get(), "musicRoomFollows",
                                          Q_RETURN_ARG(QVariant, value), Q_ARG(QVariant, music),
                                          Q_ARG(QVariant, home), Q_ARG(QVariant, shelf)));
      }();
      return value.toBool();
    };
    // Home's pins come from the stored targets, also for a placeholder row
    // whose MusicPins1 record is missing (Codex review of #836).
    const QVariantList stored{
        QVariantMap{{"id", "app"}, {"providerId", "apps"}, {"capability", "application"},
                    {"localId", "a"}},
        QVariantMap{{"id", "p1"}, {"providerId", "local"}, {"capability", "music-station"},
                    {"localId", "station:library"}, {"musicStation", true}},
        QVariantMap{{"id", "p2"}, {"providerId", "cloud"}, {"capability", "music-station"},
                    {"localId", "station:wave"}, {"unavailable", true}}};
    QVariant pairs;
    QVERIFY(QMetaObject::invokeMethod(root.get(), "pinnedMusicStations",
                                      Q_RETURN_ARG(QVariant, pairs), Q_ARG(QVariant, stored)));
    QCOMPARE(qvariant_cast<QJSValue>(pairs).toVariant().toList(),
             (QVariantList{QVariant(QVariantList{"local", "station:library"}),
                           QVariant(QVariantList{"cloud", "station:wave"})}));
    const QVariantList noPins{favorites.first()};
    QVERIFY(follows(true, false, noPins));
    QVERIFY(follows(false, true, favorites));
    QVERIFY(!follows(false, true, noPins));
    QVERIFY(!follows(false, false, favorites));
  }
  // #804: a pin's stored artwork ID becomes an image URL through the
  // source's current endpoint and generation, and none while it is absent.
  void pinArtworkFollowsTheSourceEndpoint() {
    daemon.providers = {{"local", "Local music", "g1"}};
    daemon.states.insert("local", state({"tracks"}));
    MusicCatalog catalog;
    QTRY_VERIFY(!catalog.sources().isEmpty());
    const auto path = catalog.sources().first().toMap().value("path").toString();
    QCOMPARE(catalog.artworkUrl("local", "cover:wave"),
             AssetImages::imageUrl("local", path, "g1", "cover:wave"));
    QVERIFY(catalog.artworkUrl("local", "cover:wave").startsWith("image://arinn-assets/"));
    QVERIFY(catalog.artworkUrl("away", "cover:wave").isEmpty());
    QVERIFY(catalog.artworkUrl("local", QString()).isEmpty());
  }
  // #804: Arinn's pin is mirrored to the provider's `pin` only for the
  // station that plays, and only when the provider's state differs.
  void aPinIsMirroredToTheProviderForThePlayingStation() {
    daemon.providers = {{"local", "Local music", "g1"}};
    daemon.states.insert("local", state({"tracks"}));
    QJsonObject current{{"revision", 1}, {"session", "s"}, {"playback", "playing"},
                        {"continuous", true}, {"pinned", false},
                        {"station", QJsonObject{{"id", "station:wave"}, {"kind", "station"},
                                                {"title", "Wave"}}},
                        {"actions", QJsonArray{"pause", "pin"}}};
    daemon.active = QString::fromUtf8(
        QJsonDocument(QJsonObject{{"provider_id", "local"}, {"generation", "g1"},
                                  {"state", current}})
            .toJson(QJsonDocument::Compact));
    MusicCatalog catalog;
    QTRY_COMPARE(catalog.activeState().value("session").toString(), QStringLiteral("s"));
    QTRY_VERIFY(!catalog.sources().isEmpty());
    const auto before = daemon.commands.size();
    catalog.syncProviderPin("local", "station:other", true);
    catalog.syncProviderPin("other", "station:wave", true);
    catalog.syncProviderPin("local", "station:wave", false);
    QTest::qWait(100);
    QCOMPARE(daemon.commands.size(), before);
    // Codex review of #836: a mirror that meets another command in flight
    // waits for it instead of being dropped. The fake answers on the next
    // turn of the event loop, so the pin is asked while Pause is in flight.
    catalog.transport("pause");
    QVERIFY(catalog.transportBusy());
    catalog.syncProviderPin("local", "station:wave", true);
    // Another station's change from the same read keeps it waiting.
    catalog.syncProviderPin("local", "station:other", false);
    QCOMPARE(daemon.commands.size(), before + 1);
    QTRY_COMPARE(daemon.commands.size(), before + 2);
    QCOMPARE(daemon.commands.last(), QStringLiteral("pin"));
    QTest::qWait(100);
    QCOMPARE(daemon.commands.size(), before + 2);
  }
};
QTEST_GUILESS_MAIN(MusicCatalogTest)
#include "music_catalog_test.moc"
