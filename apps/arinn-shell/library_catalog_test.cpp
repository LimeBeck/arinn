// SPDX-FileCopyrightText: 2026 Arinn contributors
//
// SPDX-License-Identifier: GPL-3.0-or-later

#include "asset_images.h"
#include "library_catalog.h"
#include <QGuiApplication>
#include <QQmlApplicationEngine>
#include <QQmlContext>
#include <QQuickItem>
#include <QQuickWindow>
#include <QSignalSpy>
#include <QTest>
#include <QUuid>
#include <QtDBus>
namespace {
QString presentationId(const QString &provider, const QString &local) {
  return QUuid::createUuidV5(QUuid(), provider + QLatin1Char('\n') + local)
      .toString(QUuid::WithoutBraces);
}
struct CollectionWire {
  QString id;
  QString title;
  QString kind;
};
struct ItemWire {
  QString id;
  QString collectionId;
  QString parentId;
  QString title;
  QString kind;
  bool hasYear = false;
  int year = 0;
  bool hasSeason = false;
  quint32 season = 0;
  bool hasEpisode = false;
  quint32 episode = 0;
  qlonglong duration = -1;
  QString poster;
  QString fanart;
  bool available = true;
};
struct ResumeWire {
  QString contentId;
  qlonglong position = 0;
  qlonglong duration = -1;
  bool completed = false;
  qulonglong observed = 1;
};
struct ProviderWire {
  QString id = QStringLiteral("library-provider");
  QString plugin = QStringLiteral("plugin");
  QString source;
  QString name = QStringLiteral("library");
  QString version = QStringLiteral("1");
  QDBusObjectPath path{QStringLiteral("/org/arinn/Daemon1/providers/p_library")};
  QStringList capabilities{QStringLiteral("library")};
  QString runtime = QStringLiteral("ready");
  QString backend = QStringLiteral("available");
  QString generation = QStringLiteral("g-library");
  qulonglong revision = 1;
  QString error;
  qulonglong errorAt = 0;
  qulonglong recovered = 0;
  qulonglong restarts = 0;
  qulonglong cumulative = 0;
};
struct ShortcutWire {
  QString id, title, url, icon;
};
struct EntityWire {
  QString provider, capability, local;
};
struct ContentWire {
  QString provider, capability, local;
};
struct SessionWire {
  QString id, title, subtitle, artwork, state;
  qlonglong duration = -1;
  qlonglong position = 0;
  qulonglong anchor = 0;
  QStringList actions;
  bool hasContent = false;
  ContentWire content;
};
QDBusArgument &operator<<(QDBusArgument &argument, const CollectionWire &row) {
  argument.beginStructure();
  argument << row.id << row.title << row.kind;
  argument.endStructure();
  return argument;
}
const QDBusArgument &operator>>(const QDBusArgument &argument, CollectionWire &) {
  return argument;
}
QDBusArgument &operator<<(QDBusArgument &argument, const ItemWire &row) {
  argument.beginStructure();
  argument << row.id << row.collectionId << row.parentId << row.title << row.kind
           << row.hasYear << row.year << row.hasSeason << row.season
           << row.hasEpisode << row.episode << row.duration << row.poster
           << row.fanart << row.available;
  argument.endStructure();
  return argument;
}
const QDBusArgument &operator>>(const QDBusArgument &argument, ItemWire &) {
  return argument;
}
QDBusArgument &operator<<(QDBusArgument &argument, const ResumeWire &row) {
  argument.beginStructure();
  argument << row.contentId << row.position << row.duration << row.completed
           << row.observed;
  argument.endStructure();
  return argument;
}
const QDBusArgument &operator>>(const QDBusArgument &argument, ResumeWire &) {
  return argument;
}
QDBusArgument &operator<<(QDBusArgument &argument, const ProviderWire &provider) {
  argument.beginStructure();
  argument << provider.id << provider.plugin << provider.source << provider.name
           << provider.version << provider.path << provider.capabilities
           << provider.runtime << provider.backend << provider.generation
           << provider.revision << provider.error << provider.errorAt
           << provider.recovered << provider.restarts << provider.cumulative;
  argument.endStructure();
  return argument;
}
const QDBusArgument &operator>>(const QDBusArgument &argument, ProviderWire &) {
  return argument;
}
QDBusArgument &operator<<(QDBusArgument &argument, const ShortcutWire &row) {
  argument.beginStructure();
  argument << row.id << row.title << row.url << row.icon;
  argument.endStructure();
  return argument;
}
const QDBusArgument &operator>>(const QDBusArgument &argument, ShortcutWire &) {
  return argument;
}
QDBusArgument &operator<<(QDBusArgument &argument, const EntityWire &entity) {
  argument.beginStructure();
  argument << entity.provider << entity.capability << entity.local;
  argument.endStructure();
  return argument;
}
const QDBusArgument &operator>>(const QDBusArgument &argument, EntityWire &) {
  return argument;
}
QDBusArgument &operator<<(QDBusArgument &argument, const ContentWire &content) {
  argument.beginStructure();
  argument << content.provider << content.capability << content.local;
  argument.endStructure();
  return argument;
}
const QDBusArgument &operator>>(const QDBusArgument &argument, ContentWire &) {
  return argument;
}
QDBusArgument &operator<<(QDBusArgument &argument, const SessionWire &session) {
  argument.beginStructure();
  argument << session.id << session.title << session.subtitle << session.artwork
           << session.state << session.duration << session.position
           << session.anchor << session.actions << session.hasContent
           << session.content;
  argument.endStructure();
  return argument;
}
const QDBusArgument &operator>>(const QDBusArgument &argument, SessionWire &) {
  return argument;
}
class LibraryDaemon : public QDBusVirtualObject {
public:
  QList<ProviderWire> providers{ProviderWire{}};
  QList<CollectionWire> collections;
  QList<ItemWire> page1;
  QList<ItemWire> page2;
  QString page2Token = QStringLiteral("p2");
  QString pageGeneration = QStringLiteral("g-library");
  qulonglong page1Revision = 1;
  qulonglong page2Revision = 1;
  bool loopSecond = false;
  bool rejectLibrary = false;
  QStringList features{QStringLiteral("LibraryResume1")};
  QList<ResumeWire> resumes;
  bool rejectResume = false;
  bool rejectLaunch = false;
  bool holdOperation = false;
  QString operationState = QStringLiteral("succeeded");
  QString operationError;
  QList<EntityWire> operationEntities;
  QList<SessionWire> sessions;
  int libraryReads = 0;
  int resumeReads = 0;
  int featureReads = 0;
  int operationReads = 0;
  QStringList commands;
  QString lastGeneration, lastLocal, lastRequest;
  QString introspect(const QString &) const override { return {}; }
  bool handleMessage(const QDBusMessage &message,
                     const QDBusConnection &bus) override {
    QDBusArgument body;
    const auto libraryPath =
        QStringLiteral("/org/arinn/Daemon1/providers/p_library");
    if (message.member() == QLatin1String("Status")) {
      body.beginStructure();
      body << QStringLiteral("epoch") << true << QStringLiteral("generation")
           << qulonglong(1) << true << QString() << QString() << QString()
           << false << true << QString() << QString();
      body.endStructure();
    } else if (message.member() == QLatin1String("Providers")) {
      body.beginStructure();
      body << QStringLiteral("snapshot");
      body.beginArray(QMetaType::fromType<ProviderWire>());
      for (const auto &provider : providers)
        body << provider;
      body.endArray();
      body << QString();
      body.endStructure();
    } else if (message.member() == QLatin1String("List")) {
      body.beginStructure();
      body << qulonglong(1);
      body.beginArray(QMetaType::fromType<ShortcutWire>());
      body.endArray();
      body.endStructure();
    } else if (message.member() == QLatin1String("GetSessions")) {
      body.beginStructure();
      body << QStringLiteral("g-media") << qulonglong(1);
      body.beginArray(QMetaType::fromType<SessionWire>());
      for (const auto &row : sessions)
        body << row;
      body.endArray();
      body << QString();
      body.endStructure();
    } else if (message.member() == QLatin1String("GetLibrary")) {
      ++libraryReads;
      if (message.path() != libraryPath || rejectLibrary) {
        bus.send(message.createErrorReply(
            QStringLiteral("org.arinn.Error1.Unavailable"),
            QStringLiteral("Unavailable")));
        return true;
      }
      const auto token = message.arguments().value(0).toString();
      QList<ItemWire> rows;
      QString next;
      if (token.isEmpty()) {
        rows = page1;
        if (!page2.isEmpty() || loopSecond)
          next = page2Token;
      } else if (token == page2Token) {
        rows = page2;
        if (loopSecond)
          next = page2Token;
      } else {
        bus.send(message.createErrorReply(
            QStringLiteral("org.arinn.Error1.InvalidArgument"),
            QStringLiteral("InvalidArgument")));
        return true;
      }
      body.beginStructure();
      body << pageGeneration
           << (token.isEmpty() ? page1Revision : page2Revision);
      body.beginArray(QMetaType::fromType<CollectionWire>());
      if (token.isEmpty()) {
        for (const auto &row : collections)
          body << row;
      }
      body.endArray();
      body.beginArray(QMetaType::fromType<ItemWire>());
      for (const auto &row : rows)
        body << row;
      body.endArray();
      body << next;
      body.endStructure();
    } else if (message.member() == QLatin1String("Features")) {
      ++featureReads;
      bus.send(message.createReply(QVariant::fromValue(features)));
      return true;
    } else if (message.member() == QLatin1String("GetResumes")) {
      ++resumeReads;
      if (rejectResume) {
        bus.send(message.createErrorReply(
            QStringLiteral("org.arinn.Error1.Unsupported"),
            QStringLiteral("Unsupported")));
        return true;
      }
      body.beginStructure();
      body << pageGeneration << qulonglong(7);
      body.beginArray(QMetaType::fromType<ResumeWire>());
      for (const auto &row : resumes)
        body << row;
      body.endArray();
      body << QString();
      body.endStructure();
    } else if (message.member() == QLatin1String("Launch")) {
      commands.append(message.member());
      lastGeneration = message.arguments().value(0).toString();
      lastLocal = message.arguments().value(1).toString();
      lastRequest = message.arguments().value(2).toString();
      if (rejectLaunch) {
        bus.send(message.createErrorReply(
            QStringLiteral("org.arinn.Error1.Unavailable"),
            QStringLiteral("Unavailable")));
        return true;
      }
      bus.send(message.createReply(QVariant::fromValue(
          QDBusObjectPath(QStringLiteral("/org/arinn/Daemon1/ops/library")))));
      return true;
    } else if (message.member() == QLatin1String("Get") &&
               message.interface() == QLatin1String("org.arinn.Operation1")) {
      ++operationReads;
      body.beginStructure();
      body << QString(holdOperation ? QStringLiteral("running") : operationState)
           << operationError << false << qulonglong(1) << qulonglong(2);
      body.beginArray(QMetaType::fromType<EntityWire>());
      if (!holdOperation) {
        for (const auto &entity : operationEntities)
          body << entity;
      }
      body.endArray();
      body << false;
      body.endStructure();
    } else {
      bus.send(message.createErrorReply(QDBusError::NotSupported,
                                        QStringLiteral("fixture")));
      return true;
    }
    bus.send(message.createReply(QVariant::fromValue(body)));
    return true;
  }
};
class ShellStandIn : public QObject {
  Q_OBJECT
  Q_PROPERTY(bool homeVisible MEMBER m_home CONSTANT)
  Q_PROPERTY(QString returnWindow MEMBER m_return CONSTANT)
  Q_PROPERTY(QVariantList items MEMBER m_items CONSTANT)
  Q_PROPERTY(QVariantList continueEntries MEMBER m_continue CONSTANT)
  Q_PROPERTY(QVariantList applications MEMBER m_applications CONSTANT)
  Q_PROPERTY(QVariantList favorites MEMBER m_favorites CONSTANT)
  Q_PROPERTY(QString homeState MEMBER m_homeState CONSTANT)
  Q_PROPERTY(QStringList busyEntryIds MEMBER m_busy CONSTANT)
  Q_PROPERTY(QString message MEMBER m_message CONSTANT)
public:
  bool m_home = true;
  QString m_return;
  QString m_homeState = QStringLiteral("ready");
  QString m_message;
  QVariantList m_items, m_continue, m_applications, m_favorites;
  QStringList m_busy;
  Q_INVOKABLE void back() {}
  Q_INVOKABLE void launch(const QString &) {}
  Q_INVOKABLE void activate(const QString &) {}
  Q_INVOKABLE void refresh() {}
};
class SessionGuardStandIn : public QObject {
  Q_OBJECT
  Q_PROPERTY(bool blocked MEMBER m_blocked CONSTANT)
public:
  bool m_blocked = false;
};
bool focusInside(QQuickItem *item, QQuickItem *root) {
  for (auto cursor = item; cursor; cursor = cursor->parentItem())
    if (cursor == root)
      return true;
  return false;
}
QQuickItem *findItem(QQuickItem *root, const QString &name) {
  if (!root)
    return nullptr;
  if (root->objectName() == name)
    return root;
  const auto children = root->childItems();
  for (auto *child : children) {
    if (auto *found = findItem(child, name))
      return found;
  }
  return nullptr;
}
ItemWire film(const QString &id, const QString &title, bool available = true) {
  ItemWire row;
  row.id = id;
  row.collectionId = QStringLiteral("films");
  row.title = title;
  row.kind = QStringLiteral("film");
  row.hasYear = true;
  row.year = 1999;
  row.duration = 100000000;
  row.poster = QStringLiteral("asset-not-fetched");
  row.fanart = QStringLiteral("fanart-not-fetched");
  row.available = available;
  return row;
}
ItemWire showItem() {
  auto show = film(QStringLiteral("show"), QStringLiteral("Serial"));
  show.collectionId = QStringLiteral("shows");
  show.kind = QStringLiteral("series");
  return show;
}
ItemWire episodeItem(const QString &id, const QString &title, quint32 season,
                     quint32 number, bool available = true) {
  ItemWire row;
  row.id = id;
  row.collectionId = QStringLiteral("shows");
  row.parentId = QStringLiteral("show");
  row.title = title;
  row.kind = QStringLiteral("episode");
  row.hasSeason = true;
  row.season = season;
  row.hasEpisode = true;
  row.episode = number;
  row.duration = 100000000;
  row.available = available;
  return row;
}
ResumeWire resumeOn(const QString &id, qlonglong position, qlonglong duration,
                    bool completed, qulonglong observed) {
  ResumeWire resume;
  resume.contentId = id;
  resume.position = position;
  resume.duration = duration;
  resume.completed = completed;
  resume.observed = observed;
  return resume;
}
QString episodePresentation(const QVariantMap &entry, int number) {
  const auto episodes = entry.value(QStringLiteral("episodes")).toList();
  for (const auto &value : episodes) {
    const auto episode = value.toMap();
    if (episode.value(QStringLiteral("number")).toInt() == number)
      return episode.value(QStringLiteral("id")).toString();
  }
  return {};
}
bool registerBus(LibraryDaemon &daemon) {
  auto bus = QDBusConnection::sessionBus();
  return bus.registerService(QStringLiteral("org.arinn.Daemon1")) &&
         bus.registerVirtualObject(QStringLiteral("/org/arinn"), &daemon,
                                   QDBusConnection::SubPath);
}
} // namespace

