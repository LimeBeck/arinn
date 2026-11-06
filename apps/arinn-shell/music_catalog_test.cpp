// SPDX-FileCopyrightText: 2026 Arinn contributors
//
// SPDX-License-Identifier: GPL-3.0-or-later

// MusicCatalog against a scripted daemon on the session bus (ADR-0134).
// Synthetic providers and tracks only.
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
      const auto own = pages.find(id + QLatin1Char('/') + facet);
      QJsonObject page{{"generation", message.arguments().value(0).toString()},
                       {"revision", 1},
                       {"entries", own != pages.end() ? *own
                                   : QJsonArray{QJsonObject{{"id", "t1"},
                                                            {"kind", "track"},
                                                            {"title", "Synthetic"},
                                                            {"available", true}}}},
                       {"next_token", ""}};
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
  // #800 first use: without a connected source the room is not connected,
  // so the hero offers Connect and no shelf shows.
  void roomWithoutAConnectedSourceIsTheFirstUse() {
    daemon.providers = {{"account", "Fixture account", "g1"}};
    auto account = state({}, "disconnected");
    account["actions"] = QJsonArray{"connect"};
    daemon.states["account"] = account;
    MusicCatalog catalog;
    MusicRoom room;
    room.setCatalog(&catalog);
    QTRY_COMPARE(room.sources().size(), 1);
    QTRY_COMPARE(room.sources().first().toMap().value("status").toString(), QStringLiteral("signin"));
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
};
QTEST_GUILESS_MAIN(MusicCatalogTest)
#include "music_catalog_test.moc"
