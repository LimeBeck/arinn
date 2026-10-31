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
#include <algorithm>
#include <limits>
namespace {
// DownloadCatalog's presentation id: the provider id is length-prefixed.
QString downloadId(const QString &local) {
  return QUuid::createUuidV5(QUuid(), QStringLiteral("11:dl-provider") + local)
      .toString(QUuid::WithoutBraces);
}
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
struct AdditionWire {
  QString id;
  qulonglong time = 1;
};
QDBusArgument &operator<<(QDBusArgument &arg, const AdditionWire &row) {
  arg.beginStructure(); arg << row.id << row.time; arg.endStructure(); return arg;
}
const QDBusArgument &operator>>(const QDBusArgument &arg, AdditionWire &) { return arg; }
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
struct OutputWire {
  QString id, title;
};
QDBusArgument &operator<<(QDBusArgument &argument, const OutputWire &row) {
  argument.beginStructure();
  argument << row.id << row.title;
  argument.endStructure();
  return argument;
}
const QDBusArgument &operator>>(const QDBusArgument &argument, OutputWire &) {
  return argument;
}
// One org.arinn.Downloads1 row (#233), without outputs.
struct DownloadWire {
  QString id, title, state;
  QStringList actions;
  QString importState = QStringLiteral("none");
  QList<EntityWire> refs;
};
QDBusArgument &operator<<(QDBusArgument &argument, const DownloadWire &row) {
  argument.beginStructure();
  argument << row.id << row.title << row.state << qulonglong(0) << false
           << qulonglong(0) << qulonglong(0) << qlonglong(-1) << false << 0.0
           << row.actions << QString() << QString();
  argument.beginArray(QMetaType::fromType<OutputWire>());
  argument.endArray();
  argument << row.importState;
  argument.beginArray(QMetaType::fromType<EntityWire>());
  for (const auto &ref : row.refs)
    argument << ref;
  argument.endArray();
  argument.endStructure();
  return argument;
}
const QDBusArgument &operator>>(const QDBusArgument &argument, DownloadWire &) {
  return argument;
}
class LibraryDaemon : public QDBusVirtualObject {
public:
  QList<DownloadWire> downloads;
  QList<ProviderWire> providers{ProviderWire{}};
  QList<CollectionWire> collections;
  QList<ItemWire> page1;
  QList<ItemWire> page2;
  QHash<QString, QList<ItemWire>> extraPages;
  QHash<QString, QList<AdditionWire>> extraAdditions;
  QList<AdditionWire> additions;
  QString additionGeneration = QStringLiteral("g-library");
  qulonglong additionRevision = 1;
  int additionReads = 0;
  qulonglong requestedAdditionRevision = 0;
  bool rejectAdditions = false;
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
      if ((message.path() != libraryPath && !extraPages.contains(message.path())) || rejectLibrary) {
        bus.send(message.createErrorReply(
            QStringLiteral("org.arinn.Error1.Unavailable"),
            QStringLiteral("Unavailable")));
        return true;
      }
      const auto token = message.arguments().value(0).toString();
      QList<ItemWire> rows;
      QString next;
      if (token.isEmpty()) {
        rows = extraPages.value(message.path(), page1);
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
    } else if (message.member() == QLatin1String("GetDownloads")) {
      body.beginStructure();
      body << QStringLiteral("7c1d2e3f-4a5b-4c6d-8e9f-0a1b2c3d4e5f") << qulonglong(1);
      body.beginArray(QMetaType::fromType<DownloadWire>());
      for (const auto &row : downloads)
        body << row;
      body.endArray();
      body << QString();
      body.endStructure();
    } else if (message.interface() == QLatin1String("org.arinn.Downloads1")) {
      commands.append(message.member());
      lastLocal = message.arguments().value(1).toString();
      bus.send(message.createReply(QVariant::fromValue(
          QDBusObjectPath(QStringLiteral("/org/arinn/Daemon1/ops/dl")))));
      return true;
    } else if (message.member() == QLatin1String("Features")) {
      ++featureReads;
      bus.send(message.createReply(QVariant::fromValue(features)));
      return true;
    } else if (message.member() == QLatin1String("GetAdditions")) {
      ++additionReads;
      requestedAdditionRevision = message.arguments().value(1).toULongLong();
      if (rejectAdditions) {
        bus.send(message.createErrorReply(QStringLiteral("org.arinn.Error1.Unavailable"), QStringLiteral("Unavailable")));
        return true;
      }
      body.beginStructure();
      body << additionGeneration << additionRevision;
      body.beginArray(QMetaType::fromType<AdditionWire>());
      for (const auto &row : extraAdditions.value(message.path(), additions)) body << row;
      body.endArray(); body.endStructure();
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
    qDBusRegisterMetaType<AdditionWire>();
    qDBusRegisterMetaType<ProviderWire>();
    qDBusRegisterMetaType<ShortcutWire>();
    qDBusRegisterMetaType<EntityWire>();
    qDBusRegisterMetaType<ContentWire>();
    qDBusRegisterMetaType<SessionWire>();
    qDBusRegisterMetaType<OutputWire>();
    qDBusRegisterMetaType<DownloadWire>();
  }
  void cleanup() {
    auto bus = QDBusConnection::sessionBus();
    bus.unregisterObject(QStringLiteral("/org/arinn"),
                         QDBusConnection::UnregisterTree);
    bus.unregisterService(QStringLiteral("org.arinn.Daemon1"));
  }
  void newEntriesMergeProvidersAndKeepTheLatestTwelve() {
    LibraryDaemon daemon;
    daemon.features.append(QStringLiteral("LibraryAdditions1"));
    daemon.collections = {{QStringLiteral("films"), QStringLiteral("Films"), QStringLiteral("film")}};
    ProviderWire second;
    second.id = QStringLiteral("second");
    second.path = QDBusObjectPath(QStringLiteral("/org/arinn/Daemon1/providers/p_second"));
    daemon.providers.append(second);
    QList<ItemWire> other;
    QList<AdditionWire> otherAdditions;
    for (int i = 9; i >= 0; --i) {
      const auto id = QStringLiteral("film%1").arg(i);
      daemon.page1.append(film(id, id)); other.append(film(id, id));
      daemon.additions.append({id, qulonglong(100 + i * 2)});
      otherAdditions.append({id, qulonglong(101 + i * 2)});
    }
    daemon.extraPages.insert(second.path.path(), other);
    daemon.extraAdditions.insert(second.path.path(), otherAdditions);
    QVERIFY(registerBus(daemon));
    LibraryCatalog catalog;
    QTRY_COMPARE(catalog.newEntries().size(), 12);
    QCOMPARE(catalog.entries().size(), 20);
    QCOMPARE(daemon.additionReads, 2);
    QCOMPARE(daemon.requestedAdditionRevision, qulonglong(1)); // not resume revision 7
    QCOMPARE(catalog.newEntries().at(0).toMap().value(QStringLiteral("id")).toString(), presentationId(second.id, QStringLiteral("film9")));
    QCOMPARE(catalog.newEntries().at(1).toMap().value(QStringLiteral("id")).toString(), presentationId(QStringLiteral("library-provider"), QStringLiteral("film9")));
    QSignalSpy changed(&catalog, &LibraryCatalog::catalogChanged);
    const auto reads = daemon.additionReads;
    catalog.refresh();
    QTRY_COMPARE(daemon.additionReads, reads + 2);
    QTest::qWait(100);
    QCOMPARE(changed.count(), 0);
    QVERIFY(QDBusConnection::sessionBus().unregisterService(QStringLiteral("org.arinn.Daemon1")));
    QTRY_VERIFY(catalog.newEntries().isEmpty());
  }
  void additionTiesUseUtf8OpaqueIdOrdering() {
    LibraryDaemon daemon;
    daemon.features = {QStringLiteral("LibraryAdditions1")};
    daemon.collections = {{QStringLiteral("films"), QStringLiteral("Films"), QStringLiteral("film")}};
    // UTF-16 code-unit ordering differs from the protocol's UTF-8 ordering.
    const auto bmp = QString::fromUtf8("\xee\x80\x80");
    const auto supplementary = QString::fromUtf8("\xf0\x90\x80\x80");
    daemon.page1 = {film(bmp, QStringLiteral("BMP")), film(supplementary, QStringLiteral("Supplementary"))};
    daemon.additions = {{bmp, 1}, {supplementary, 1}};
    QVERIFY(registerBus(daemon));
    LibraryCatalog catalog;
    QTRY_COMPARE(catalog.newEntries().size(), 2);
    QCOMPARE(catalog.newEntries().at(0).toMap().value(QStringLiteral("id")).toString(), presentationId(QStringLiteral("library-provider"), bmp));
    std::reverse(daemon.additions.begin(), daemon.additions.end());
    daemon.page1Revision = 2; daemon.additionRevision = 2;
    catalog.refresh();
    QTRY_VERIFY(catalog.newEntries().isEmpty());
    QCOMPARE(catalog.libraryState(), QStringLiteral("ready"));
  }
  void invalidAdditionsLeaveTheBaseLibraryUsable_data() {
    QTest::addColumn<int>("variant");
    for (int i = 0; i < 10; ++i) QTest::newRow(qPrintable(QString::number(i))) << i;
  }
  void invalidAdditionsLeaveTheBaseLibraryUsable() {
    QFETCH(int, variant);
    LibraryDaemon daemon;
    daemon.features.append(QStringLiteral("LibraryAdditions1"));
    daemon.collections = {{QStringLiteral("films"), QStringLiteral("Films"), QStringLiteral("film")}};
    daemon.page1 = {film(QStringLiteral("a"), QStringLiteral("A")), film(QStringLiteral("b"), QStringLiteral("B"))};
    daemon.additions = {{QStringLiteral("a"), 2}, {QStringLiteral("b"), 1}};
    switch (variant) {
    case 0: daemon.additionGeneration = QStringLiteral("stale"); break;
    case 1: daemon.additionRevision = 2; break;
    case 2: daemon.additions[1].id = QStringLiteral("a"); break;
    case 3: daemon.additions[1].id = QStringLiteral("unknown"); break;
    case 4: daemon.additions[0].time = 0; break;
    case 5: daemon.additions[0].time = std::numeric_limits<qulonglong>::max(); break;
    case 6: daemon.page1[0].available = false; break;
    case 7: std::reverse(daemon.additions.begin(), daemon.additions.end()); break;
    case 8: daemon.additions = {{QStringLiteral("b"), 1}, {QStringLiteral("a"), 1}}; break;
    case 9: daemon.rejectAdditions = true; break;
    }
    QVERIFY(registerBus(daemon));
    LibraryCatalog catalog;
    QTRY_COMPARE(catalog.libraryState(), QStringLiteral("ready"));
    QCOMPARE(catalog.entries().size(), 2);
    QVERIFY(catalog.newEntries().isEmpty());
  }
  void additionsAreOptionalAndBounded() {
    LibraryDaemon daemon;
    daemon.collections = {{QStringLiteral("films"), QStringLiteral("Films"), QStringLiteral("film")}};
    for (int i = 0; i < 65; ++i) {
      const auto id = QStringLiteral("film%1").arg(i, 3, 10, QLatin1Char('0'));
      daemon.page1.append(film(id, id)); daemon.additions.append({id, 1});
    }
    QVERIFY(registerBus(daemon));
    LibraryCatalog catalog;
    QTRY_COMPARE(catalog.libraryState(), QStringLiteral("ready"));
    QCOMPARE(daemon.additionReads, 0);
    QVERIFY(catalog.newEntries().isEmpty());
    daemon.features.append(QStringLiteral("LibraryAdditions1"));
    catalog.refresh();
    QTRY_COMPARE(daemon.additionReads, 1);
    QTest::qWait(100);
    QCOMPARE(catalog.libraryState(), QStringLiteral("ready"));
    QVERIFY(catalog.newEntries().isEmpty());
  }
  void collectionTabsUseTheShellsNames() {
    // #431: a provider's collection title is not translated; the shell
    // names the contract's kinds itself.
    LibraryDaemon daemon;
    daemon.collections = {{QStringLiteral("films"), QStringLiteral("Movies"),
                           QStringLiteral("film")},
                          {QStringLiteral("shows"), QStringLiteral("TV"),
                           QStringLiteral("series")},
                          {QStringLiteral("rest"), QStringLiteral("Misc"),
                           QStringLiteral("other")}};
    daemon.page1 = {film(QStringLiteral("north"), QStringLiteral("North"))};
    QVERIFY(registerBus(daemon));
    LibraryCatalog catalog;
    QTRY_COMPARE(catalog.libraryState(), QStringLiteral("ready"));
    const auto tabs = catalog.tabs();
    QCOMPARE(tabs.size(), 3);
    QStringList titles;
    for (const auto &tab : tabs)
      titles << tab.toMap().value(QStringLiteral("title")).toString();
    QCOMPARE(titles, (QStringList{QStringLiteral("Films"), QStringLiteral("Series"),
                                  QStringLiteral("Other")}));
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
  // #247: Continue resumes a stopped film by its provider-local item id.
  void launchContentFindsTheItem() {
    LibraryDaemon daemon;
    daemon.collections = {{QStringLiteral("films"), QStringLiteral("Films"),
                           QStringLiteral("film")}};
    daemon.page1 = {film(QStringLiteral("north"), QStringLiteral("North"))};
    daemon.features.clear();
    QVERIFY(registerBus(daemon));
    LibraryCatalog catalog;
    QTRY_COMPARE(catalog.libraryState(), QStringLiteral("ready"));
    catalog.launchContent(QStringLiteral("library-provider"), QStringLiteral("missing"));
    QCOMPARE(catalog.operationState(), QStringLiteral("NotFound"));
    QCOMPARE(daemon.commands.size(), 0);
    catalog.launchContent(QStringLiteral("library-provider"), QStringLiteral("north"));
    QTRY_COMPARE(daemon.commands, QStringList({QStringLiteral("Launch")}));
    QCOMPARE(daemon.lastLocal, QStringLiteral("north"));
  }
  // Home can offer Continue before the first catalog read has finished.
  void launchContentWaitsForTheCatalog() {
    LibraryDaemon daemon;
    daemon.collections = {{QStringLiteral("films"), QStringLiteral("Films"),
                           QStringLiteral("film")}};
    daemon.page1 = {film(QStringLiteral("north"), QStringLiteral("North"))};
    daemon.features.clear();
    QVERIFY(registerBus(daemon));
    LibraryCatalog catalog;
    QVERIFY(catalog.libraryState() != QStringLiteral("ready"));
    catalog.launchContent(QStringLiteral("library-provider"), QStringLiteral("north"));
    QCOMPARE(catalog.operationState(), QStringLiteral("queued"));
    QTRY_COMPARE(daemon.commands, QStringList({QStringLiteral("Launch")}));
    QCOMPARE(daemon.lastLocal, QStringLiteral("north"));
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
  void unchangedReloadDoesNotNotify() {
    // Every ProvidersChanged reloads the catalog. An unchanged reload must not
    // announce catalogChanged: Home rebuilt its Continue cards on each (#441).
    LibraryDaemon daemon;
    daemon.collections = {{QStringLiteral("films"), QStringLiteral("Films"),
                           QStringLiteral("film")}};
    daemon.page1 = {film(QStringLiteral("north"), QStringLiteral("North"))};
    QVERIFY(registerBus(daemon));
    LibraryCatalog catalog;
    QTRY_COMPARE(catalog.libraryState(), QStringLiteral("ready"));
    QTest::qWait(100);
    QSignalSpy changes(&catalog, &LibraryCatalog::catalogChanged);
    const auto providersChanged = [] {
      return QDBusConnection::sessionBus().send(QDBusMessage::createSignal(
          QStringLiteral("/org/arinn/Daemon1"), QStringLiteral("org.arinn.Daemon1"),
          QStringLiteral("ProvidersChanged")));
    };
    for (int round = 1; round <= 3; ++round) {
      const auto reads = daemon.libraryReads;
      QVERIFY(providersChanged());
      QTRY_VERIFY(daemon.libraryReads > reads);
      QTest::qWait(50);
    }
    QCOMPARE(changes.count(), 0);
    daemon.page1[0].title = QStringLiteral("Changed");
    QVERIFY(providersChanged());
    QTRY_COMPARE(changes.count(), 1);
    QCOMPARE(catalog.entries().at(0).toMap().value(QStringLiteral("title")).toString(),
             QStringLiteral("Changed"));
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
  void homeNewOpensLibraryDetailAndBackRestoresItsCard_data() {
    QTest::addColumn<bool>("keepNeighbor");
    QTest::newRow("same-row-neighbor") << true;
    QTest::newRow("previous-row-fallback") << false;
  }
  void homeNewOpensLibraryDetailAndBackRestoresItsCard() {
    QFETCH(bool, keepNeighbor);
    LibraryDaemon daemon;
    daemon.features.append(QStringLiteral("LibraryAdditions1"));
    daemon.collections = {{QStringLiteral("films"), QStringLiteral("Films"), QStringLiteral("film")}};
    daemon.page1 = {film(QStringLiteral("north"), QStringLiteral("North")), film(QStringLiteral("south"), QStringLiteral("South"))};
    daemon.additions = {{QStringLiteral("north"), 10}, {QStringLiteral("south"), 9}};
    QVERIFY(registerBus(daemon));
    ShellStandIn backend;
    backend.m_continue = {QVariantMap{{QStringLiteral("id"), QStringLiteral("continue")}, {QStringLiteral("title"), QStringLiteral("Activity")}}};
    SessionGuardStandIn guard;
    QQmlApplicationEngine engine;
    installAssetImageProvider(&engine);
    engine.rootContext()->setContextProperty(QStringLiteral("backend"), &backend);
    engine.rootContext()->setContextProperty(QStringLiteral("sessionGuard"), &guard);
    engine.load(QUrl::fromLocalFile(QStringLiteral(ARINN_MAIN_QML)));
    auto window = qobject_cast<QQuickWindow *>(engine.rootObjects().value(0));
    QVERIFY(window);
    window->show(); window->requestActivate();
    auto home = window->findChild<QQuickItem *>(QStringLiteral("productionHome"));
    auto library = window->findChild<QQuickItem *>(QStringLiteral("productionLibrary"));
    auto catalog = window->findChild<LibraryCatalog *>(QStringLiteral("libraryCatalog"));
    QVERIFY(home); QVERIFY(library); QVERIFY(catalog);
    QTRY_COMPARE(catalog->newEntries().size(), 2);
    const auto id = catalog->newEntries().at(0).toMap().value(QStringLiteral("id")).toString();
    QQuickItem *card = nullptr;
    QTRY_VERIFY((card = findItem(home, QStringLiteral("entry-") + id)));
    card->forceActiveFocus();
    QTRY_VERIFY(card->hasActiveFocus());
    QTest::keyClick(window, Qt::Key_Return);
    QTRY_COMPARE(home->property("screen").toString(), QStringLiteral("library"));
    QCOMPARE(library->property("detailEntryId").toString(), id);
    QVERIFY(daemon.commands.isEmpty()); // Opening details never starts playback.
    QTest::keyClick(window, Qt::Key_Escape);
    QTRY_COMPARE(home->property("screen").toString(), QStringLiteral("home"));
    QTRY_VERIFY(card->hasActiveFocus());
    // Normal library navigation retains its own Back behaviour.
    home->setProperty("screen", QStringLiteral("library"));
    QVERIFY(QMetaObject::invokeMethod(library, "openEntry", Q_ARG(QVariant, QVariant(id))));
    QVERIFY(QMetaObject::invokeMethod(library, "back"));
    QCOMPARE(home->property("screen").toString(), QStringLiteral("library"));
    home->setProperty("screen", QStringLiteral("home"));
    card->forceActiveFocus();
    QTest::keyClick(window, Qt::Key_Return);
    QTRY_VERIFY(library->property("detailOpen").toBool());
    daemon.additions = keepNeighbor ? QList<AdditionWire>{{QStringLiteral("south"), 9}} : QList<AdditionWire>{};
    catalog->refresh();
    QTRY_COMPARE(catalog->newEntries().size(), keepNeighbor ? 1 : 0);
    QCOMPARE(catalog->libraryState(), QStringLiteral("ready"));
    QTest::keyClick(window, Qt::Key_Escape);
    QTRY_COMPARE(home->property("screen").toString(), QStringLiteral("home"));
    const auto fallback = keepNeighbor ? presentationId(QStringLiteral("library-provider"), QStringLiteral("south")) : QStringLiteral("arinn-browse");
    QQuickItem *next = nullptr;
    QTRY_VERIFY((next = findItem(home, QStringLiteral("entry-") + fallback)));
    QTRY_VERIFY(next->hasActiveFocus());
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
  // #233: the Downloads route shows Downloads1, commands go to the daemon,
  // and Watch opens the imported film's Detail in the library.
  void downloadsScreenWatchesTheImportedFilm() {
    LibraryDaemon daemon;
    daemon.collections = {{QStringLiteral("films"), QStringLiteral("Films"),
                           QStringLiteral("film")}};
    daemon.page1 = {film(QStringLiteral("north"), QStringLiteral("North"))};
    ProviderWire downloads;
    downloads.id = QStringLiteral("dl-provider");
    downloads.name = QStringLiteral("downloads");
    downloads.path = QDBusObjectPath(QStringLiteral("/org/arinn/Daemon1/providers/p_dl"));
    downloads.capabilities = {QStringLiteral("download")};
    downloads.generation = QStringLiteral("7c1d2e3f-4a5b-4c6d-8e9f-0a1b2c3d4e5f");
    daemon.providers.append(downloads);
    DownloadWire done;
    done.id = QStringLiteral("done");
    done.title = QStringLiteral("North");
    done.state = QStringLiteral("completed");
    done.importState = QStringLiteral("imported");
    done.refs = {{QStringLiteral("library-provider"), QStringLiteral("library"),
                  QStringLiteral("north")}};
    DownloadWire running;
    running.id = QStringLiteral("running");
    running.title = QStringLiteral("South");
    running.state = QStringLiteral("downloading");
    running.actions = {QStringLiteral("pause")};
    daemon.downloads = {done, running};
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
    auto view =
        window->findChild<QQuickItem *>(QStringLiteral("productionDownloads"));
    QVERIFY(home);
    QVERIFY(library);
    QVERIFY(view);
    QCOMPARE(view->property("visible").toBool(), false);
    QTRY_COMPARE(library->property("libraryState").toString(),
                 QStringLiteral("ready"));
    QTRY_COMPARE(view->property("viewState").toString(),
                 QStringLiteral("populated"));
    QCOMPARE(view->property("jobs").toList().size(), 2);
    auto libraryCatalog = window->findChild<LibraryCatalog *>(QStringLiteral("libraryCatalog"));
    QVERIFY(libraryCatalog);
    const auto northPoster = libraryCatalog->posterFor(
        QStringLiteral("library-provider"), QStringLiteral("north"));
    QVERIFY(northPoster.startsWith(QStringLiteral("image://arinn-assets/")));
    // #360: the imported download carries its library item's poster; the
    // running one has none and keeps the view's plate.
    const auto artworkOf = [&](const QString &local) {
      for (const auto &job : view->property("jobs").toList())
        if (job.toMap().value(QStringLiteral("id")) == downloadId(local))
          return job.toMap().value(QStringLiteral("artwork")).toString();
      return QStringLiteral("<no row>");
    };
    QTRY_COMPARE(artworkOf(QStringLiteral("done")), northPoster);
    QCOMPARE(artworkOf(QStringLiteral("running")), QString());

    home->setProperty("screen", QStringLiteral("downloads"));
    QTRY_COMPARE(view->property("visible").toBool(), true);
    const auto doneId =
        downloadId( QStringLiteral("done"));
    const auto runningId =
        downloadId( QStringLiteral("running"));
    {
      auto slot = findItem(view, QStringLiteral("artwork-") + doneId);
      QVERIFY(slot);
      QCOMPARE(slot->property("source").toUrl(), QUrl(northPoster));
      auto plain = findItem(view, QStringLiteral("artwork-") + runningId);
      QVERIFY(plain);
      QVERIFY(plain->property("source").toUrl().isEmpty());
    }
    // Rows are looked up on every check: a catalog reload replaces the
    // jobs array and the Repeater recreates its delegates.
    const auto focused = [&](const QString &id) {
      auto row = findItem(view, QStringLiteral("job-") + id);
      return row && row->hasActiveFocus();
    };
    QTRY_VERIFY(focused(doneId));

    QTest::keyClick(window, Qt::Key_Down);
    QTRY_VERIFY(focused(runningId));
    QTest::keyClick(window, Qt::Key_Return);
    QTRY_COMPARE(daemon.commands, QStringList{QStringLiteral("Pause")});
    QCOMPARE(daemon.lastLocal, QStringLiteral("running"));

    QTest::keyClick(window, Qt::Key_Up);
    QTRY_VERIFY(focused(doneId));
    QTest::keyClick(window, Qt::Key_Return);
    QTRY_COMPARE(home->property("screen").toString(), QStringLiteral("library"));
    QTRY_COMPARE(library->property("detailOpen").toBool(), true);
    QCOMPARE(library->property("detailEntryId").toString(),
             presentationId(QStringLiteral("library-provider"),
                            QStringLiteral("north")));
    QTRY_VERIFY(focusInside(window->activeFocusItem(), library));

    // Review of #310: Watch before the library lists the item opens its
    // Detail once a later read lists it.
    QVERIFY(QMetaObject::invokeMethod(library, "closeDetail"));
    QTRY_COMPARE(library->property("detailOpen").toBool(), false);
    DownloadWire late;
    late.id = QStringLiteral("late");
    late.title = QStringLiteral("West");
    late.state = QStringLiteral("completed");
    late.importState = QStringLiteral("imported");
    late.refs = {{QStringLiteral("library-provider"), QStringLiteral("library"),
                  QStringLiteral("west")}};
    daemon.downloads = {late, done, running};
    home->setProperty("screen", QStringLiteral("downloads"));
    const auto lateId =
        downloadId( QStringLiteral("late"));
    auto catalog = window->findChild<QObject *>(QStringLiteral("downloadCatalog"));
    QVERIFY(catalog);
    QVERIFY(QMetaObject::invokeMethod(catalog, "refresh"));
    QTRY_COMPARE(view->property("jobs").toList().size(), 3);
    // Not in the library yet: no poster.
    QCOMPARE(artworkOf(QStringLiteral("late")), QString());
    // The view keeps the selected download; the new one is above it.
    QTRY_VERIFY(focused(doneId));
    QTest::keyClick(window, Qt::Key_Up);
    QTRY_VERIFY(focused(lateId));
    // Leaving the library before the item appears drops the request.
    QTest::keyClick(window, Qt::Key_Return);
    QTRY_COMPARE(home->property("screen").toString(), QStringLiteral("library"));
    QVERIFY(!window->property("pendingWatch").isNull());
    home->setProperty("screen", QStringLiteral("home"));
    home->setProperty("screen", QStringLiteral("library"));
    QVERIFY(window->property("pendingWatch").isNull());
    daemon.page1.append(film(QStringLiteral("west"), QStringLiteral("West")));
    QVERIFY(QMetaObject::invokeMethod(libraryCatalog, "refresh"));
    QTest::qWait(300);
    QCOMPARE(library->property("detailOpen").toBool(), false);
    // Once the library lists it, the row follows with the poster.
    QTRY_COMPARE(artworkOf(QStringLiteral("late")),
                 libraryCatalog->posterFor(QStringLiteral("library-provider"),
                                           QStringLiteral("west")));
    QVERIFY(!artworkOf(QStringLiteral("late")).isEmpty());
    // With the item listed, Watch opens it directly.
    home->setProperty("screen", QStringLiteral("downloads"));
    QTRY_VERIFY(focused(lateId));
    QTest::keyClick(window, Qt::Key_Return);
    QTRY_COMPARE(home->property("screen").toString(), QStringLiteral("library"));
    QTRY_COMPARE(library->property("detailEntryId").toString(),
                 presentationId(QStringLiteral("library-provider"),
                                QStringLiteral("west")));

    // #341: a Detail open already scheduled for a listed item does not
    // survive leaving the library before it runs, even after returning.
    // The signal is emitted directly: a key click would also run the
    // deferred callback before the test could leave.
    QVERIFY(QMetaObject::invokeMethod(library, "closeDetail"));
    QTRY_COMPARE(library->property("detailOpen").toBool(), false);
    QVERIFY(QMetaObject::invokeMethod(view, "watchRequested", Q_ARG(QString, lateId)));
    QCOMPARE(home->property("screen").toString(), QStringLiteral("library"));
    home->setProperty("screen", QStringLiteral("home"));
    home->setProperty("screen", QStringLiteral("library"));
    QTest::qWait(200);
    QCOMPARE(library->property("detailOpen").toBool(), false);

    // Back from Downloads goes Home.
    home->setProperty("screen", QStringLiteral("downloads"));
    QTRY_VERIFY(focusInside(window->activeFocusItem(), view));
    QTest::keyClick(window, Qt::Key_Escape);
    QTRY_COMPARE(home->property("screen").toString(), QStringLiteral("home"));
  }
  void hiddenLibraryLeavesHomeFocusAlone() {
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

    // A hidden screen cannot take focus at all (ADR-0076).
    QVERIFY(QMetaObject::invokeMethod(library, "focusInitial"));
    QTest::qWait(50);
    QCOMPARE(moves.count(), 0);
    QVERIFY(menu->hasActiveFocus());

    // Focus that still falls to the window's root item, here because the
    // focused item is destroyed, returns to the screen on display, so the
    // remote keeps working.
    auto stray = new QQuickItem(window->contentItem());
    stray->forceActiveFocus();
    QVERIFY(stray->hasActiveFocus());
    delete stray;
    QTRY_VERIFY(focusInside(window->activeFocusItem(), home));
    QVERIFY(window->activeFocusItem() != window->contentItem());
  }
  // #608: Library and Downloads stay visible and enabled under Home's
  // navigation menu, so a data update must not pull focus out of the menu.
  void dataUpdatesUnderTheMenuKeepItsFocus() {
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
    auto downloads =
        window->findChild<QQuickItem *>(QStringLiteral("productionDownloads"));
    QVERIFY(home);
    QVERIFY(library);
    QVERIFY(downloads);
    QTRY_COMPARE(library->property("libraryState").toString(),
                 QStringLiteral("ready"));

    // Library: a catalogue change under the open menu.
    home->setProperty("screen", QStringLiteral("library"));
    QTRY_VERIFY(focusInside(window->activeFocusItem(), library));
    QTest::keyClick(window, Qt::Key_Menu);
    QTRY_COMPARE(home->property("navOpen").toBool(), true);
    auto libraryEntry = findItem(home, QStringLiteral("nav-1"));
    QVERIFY(libraryEntry);
    QTRY_VERIFY(libraryEntry->hasActiveFocus());
    QVERIFY(library->isVisible());
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
    QVERIFY(libraryEntry->hasActiveFocus());
    QTest::keyClick(window, Qt::Key_Escape);
    QTRY_COMPARE(home->property("navOpen").toBool(), false);
    QTRY_VERIFY(focusInside(window->activeFocusItem(), library));

    // Downloads: a queue update under the open menu.
    home->setProperty("screen", QStringLiteral("downloads"));
    QTRY_VERIFY(focusInside(window->activeFocusItem(), downloads));
    QTest::keyClick(window, Qt::Key_Menu);
    QTRY_COMPARE(home->property("navOpen").toBool(), true);
    auto navigation = findItem(home, QStringLiteral("navigation"));
    QVERIFY(navigation);
    QTRY_VERIFY(focusInside(window->activeFocusItem(), navigation));
    auto downloadsEntry = window->activeFocusItem();
    QVERIFY(downloadsEntry);
    QVERIFY(downloads->isVisible());
    const QVariantMap job{{QStringLiteral("id"), QStringLiteral("job-1")},
                          {QStringLiteral("title"), QStringLiteral("Feature")},
                          {QStringLiteral("state"), QStringLiteral("downloading")},
                          {QStringLiteral("progress"), 0.5},
                          {QStringLiteral("importState"), QStringLiteral("pending")}};
    downloads->setProperty("viewState", QStringLiteral("populated"));
    downloads->setProperty("jobs", QVariantList{job});
    QTest::qWait(100);
    QVERIFY(downloadsEntry->hasActiveFocus());
    QVariantMap progressed = job;
    progressed.insert(QStringLiteral("progress"), 0.6);
    downloads->setProperty("jobs", QVariantList{progressed});
    QTest::qWait(100);
    QVERIFY(downloadsEntry->hasActiveFocus());

    // Closing the menu hands focus back to the queue row.
    QTest::keyClick(window, Qt::Key_Escape);
    QTRY_COMPARE(home->property("navOpen").toBool(), false);
    auto row = findItem(downloads, QStringLiteral("job-job-1"));
    QVERIFY(row);
    QTRY_VERIFY(row->hasActiveFocus());
  }

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
  // #329: Menu opens the side navigation over Library and Settings, drawn
  // above them; Escape returns to the screen; Home from Settings goes Home.
  void menuOpensNavigationOverSectionScreens() {
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
    auto settings =
        window->findChild<QQuickItem *>(QStringLiteral("productionSettings"));
    QVERIFY(home);
    QVERIFY(library);
    QVERIFY(settings);
    auto navigation = findItem(home, QStringLiteral("navigation"));
    auto menuButton = findItem(home, QStringLiteral("menuButton"));
    QVERIFY(navigation);
    QVERIFY(menuButton);
    QTRY_COMPARE(library->property("libraryState").toString(),
                 QStringLiteral("ready"));
    home->setProperty("screen", QStringLiteral("library"));
    QTRY_VERIFY(focusInside(window->activeFocusItem(), library));

    QTest::keyClick(window, Qt::Key_Menu);
    QTRY_COMPARE(home->property("navOpen").toBool(), true);
    auto libraryItem = findItem(home, QStringLiteral("nav-1"));
    QVERIFY(libraryItem);
    QTRY_VERIFY(libraryItem->hasActiveFocus());
    // The menu is drawn above the library, without Home's own content.
    QVERIFY(navigation->isVisible());
    QVERIFY(home->z() > library->z());
    QVERIFY(library->isVisible());
    QVERIFY(!menuButton->isVisible());
    QTest::keyClick(window, Qt::Key_Escape);
    QTRY_COMPARE(home->property("navOpen").toBool(), false);
    QTRY_VERIFY(focusInside(window->activeFocusItem(), library));
    QVERIFY(home->z() < library->z());

    // From the menu to Settings, then Home from Settings.
    QTest::keyClick(window, Qt::Key_Menu);
    QTRY_VERIFY(libraryItem->hasActiveFocus());
    auto settingsItem = findItem(home, QStringLiteral("nav-4"));
    QVERIFY(settingsItem);
    for (int i = 0; i < 3; ++i)
      QTest::keyClick(window, Qt::Key_Down);
    QTRY_VERIFY(settingsItem->hasActiveFocus());
    QTest::keyClick(window, Qt::Key_Return);
    QTRY_COMPARE(home->property("screen").toString(), QStringLiteral("settings"));
    QTRY_VERIFY(focusInside(window->activeFocusItem(), settings));
    QTest::keyClick(window, Qt::Key_Menu);
    QTRY_COMPARE(home->property("navOpen").toBool(), true);
    QTest::keyClick(window, Qt::Key_Escape);
    QTRY_VERIFY(focusInside(window->activeFocusItem(), settings));
    QTest::keyClick(window, Qt::Key_Home);
    QTRY_COMPARE(home->property("screen").toString(), QStringLiteral("home"));
    QTRY_VERIFY(focusInside(window->activeFocusItem(), home));
    QVERIFY(menuButton->isVisible());

    // Downloads is a section screen too: Menu opens navigation over it and
    // Home leaves it.
    auto downloads = window->findChild<QQuickItem *>(QStringLiteral("productionDownloads"));
    QVERIFY(downloads);
    home->setProperty("screen", QStringLiteral("downloads"));
    QTRY_VERIFY(focusInside(window->activeFocusItem(), downloads));
    QTest::keyClick(window, Qt::Key_Menu);
    QTRY_COMPARE(home->property("navOpen").toBool(), true);
    QVERIFY(navigation->isVisible());
    QVERIFY(home->z() > downloads->z());
    QTest::keyClick(window, Qt::Key_Escape);
    QTRY_VERIFY(focusInside(window->activeFocusItem(), downloads));
    QTest::keyClick(window, Qt::Key_Home);
    QTRY_COMPARE(home->property("screen").toString(), QStringLiteral("home"));

    // Review of #334: Home from the focused Browse address field and from an
    // open Settings editor still reaches Home.
    auto browse = window->findChild<QQuickItem *>(QStringLiteral("productionBrowse"));
    QVERIFY(browse);
    browse->setProperty("keyboardAvailable", true);
    browse->setProperty("browserUnavailable", false);
    window->setProperty("browseOpen", true);
    auto field = findItem(browse, QStringLiteral("browseInput"));
    QVERIFY(field);
    QTRY_VERIFY(browse->isVisible());
    QVERIFY(QMetaObject::invokeMethod(browse, "focusInitial"));
    QTRY_VERIFY(field->hasActiveFocus());
    QTest::keyClick(window, Qt::Key_Home);
    QTRY_VERIFY(!window->property("browseOpen").toBool());
    QTRY_COMPARE(home->property("screen").toString(), QStringLiteral("home"));

    settings->setProperty("screenState", QStringLiteral("ready"));
    settings->setProperty("categories", QVariantList{QVariantMap{
        {QStringLiteral("id"), QStringLiteral("system")},
        {QStringLiteral("title"), QStringLiteral("System")},
        {QStringLiteral("icon"), QStringLiteral("memory")}}});
    settings->setProperty("rows", QVariantList{QVariantMap{
        {QStringLiteral("providerId"), QStringLiteral("shell")},
        {QStringLiteral("providerName"), QStringLiteral("Shell")},
        {QStringLiteral("id"), QStringLiteral("idle")},
        {QStringLiteral("categoryId"), QStringLiteral("system")},
        {QStringLiteral("title"), QStringLiteral("Idle minutes")},
        {QStringLiteral("description"), QString()},
        {QStringLiteral("type"), QStringLiteral("integer")},
        {QStringLiteral("currentValue"), 10},
        {QStringLiteral("constraints"), QVariantMap{{QStringLiteral("min"), 1},
                                                    {QStringLiteral("max"), 60}}},
        {QStringLiteral("enabled"), true}}});
    home->setProperty("screen", QStringLiteral("settings"));
    auto category = findItem(settings, QStringLiteral("category-system"));
    QVERIFY(category);
    QTRY_VERIFY(category->hasActiveFocus());
    QTest::keyClick(window, Qt::Key_Right);
    QTest::keyClick(window, Qt::Key_Return);
    QTRY_COMPARE(settings->property("editingKey").toString(), QStringLiteral("shell-idle"));
    QTest::keyClick(window, Qt::Key_Home);
    QTRY_COMPARE(home->property("screen").toString(), QStringLiteral("home"));
    QCOMPARE(settings->property("editingKey").toString(), QString());

    // Review of #334: Menu opens the navigation over Open services, Browse and
    // a session; Escape returns to the screen, and a section picked in the
    // menu replaces the overlay.
    for (const auto &[flag, name] :
         {std::pair{"browserOpen", "productionBrowser"},
          std::pair{"browseOpen", "productionBrowse"},
          std::pair{"sessionOpen", "productionSession"}}) {
      auto screen = window->findChild<QQuickItem *>(QString::fromLatin1(name));
      QVERIFY2(screen, name);
      home->setProperty("screen", QStringLiteral("home"));
      window->setProperty(flag, true);
      QTRY_VERIFY2(focusInside(window->activeFocusItem(), screen), name);
      QTest::keyClick(window, Qt::Key_Menu);
      QTRY_VERIFY2(home->property("navOpen").toBool(), name);
      // The menu focuses the entry of the current section.
      const auto onMenu = [&] {
        auto item = window->activeFocusItem();
        return item && item->objectName().startsWith(QStringLiteral("nav-")) &&
               focusInside(item, navigation);
      };
      QTRY_VERIFY2(onMenu(), name);
      QVERIFY2(navigation->isVisible() && home->z() > screen->z(), name);
      QTest::keyClick(window, Qt::Key_Escape);
      QTRY_VERIFY2(!home->property("navOpen").toBool(), name);
      QTRY_VERIFY2(focusInside(window->activeFocusItem(), screen), name);
      QVERIFY2(window->property(flag).toBool(), name);
      QTest::keyClick(window, Qt::Key_Menu);
      QTRY_VERIFY2(onMenu(), name);
      auto libraryEntry = findItem(home, QStringLiteral("nav-1"));
      QVERIFY(libraryEntry);
      for (int i = 0; i < 6 && !libraryEntry->hasActiveFocus(); ++i)
        QTest::keyClick(window, Qt::Key_Up);
      if (!libraryEntry->hasActiveFocus())
        QTest::keyClick(window, Qt::Key_Down);
      QTRY_VERIFY2(libraryEntry->hasActiveFocus(), name);
      QTest::keyClick(window, Qt::Key_Return);
      QTRY_VERIFY2(!window->property(flag).toBool(), name);
      QTRY_COMPARE(home->property("screen").toString(), QStringLiteral("library"));
      QTRY_VERIFY2(focusInside(window->activeFocusItem(), library), name);

      // Review of #334: Open services picked in the menu replaces the overlay
      // and takes focus.
      window->setProperty(flag, true);
      QTRY_VERIFY2(focusInside(window->activeFocusItem(), screen), name);
      QTest::keyClick(window, Qt::Key_Menu);
      QTRY_VERIFY2(onMenu(), name);
      auto servicesEntry = findItem(home, QStringLiteral("nav-5"));
      QVERIFY(servicesEntry);
      for (int i = 0; i < 8 && !servicesEntry->hasActiveFocus(); ++i)
        QTest::keyClick(window, Qt::Key_Down);
      QTRY_VERIFY2(servicesEntry->hasActiveFocus(), name);
      QTest::keyClick(window, Qt::Key_Return);
      auto browser = window->findChild<QQuickItem *>(QStringLiteral("productionBrowser"));
      QTRY_VERIFY2(window->property("browserOpen").toBool(), name);
      QTRY_COMPARE(window->property("focusedScreen").toString(), QStringLiteral("browser"));
      QTRY_VERIFY2(focusInside(window->activeFocusItem(), browser), name);
      if (QByteArray(flag) != "browserOpen")
        QVERIFY2(!window->property(flag).toBool(), name);
      window->setProperty("browserOpen", false);

      // Review of #334: Home in the menu over the overlay shows Home.
      window->setProperty(flag, true);
      QTRY_VERIFY2(focusInside(window->activeFocusItem(), screen), name);
      QTest::keyClick(window, Qt::Key_Menu);
      QTRY_VERIFY2(onMenu(), name);
      QTest::keyClick(window, Qt::Key_Home);
      QTRY_VERIFY2(!window->property(flag).toBool(), name);
      QTRY_COMPARE(home->property("screen").toString(), QStringLiteral("home"));
      QTRY_COMPARE(window->property("focusedScreen").toString(), QStringLiteral("home"));
      QTRY_VERIFY2(focusInside(window->activeFocusItem(), home), name);
      // A visible, enabled Home control holds focus, not the closed menu.
      auto focusedItem = window->activeFocusItem();
      QVERIFY2(focusedItem && focusedItem->isVisible() && focusedItem->isEnabled() &&
                   !focusInside(focusedItem, navigation),
               name);
    }

    // Home from Open services, Browse and a session also shows Home.
    for (const auto &[flag, name] :
         {std::pair{"browserOpen", "productionBrowser"},
          std::pair{"browseOpen", "productionBrowse"},
          std::pair{"sessionOpen", "productionSession"}}) {
      auto screen = window->findChild<QQuickItem *>(QString::fromLatin1(name));
      QVERIFY2(screen, name);
      home->setProperty("screen", QStringLiteral("library"));
      window->setProperty(flag, true);
      QTRY_VERIFY2(focusInside(window->activeFocusItem(), screen), name);
      QTest::keyClick(window, Qt::Key_Home);
      QTRY_VERIFY2(!window->property(flag).toBool(), name);
      QTRY_COMPARE(home->property("screen").toString(), QStringLiteral("home"));
      QTRY_VERIFY2(focusInside(window->activeFocusItem(), home), name);
    }
  }
  // #247: Continue on a stopped, library-linked film launches the item through
  // the library instead of opening the stopped session.
  void continueResumesAStoppedFilmThroughTheLibrary() {
    LibraryDaemon daemon;
    daemon.holdOperation = true;
    daemon.collections = {{QStringLiteral("films"), QStringLiteral("Films"),
                           QStringLiteral("film")}};
    daemon.page1 = {film(QStringLiteral("north"), QStringLiteral("North"))};
    ProviderWire media;
    media.id = QStringLiteral("media-provider");
    media.name = QStringLiteral("media");
    media.path = QDBusObjectPath(QStringLiteral("/org/arinn/Daemon1/providers/p_media"));
    media.capabilities = {QStringLiteral("media")};
    media.generation = QStringLiteral("g-media");
    daemon.providers.append(media);
    SessionWire stopped;
    stopped.id = QStringLiteral("old");
    stopped.title = QStringLiteral("North");
    stopped.state = QStringLiteral("stopped");
    stopped.hasContent = true;
    stopped.content = {QStringLiteral("library-provider"), QStringLiteral("library"),
                       QStringLiteral("north")};
    SessionWire unknown = stopped;
    unknown.id = QStringLiteral("gone");
    unknown.title = QStringLiteral("Gone");
    unknown.content.local = QStringLiteral("missing");
    SessionWire resumed = stopped;
    resumed.id = QStringLiteral("new");
    resumed.state = QStringLiteral("playing");
    resumed.actions = {QStringLiteral("pause")};
    daemon.sessions = {stopped, unknown, resumed};
    daemon.operationEntities = {{QStringLiteral("media-provider"),
                                 QStringLiteral("media"), QStringLiteral("new")}};
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

    // A film that is no longer in the library reports why on Home.
    const auto goneId = presentationId(QStringLiteral("media-provider"),
                                       QStringLiteral("gone"));
    QTRY_VERIFY(findItem(home, QStringLiteral("entry-") + goneId));
    findItem(home, QStringLiteral("entry-") + goneId)->forceActiveFocus();
    QTest::keyClick(window, Qt::Key_Return);
    auto feedback = window->findChild<QQuickItem *>(QStringLiteral("feedback"));
    QVERIFY(feedback);
    QTRY_COMPARE(feedback->property("text").toString(),
                 QStringLiteral("That title is not in the library."));
    QCOMPARE(daemon.commands.size(), 0);
    QCOMPARE(window->property("sessionOpen").toBool(), false);

    const auto mediaId = presentationId(QStringLiteral("media-provider"),
                                        QStringLiteral("old"));
    auto card = findItem(home, QStringLiteral("entry-") + mediaId);
    QVERIFY(card);
    card->forceActiveFocus();
    QTest::keyClick(window, Qt::Key_Return);
    QTRY_COMPARE(daemon.commands, QStringList({QStringLiteral("Launch")}));
    QCOMPARE(daemon.lastLocal, QStringLiteral("north"));
    // The stopped session itself is never opened or presented.
    QCOMPARE(window->property("sessionOpen").toBool(), false);
    QCOMPARE(window->property("playerOpen").toBool(), false);

    // The new session opens, and leaving it returns to Home, where the
    // resume started (§6.4), not to the library.
    daemon.holdOperation = false;
    QTRY_COMPARE(window->property("sessionOpen").toBool(), true);
    QCOMPARE(window->property("sessionFromLibrary").toBool(), false);
    QTest::keyClick(window, Qt::Key_Escape);
    QTRY_COMPARE(window->property("sessionOpen").toBool(), false);
    QCOMPARE(home->property("screen").toString(), QStringLiteral("home"));
  }
  // #365: Continue shows the backdrop of the item a session plays: its
  // fanart, else its poster (an episode's still), else its series' artwork.
  void artworkForPrefersFanartAndFallsBackToTheSeries() {
    LibraryDaemon daemon;
    daemon.collections = {{QStringLiteral("films"), QStringLiteral("Films"),
                           QStringLiteral("film")},
                          {QStringLiteral("shows"), QStringLiteral("Series"),
                           QStringLiteral("series")}};
    auto posterOnly = film(QStringLiteral("poster"), QStringLiteral("Poster"));
    posterOnly.fanart.clear();
    auto bare = film(QStringLiteral("bare"), QStringLiteral("Bare"));
    bare.poster.clear();
    bare.fanart.clear();
    daemon.page1 = {film(QStringLiteral("north"), QStringLiteral("North")), posterOnly,
                    bare, showItem(), episodeItem(QStringLiteral("ep1"), QStringLiteral("Pilot"), 1, 1),
                    episodeItem(QStringLiteral("ep2"), QStringLiteral("Second"), 1, 2)};
    daemon.page1.last().poster = QStringLiteral("still-not-fetched");
    QVERIFY(registerBus(daemon));
    LibraryCatalog catalog;
    QTRY_COMPARE(catalog.libraryState(), QStringLiteral("ready"));
    QHash<QString, QVariantMap> byTitle;
    for (const auto &entry : catalog.entries())
      byTitle.insert(entry.toMap().value(QStringLiteral("title")).toString(), entry.toMap());
    const auto provider = QStringLiteral("library-provider");
    const auto north = catalog.artworkFor(provider, QStringLiteral("north"));
    QVERIFY(north.startsWith(QStringLiteral("image://arinn-assets/")));
    QCOMPARE(north, byTitle.value(QStringLiteral("North")).value(QStringLiteral("fanart")).toString());
    QCOMPARE(catalog.artworkFor(provider, QStringLiteral("poster")),
             byTitle.value(QStringLiteral("Poster")).value(QStringLiteral("artwork")).toString());
    QCOMPARE(catalog.artworkFor(provider, QStringLiteral("ep1")),
             byTitle.value(QStringLiteral("Serial")).value(QStringLiteral("fanart")).toString());
    // An episode's own still wins over the series' fanart.
    QString still;
    for (const auto &episode : byTitle.value(QStringLiteral("Serial")).value(QStringLiteral("episodes")).toList())
      if (episode.toMap().value(QStringLiteral("title")) == QStringLiteral("Second"))
        still = episode.toMap().value(QStringLiteral("artwork")).toString();
    QVERIFY(!still.isEmpty());
    QCOMPARE(catalog.artworkFor(provider, QStringLiteral("ep2")), still);
    QVERIFY(catalog.artworkFor(provider, QStringLiteral("bare")).isEmpty());
    QVERIFY(catalog.artworkFor(provider, QStringLiteral("missing")).isEmpty());
    QVERIFY(catalog.artworkFor(QStringLiteral("other"), QStringLiteral("north")).isEmpty());
  }
  // #360: Downloads shows the poster of the entry Watch opens.
  void posterForUsesTheTopLevelEntrysPoster() {
    LibraryDaemon daemon;
    daemon.collections = {{QStringLiteral("films"), QStringLiteral("Films"),
                           QStringLiteral("film")},
                          {QStringLiteral("shows"), QStringLiteral("Series"),
                           QStringLiteral("series")}};
    auto fanartOnly = film(QStringLiteral("fanart"), QStringLiteral("Fanart"));
    fanartOnly.poster.clear();
    daemon.page1 = {film(QStringLiteral("north"), QStringLiteral("North")), fanartOnly,
                    showItem(), episodeItem(QStringLiteral("ep1"), QStringLiteral("Pilot"), 1, 1)};
    daemon.page1.last().poster = QStringLiteral("still-not-fetched");
    QVERIFY(registerBus(daemon));
    LibraryCatalog catalog;
    QTRY_COMPARE(catalog.libraryState(), QStringLiteral("ready"));
    QHash<QString, QVariantMap> byTitle;
    for (const auto &entry : catalog.entries())
      byTitle.insert(entry.toMap().value(QStringLiteral("title")).toString(), entry.toMap());
    const auto provider = QStringLiteral("library-provider");
    const auto north = catalog.posterFor(provider, QStringLiteral("north"));
    QVERIFY(north.startsWith(QStringLiteral("image://arinn-assets/")));
    QCOMPARE(north, byTitle.value(QStringLiteral("North")).value(QStringLiteral("artwork")).toString());
    QVERIFY(north != byTitle.value(QStringLiteral("North")).value(QStringLiteral("fanart")).toString());
    // An episode resolves to its series' poster, not its own still.
    const auto pilot = catalog.posterFor(provider, QStringLiteral("ep1"));
    QCOMPARE(pilot, byTitle.value(QStringLiteral("Serial")).value(QStringLiteral("artwork")).toString());
    QVERIFY(!pilot.isEmpty());
    // No poster, no invented artwork: the fanart is not used.
    QVERIFY(catalog.posterFor(provider, QStringLiteral("fanart")).isEmpty());
    QVERIFY(catalog.posterFor(provider, QStringLiteral("missing")).isEmpty());
    QVERIFY(catalog.posterFor(QStringLiteral("other"), QStringLiteral("north")).isEmpty());
    QVERIFY(catalog.posterFor(provider, QString()).isEmpty());
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