class LibraryCatalogTest : public QObject {
  Q_OBJECT
private slots:
  void initTestCase() {
    qDBusRegisterMetaType<CollectionWire>();
    qDBusRegisterMetaType<ItemWire>();
    qDBusRegisterMetaType<ResumeWire>();
    qDBusRegisterMetaType<ProviderWire>();
    qDBusRegisterMetaType<ShortcutWire>();
    qDBusRegisterMetaType<EntityWire>();
    qDBusRegisterMetaType<ContentWire>();
    qDBusRegisterMetaType<SessionWire>();
  }
  void cleanup() {
    auto bus = QDBusConnection::sessionBus();
    bus.unregisterObject(QStringLiteral("/org/arinn"),
                         QDBusConnection::UnregisterTree);
    bus.unregisterService(QStringLiteral("org.arinn.Daemon1"));
  }
  void catalogueSeriesMissingAndResume() {
    LibraryDaemon daemon;
    daemon.collections = {{QStringLiteral("films"), QStringLiteral("Films"),
                           QStringLiteral("film")},
                          {QStringLiteral("shows"), QStringLiteral("Series"),
                           QStringLiteral("series")}};
    auto show = film(QStringLiteral("show"), QStringLiteral("Serial"));
    show.collectionId = QStringLiteral("shows");
    show.kind = QStringLiteral("series");
    show.hasYear = true;
    show.year = 2011;
    show.available = true;
    ItemWire episode;
    episode.id = QStringLiteral("ep1");
    episode.collectionId = QStringLiteral("shows");
    episode.parentId = QStringLiteral("show");
    episode.title = QStringLiteral("Pilot");
    episode.kind = QStringLiteral("episode");
    episode.hasSeason = true;
    episode.season = 1;
    episode.hasEpisode = true;
    episode.episode = 1;
    episode.duration = 2400000000;
    episode.available = true;
    ItemWire missingEpisode = episode;
    missingEpisode.id = QStringLiteral("ep2");
    missingEpisode.title = QStringLiteral("Next");
    missingEpisode.season = 2;
    missingEpisode.available = false;
    missingEpisode.duration = 2460000000;
    daemon.page1 = {film(QStringLiteral("north"), QStringLiteral("North")),
                    film(QStringLiteral("gone"), QStringLiteral("Gone"), false),
                    show, episode, missingEpisode};
    ResumeWire resume;
    resume.contentId = QStringLiteral("north");
    resume.position = 50000000;
    resume.duration = 100000000;
    daemon.resumes = {resume};
    QVERIFY(registerBus(daemon));
    LibraryCatalog catalog;
    QTRY_COMPARE(catalog.libraryState(), QStringLiteral("ready"));
    QCOMPARE(daemon.libraryReads, 1);
    QVERIFY(daemon.resumeReads >= 1);
    QCOMPARE(catalog.tabs().size(), 2);
    QCOMPARE(catalog.tabs().at(0).toMap().value(QStringLiteral("title")).toString(),
             QStringLiteral("Films"));
    QCOMPARE(catalog.entries().size(), 3);
    const auto north = catalog.entries().at(0).toMap();
    QCOMPARE(north.value(QStringLiteral("title")).toString(),
             QStringLiteral("North"));
    QCOMPARE(north.value(QStringLiteral("noArt")).toBool(), false);
    QVERIFY(north.value(QStringLiteral("artwork"))
                .toString()
                .startsWith(QStringLiteral("image://arinn-assets/")));
    QVERIFY(north.value(QStringLiteral("fanart"))
                .toString()
                .startsWith(QStringLiteral("image://arinn-assets/")));
    QCOMPARE(north.value(QStringLiteral("missing")).toBool(), false);
    QCOMPARE(north.value(QStringLiteral("year")).toInt(), 1999);
    QCOMPARE(north.value(QStringLiteral("progress")).toDouble(), 0.5);
    QCOMPARE(north.value(QStringLiteral("tabId")).toString(),
             catalog.tabs().at(0).toMap().value(QStringLiteral("id")).toString());
    const auto gone = catalog.entries().at(1).toMap();
    QCOMPARE(gone.value(QStringLiteral("missing")).toBool(), true);
    const auto serial = catalog.entries().at(2).toMap();
    QCOMPARE(serial.value(QStringLiteral("kind")).toString(),
             QStringLiteral("series"));
    QCOMPARE(serial.value(QStringLiteral("seasons")).toList().size(), 2);
    const auto episodes = serial.value(QStringLiteral("episodes")).toList();
    QCOMPARE(episodes.size(), 2);
    QCOMPARE(episodes.at(0).toMap().value(QStringLiteral("title")).toString(),
             QStringLiteral("Pilot"));
    QCOMPARE(episodes.at(0).toMap().value(QStringLiteral("duration")).toString(),
             QStringLiteral("40 min"));
    QCOMPARE(episodes.at(0).toMap().value(QStringLiteral("seasonId")).toString(),
             QStringLiteral("s1"));
    QCOMPARE(episodes.at(0).toMap().value(QStringLiteral("number")).toInt(), 1);
    QCOMPARE(episodes.at(1).toMap().value(QStringLiteral("missing")).toBool(),
             true);
    QCOMPARE(serial.value(QStringLiteral("tabId")).toString(),
             catalog.tabs().at(1).toMap().value(QStringLiteral("id")).toString());
  }
  void pagingAndClosedFailures() {
    LibraryDaemon daemon;
    daemon.collections = {{QStringLiteral("films"), QStringLiteral("Films"),
                           QStringLiteral("film")}};
    daemon.page1 = {film(QStringLiteral("one"), QStringLiteral("One"))};
    daemon.page2 = {film(QStringLiteral("two"), QStringLiteral("Two"))};
    daemon.features.clear();
    QVERIFY(registerBus(daemon));
    {
      LibraryCatalog catalog;
      QTRY_COMPARE(catalog.entries().size(), 2);
      QCOMPARE(daemon.resumeReads, 0);
      QCOMPARE(catalog.entries().at(0).toMap().value(QStringLiteral("progress")).toDouble(),
               0);
    }
    cleanup();
    {
      LibraryDaemon mixed;
      mixed.collections = daemon.collections;
      mixed.page1 = daemon.page1;
      mixed.page2 = daemon.page2;
      mixed.page2Revision = 2;
      mixed.features.clear();
      QVERIFY(registerBus(mixed));
      LibraryCatalog failed;
      QTRY_VERIFY(mixed.libraryReads >= 2);
      QTRY_COMPARE(failed.libraryState(), QStringLiteral("error"));
      QCOMPARE(failed.entries().size(), 0);
    }
  }
  void repeatedTokenAndBadResumeKeepBounds() {
    LibraryDaemon daemon;
    daemon.loopSecond = true;
    daemon.collections = {{QStringLiteral("films"), QStringLiteral("Films"),
                           QStringLiteral("film")}};
    daemon.page1 = {film(QStringLiteral("one"), QStringLiteral("One"))};
    daemon.page2 = {film(QStringLiteral("two"), QStringLiteral("Two"))};
    QVERIFY(registerBus(daemon));
    {
      LibraryCatalog catalog;
      QTRY_COMPARE(catalog.libraryState(), QStringLiteral("error"));
      QCOMPARE(catalog.entries().size(), 0);
      QTest::qWait(200);
      QVERIFY(daemon.libraryReads < 8);
    }
    cleanup();
    LibraryDaemon resumes;
    resumes.collections = daemon.collections;
    resumes.page1 = daemon.page1;
    resumes.rejectResume = true;
    ResumeWire resume;
    resume.contentId = QStringLiteral("one");
    resume.position = 10;
    resume.duration = 20;
    resumes.resumes = {resume};
    QVERIFY(registerBus(resumes));
    LibraryCatalog kept;
    QTRY_COMPARE(kept.libraryState(), QStringLiteral("ready"));
    QCOMPARE(kept.entries().at(0).toMap().value(QStringLiteral("progress")).toDouble(),
             0);
    QVERIFY(resumes.resumeReads >= 1);
  }
  void emptyUnavailableAndUnready() {
    {
      LibraryDaemon daemon;
      daemon.collections = {{QStringLiteral("films"), QStringLiteral("Films"),
                             QStringLiteral("film")}};
      daemon.features.clear();
      QVERIFY(registerBus(daemon));
      LibraryCatalog catalog;
      QTRY_COMPARE(catalog.libraryState(), QStringLiteral("empty"));
      QCOMPARE(catalog.tabs().size(), 1);
    }
    cleanup();
    {
      LibraryDaemon daemon;
      daemon.providers = {};
      QVERIFY(registerBus(daemon));
      LibraryCatalog catalog;
      QTRY_COMPARE(catalog.libraryState(), QStringLiteral("empty"));
    }
    cleanup();
    {
      LibraryDaemon daemon;
      daemon.providers[0].runtime = QStringLiteral("failed");
      QVERIFY(registerBus(daemon));
      LibraryCatalog catalog;
      QTRY_COMPARE(catalog.libraryState(), QStringLiteral("unavailable"));
    }
    cleanup();
    LibraryCatalog missing;
    QTRY_COMPARE(missing.libraryState(), QStringLiteral("unavailable"));
  }
  void unavailableMountKeepsMissingEntries() {
    LibraryDaemon daemon;
    daemon.providers[0].backend = QStringLiteral("unavailable");
    daemon.collections = {{QStringLiteral("films"), QStringLiteral("Films"),
                           QStringLiteral("film")}};
    daemon.page1 = {film(QStringLiteral("gone"), QStringLiteral("Gone"), false)};
    daemon.features.clear();
    QVERIFY(registerBus(daemon));
    LibraryCatalog catalog;
    QTRY_COMPARE(catalog.libraryState(), QStringLiteral("ready"));
    QCOMPARE(catalog.entries().at(0).toMap().value(QStringLiteral("missing")).toBool(),
             true);
  }
  void launchIsAnOperation() {
    LibraryDaemon daemon;
    daemon.holdOperation = true;
    daemon.collections = {{QStringLiteral("films"), QStringLiteral("Films"),
                           QStringLiteral("film")}};
    daemon.page1 = {film(QStringLiteral("north"), QStringLiteral("North")),
                    film(QStringLiteral("gone"), QStringLiteral("Gone"), false)};
    daemon.features.clear();
    daemon.operationEntities = {{QStringLiteral("media-provider"),
                                 QStringLiteral("media"),
                                 QStringLiteral("sess")}};
    QVERIFY(registerBus(daemon));
    LibraryCatalog catalog;
    QTRY_COMPARE(catalog.libraryState(), QStringLiteral("ready"));
    const auto gone = catalog.entries().at(1).toMap().value(QStringLiteral("id")).toString();
    catalog.launch(gone);
    QCOMPARE(catalog.operationState(), QStringLiteral("Unavailable"));
    QCOMPARE(daemon.commands.size(), 0);
    QStringList launched;
    connect(&catalog, &LibraryCatalog::sessionLaunched, &catalog,
            [&](const QString &id) { launched.append(id); });
    const auto north = catalog.entries().at(0).toMap().value(QStringLiteral("id")).toString();
    catalog.launch(north);
    QTRY_COMPARE(catalog.operationState(), QStringLiteral("pending"));
    QCOMPARE(catalog.launchingId(), north);
    QCOMPARE(daemon.commands, QStringList({QStringLiteral("Launch")}));
    QCOMPARE(daemon.lastGeneration, QStringLiteral("g-library"));
    QCOMPARE(daemon.lastLocal, QStringLiteral("north"));
    QVERIFY(!daemon.lastRequest.isEmpty());
    QVERIFY(launched.isEmpty());
    daemon.holdOperation = false;
    QTRY_COMPARE(catalog.operationState(), QStringLiteral("succeeded"));
    QCOMPARE(launched, QStringList({presentationId(QStringLiteral("media-provider"),
                                                   QStringLiteral("sess"))}));
    QCOMPARE(catalog.launchingId(), QString());
    daemon.rejectLaunch = true;
    catalog.launch(north);
    QTRY_COMPARE(catalog.operationState(), QStringLiteral("Unavailable"));
    QCOMPARE(launched.size(), 1);
  }
  void stateChangedRefreshesWithoutAFastPoll() {
    LibraryDaemon daemon;
    daemon.collections = {{QStringLiteral("films"), QStringLiteral("Films"),
                           QStringLiteral("film")}};
    daemon.page1 = {film(QStringLiteral("north"), QStringLiteral("Before"))};
    daemon.features.clear();
    QVERIFY(registerBus(daemon));
    LibraryCatalog catalog;
    QTRY_VERIFY(catalog.entries().size() >= 1);
    QCOMPARE(catalog.entries().at(0).toMap().value(QStringLiteral("title")).toString(),
             QStringLiteral("Before"));
    QTest::qWait(100);
    const auto reads = daemon.libraryReads;
    QVERIFY(reads >= 1);
    QTest::qWait(1500);
    QCOMPARE(daemon.libraryReads, reads);
    daemon.page1[0].title = QStringLiteral("After");
    auto signal = QDBusMessage::createSignal(
        QStringLiteral("/org/arinn/Daemon1"), QStringLiteral("org.arinn.Daemon1"),
        QStringLiteral("StateChanged"));
    signal << QStringLiteral("library-provider") << QStringLiteral("library")
           << QStringLiteral("g-library") << qulonglong(2);
    QVERIFY(QDBusConnection::sessionBus().send(signal));
    QTRY_VERIFY(catalog.entries().size() >= 1 &&
                catalog.entries().at(0).toMap().value(QStringLiteral("title")).toString() ==
                    QStringLiteral("After"));
    QVERIFY(daemon.libraryReads > reads);
  }
  void operationWaitIsBounded() {
    LibraryDaemon daemon;
    daemon.holdOperation = true;
    daemon.collections = {{QStringLiteral("films"), QStringLiteral("Films"),
                           QStringLiteral("film")}};
    daemon.page1 = {film(QStringLiteral("north"), QStringLiteral("North"))};
    daemon.features.clear();
    QVERIFY(registerBus(daemon));
    LibraryCatalog catalog;
    QTRY_COMPARE(catalog.libraryState(), QStringLiteral("ready"));
    catalog.launch(catalog.entries().at(0).toMap().value(QStringLiteral("id")).toString());
    QTRY_COMPARE(catalog.operationState(), QStringLiteral("pending"));
    QTRY_COMPARE_WITH_TIMEOUT(catalog.operationState(), QStringLiteral("Timeout"),
                              40000);
    QTest::qWait(400);
    const auto reads = daemon.operationReads;
    QVERIFY(reads > 0);
    QVERIFY(reads < 400);
    QTest::qWait(400);
    QCOMPARE(daemon.operationReads, reads);
  }
  void screenShowsResumeEmptyAndUnavailable() {
    LibraryDaemon daemon;
    daemon.holdOperation = true;
    daemon.collections = {{QStringLiteral("films"), QStringLiteral("Films"),
                           QStringLiteral("film")}};
    daemon.page1 = {film(QStringLiteral("north"), QStringLiteral("North")),
                    film(QStringLiteral("gone"), QStringLiteral("Gone"), false)};
    ResumeWire resume;
    resume.contentId = QStringLiteral("north");
    resume.position = 50000000;
    resume.duration = 100000000;
    daemon.resumes = {resume};
    ProviderWire media;
    media.id = QStringLiteral("media-provider");
    media.name = QStringLiteral("media");
    media.path = QDBusObjectPath(QStringLiteral("/org/arinn/Daemon1/providers/p_media"));
    media.capabilities = {QStringLiteral("media")};
    media.generation = QStringLiteral("g-media");
    daemon.providers.append(media);
    SessionWire session;
    session.id = QStringLiteral("sess");
    session.title = QStringLiteral("North");
    session.state = QStringLiteral("paused");
    session.actions = {QStringLiteral("play")};
    daemon.sessions = {session};
    daemon.operationEntities = {{QStringLiteral("media-provider"),
                                 QStringLiteral("media"),
                                 QStringLiteral("sess")}};
    QVERIFY(registerBus(daemon));
    ShellStandIn backend;
    SessionGuardStandIn guard;
    QQmlApplicationEngine engine;
    QStringList problems;
    connect(&engine, &QQmlEngine::warnings, &engine,
            [&](const QList<QQmlError> &warnings) {
              for (const auto &warning : warnings) {
                const auto text = warning.toString();
                if (text.contains(QStringLiteral("qrc:/brand/")))
                  continue;
                problems.append(text);
              }
            });
    installAssetImageProvider(&engine);
    engine.rootContext()->setContextProperty(QStringLiteral("backend"), &backend);
    engine.rootContext()->setContextProperty(QStringLiteral("sessionGuard"),
                                             &guard);
    engine.load(QUrl::fromLocalFile(QStringLiteral(ARINN_MAIN_QML)));
    QVERIFY2(problems.isEmpty(), qPrintable(problems.join(QLatin1Char('\n'))));
    auto window = qobject_cast<QQuickWindow *>(engine.rootObjects().value(0));
    QVERIFY(window);
    window->show();
    window->requestActivate();
    auto home = window->findChild<QQuickItem *>(QStringLiteral("productionHome"));
    auto library =
        window->findChild<QQuickItem *>(QStringLiteral("productionLibrary"));
    auto catalog =
        window->findChild<LibraryCatalog *>(QStringLiteral("libraryCatalog"));
    QVERIFY(home);
    QVERIFY(library);
    QVERIFY(catalog);
    QTRY_COMPARE(library->property("libraryState").toString(),
                 QStringLiteral("ready"));
    home->setProperty("screen", QStringLiteral("library"));
    QCOMPARE(home->property("screen").toString(), QStringLiteral("library"));
    QCOMPARE(library->property("visible").toBool(), true);
    const auto entries = library->property("entries").toList();
    QCOMPARE(entries.size(), 2);
    const auto north = entries.at(0).toMap();
    QCOMPARE(north.value(QStringLiteral("progress")).toDouble(), 0.5);
    QCOMPARE(north.value(QStringLiteral("noArt")).toBool(), false);
    QVERIFY(north.value(QStringLiteral("artwork"))
                .toString()
                .startsWith(QStringLiteral("image://arinn-assets/")));
    QCOMPARE(entries.at(1).toMap().value(QStringLiteral("missing")).toBool(), true);
    library->setProperty("detailEntryId", north.value(QStringLiteral("id")));
    auto play = window->findChild<QQuickItem *>(QStringLiteral("detailPlay"));
    QVERIFY(play);
    QCOMPARE(play->property("text").toString(), QStringLiteral("Continue"));
    auto progress = window->findChild<QQuickItem *>(QStringLiteral("detailProgress"));
    QVERIFY(progress);
    QCOMPARE(progress->property("visible").toBool(), true);
    play->forceActiveFocus();
    QTest::keyClick(window, Qt::Key_Return);
    QTRY_COMPARE(catalog->operationState(), QStringLiteral("pending"));
    auto waiting = library->findChild<QQuickItem *>(QStringLiteral("operationPending"));
    QVERIFY(waiting);
    QTRY_COMPARE(waiting->property("visible").toBool(), true);
    daemon.holdOperation = false;
    QTRY_COMPARE(catalog->operationState(), QStringLiteral("succeeded"));
    QTRY_COMPARE(window->property("sessionOpen").toBool(), true);
    auto sessionView =
        window->findChild<QQuickItem *>(QStringLiteral("productionSession"));
    QVERIFY(sessionView);
    QTRY_COMPARE(sessionView->property("title").toString(), QStringLiteral("North"));
    window->setProperty("sessionOpen", false);
    daemon.page1.clear();
    auto signal = QDBusMessage::createSignal(
        QStringLiteral("/org/arinn/Daemon1"), QStringLiteral("org.arinn.Daemon1"),
        QStringLiteral("StateChanged"));
    signal << QStringLiteral("library-provider") << QStringLiteral("library")
           << QStringLiteral("g-library") << qulonglong(2);
    QVERIFY(QDBusConnection::sessionBus().send(signal));
    QTRY_COMPARE(library->property("libraryState").toString(),
                 QStringLiteral("empty"));
    auto status = window->findChild<QQuickItem *>(QStringLiteral("libraryStatus"));
    QVERIFY(status);
    QCOMPARE(status->property("kind").toString(), QStringLiteral("empty"));
    QCOMPARE(status->property("visible").toBool(), true);
    QVERIFY(QDBusConnection::sessionBus().unregisterService(
        QStringLiteral("org.arinn.Daemon1")));
    QTRY_COMPARE(library->property("libraryState").toString(),
                 QStringLiteral("unavailable"));
    QCOMPARE(status->property("kind").toString(), QStringLiteral("unavailable"));
  }
  void seriesLaunchResolvesToAnEpisode() {
    {
      LibraryDaemon daemon;
      daemon.holdOperation = true;
      daemon.collections = {{QStringLiteral("shows"), QStringLiteral("Series"),
                             QStringLiteral("series")}};
      daemon.page1 = {showItem(),
                      episodeItem(QStringLiteral("ep4"), QStringLiteral("Fourth"), 1, 4),
                      episodeItem(QStringLiteral("ep1"), QStringLiteral("Pilot"), 1, 1)};
      daemon.resumes = {resumeOn(QStringLiteral("ep4"), 50000000, 100000000, false, 20)};
      QVERIFY(registerBus(daemon));
      LibraryCatalog catalog;
      QTRY_COMPARE(catalog.libraryState(), QStringLiteral("ready"));
      const auto serial = catalog.entries().at(0).toMap();
      QCOMPARE(serial.value(QStringLiteral("progress")).toDouble(), 0.5);
      const auto seriesId = serial.value(QStringLiteral("id")).toString();
      const auto fourth = episodePresentation(serial, 4);
      catalog.launch(seriesId);
      QTRY_COMPARE(daemon.commands.size(), 1);
      QCOMPARE(daemon.lastLocal, QStringLiteral("ep4"));
      QCOMPARE(catalog.launchingId(), fourth);
      QCOMPARE(catalog.operationState(), QStringLiteral("pending"));
      catalog.launch(seriesId);
      catalog.launch(fourth);
      QTest::qWait(200);
      QCOMPARE(daemon.commands.size(), 1);
      QCOMPARE(daemon.lastLocal, QStringLiteral("ep4"));
      QCOMPARE(catalog.launchingId(), fourth);
    }
    cleanup();
    {
      LibraryDaemon daemon;
      daemon.holdOperation = true;
      daemon.collections = {{QStringLiteral("shows"), QStringLiteral("Series"),
                             QStringLiteral("series")}};
      daemon.page1 = {showItem(),
                      episodeItem(QStringLiteral("ep4"), QStringLiteral("Fourth"), 1, 4),
                      episodeItem(QStringLiteral("ep1"), QStringLiteral("Pilot"), 1, 1)};
      QVERIFY(registerBus(daemon));
      LibraryCatalog catalog;
      QTRY_COMPARE(catalog.libraryState(), QStringLiteral("ready"));
      const auto serial = catalog.entries().at(0).toMap();
      QCOMPARE(serial.value(QStringLiteral("progress")).toDouble(), 0);
      catalog.launch(serial.value(QStringLiteral("id")).toString());
      QTRY_COMPARE(daemon.lastLocal, QStringLiteral("ep1"));
      daemon.holdOperation = false;
      QTRY_COMPARE(catalog.operationState(), QStringLiteral("succeeded"));
      const auto second = episodePresentation(serial, 4);
      catalog.launch(second);
      QTRY_COMPARE(daemon.lastLocal, QStringLiteral("ep4"));
      QCOMPARE(daemon.commands.size(), 2);
    }
    cleanup();
    {
      LibraryDaemon daemon;
      daemon.collections = {{QStringLiteral("shows"), QStringLiteral("Series"),
                             QStringLiteral("series")}};
      daemon.page1 = {showItem(),
                      episodeItem(QStringLiteral("ep2"), QStringLiteral("Second"), 1, 2),
                      episodeItem(QStringLiteral("ep1"), QStringLiteral("Pilot"), 1, 1, false),
                      episodeItem(QStringLiteral("ep3"), QStringLiteral("Later"), 2, 1)};
      QVERIFY(registerBus(daemon));
      LibraryCatalog catalog;
      QTRY_COMPARE(catalog.libraryState(), QStringLiteral("ready"));
      catalog.launch(catalog.entries().at(0).toMap().value(QStringLiteral("id")).toString());
      QTRY_COMPARE(daemon.lastLocal, QStringLiteral("ep2"));
    }
    cleanup();
    {
      LibraryDaemon daemon;
      daemon.collections = {{QStringLiteral("shows"), QStringLiteral("Series"),
                             QStringLiteral("series")}};
      daemon.page1 = {showItem()};
      QVERIFY(registerBus(daemon));
      LibraryCatalog catalog;
      QTRY_COMPARE(catalog.libraryState(), QStringLiteral("ready"));
      catalog.launch(catalog.entries().at(0).toMap().value(QStringLiteral("id")).toString());
      QCOMPARE(catalog.operationState(), QStringLiteral("Unavailable"));
      QCOMPARE(daemon.commands.size(), 0);
    }
    cleanup();
    {
      LibraryDaemon daemon;
      daemon.collections = {{QStringLiteral("shows"), QStringLiteral("Series"),
                             QStringLiteral("series")}};
      daemon.page1 = {showItem(),
                      episodeItem(QStringLiteral("ep4"), QStringLiteral("Fourth"), 1, 4),
                      episodeItem(QStringLiteral("ep1"), QStringLiteral("Pilot"), 1, 1)};
      daemon.resumes = {resumeOn(QStringLiteral("ep4"), 50000000, 100000000, true, 20)};
      QVERIFY(registerBus(daemon));
      LibraryCatalog catalog;
      QTRY_COMPARE(catalog.libraryState(), QStringLiteral("ready"));
      const auto serial = catalog.entries().at(0).toMap();
      QCOMPARE(serial.value(QStringLiteral("progress")).toDouble(), 0);
      catalog.launch(serial.value(QStringLiteral("id")).toString());
      QTRY_COMPARE(daemon.lastLocal, QStringLiteral("ep1"));
    }
    cleanup();
    {
      LibraryDaemon daemon;
      daemon.collections = {{QStringLiteral("shows"), QStringLiteral("Series"),
                             QStringLiteral("series")}};
      daemon.page1 = {showItem(),
                      episodeItem(QStringLiteral("ep4"), QStringLiteral("Fourth"), 1, 4),
                      episodeItem(QStringLiteral("ep1"), QStringLiteral("Pilot"), 1, 1)};
      daemon.resumes = {resumeOn(QStringLiteral("ep1"), 10000000, 100000000, false, 5),
                        resumeOn(QStringLiteral("ep4"), 50000000, 100000000, false, 40)};
      QVERIFY(registerBus(daemon));
      LibraryCatalog catalog;
      QTRY_COMPARE(catalog.libraryState(), QStringLiteral("ready"));
      const auto serial = catalog.entries().at(0).toMap();
      QCOMPARE(serial.value(QStringLiteral("progress")).toDouble(), 0.5);
      catalog.launch(serial.value(QStringLiteral("id")).toString());
      QTRY_COMPARE(daemon.lastLocal, QStringLiteral("ep4"));
    }
    cleanup();
    {
      LibraryDaemon daemon;
      daemon.collections = {{QStringLiteral("shows"), QStringLiteral("Series"),
                             QStringLiteral("series")}};
      daemon.page1 = {showItem(),
                      episodeItem(QStringLiteral("ep4"), QStringLiteral("Fourth"), 1, 4),
                      episodeItem(QStringLiteral("ep1"), QStringLiteral("Pilot"), 1, 1)};
      daemon.resumes = {resumeOn(QStringLiteral("ep1"), 80000000, 100000000, false, 80),
                        resumeOn(QStringLiteral("ep4"), 10000000, 100000000, false, 10)};
      QVERIFY(registerBus(daemon));
      LibraryCatalog catalog;
      QTRY_COMPARE(catalog.libraryState(), QStringLiteral("ready"));
      catalog.launch(catalog.entries().at(0).toMap().value(QStringLiteral("id")).toString());
      QTRY_COMPARE(daemon.lastLocal, QStringLiteral("ep1"));
    }
    cleanup();
    {
      LibraryDaemon daemon;
      daemon.collections = {{QStringLiteral("shows"), QStringLiteral("Series"),
                             QStringLiteral("series")}};
      daemon.page1 = {showItem(),
                      episodeItem(QStringLiteral("ep4"), QStringLiteral("Fourth"), 1, 4),
                      episodeItem(QStringLiteral("ep1"), QStringLiteral("Pilot"), 1, 1)};
      daemon.resumes = {resumeOn(QStringLiteral("ep1"), 10000000, 100000000, false, 10),
                        resumeOn(QStringLiteral("ep4"), 20000000, 100000000, false, 10)};
      QVERIFY(registerBus(daemon));
      LibraryCatalog catalog;
      QTRY_COMPARE(catalog.libraryState(), QStringLiteral("ready"));
      catalog.launch(catalog.entries().at(0).toMap().value(QStringLiteral("id")).toString());
      QTRY_COMPARE(daemon.lastLocal, QStringLiteral("ep1"));
    }
  }
  void episodeLaunchReturnsToTheLibrary() {
    LibraryDaemon daemon;
    daemon.holdOperation = true;
    daemon.collections = {{QStringLiteral("shows"), QStringLiteral("Series"),
                           QStringLiteral("series")}};
    daemon.page1 = {showItem(),
                    episodeItem(QStringLiteral("ep1"), QStringLiteral("Pilot"), 1, 1),
                    episodeItem(QStringLiteral("ep2"), QStringLiteral("Second"), 1, 2)};
    ProviderWire media;
    media.id = QStringLiteral("media-provider");
    media.name = QStringLiteral("media");
    media.path = QDBusObjectPath(QStringLiteral("/org/arinn/Daemon1/providers/p_media"));
    media.capabilities = {QStringLiteral("media")};
    media.generation = QStringLiteral("g-media");
    daemon.providers.append(media);
    SessionWire session;
    session.id = QStringLiteral("sess");
    session.title = QStringLiteral("North");
    session.state = QStringLiteral("paused");
    session.actions = {QStringLiteral("play")};
    daemon.sessions = {session};
    daemon.operationEntities = {{QStringLiteral("media-provider"),
                                 QStringLiteral("media"),
                                 QStringLiteral("sess")}};
    QVERIFY(registerBus(daemon));
    ShellStandIn backend;
    SessionGuardStandIn guard;
    QQmlApplicationEngine engine;
    QStringList problems;
    connect(&engine, &QQmlEngine::warnings, &engine,
            [&](const QList<QQmlError> &warnings) {
              for (const auto &warning : warnings) {
                const auto text = warning.toString();
                if (text.contains(QStringLiteral("qrc:/brand/")))
                  continue;
                problems.append(text);
              }
            });
    installAssetImageProvider(&engine);
    engine.rootContext()->setContextProperty(QStringLiteral("backend"), &backend);
    engine.rootContext()->setContextProperty(QStringLiteral("sessionGuard"),
                                             &guard);
    engine.load(QUrl::fromLocalFile(QStringLiteral(ARINN_MAIN_QML)));
    QVERIFY2(problems.isEmpty(), qPrintable(problems.join(QLatin1Char('\n'))));
    auto window = qobject_cast<QQuickWindow *>(engine.rootObjects().value(0));
    QVERIFY(window);
    window->show();
    window->requestActivate();
    auto home = window->findChild<QQuickItem *>(QStringLiteral("productionHome"));
    auto library =
        window->findChild<QQuickItem *>(QStringLiteral("productionLibrary"));
    auto sessionView =
        window->findChild<QQuickItem *>(QStringLiteral("productionSession"));
    QVERIFY(home);
    QVERIFY(library);
    QVERIFY(sessionView);
    QTRY_COMPARE(library->property("libraryState").toString(),
                 QStringLiteral("ready"));
    home->setProperty("screen", QStringLiteral("library"));
    QTRY_COMPARE(library->property("visible").toBool(), true);
    QTest::qWait(50);
    const auto entries = library->property("entries").toList();
    QCOMPARE(entries.size(), 1);
    const auto serial = entries.at(0).toMap();
    const auto first = episodePresentation(serial, 1);
    const auto second = episodePresentation(serial, 2);
    QVERIFY(!first.isEmpty());
    QVERIFY(!second.isEmpty());
    const auto posterName =
        QStringLiteral("poster-") + serial.value(QStringLiteral("id")).toString();
    QTRY_VERIFY(findItem(library, posterName));
    auto poster = findItem(library, posterName);
    poster->forceActiveFocus();
    QTest::keyClick(window, Qt::Key_Return);
    auto play = library->findChild<QQuickItem *>(QStringLiteral("detailPlay"));
    QVERIFY(play);
    QTRY_VERIFY(play->hasActiveFocus());
    QCOMPARE(play->property("text").toString(), QStringLiteral("Play"));
    QTest::keyClick(window, Qt::Key_Down);
    QTest::keyClick(window, Qt::Key_Down);
    QTest::keyClick(window, Qt::Key_Right);
    auto still = findItem(library, QStringLiteral("episode-") + second);
    auto other = findItem(library, QStringLiteral("episode-") + first);
    QVERIFY(still);
    QVERIFY(other);
    QTRY_VERIFY(still->hasActiveFocus());
    QTest::keyClick(window, Qt::Key_Return);
    QTRY_COMPARE(daemon.commands.size(), 1);
    QCOMPARE(daemon.lastLocal, QStringLiteral("ep2"));
    QTRY_COMPARE(still->property("busy").toBool(), true);
    QCOMPARE(other->property("busy").toBool(), false);
    QTRY_COMPARE(play->property("busy").toBool(), true);
    QTest::keyClick(window, Qt::Key_Return);
    QTest::qWait(300);
    QCOMPARE(daemon.commands.size(), 1);
    QCOMPARE(daemon.lastLocal, QStringLiteral("ep2"));
    daemon.holdOperation = false;
    QTRY_COMPARE(window->property("sessionOpen").toBool(), true);
    QTRY_COMPARE(sessionView->property("title").toString(), QStringLiteral("North"));
    auto sessionPlay = window->findChild<QQuickItem *>(QStringLiteral("action-play"));
    QVERIFY(sessionPlay);
    QTRY_VERIFY(sessionPlay->hasActiveFocus());
    QTest::keyClick(window, Qt::Key_Escape);
    QTRY_COMPARE(window->property("sessionOpen").toBool(), false);
    QCOMPARE(home->property("screen").toString(), QStringLiteral("library"));
    QCOMPARE(library->property("visible").toBool(), true);
    QCOMPARE(library->property("detailOpen").toBool(), true);
    QTRY_VERIFY(play->hasActiveFocus());
    QVERIFY(focusInside(window->activeFocusItem(), library));

    home->setProperty("screen", QStringLiteral("home"));
    const auto mediaId = presentationId(QStringLiteral("media-provider"),
                                        QStringLiteral("sess"));
    QTRY_VERIFY(findItem(home, QStringLiteral("entry-") + mediaId));
    auto card = findItem(home, QStringLiteral("entry-") + mediaId);
    card->forceActiveFocus();
    QTest::keyClick(window, Qt::Key_Return);
    QTRY_COMPARE(window->property("sessionOpen").toBool(), true);
    home->setProperty("screen", QStringLiteral("library"));
    QTest::qWait(50);
    sessionPlay->forceActiveFocus();
    QVERIFY(sessionPlay->hasActiveFocus());
    QTest::keyClick(window, Qt::Key_Escape);
    QTRY_COMPARE(window->property("sessionOpen").toBool(), false);
    QCOMPARE(home->property("screen").toString(), QStringLiteral("home"));
    QTRY_VERIFY(focusInside(window->activeFocusItem(), home));
    QVERIFY(!focusInside(window->activeFocusItem(), library));
  }
  void hiddenLibraryLeavesHomeFocusAlone() {
  void navigationMenuFocusesTheLibrary() {
    LibraryDaemon daemon;
    daemon.collections = {{QStringLiteral("films"), QStringLiteral("Films"),
                           QStringLiteral("film")}};
    daemon.page1 = {film(QStringLiteral("north"), QStringLiteral("North"))};
    QVERIFY(registerBus(daemon));
    ShellStandIn backend;
    SessionGuardStandIn guard;
    QQmlApplicationEngine engine;
    installAssetImageProvider(&engine);
    engine.rootContext()->setContextProperty(QStringLiteral("backend"), &backend);
    engine.rootContext()->setContextProperty(QStringLiteral("sessionGuard"),
                                             &guard);
    engine.load(QUrl::fromLocalFile(QStringLiteral(ARINN_MAIN_QML)));
    auto window = qobject_cast<QQuickWindow *>(engine.rootObjects().value(0));
    QVERIFY(window);
    window->show();
    window->requestActivate();
    auto home = window->findChild<QQuickItem *>(QStringLiteral("productionHome"));
    auto library =
        window->findChild<QQuickItem *>(QStringLiteral("productionLibrary"));
    QVERIFY(home);
    QVERIFY(library);
    QTRY_COMPARE(library->property("libraryState").toString(),
                 QStringLiteral("ready"));
    QCOMPARE(library->property("visible").toBool(), false);
    auto menu = findItem(home, QStringLiteral("menuButton"));
    QVERIFY(menu);
    menu->forceActiveFocus();
    QTRY_VERIFY(menu->hasActiveFocus());

    // A state change of the hidden library must not move focus at all.
    QSignalSpy moves(window, &QQuickWindow::activeFocusItemChanged);
    daemon.page1.clear();
    auto signal = QDBusMessage::createSignal(
        QStringLiteral("/org/arinn/Daemon1"), QStringLiteral("org.arinn.Daemon1"),
        QStringLiteral("StateChanged"));
    signal << QStringLiteral("library-provider") << QStringLiteral("library")
           << QStringLiteral("g-library") << qulonglong(2);
    QVERIFY(QDBusConnection::sessionBus().send(signal));
    QTRY_COMPARE(library->property("libraryState").toString(),
                 QStringLiteral("empty"));
    QTest::qWait(100);
    QCOMPARE(moves.count(), 0);
    QVERIFY(menu->hasActiveFocus());

    // A hidden screen that still forces focus drops it on the window's root
    // item. The shell hands it back to the screen on display, so the remote
    // keeps working.
    QVERIFY(QMetaObject::invokeMethod(library, "focusInitial"));
    QTRY_VERIFY(focusInside(window->activeFocusItem(), home));
    QVERIFY(window->activeFocusItem() != window->contentItem());
    QVERIFY(QMetaObject::invokeMethod(home, "openNavigation"));
    auto homeItem = findItem(home, QStringLiteral("nav-0"));
    QVERIFY(homeItem);
    QTRY_VERIFY(homeItem->hasActiveFocus());
    QTest::keyClick(window, Qt::Key_Down);
    QTest::keyClick(window, Qt::Key_Return);
    QTRY_COMPARE(library->property("visible").toBool(), true);
    // The hidden Back to Home button under the library must not keep focus.
    QTRY_VERIFY(focusInside(window->activeFocusItem(), library));
    QTest::keyClick(window, Qt::Key_Return);
    QTest::qWait(50);
    QCOMPARE(home->property("screen").toString(), QStringLiteral("library"));
  }
  void emptyPosterAndFanartStayGeneric() {
    LibraryDaemon daemon;
    daemon.collections = {{QStringLiteral("films"), QStringLiteral("Films"),
                           QStringLiteral("film")}};
    auto row = film(QStringLiteral("plain"), QStringLiteral("Plain"));
    row.poster.clear();
    row.fanart.clear();
    daemon.page1 = {row};
    QVERIFY(registerBus(daemon));
    LibraryCatalog catalog;
    QTRY_COMPARE(catalog.libraryState(), QStringLiteral("ready"));
    const auto entry = catalog.entries().at(0).toMap();
    QCOMPARE(entry.value(QStringLiteral("noArt")).toBool(), true);
    QVERIFY(!entry.contains(QStringLiteral("artwork")));
    QVERIFY(!entry.contains(QStringLiteral("fanart")));
  }
};

int main(int argc, char **argv) {
  QGuiApplication app(argc, argv);
  LibraryCatalogTest test;
  return QTest::qExec(&test, argc, argv);
}
#include "library_catalog_test.moc"
