// SPDX-FileCopyrightText: 2026 Arinn contributors
//
// SPDX-License-Identifier: GPL-3.0-or-later

#include "asset_images.h"
#include "media_catalog.h"
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QJSEngine>
#include <QGuiApplication>
#include <QQmlApplicationEngine>
#include <QQmlContext>
#include <QQuickItem>
#include <QQuickWindow>
#include <QSignalSpy>
#include <QScopeGuard>
#include <QTest>
#include <QtDBus>
#include <time.h>
// QObject wrappers exercise the actual Qt engine's native type observation.
// The compositor classes have these Q_OBJECT names; no invented property.
namespace KWin {
class TestWindow : public QObject {
  Q_OBJECT
  Q_PROPERTY(QString resourceClass MEMBER appId)
  Q_PROPERTY(QString resourceName MEMBER instance)
public:
  QString instance;
  QString appId = QStringLiteral("org.arinn.Playback.aaaaaaaa-0000-4000-8000-000000000001");
};
class XdgToplevelWindow final : public TestWindow { Q_OBJECT };
class X11Window : public TestWindow { Q_OBJECT };
}
namespace {
quint64 monotonicUs() {
  timespec ts{};
  if (clock_gettime(CLOCK_MONOTONIC, &ts) != 0)
    return 0;
  return static_cast<quint64>(ts.tv_sec) * 1000000ull +
         static_cast<quint64>(ts.tv_nsec) / 1000ull;
}
struct WindowWire { QString generation, token; };
struct ContentWire {
  QString provider;
  QString capability;
  QString local;
};
struct SessionWire {
  QString id;
  QString title;
  QString subtitle;
  QString artwork;
  QString state;
  qlonglong duration = -1;
  qlonglong position = 0;
  qulonglong anchor = 0;
  QStringList actions;
  bool hasContent = false;
  ContentWire content;
};
struct ProviderWire {
  QString id = QStringLiteral("media-provider");
  QString plugin = QStringLiteral("plugin");
  QString source;
  QString name = QStringLiteral("media");
  QString version = QStringLiteral("1");
  QDBusObjectPath path{QStringLiteral("/org/arinn/Daemon1/providers/p_media")};
  QStringList capabilities{QStringLiteral("media")};
  QString runtime = QStringLiteral("ready");
  QString backend = QStringLiteral("available");
  QString generation = QStringLiteral("g-media");
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
QDBusArgument &operator<<(QDBusArgument &argument, const WindowWire &window) {
  argument.beginStructure(); argument << window.generation << window.token; argument.endStructure(); return argument;
}
const QDBusArgument &operator>>(const QDBusArgument &argument, WindowWire &) { return argument; }
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
struct TrackWire {
  QString id, kind, language, label, codec;
  quint32 channels = 0;
  bool selected = false;
  bool external = false;
};
struct ChapterWire {
  QString id, title;
  qlonglong start = 0;
};
struct RangeWire {
  qlonglong start = 0, end = 0;
};
QDBusArgument &operator<<(QDBusArgument &argument, const TrackWire &track) {
  argument.beginStructure();
  argument << track.id << track.kind << track.language << track.label << track.codec
           << track.channels << track.selected << track.external;
  argument.endStructure();
  return argument;
}
const QDBusArgument &operator>>(const QDBusArgument &argument, TrackWire &) {
  return argument;
}
QDBusArgument &operator<<(QDBusArgument &argument, const ChapterWire &chapter) {
  argument.beginStructure();
  argument << chapter.id << chapter.start << chapter.title;
  argument.endStructure();
  return argument;
}
const QDBusArgument &operator>>(const QDBusArgument &argument, ChapterWire &) {
  return argument;
}
QDBusArgument &operator<<(QDBusArgument &argument, const RangeWire &range) {
  argument.beginStructure();
  argument << range.start << range.end;
  argument.endStructure();
  return argument;
}
const QDBusArgument &operator>>(const QDBusArgument &argument, RangeWire &) {
  return argument;
}
const QStringList commandMethods{QStringLiteral("Play"),
                                 QStringLiteral("Pause"),
                                 QStringLiteral("PlayPause"),
                                 QStringLiteral("Seek"),
                                 QStringLiteral("Next"),
                                 QStringLiteral("Previous"),
                                 QStringLiteral("Stop")};
class MediaDaemon : public QDBusVirtualObject {
public:
  QList<ProviderWire> providers{ProviderWire{}};
  QList<SessionWire> page1;
  QList<SessionWire> page2;
  QString page2Token = QStringLiteral("p2");
  QString pageGeneration = QStringLiteral("g-media");
  qulonglong page1Revision = 1;
  qulonglong page2Revision = 1;
  bool loopSecond = false;
  bool rejectCommand = false;
  bool holdOperation = false;
  bool rejectOperationRead = false;
  QString operationState = QStringLiteral("succeeded");
  QString operationError;
  int sessionReads = 0;
  int operationReads = 0;
  int featureReads = 0;
  int trackReads = 0;
  int timelineReads = 0;
  int thumbnailReads = 0;
  bool rejectFeatures = false;
  bool thumbnailUnsupported = false;
  QStringList features;
  bool windowReady = true;
  bool rejectRelease = false;
  bool delayPresentAdmission = false;
  QDBusMessage delayedPresent;
  int windowReads = 0;
  QStringList windowCommands;
  int homeCommands = 0;
  QList<TrackWire> tracks;
  QList<ChapterWire> chapters;
  QList<RangeWire> ranges;
  QString thumbnailAsset = QStringLiteral("thumb-asset");
  QList<EntityWire> operationEntities;
  QStringList commands;
  QStringList extensionCommands;
  QString lastTrackKind, lastTrackId;
  qlonglong lastThumbnail = -1;
  QString lastGeneration, lastLocal, lastRequest;
  qlonglong lastSeek = -1;
  QString introspect(const QString &) const override { return {}; }
  bool handleMessage(const QDBusMessage &message,
                     const QDBusConnection &bus) override {
    QDBusArgument body;
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
      ++sessionReads;
      const auto token = message.arguments().value(0).toString();
      QList<SessionWire> rows;
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
        bus.send(message.createErrorReply(QStringLiteral("org.arinn.Error1.InvalidArgument"),
                                          QStringLiteral("InvalidArgument")));
        return true;
      }
      body.beginStructure();
      body << pageGeneration
           << (token.isEmpty() ? page1Revision : page2Revision);
      body.beginArray(QMetaType::fromType<SessionWire>());
      for (const auto &row : rows)
        body << row;
      body.endArray();
      body << next;
      body.endStructure();
    } else if (commandMethods.contains(message.member())) {
      commands.append(message.member());
      lastGeneration = message.arguments().value(0).toString();
      lastLocal = message.arguments().value(1).toString();
      if (message.member() == QLatin1String("Seek")) {
        lastSeek = message.arguments().value(2).toLongLong();
        lastRequest = message.arguments().value(3).toString();
      } else {
        lastRequest = message.arguments().value(2).toString();
      }
      if (rejectCommand) {
        bus.send(message.createErrorReply(QStringLiteral("org.arinn.Error1.Unsupported"),
                                          QStringLiteral("Unsupported")));
        return true;
      }
      operationEntities.clear();
      bus.send(message.createReply(QVariant::fromValue(
          QDBusObjectPath(QStringLiteral("/org/arinn/Daemon1/ops/media")))));
      return true;
    } else if (message.member() == QLatin1String("Home")) {
      ++homeCommands; bus.send(message.createReply()); return true;
    } else if (message.member() == QLatin1String("GetWindow")) {
      ++windowReads;
      QDBusArgument windows;
      windows.beginArray(QMetaType::fromType<WindowWire>());
      if (windowReady)
        windows << WindowWire{QStringLiteral("platform"),
                              QStringLiteral("window")};
      windows.endArray();
      bus.send(message.createReply(QVariant::fromValue(windows)));
      return true;
    } else if (message.member() == QLatin1String("Present") ||
               message.member() == QLatin1String("Release")) {
      windowCommands.append(message.member());
      if (message.member() == QLatin1String("Present") && delayPresentAdmission) {
        message.setDelayedReply(true);
        delayedPresent = message;
        return true;
      }
      if (message.member() == QLatin1String("Release") && rejectRelease) {
        bus.send(message.createErrorReply(QStringLiteral("org.arinn.Error1.Unavailable"), QStringLiteral("Unavailable")));
        return true;
      }
      lastGeneration = message.arguments().value(0).toString();
      lastLocal = message.arguments().value(1).toString();
      lastRequest = message.arguments().value(2).toString();
      bus.send(message.createReply(QVariant::fromValue(
          QDBusObjectPath(QStringLiteral("/org/arinn/Daemon1/ops/media")))));
      return true;
    } else if (message.member() == QLatin1String("Features")) {
      ++featureReads;
      if (rejectFeatures) {
        bus.send(message.createErrorReply(
            QStringLiteral("org.arinn.Error1.Unavailable"),
            QStringLiteral("Unavailable")));
        return true;
      }
      bus.send(message.createReply(QVariant::fromValue(features)));
      return true;
    } else if (message.member() == QLatin1String("ListTracks")) {
      ++trackReads;
      lastGeneration = message.arguments().value(0).toString();
      lastLocal = message.arguments().value(1).toString();
      body.beginArray(QMetaType::fromType<TrackWire>());
      for (const auto &track : tracks)
        body << track;
      body.endArray();
    } else if (message.member() == QLatin1String("GetTimeline")) {
      ++timelineReads;
      lastGeneration = message.arguments().value(0).toString();
      lastLocal = message.arguments().value(1).toString();
      body.beginStructure();
      body.beginArray(QMetaType::fromType<ChapterWire>());
      for (const auto &chapter : chapters)
        body << chapter;
      body.endArray();
      body.beginArray(QMetaType::fromType<RangeWire>());
      for (const auto &range : ranges)
        body << range;
      body.endArray();
      body.endStructure();
    } else if (message.member() == QLatin1String("SelectTrack") ||
               message.member() == QLatin1String("DisableSubtitles") ||
               message.member() == QLatin1String("RequestThumbnail")) {
      extensionCommands.append(message.member());
      lastGeneration = message.arguments().value(0).toString();
      lastLocal = message.arguments().value(1).toString();
      if (message.member() == QLatin1String("SelectTrack")) {
        lastTrackKind = message.arguments().value(2).toString();
        lastTrackId = message.arguments().value(3).toString();
        lastRequest = message.arguments().value(4).toString();
      } else if (message.member() == QLatin1String("RequestThumbnail")) {
        lastThumbnail = message.arguments().value(2).toLongLong();
        lastRequest = message.arguments().value(3).toString();
        ++thumbnailReads;
      } else {
        lastRequest = message.arguments().value(2).toString();
      }
      if (message.member() == QLatin1String("RequestThumbnail") && thumbnailUnsupported) {
        bus.send(message.createErrorReply(QStringLiteral("org.arinn.Error1.Unsupported"),
                                          QStringLiteral("Unsupported")));
        return true;
      }
      if (rejectCommand) {
        bus.send(message.createErrorReply(QStringLiteral("org.arinn.Error1.Unsupported"),
                                          QStringLiteral("Unsupported")));
        return true;
      }
      operationEntities.clear();
      if (message.member() == QLatin1String("RequestThumbnail"))
        operationEntities.append(
            EntityWire{providers.value(0).id, QStringLiteral("media"), thumbnailAsset});
      bus.send(message.createReply(QVariant::fromValue(
          QDBusObjectPath(QStringLiteral("/org/arinn/Daemon1/ops/media")))));
      return true;
    } else if (message.member() == QLatin1String("Get") &&
               message.interface() == QLatin1String("org.arinn.Operation1")) {
      ++operationReads;
      if (rejectOperationRead) {
        bus.send(message.createErrorReply(QStringLiteral("org.arinn.Error1.Unavailable"), QStringLiteral("Unavailable"))); return true;
      }
      body.beginStructure();
      body << QString(holdOperation ? QStringLiteral("running") : operationState)
           << operationError << false << qulonglong(1) << qulonglong(2);
      body.beginArray(QMetaType::fromType<EntityWire>());
      for (const auto &entity : operationEntities)
        body << entity;
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
  Q_PROPERTY(bool homeVisible MEMBER m_home NOTIFY changed)
  Q_PROPERTY(QVariantMap shortcutEditor MEMBER m_editor NOTIFY changed)
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
  QVariantMap m_editor;
  QString m_return;
  QString m_homeState = QStringLiteral("ready");
  QString m_message;
  QVariantList m_items, m_continue, m_applications, m_favorites;
  QStringList m_busy;
signals:
  void changed();
public:
  Q_INVOKABLE void back() {}
  Q_INVOKABLE void closeShortcutEditor() { m_editor.clear(); emit changed(); }
  Q_INVOKABLE void launch(const QString &) {}
  Q_INVOKABLE void activate(const QString &) {}
  Q_INVOKABLE void refresh() {}
};
class SessionGuardStandIn : public QObject {
  Q_OBJECT
  Q_PROPERTY(bool blocked MEMBER m_blocked NOTIFY changed)
public:
  bool m_blocked = false;
signals:
  void changed();
};
SessionWire session(const QString &id, const QString &title, const QString &state,
                    qlonglong position, qulonglong anchor, qlonglong duration,
                    const QStringList &actions) {
  SessionWire row;
  row.id = id;
  row.title = title;
  row.subtitle = QStringLiteral("Line");
  row.artwork = QStringLiteral("asset-not-fetched");
  row.state = state;
  row.position = position;
  row.anchor = anchor;
  row.duration = duration;
  row.actions = actions;
  return row;
}
} // namespace

class MediaCatalogTest : public QObject {
  Q_OBJECT
private slots:
  void initTestCase() {
    qDBusRegisterMetaType<WindowWire>();
    qDBusRegisterMetaType<ContentWire>();
    qDBusRegisterMetaType<SessionWire>();
    qDBusRegisterMetaType<ProviderWire>();
    qDBusRegisterMetaType<ShortcutWire>();
    qDBusRegisterMetaType<EntityWire>();
    qDBusRegisterMetaType<TrackWire>();
    qDBusRegisterMetaType<ChapterWire>();
    qDBusRegisterMetaType<RangeWire>();
  }
  void cleanup() {
    auto bus = QDBusConnection::sessionBus();
    bus.unregisterObject(QStringLiteral("/org/arinn"),
                         QDBusConnection::UnregisterTree);
    bus.unregisterService(QStringLiteral("org.arinn.Daemon1"));
  }
  void nativeIdentityUsesRealQtQObjectTypeObservation() {
    QFile adapter(QDir(QFileInfo(QStringLiteral(ARINN_MAIN_QML)).absolutePath()).filePath(QStringLiteral("../../platform/kde/phase0.js")));
    QVERIFY(adapter.open(QIODevice::ReadOnly));
    const auto source = QString::fromUtf8(adapter.readAll());
    const auto start = source.indexOf(QStringLiteral("function launchId(w)"));
    const auto end = source.indexOf(QStringLiteral("function mediaRows()"), start);
    QVERIFY(start >= 0 && end > start);
    QJSEngine engine;
    QVERIFY(!engine.evaluate(source.mid(start, end - start)).isError());
    KWin::XdgToplevelWindow native;
    KWin::X11Window x11;
    const auto wrapped = engine.newQObject(&native);
    QVERIFY(wrapped.property(QStringLiteral("waylandClient")).isUndefined());
    const auto launch = engine.globalObject().property(QStringLiteral("launchId"));
    QCOMPARE(launch.call({wrapped}).toString(), QStringLiteral("aaaaaaaa-0000-4000-8000-000000000001"));
    QCOMPARE(launch.call({engine.newQObject(&x11)}).toString(), QString());
    x11.instance = QStringLiteral("org.arinn.playback.aaaaaaaa-0000-4000-8000-000000000001");
    QCOMPARE(launch.call({engine.newQObject(&x11)}).toString(), QStringLiteral("aaaaaaaa-0000-4000-8000-000000000001"));
    x11.instance = QStringLiteral("org.arinn.playback.00000000-0000-0000-0000-000000000000");
    QCOMPARE(launch.call({engine.newQObject(&x11)}).toString(), QString());
    KWin::TestWindow base;
    QCOMPARE(launch.call({engine.newQObject(&base)}).toString(), QString());
    native.setObjectName(QStringLiteral("named native fixture"));
    QCOMPARE(launch.call({wrapped}).toString(), QStringLiteral("aaaaaaaa-0000-4000-8000-000000000001"));
    auto deleted = new KWin::XdgToplevelWindow;
    const auto deletedWrapper = engine.newQObject(deleted);
    delete deleted;
    QCOMPARE(launch.call({deletedWrapper}).toString(), QString());
    native.appId = QStringLiteral("org.arinn.Playback.00000000-0000-0000-0000-000000000000");
    QCOMPARE(launch.call({wrapped}).toString(), QString());
  }
  void pagingAnchorAndActions() {
    MediaDaemon daemon;
    const auto now = monotonicUs();
    daemon.page1 = {
        session(QStringLiteral("paused"), QStringLiteral("PausedRecent"),
                QStringLiteral("paused"), 1500000, now, 8000000,
                {QStringLiteral("play"), QStringLiteral("frobnicate")}),
        session(QStringLiteral("live"), QStringLiteral("Playing"),
                QStringLiteral("playing"), 0, now - 2000000, -1,
                {QStringLiteral("pause"), QStringLiteral("play-pause")}),
        session(QStringLiteral("done"), QStringLiteral("Stopped"),
                QStringLiteral("stopped"), 4000000, now, 4000000,
                {QStringLiteral("play")}),
    };
    daemon.page2 = {session(QStringLiteral("paged"), QStringLiteral("Paged"),
                            QStringLiteral("paused"), 10, 90, -1,
                            {QStringLiteral("play")})};
    daemon.page1.last().artwork.clear();
    auto bus = QDBusConnection::sessionBus();
    QVERIFY(bus.registerService(QStringLiteral("org.arinn.Daemon1")));
    QVERIFY(bus.registerVirtualObject(QStringLiteral("/org/arinn"), &daemon,
                                      QDBusConnection::SubPath));
    MediaCatalog catalog;
    QTRY_COMPARE(catalog.continueEntries().size(), 4);
    QCOMPARE(daemon.sessionReads, 2);
    const auto entries = catalog.continueEntries();
    QCOMPARE(entries.at(0).toMap().value(QStringLiteral("title")).toString(),
             QStringLiteral("Playing"));
    QCOMPARE(entries.at(1).toMap().value(QStringLiteral("title")).toString(),
             QStringLiteral("PausedRecent"));
    QCOMPARE(entries.at(2).toMap().value(QStringLiteral("title")).toString(),
             QStringLiteral("Paged"));
    QCOMPARE(entries.at(3).toMap().value(QStringLiteral("title")).toString(),
             QStringLiteral("Stopped"));
    QVERIFY(catalog.sessionArtwork().startsWith(
        QStringLiteral("image://arinn-assets/")));
    bool sawEmptyArtwork = false;
    for (const auto &entry : entries) {
      const auto map = entry.toMap();
      QVERIFY(map.value(QStringLiteral("kind")).toString() ==
              QLatin1String("media"));
      if (map.value(QStringLiteral("title")).toString() ==
          QLatin1String("Stopped")) {
        QVERIFY(!map.contains(QStringLiteral("artwork")));
        sawEmptyArtwork = true;
      } else {
        QVERIFY(map.value(QStringLiteral("artwork"))
                    .toString()
                    .startsWith(QStringLiteral("image://arinn-assets/")));
      }
    }
    QVERIFY(sawEmptyArtwork);
    QCOMPARE(entries.at(0).toMap().value(QStringLiteral("running")).toBool(),
             true);
    QVERIFY(catalog.positionUs() >= 1500000);
    QVERIFY(catalog.positionUs() < 8000000);
    const auto marked = catalog.positionUs();
    QTRY_VERIFY(catalog.positionUs() > marked + 200000);
    QCOMPARE(catalog.sessionActions(),
             QStringList({QStringLiteral("pause"), QStringLiteral("playPause")}));
    QCOMPARE(catalog.playbackState(), QStringLiteral("playing"));
    catalog.select(entries.at(1).toMap().value(QStringLiteral("id")).toString());
    QCOMPARE(catalog.sessionTitle(), QStringLiteral("PausedRecent"));
    QCOMPARE(catalog.positionUs(), 1500000);
    QCOMPARE(catalog.sessionActions(), QStringList({QStringLiteral("play")}));
    QTest::qWait(250);
    QCOMPARE(catalog.positionUs(), 1500000);
    catalog.next();
    QCOMPARE(catalog.operationState(), QStringLiteral("Unsupported"));
    QVERIFY(!daemon.commands.contains(QStringLiteral("Next")));
    catalog.select(entries.at(3).toMap().value(QStringLiteral("id")).toString());
    QCOMPARE(catalog.sessionTitle(), QStringLiteral("Stopped"));
    QCOMPARE(catalog.sessionArtwork(), QString());
  }
  void pausedDoesNotAdvanceAndDurationClamps() {
    MediaDaemon daemon;
    const auto now = monotonicUs();
    daemon.page1 = {session(QStringLiteral("live"), QStringLiteral("Playing"),
                            QStringLiteral("playing"), 0, now - 5000000, 1000000,
                            {QStringLiteral("pause")}),
                    session(QStringLiteral("buffer"), QStringLiteral("Buffer"),
                            QStringLiteral("buffering"), 20, now - 5000000, -1,
                            {})};
    auto bus = QDBusConnection::sessionBus();
    QVERIFY(bus.registerService(QStringLiteral("org.arinn.Daemon1")));
    QVERIFY(bus.registerVirtualObject(QStringLiteral("/org/arinn"), &daemon,
                                      QDBusConnection::SubPath));
    MediaCatalog catalog;
    QTRY_COMPARE(catalog.sessionTitle(), QStringLiteral("Playing"));
    QCOMPARE(catalog.positionUs(), 1000000);
    catalog.select(catalog.continueEntries()
                       .at(1)
                       .toMap()
                       .value(QStringLiteral("id"))
                       .toString());
    QCOMPARE(catalog.playbackState(), QStringLiteral("buffering"));
    QCOMPARE(catalog.positionUs(), 20);
    QTest::qWait(200);
    QCOMPARE(catalog.positionUs(), 20);
  }
  void unsupportedCommandIsNotSuccess() {
    MediaDaemon daemon;
    daemon.rejectCommand = true;
    daemon.page1 = {session(QStringLiteral("live"), QStringLiteral("Playing"),
                            QStringLiteral("paused"), 10, monotonicUs(), -1,
                            {QStringLiteral("play"), QStringLiteral("seek")})};
    auto bus = QDBusConnection::sessionBus();
    QVERIFY(bus.registerService(QStringLiteral("org.arinn.Daemon1")));
    QVERIFY(bus.registerVirtualObject(QStringLiteral("/org/arinn"), &daemon,
                                      QDBusConnection::SubPath));
    MediaCatalog catalog;
    QTRY_COMPARE(catalog.sessionState(), QStringLiteral("ready"));
    QStringList states;
    connect(&catalog, &MediaCatalog::operationChanged, &catalog,
            [&] { states.append(catalog.operationState()); });
    catalog.play();
    QTRY_COMPARE(catalog.operationState(), QStringLiteral("Unsupported"));
    QCOMPARE(daemon.commands, QStringList({QStringLiteral("Play")}));
    QCOMPARE(daemon.operationReads, 0);
    QVERIFY(!states.contains(QStringLiteral("succeeded")));
    QCOMPARE(catalog.playbackState(), QStringLiteral("paused"));
    states.clear();
    catalog.seek(-1);
    QCOMPARE(catalog.operationState(), QStringLiteral("InvalidArgument"));
    QCOMPARE(daemon.commands.size(), 1);
    QVERIFY(!states.contains(QStringLiteral("succeeded")));
  }
  void operationSuccessWaitsForGet() {
    MediaDaemon daemon;
    daemon.holdOperation = true;
    daemon.page1 = {session(QStringLiteral("live"), QStringLiteral("Playing"),
                            QStringLiteral("paused"), 10, monotonicUs(), 9000000,
                            {QStringLiteral("play"), QStringLiteral("seek")})};
    auto bus = QDBusConnection::sessionBus();
    QVERIFY(bus.registerService(QStringLiteral("org.arinn.Daemon1")));
    QVERIFY(bus.registerVirtualObject(QStringLiteral("/org/arinn"), &daemon,
                                      QDBusConnection::SubPath));
    MediaCatalog catalog;
    QTRY_COMPARE(catalog.sessionState(), QStringLiteral("ready"));
    QStringList states;
    connect(&catalog, &MediaCatalog::operationChanged, &catalog,
            [&] { states.append(catalog.operationState()); });
    catalog.seek(2500000);
    QTRY_COMPARE(catalog.operationState(), QStringLiteral("pending"));
    QTRY_VERIFY(daemon.operationReads > 0);
    QCOMPARE(daemon.lastSeek, 2500000);
    QCOMPARE(daemon.lastLocal, QStringLiteral("live"));
    QCOMPARE(daemon.lastGeneration, QStringLiteral("g-media"));
    QVERIFY(!daemon.lastRequest.isEmpty());
    QVERIFY(!states.contains(QStringLiteral("succeeded")));
    QCOMPARE(catalog.playbackState(), QStringLiteral("paused"));
    QCOMPARE(catalog.positionUs(), 10);
    daemon.holdOperation = false;
    QTRY_COMPARE(catalog.operationState(), QStringLiteral("succeeded"));
    QCOMPARE(catalog.playbackState(), QStringLiteral("paused"));
    daemon.operationState = QStringLiteral("failed");
    daemon.operationError = QStringLiteral("Unavailable");
    const auto atPlay = states.size();
    catalog.play();
    QTRY_COMPARE(catalog.operationState(), QStringLiteral("Unavailable"));
    for (int i = atPlay; i < states.size(); ++i)
      QVERIFY(states.at(i) != QStringLiteral("succeeded"));
  }
  void mixedRevisionIsUnavailable() {
    MediaDaemon daemon;
    daemon.page2Revision = 2;
    daemon.page1 = {session(QStringLiteral("one"), QStringLiteral("First"),
                            QStringLiteral("paused"), 1, 1, -1,
                            {QStringLiteral("play")})};
    daemon.page2 = {session(QStringLiteral("two"), QStringLiteral("Second"),
                            QStringLiteral("paused"), 1, 2, -1,
                            {QStringLiteral("play")})};
    auto bus = QDBusConnection::sessionBus();
    QVERIFY(bus.registerService(QStringLiteral("org.arinn.Daemon1")));
    QVERIFY(bus.registerVirtualObject(QStringLiteral("/org/arinn"), &daemon,
                                      QDBusConnection::SubPath));
    MediaCatalog catalog;
    QTRY_VERIFY(daemon.sessionReads >= 2);
    QTRY_COMPARE(catalog.sessionState(), QStringLiteral("unavailable"));
    QCOMPARE(catalog.continueEntries().size(), 0);
  }
  void pageGenerationMustMatchProvider() {
    MediaDaemon daemon;
    daemon.pageGeneration = QStringLiteral("other-generation");
    daemon.page1 = {session(QStringLiteral("one"), QStringLiteral("First"),
                            QStringLiteral("paused"), 1, 1, -1,
                            {QStringLiteral("play")})};
    auto bus = QDBusConnection::sessionBus();
    QVERIFY(bus.registerService(QStringLiteral("org.arinn.Daemon1")));
    QVERIFY(bus.registerVirtualObject(QStringLiteral("/org/arinn"), &daemon,
                                      QDBusConnection::SubPath));
    MediaCatalog catalog;
    QTRY_VERIFY(daemon.sessionReads >= 1);
    QTRY_COMPARE(catalog.sessionState(), QStringLiteral("unavailable"));
    QCOMPARE(catalog.continueEntries().size(), 0);
  }
  void repeatedPageTokenFailsClosed() {
    MediaDaemon daemon;
    daemon.loopSecond = true;
    daemon.page1 = {session(QStringLiteral("live"), QStringLiteral("Playing"),
                            QStringLiteral("paused"), 1, 1, -1,
                            {QStringLiteral("play")})};
    daemon.page2 = {session(QStringLiteral("more"), QStringLiteral("More"),
                            QStringLiteral("paused"), 1, 1, -1,
                            {QStringLiteral("play")})};
    auto bus = QDBusConnection::sessionBus();
    QVERIFY(bus.registerService(QStringLiteral("org.arinn.Daemon1")));
    QVERIFY(bus.registerVirtualObject(QStringLiteral("/org/arinn"), &daemon,
                                      QDBusConnection::SubPath));
    MediaCatalog catalog;
    QTRY_COMPARE(catalog.sessionState(), QStringLiteral("unavailable"));
    QCOMPARE(catalog.continueEntries().size(), 0);
    QTest::qWait(200);
    QVERIFY(daemon.sessionReads < 8);
  }
  void emptyAndMissingDaemon() {
    {
      MediaDaemon daemon;
      ProviderWire application;
      application.capabilities = {QStringLiteral("application")};
      daemon.providers = {application};
      auto bus = QDBusConnection::sessionBus();
      QVERIFY(bus.registerService(QStringLiteral("org.arinn.Daemon1")));
      QVERIFY(bus.registerVirtualObject(QStringLiteral("/org/arinn"), &daemon,
                                        QDBusConnection::SubPath));
      MediaCatalog catalog;
      QTRY_COMPARE(catalog.sessionState(), QStringLiteral("empty"));
      QCOMPARE(catalog.continueEntries().size(), 0);
    }
    cleanup();
    MediaCatalog missing;
    QTRY_COMPARE(missing.sessionState(), QStringLiteral("unavailable"));
  }
  void shellBindsContinueAndSession() {
    MediaDaemon daemon;
    daemon.holdOperation = true;
    const auto now = monotonicUs();
    daemon.page1 = {session(QStringLiteral("live"), QStringLiteral("Piece"),
                            QStringLiteral("paused"), 4000000, now - 3000000,
                            9000000,
                            {QStringLiteral("play"), QStringLiteral("pause")})};
    auto bus = QDBusConnection::sessionBus();
    QVERIFY(bus.registerService(QStringLiteral("org.arinn.Daemon1")));
    QVERIFY(bus.registerVirtualObject(QStringLiteral("/org/arinn"), &daemon,
                                      QDBusConnection::SubPath));
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
    auto session =
        window->findChild<QQuickItem *>(QStringLiteral("productionSession"));
    auto catalog = window->findChild<MediaCatalog *>(QStringLiteral("mediaCatalog"));
    QVERIFY(home);
    QVERIFY(session);
    QVERIFY(catalog);
    QTRY_COMPARE(home->property("continueEntries").toList().size(), 1);
    const auto entry = home->property("continueEntries").toList().at(0).toMap();
    QCOMPARE(entry.value(QStringLiteral("title")).toString(),
             QStringLiteral("Piece"));
    QCOMPARE(entry.value(QStringLiteral("kind")).toString(),
             QStringLiteral("media"));
    window->setProperty("sessionOpen", true);
    QTRY_COMPARE(session->property("sessionState").toString(),
                 QStringLiteral("ready"));
    QCOMPARE(session->property("title").toString(), QStringLiteral("Piece"));
    QCOMPARE(session->property("playbackState").toString(),
             QStringLiteral("paused"));
    QCOMPARE(session->property("positionUs").toDouble(), 4000000);
    auto play = window->findChild<QQuickItem *>(QStringLiteral("action-play"));
    auto next = window->findChild<QQuickItem *>(QStringLiteral("action-next"));
    auto plate = window->findChild<QQuickItem *>(QStringLiteral("artworkPlate"));
    QVERIFY(play);
    QVERIFY(next);
    QVERIFY(plate);
    QCOMPARE(play->property("visible").toBool(), true);
    QCOMPARE(next->property("visible").toBool(), false);
    const auto color = plate->property("color").value<QColor>();
    QVERIFY(qAbs(color.redF() - 1.0) < 0.01);
    QVERIFY(qAbs(color.alphaF() - 0.06) < 0.02);
    play->forceActiveFocus();
    QTest::keyClick(window, Qt::Key_Return);
    QTRY_COMPARE(catalog->operationState(), QStringLiteral("pending"));
    QCOMPARE(session->property("operationState").toString(),
             QStringLiteral("pending"));
    auto pending =
        window->findChild<QQuickItem *>(QStringLiteral("operationPending"));
    auto banner =
        window->findChild<QQuickItem *>(QStringLiteral("operationBanner"));
    QVERIFY(pending);
    QVERIFY(banner);
    QTRY_COMPARE(pending->property("visible").toBool(), true);
    QCOMPARE(banner->property("visible").toBool(), false);
    QCOMPARE(daemon.commands, QStringList({QStringLiteral("Play")}));
    QCOMPARE(session->property("playbackState").toString(),
             QStringLiteral("paused"));
    auto label = window->findChild<QQuickItem *>(QStringLiteral("playbackLabel"));
    QVERIFY(label);
    QCOMPARE(label->property("text").toString(), QStringLiteral("Paused"));
    daemon.holdOperation = false;
    QTRY_COMPARE(catalog->operationState(), QStringLiteral("succeeded"));
    QCOMPARE(session->property("operationState").toString(),
             QStringLiteral("succeeded"));
    QTRY_COMPARE(pending->property("visible").toBool(), false);
    QCOMPARE(banner->property("visible").toBool(), false);
    QCOMPARE(session->property("playbackState").toString(),
             QStringLiteral("paused"));
    QCOMPARE(label->property("text").toString(), QStringLiteral("Paused"));
    catalog->next();
    QCOMPARE(catalog->operationState(), QStringLiteral("Unsupported"));
    QCOMPARE(session->property("operationState").toString(),
             QStringLiteral("Unsupported"));
    QTRY_COMPARE(banner->property("visible").toBool(), true);
    auto bannerText =
        window->findChild<QQuickItem *>(QStringLiteral("operationBannerText"));
    QVERIFY(bannerText);
    QCOMPARE(bannerText->property("text").toString(),
             QStringLiteral("That control is not available."));
  }
  void stateChangedRefreshesWithoutAFastPoll() {
    MediaDaemon daemon;
    daemon.page1 = {session(QStringLiteral("live"), QStringLiteral("Before"),
                            QStringLiteral("paused"), 10, monotonicUs(), -1,
                            {QStringLiteral("play")})};
    auto bus = QDBusConnection::sessionBus();
    QVERIFY(bus.registerService(QStringLiteral("org.arinn.Daemon1")));
    QVERIFY(bus.registerVirtualObject(QStringLiteral("/org/arinn"), &daemon,
                                      QDBusConnection::SubPath));
    MediaCatalog catalog;
    QTRY_COMPARE(catalog.sessionTitle(), QStringLiteral("Before"));
    QTest::qWait(100);
    const auto reads = daemon.sessionReads;
    QVERIFY(reads >= 1);
    QTest::qWait(1500);
    QCOMPARE(daemon.sessionReads, reads);
    daemon.page1[0].title = QStringLiteral("After");
    auto signal = QDBusMessage::createSignal(
        QStringLiteral("/org/arinn/Daemon1"), QStringLiteral("org.arinn.Daemon1"),
        QStringLiteral("StateChanged"));
    signal << QStringLiteral("media-provider") << QStringLiteral("media")
           << QStringLiteral("g-media") << qulonglong(2);
    QVERIFY(bus.send(signal));
    QTRY_COMPARE(catalog.sessionTitle(), QStringLiteral("After"));
    QVERIFY(daemon.sessionReads > reads);
  }
  void zeroOrFutureAnchorDoesNotAdvance() {
    {
      MediaDaemon daemon;
      daemon.page1 = {session(QStringLiteral("zero"), QStringLiteral("Zero"),
                              QStringLiteral("playing"), 250000, 0, -1,
                              {QStringLiteral("pause")})};
      auto bus = QDBusConnection::sessionBus();
      QVERIFY(bus.registerService(QStringLiteral("org.arinn.Daemon1")));
      QVERIFY(bus.registerVirtualObject(QStringLiteral("/org/arinn"), &daemon,
                                        QDBusConnection::SubPath));
      MediaCatalog catalog;
      QTRY_COMPARE(catalog.sessionTitle(), QStringLiteral("Zero"));
      QCOMPARE(catalog.positionUs(), 250000);
      QTest::qWait(300);
      QCOMPARE(catalog.positionUs(), 250000);
    }
    cleanup();
    {
      MediaDaemon daemon;
      const auto now = monotonicUs();
      daemon.page1 = {session(QStringLiteral("future"), QStringLiteral("Future"),
                              QStringLiteral("playing"), 250000,
                              now + 60000000ull, -1, {QStringLiteral("pause")})};
      auto bus = QDBusConnection::sessionBus();
      QVERIFY(bus.registerService(QStringLiteral("org.arinn.Daemon1")));
      QVERIFY(bus.registerVirtualObject(QStringLiteral("/org/arinn"), &daemon,
                                        QDBusConnection::SubPath));
      MediaCatalog catalog;
      QTRY_COMPARE(catalog.sessionTitle(), QStringLiteral("Future"));
      QCOMPARE(catalog.positionUs(), 250000);
      QTest::qWait(300);
      QCOMPARE(catalog.positionUs(), 250000);
    }
  }
  void operationWaitIsBounded() {
    MediaDaemon daemon;
    daemon.holdOperation = true;
    daemon.page1 = {session(QStringLiteral("live"), QStringLiteral("Playing"),
                            QStringLiteral("paused"), 10, monotonicUs(), -1,
                            {QStringLiteral("play")})};
    auto bus = QDBusConnection::sessionBus();
    QVERIFY(bus.registerService(QStringLiteral("org.arinn.Daemon1")));
    QVERIFY(bus.registerVirtualObject(QStringLiteral("/org/arinn"), &daemon,
                                      QDBusConnection::SubPath));
    MediaCatalog catalog;
    QTRY_COMPARE(catalog.sessionState(), QStringLiteral("ready"));
    catalog.play();
    QTRY_COMPARE(catalog.operationState(), QStringLiteral("pending"));
    QTRY_COMPARE_WITH_TIMEOUT(catalog.operationState(),
                              QStringLiteral("Timeout"), 10000);
    QTest::qWait(400);
    const auto reads = daemon.operationReads;
    QVERIFY(reads > 0);
    QVERIFY(reads < 100);
    QTest::qWait(400);
    QCOMPARE(daemon.operationReads, reads);
  }
  void missingFlagSkipsTracksAndTimeline() {
    MediaDaemon daemon;
    daemon.rejectFeatures = true;
    daemon.page1 = {session(QStringLiteral("live"), QStringLiteral("Plain"),
                            QStringLiteral("paused"), 1000000, monotonicUs(),
                            5000000,
                            {QStringLiteral("play"), QStringLiteral("seek")})};
    auto bus = QDBusConnection::sessionBus();
    QVERIFY(bus.registerService(QStringLiteral("org.arinn.Daemon1")));
    QVERIFY(bus.registerVirtualObject(QStringLiteral("/org/arinn"), &daemon,
                                      QDBusConnection::SubPath));
    MediaCatalog catalog;
    QTRY_COMPARE(catalog.sessionState(), QStringLiteral("ready"));
    QVERIFY(daemon.featureReads >= 1);
    QCOMPARE(daemon.trackReads, 0);
    QCOMPARE(daemon.timelineReads, 0);
    QVERIFY(!catalog.playerSession());
    QVERIFY(!catalog.tracksAvailable());
    QVERIFY(!catalog.timelineAvailable());
    QCOMPARE(catalog.audioTracks().size(), 0);
    QCOMPARE(catalog.subtitleTracks().size(), 0);
    QCOMPARE(catalog.chapters().size(), 0);
    QCOMPARE(catalog.bufferedSeconds(), 0);
    QCOMPARE(catalog.thumbnail(), QString());
    catalog.selectAudio(QStringLiteral("a1"));
    QCOMPARE(catalog.operationState(), QStringLiteral("Unsupported"));
    catalog.selectSubtitle(QStringLiteral("s1"));
    QCOMPARE(catalog.operationState(), QStringLiteral("Unsupported"));
    catalog.seek(2000000);
    QTRY_COMPARE(catalog.operationState(), QStringLiteral("succeeded"));
    QCOMPARE(daemon.commands, QStringList({QStringLiteral("Seek")}));
    QCOMPARE(daemon.thumbnailReads, 0);
    QCOMPARE(daemon.extensionCommands, QStringList());
  }
  void richSessionBindsTracksTimelineAndThumbnail() {
    MediaDaemon daemon;
    daemon.features = {QStringLiteral("MediaOpen1"), QStringLiteral("MediaTracks1"),
                       QStringLiteral("MediaTimeline1"),
                       QStringLiteral("NotAFlag")};
    daemon.tracks = {
        TrackWire{QStringLiteral("a1"), QStringLiteral("audio"),
                  QStringLiteral("eng"), QStringLiteral("English"),
                  QStringLiteral("aac"), 2, true, false},
        TrackWire{QStringLiteral("a2"), QStringLiteral("audio"),
                  QStringLiteral("deu"), QStringLiteral("German"),
                  QStringLiteral("aac"), 2, false, false},
        TrackWire{QStringLiteral("s1"), QStringLiteral("subtitle"),
                  QStringLiteral("eng"), QStringLiteral("Notes"),
                  QStringLiteral("subrip"), 0, true, false},
        TrackWire{QStringLiteral("bad"), QStringLiteral("video"), {}, {}, {}, 0,
                  false, false},
    };
    daemon.chapters = {ChapterWire{QStringLiteral("c1"), QStringLiteral("Open"),
                                   1500000}};
    daemon.ranges = {RangeWire{0, 2500000}};
    daemon.page1 = {session(QStringLiteral("live"), QStringLiteral("Piece"),
                            QStringLiteral("paused"), 1000000, monotonicUs(),
                            8000000,
                            {QStringLiteral("play"), QStringLiteral("pause"),
                             QStringLiteral("seek"), QStringLiteral("stop")})};
    auto bus = QDBusConnection::sessionBus();
    QVERIFY(bus.registerService(QStringLiteral("org.arinn.Daemon1")));
    QVERIFY(bus.registerVirtualObject(QStringLiteral("/org/arinn"), &daemon,
                                      QDBusConnection::SubPath));
    MediaCatalog catalog;
    QTRY_COMPARE(catalog.sessionTitle(), QStringLiteral("Piece"));
    QVERIFY(catalog.playerSession());
    QVERIFY(catalog.tracksAvailable());
    QVERIFY(catalog.timelineAvailable());
    QTRY_COMPARE(catalog.audioTracks().size(), 2);
    QCOMPARE(daemon.trackReads, 1);
    QCOMPARE(daemon.timelineReads, 1);
    QCOMPARE(catalog.audioTrackId(), QStringLiteral("a1"));
    const auto audio = catalog.audioTracks().at(0).toMap();
    QCOMPARE(audio.value(QStringLiteral("label")).toString(),
             QStringLiteral("English"));
    QCOMPARE(audio.value(QStringLiteral("detail")).toString(),
             QStringLiteral("eng · aac · 2"));
    QTRY_VERIFY(catalog.subtitleTracks().size() >= 2);
    QCOMPARE(catalog.subtitleTrackId(), QStringLiteral("s1"));
    QCOMPARE(catalog.subtitleTracks().at(0).toMap().value(QStringLiteral("label")).toString(),
             QStringLiteral("Off"));
    const auto offId =
        catalog.subtitleTracks().at(0).toMap().value(QStringLiteral("id")).toString();
    QVERIFY(offId != QLatin1String("s1"));
    QCOMPARE(catalog.chapters().size(), 1);
    QCOMPARE(catalog.chapters().at(0).toDouble(), 1.5);
    QCOMPARE(catalog.bufferedSeconds(), 2.5);
    catalog.selectAudio(QStringLiteral("missing"));
    QCOMPARE(catalog.operationState(), QStringLiteral("InvalidArgument"));
    QCOMPARE(daemon.extensionCommands, QStringList());
    catalog.selectAudio(QStringLiteral("a2"));
    QTRY_COMPARE(catalog.operationState(), QStringLiteral("succeeded"));
    QCOMPARE(daemon.extensionCommands, QStringList({QStringLiteral("SelectTrack")}));
    QCOMPARE(daemon.lastTrackKind, QStringLiteral("audio"));
    QCOMPARE(daemon.lastTrackId, QStringLiteral("a2"));
    QCOMPARE(daemon.lastGeneration, QStringLiteral("g-media"));
    QCOMPARE(daemon.lastLocal, QStringLiteral("live"));
    catalog.selectSubtitle(offId);
    QTRY_COMPARE(daemon.extensionCommands,
                 QStringList({QStringLiteral("SelectTrack"),
                              QStringLiteral("DisableSubtitles")}));
    QCOMPARE(daemon.commands, QStringList());
    catalog.setThumbnailWanted(true);
    QTRY_VERIFY(daemon.thumbnailReads >= 1);
    QTRY_VERIFY(catalog.thumbnail().startsWith(QStringLiteral("image://arinn-assets/")));
    QString provider, path, generation, asset;
    const auto token = catalog.thumbnail().mid(QStringLiteral("image://arinn-assets/").size());
    QVERIFY(AssetImages::parseImageId(token, &provider, &path, &generation, &asset));
    QCOMPARE(asset, QStringLiteral("thumb-asset"));
    QCOMPARE(provider, QStringLiteral("media-provider"));
    const auto thumbs = daemon.thumbnailReads;
    const auto seeks = daemon.commands.size();
    catalog.seek(3000000);
    QTRY_COMPARE(catalog.operationState(), QStringLiteral("succeeded"));
    QCOMPARE(daemon.commands, QStringList({QStringLiteral("Seek")}));
    QCOMPARE(daemon.lastSeek, 3000000);
    QTRY_VERIFY(daemon.thumbnailReads > thumbs);
    QCOMPARE(daemon.lastThumbnail, 3000000);
    QCOMPARE(daemon.commands.size(), seeks + 1);
    daemon.thumbnailUnsupported = true;
    catalog.seek(4000000);
    QTRY_COMPARE(daemon.lastSeek, 4000000);
    QTRY_COMPARE(catalog.thumbnail().isEmpty(), false);
    QCOMPARE(daemon.commands.size(), seeks + 2);
  }
  void shellOpensPlayerForRichSession() {
    MediaDaemon daemon;
    daemon.features = {QStringLiteral("MediaTracks1"), QStringLiteral("MediaTimeline1"), QStringLiteral("MediaWindow1")};
    daemon.tracks = {
        TrackWire{QStringLiteral("a1"), QStringLiteral("audio"),
                  QStringLiteral("eng"), QStringLiteral("English"),
                  QStringLiteral("aac"), 2, true, false},
        TrackWire{QStringLiteral("a2"), QStringLiteral("audio"),
                  QStringLiteral("deu"), QStringLiteral("German"),
                  QStringLiteral("aac"), 2, false, false},
    };
    daemon.page1 = {session(QStringLiteral("live"), QStringLiteral("Piece"),
                            QStringLiteral("paused"), 4000000, monotonicUs(),
                            9000000,
                            {QStringLiteral("play"), QStringLiteral("pause"),
                             QStringLiteral("seek"), QStringLiteral("stop")})};
    auto bus = QDBusConnection::sessionBus();
    QVERIFY(bus.registerService(QStringLiteral("org.arinn.Daemon1")));
    QVERIFY(bus.registerVirtualObject(QStringLiteral("/org/arinn"), &daemon,
                                      QDBusConnection::SubPath));
    ShellStandIn backend;
    // #258: a lingering action result must not cover the picture.
    backend.m_message = QStringLiteral("Done");
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
    auto catalog = window->findChild<MediaCatalog *>(QStringLiteral("mediaCatalog"));
    auto player = window->findChild<QQuickItem *>(QStringLiteral("productionPlayer"));
    auto session =
        window->findChild<QQuickItem *>(QStringLiteral("productionSession"));
    QVERIFY(catalog);
    QVERIFY(player);
    QVERIFY(session);
    QTRY_VERIFY(catalog->playerSession());
    const auto entryId =
        catalog->continueEntries().at(0).toMap().value(QStringLiteral("id")).toString();
    QVERIFY(QMetaObject::invokeMethod(window, "openSession", Q_ARG(QVariant, entryId),
                                      Q_ARG(QVariant, false)));
    QTRY_COMPARE(catalog->presentationState(), QStringLiteral("active"));
    QCOMPARE(daemon.windowCommands, QStringList({QStringLiteral("Present")}));
    auto strip = window->findChild<QQuickItem *>(QStringLiteral("feedbackStrip"));
    QVERIFY(strip);
    QTRY_COMPARE(player->property("visible").toBool(), true);
    QCOMPARE(session->property("visible").toBool(), false);
    QCOMPARE(strip->property("visible").toBool(), false);
    QCOMPARE(player->property("pictureStandIn").toBool(), false);
    QCOMPARE(player->property("title").toString(), QStringLiteral("Piece"));
    QCOMPARE(player->property("tracksSupported").toBool(), true);
    QCOMPARE(player->property("timelineSupported").toBool(), true);
    QCOMPARE(player->property("seekSupported").toBool(), true);
    QTRY_COMPARE(player->property("activeFocus").toBool(), true);
    auto audio = window->findChild<QQuickItem *>(QStringLiteral("audioPillLabel"));
    QVERIFY(audio);
    QTRY_COMPARE(audio->property("text").toString(), QStringLiteral("Audio · English"));
    const auto before = daemon.commands;
    QTest::keyClick(window, Qt::Key_Right);
    QTRY_COMPARE(daemon.commands, QStringList({QStringLiteral("Seek")}));
    QCOMPARE(daemon.commands.size(), before.size() + 1);
    QVERIFY(!daemon.extensionCommands.contains(QStringLiteral("Seek")));
    QTest::keyClick(window, Qt::Key_Down);
    QTest::keyClick(window, Qt::Key_Right);
    QTest::keyClick(window, Qt::Key_Return);
    QTest::keyClick(window, Qt::Key_Down);
    QTest::keyClick(window, Qt::Key_Return);
    QTRY_VERIFY(daemon.extensionCommands.contains(QStringLiteral("SelectTrack")));
    QCOMPARE(daemon.lastTrackKind, QStringLiteral("audio"));
    QCOMPARE(daemon.lastTrackId, QStringLiteral("a2"));
    const auto stops = daemon.commands;
    // Losing guarded input is not an automatic retry intent.
    guard.m_blocked = true; emit guard.changed();
    QTRY_VERIFY(!catalog->inputLayerActive());
    catalog->refresh(); QTest::qWait(300);
    QCOMPARE(daemon.windowCommands.count(QStringLiteral("Present")), 1);
    QCOMPARE(daemon.commands, stops);
    guard.m_blocked = false; emit guard.changed();
    QVERIFY(QMetaObject::invokeMethod(window, "openSession", Q_ARG(QVariant, entryId), Q_ARG(QVariant, false)));
    window->requestActivate();
    QTRY_COMPARE(catalog->presentationState(), QStringLiteral("active"));
    QCOMPARE(daemon.windowCommands.count(QStringLiteral("Present")), 2);
    QTest::keyClick(window, Qt::Key_Home);
    QTRY_COMPARE(daemon.homeCommands, 1);
    QTRY_COMPARE(player->property("visible").toBool(), false);
    QCOMPARE(daemon.commands, stops);
    QCOMPARE(catalog->presentationState(), QStringLiteral("active"));
    backend.m_home = true;
    emit backend.changed();
    backend.m_home = false;
    emit backend.changed();
    QTRY_COMPARE(player->property("visible").toBool(), true);
    QTRY_COMPARE(player->property("activeFocus").toBool(), true);
    QCOMPARE(daemon.windowCommands.count(QStringLiteral("Present")), 2);
    // Restore visible OSD to verify the two Back stages after Return.
    player->setProperty("osdVisible", true);
    QTest::keyClick(window, Qt::Key_Escape);
    QCOMPARE(player->property("osdVisible").toBool(), false);
    QCOMPARE(daemon.commands, stops);
    daemon.holdOperation = true;
    const auto releasesBeforeStop = daemon.windowCommands.count(QStringLiteral("Release"));
    QTest::keyClick(window, Qt::Key_Escape);
    QTRY_COMPARE(catalog->operationState(), QStringLiteral("pending"));
    QTest::qWait(200);
    QVERIFY(player->isVisible());
    QVERIFY(!window->findChild<QQuickItem *>(QStringLiteral("productionHome"))->isVisible());
    QCOMPARE(daemon.windowCommands.count(QStringLiteral("Release")), releasesBeforeStop);
    // A failed Stop keeps the player route and makes Back a retry, rather
    // than exposing Home under a player that has not actually exited.
    daemon.operationState = QStringLiteral("failed");
    daemon.holdOperation = false;
    QTRY_VERIFY(!window->property("leavingStopPending").toBool());
    QVERIFY(player->isVisible());
    QVERIFY(!window->property("leaveError").toString().isEmpty());
    QCOMPARE(daemon.windowCommands.count(QStringLiteral("Release")), releasesBeforeStop);
    // Losing the input layer and expiring the handoff grace must not hide
    // Stop failure feedback or its Back retry route.
    catalog->abandonPresentation();
    QTRY_VERIFY(!catalog->inputLayerActive());
    window->setProperty("leavingPlayback", false);
    QTRY_VERIFY(window->isVisible());
    QVERIFY(player->isVisible());
    // Leaving a failed Stop via Home must not poison a later Continue.
    QTest::keyClick(window, Qt::Key_Home);
    QTRY_VERIFY(!player->isVisible());
    QVERIFY(window->property("leaveError").toString().isEmpty());
    daemon.operationState = QStringLiteral("succeeded");
    backend.m_home = true; emit backend.changed();
    window->requestActivate();
    QTRY_VERIFY(window->isActive());
    QVERIFY(QMetaObject::invokeMethod(window, "openSession", Q_ARG(QVariant, entryId), Q_ARG(QVariant, false)));
    QTRY_COMPARE(catalog->presentationState(), QStringLiteral("active"));
    backend.m_home = false; emit backend.changed();
    QTRY_VERIFY(player->isVisible());
    QTRY_COMPARE(player->property("playbackState").toString(), QStringLiteral("ready"));
    daemon.operationState = QStringLiteral("succeeded");
    player->setProperty("osdVisible", false);
    QTest::keyClick(window, Qt::Key_Escape);
    QTRY_COMPARE(player->property("visible").toBool(), false);
    QCOMPARE(daemon.commands,
             QStringList({QStringLiteral("Seek"), QStringLiteral("Stop"), QStringLiteral("Stop")}));
    QTRY_COMPARE(daemon.windowCommands.last(), QStringLiteral("Release"));
    QCOMPARE(daemon.windowCommands.count(QStringLiteral("Present")), 3);
    // #246: until the daemon reports Home again the shell stays on screen,
    // and the repeats of a held Back key do not walk into navigation.
    auto home = window->findChild<QQuickItem *>(QStringLiteral("productionHome"));
    QVERIFY(home);
    QCOMPARE(backend.m_home, false);
    QTest::qWait(300);
    QVERIFY(window->isVisible());
    for (int i = 0; i < 5; ++i) {
      QKeyEvent repeat(QEvent::KeyPress, Qt::Key_Backspace, Qt::NoModifier, QString(), true);
      QCoreApplication::sendEvent(window, &repeat);
    }
    QTest::qWait(200);
    QCOMPARE(home->property("navOpen").toBool(), false);
    backend.m_home = true; emit backend.changed();
    QTRY_VERIFY(window->isVisible());
    QCOMPARE(home->property("navOpen").toBool(), false);

    // #246, library origin: playback started from a film's detail page
    // returns there, and held Back repeats do not close the detail.
    auto library =
        window->findChild<QQuickItem *>(QStringLiteral("productionLibrary"));
    QVERIFY(library);
    library->setProperty(
        "entries",
        QVariantList{QVariantMap{{QStringLiteral("id"), QStringLiteral("film-1")},
                                 {QStringLiteral("title"), QStringLiteral("Film")},
                                 {QStringLiteral("kind"), QStringLiteral("film")}}});
    home->setProperty("screen", QStringLiteral("library"));
    library->setProperty("detailEntryId", QStringLiteral("film-1"));
    backend.m_home = false; emit backend.changed();
    QVERIFY(QMetaObject::invokeMethod(window, "openSession", Q_ARG(QVariant, entryId),
                                      Q_ARG(QVariant, true)));
    window->requestActivate();
    QTRY_COMPARE(catalog->presentationState(), QStringLiteral("active"));
    QTRY_COMPARE(player->property("activeFocus").toBool(), true);
    player->setProperty("osdVisible", false);
    QTest::keyClick(window, Qt::Key_Escape);
    QTRY_COMPARE(player->property("visible").toBool(), false);
    QTRY_VERIFY(library->property("visible").toBool());
    for (int i = 0; i < 5; ++i) {
      QKeyEvent repeat(QEvent::KeyPress, Qt::Key_Backspace, Qt::NoModifier, QString(), true);
      QCoreApplication::sendEvent(window, &repeat);
    }
    QTest::qWait(200);
    QVERIFY(window->isVisible());
    QCOMPARE(home->property("screen").toString(), QStringLiteral("library"));
    QCOMPARE(library->property("detailEntryId").toString(), QStringLiteral("film-1"));
    QCOMPARE(home->property("navOpen").toBool(), false);

    // Back before the newly launched session reaches the media snapshot waits
    // for that exact row and stops it before exposing the library again.
    const auto lateId = QUuid::createUuidV5(QUuid(), QStringLiteral("media-provider\nlate"))
                            .toString(QUuid::WithoutBraces);
    const auto beforeLateStop = daemon.commands.size();
    QVERIFY(QMetaObject::invokeMethod(window, "openSession", Q_ARG(QVariant, lateId), Q_ARG(QVariant, true)));
    QVERIFY(player->isVisible());
    QVERIFY(QMetaObject::invokeMethod(window, "leavePlayback"));
    QVERIFY(window->property("leavingWaitForSession").toBool());
    QVERIFY(window->property("leavingStopPending").toBool());
    QCOMPARE(daemon.commands.size(), beforeLateStop);
    QVERIFY(!library->isVisible());
    auto waitBound = window->findChild<QObject *>(QStringLiteral("leaveSessionBound"));
    QVERIFY(waitBound);
    QVERIFY(QMetaObject::invokeMethod(waitBound, "triggered"));
    QVERIFY(!window->property("leaveError").toString().isEmpty());
    QVERIFY(window->property("leavingStopPending").toBool());
    // A timeout does not allow another selection to replace the session
    // whose pending launch must still be stopped.
    QVERIFY(QMetaObject::invokeMethod(window, "openSession", Q_ARG(QVariant, entryId), Q_ARG(QVariant, false)));
    QVERIFY(QMetaObject::invokeMethod(window, "showPlaybackHome"));
    QVERIFY(window->property("leavingWaitForSession").toBool());
    QVERIFY(player->isVisible());
    daemon.page1 = {::session(QStringLiteral("late"), QStringLiteral("Late session"),
                            QStringLiteral("playing"), 0, monotonicUs(), 9000000,
                            {QStringLiteral("stop")})};
    catalog->refresh();
    QTRY_COMPARE(daemon.commands.size(), beforeLateStop + 1);
    QCOMPARE(daemon.commands.last(), QStringLiteral("Stop"));
    QTRY_VERIFY(!window->property("leavingStopPending").toBool());
    QTRY_VERIFY(!player->isVisible());
    QVERIFY(library->isVisible());
  }
  // #247: Continue on a stopped film resumes the library item; a stopped
  // session itself cannot be presented again.
  void resumeTargetOnlyForStoppedLibraryContent() {
    MediaDaemon daemon;
    auto stopped = session(QStringLiteral("stopped"), QStringLiteral("Film"), QStringLiteral("stopped"), 0, monotonicUs(), 9000000, {});
    stopped.hasContent = true;
    stopped.content = {QStringLiteral("library"), QStringLiteral("library"), QStringLiteral("film-1")};
    auto playing = session(QStringLiteral("playing"), QStringLiteral("Other"), QStringLiteral("playing"), 0, monotonicUs(), 9000000, {QStringLiteral("stop")});
    playing.hasContent = true;
    playing.content = {QStringLiteral("library"), QStringLiteral("library"), QStringLiteral("film-2")};
    auto bare = session(QStringLiteral("bare"), QStringLiteral("Song"), QStringLiteral("stopped"), 0, monotonicUs(), 9000000, {});
    daemon.page1 = {stopped, playing, bare};
    auto bus = QDBusConnection::sessionBus();
    QVERIFY(bus.registerService(QStringLiteral("org.arinn.Daemon1")));
    QVERIFY(bus.registerVirtualObject(QStringLiteral("/org/arinn"), &daemon, QDBusConnection::SubPath));
    MediaCatalog catalog;
    QTRY_COMPARE(catalog.continueEntries().size(), 3);
    QHash<QString, QString> ids;
    for (const auto &entry : catalog.continueEntries())
      ids.insert(entry.toMap().value(QStringLiteral("title")).toString(), entry.toMap().value(QStringLiteral("id")).toString());
    const auto target = catalog.resumeTarget(ids.value(QStringLiteral("Film")));
    QCOMPARE(target.value(QStringLiteral("provider")).toString(), QStringLiteral("library"));
    QCOMPARE(target.value(QStringLiteral("local")).toString(), QStringLiteral("film-1"));
    QVERIFY(catalog.resumeTarget(ids.value(QStringLiteral("Other"))).isEmpty());
    QVERIFY(catalog.resumeTarget(ids.value(QStringLiteral("Song"))).isEmpty());
    QVERIFY(catalog.resumeTarget(QStringLiteral("missing")).isEmpty());
  }
  void managedWindowResolutionCanBeCancelledWithoutStartingPresentation() {
    MediaDaemon daemon;
    daemon.features = {QStringLiteral("MediaWindow1")};
    daemon.windowReady = false;
    daemon.page1 = {session(QStringLiteral("live"), QStringLiteral("Piece"),
                            QStringLiteral("playing"), 0, monotonicUs(),
                            9000000, {QStringLiteral("stop")})};
    auto bus = QDBusConnection::sessionBus();
    QVERIFY(bus.registerService(QStringLiteral("org.arinn.Daemon1")));
    QVERIFY(bus.registerVirtualObject(QStringLiteral("/org/arinn"), &daemon,
                                      QDBusConnection::SubPath));
    MediaCatalog catalog;
    QTRY_VERIFY(catalog.windowAvailable());
    catalog.present();
    QTRY_VERIFY(daemon.windowReads > 0);
    QVERIFY(catalog.inputLayerActive());
    catalog.release();
    daemon.windowReady = true;
    QTest::qWait(350);
    QVERIFY(!daemon.windowCommands.contains(QStringLiteral("Present")));
    QCOMPARE(daemon.windowCommands, QStringList({QStringLiteral("Release")}));
    QVERIFY(!catalog.inputLayerActive());
    catalog.present();
    QTRY_COMPARE(catalog.presentationState(), QStringLiteral("active"));
    QSignalSpy home(&catalog, &MediaCatalog::presentationHomeRequested);
    catalog.homeRequested(QStringLiteral("old-epoch"), {}, {});
    QCOMPARE(home.count(), 0);
    catalog.homeRequested(QStringLiteral("epoch"), {}, {});
    QCOMPARE(home.count(), 1);
    daemon.windowReady = false;
    auto changed = QDBusMessage::createSignal(daemon.providers.first().path.path(), QStringLiteral("org.arinn.MediaWindow1"), QStringLiteral("Changed"));
    changed << daemon.pageGeneration;
    QVERIFY(bus.send(changed));
    QTRY_VERIFY(!catalog.inputLayerActive());
    QCOMPARE(catalog.sessionState(), QStringLiteral("ready"));
    daemon.windowReady = true;
    catalog.present(); QTRY_COMPARE(catalog.presentationState(), QStringLiteral("active"));
    daemon.page1.clear(); catalog.refresh();
    QTRY_VERIFY(!catalog.inputLayerActive());
    QCOMPARE(daemon.commands.size(), 0);
  }
  void pendingPresentationFailureRespectsFocus_data() {
    QTest::addColumn<bool>("loseFocus");
    QTest::newRow("lost-focus") << true;
    QTest::newRow("retained-focus") << false;
  }
  void pendingPresentationFailureRespectsFocus() {
    QFETCH(bool, loseFocus);
    MediaDaemon daemon;
    daemon.features = {QStringLiteral("MediaWindow1")};
    daemon.holdOperation = true;
    daemon.page1 = {session(QStringLiteral("live"), QStringLiteral("Piece"), QStringLiteral("paused"), 0, monotonicUs(), 9000000, {QStringLiteral("stop")})};
    auto bus = QDBusConnection::sessionBus();
    QVERIFY(bus.registerService(QStringLiteral("org.arinn.Daemon1")));
    QVERIFY(bus.registerVirtualObject(QStringLiteral("/org/arinn"), &daemon, QDBusConnection::SubPath));
    ShellStandIn backend;
    SessionGuardStandIn guard;
    QQmlApplicationEngine engine;
    installAssetImageProvider(&engine);
    engine.rootContext()->setContextProperty(QStringLiteral("backend"), &backend);
    engine.rootContext()->setContextProperty(QStringLiteral("sessionGuard"), &guard);
    engine.load(QUrl::fromLocalFile(QStringLiteral(ARINN_MAIN_QML)));
    auto window = qobject_cast<QQuickWindow *>(engine.rootObjects().value(0));
    QVERIFY(window);
    window->show(); window->requestActivate();
    QTRY_VERIFY(window->isActive());
    auto catalog = window->findChild<MediaCatalog *>(QStringLiteral("mediaCatalog"));
    QVERIFY(catalog);
    QTRY_VERIFY(catalog->windowAvailable());
    const auto entryId = catalog->continueEntries().at(0).toMap().value(QStringLiteral("id")).toString();
    QVERIFY(QMetaObject::invokeMethod(window, "openSession", Q_ARG(QVariant, entryId), Q_ARG(QVariant, false)));
    QTRY_COMPARE(daemon.windowCommands, QStringList({QStringLiteral("Present")}));
    QCOMPARE(catalog->presentationState(), QStringLiteral("pending"));
    backend.m_home = false; emit backend.changed();
    QQuickWindow other;
    if (loseFocus) {
      other.show(); other.requestActivate();
      QTRY_VERIFY(other.isActive());
      QTRY_VERIFY(!window->isActive());
    } else {
      QTRY_VERIFY(window->isActive());
    }
    daemon.rejectOperationRead = true;
    QTRY_COMPARE(catalog->presentationState(), QStringLiteral("Unavailable"));
    QTRY_COMPARE(daemon.windowCommands, QStringList({QStringLiteral("Present"), QStringLiteral("Release")}));
    QCOMPARE(daemon.homeCommands, loseFocus ? 0 : 1);
    QVERIFY(daemon.commands.isEmpty());
    if (loseFocus) QVERIFY(other.isActive());
  }
  void failedPresentationOperationReleasesItsOriginalScope_data() {
    QTest::addColumn<bool>("readFailure");
    QTest::newRow("unobservable-operation") << true;
    QTest::newRow("operation-timeout") << false;
  }
  void failedPresentationOperationReleasesItsOriginalScope() {
    QFETCH(bool, readFailure);
    MediaDaemon daemon;
    daemon.features = {QStringLiteral("MediaWindow1")};
    daemon.rejectOperationRead = readFailure;
    daemon.holdOperation = !readFailure;
    daemon.page1 = {session(QStringLiteral("live"), QStringLiteral("Piece"), QStringLiteral("paused"), 0, monotonicUs(), 9000000, {QStringLiteral("stop")})};
    auto bus = QDBusConnection::sessionBus();
    QVERIFY(bus.registerService(QStringLiteral("org.arinn.Daemon1")));
    QVERIFY(bus.registerVirtualObject(QStringLiteral("/org/arinn"), &daemon, QDBusConnection::SubPath));
    MediaCatalog catalog;
    QTRY_VERIFY(catalog.windowAvailable());
    catalog.present();
    QTRY_COMPARE_WITH_TIMEOUT(catalog.presentationState(), readFailure ? QStringLiteral("Unavailable") : QStringLiteral("Timeout"), 8000);
    QTRY_COMPARE(daemon.windowCommands, QStringList({QStringLiteral("Present"), QStringLiteral("Release")}));
    QVERIFY(!catalog.inputLayerActive());
    QCOMPARE(daemon.lastLocal, QStringLiteral("live"));
    QCOMPARE(daemon.lastGeneration, daemon.pageGeneration);
    catalog.refresh(); QTest::qWait(300);
    QCOMPARE(daemon.windowCommands.size(), 2);
    QVERIFY(daemon.commands.isEmpty());
  }
  void failedReleaseRetainsScopeForExplicitRetry_data() {
    QTest::addColumn<QString>("failure");
    for (const auto *value : {"unobservable", "timeout", "operation", "admission"})
      QTest::newRow(value) << QString::fromLatin1(value);
  }
  void failedReleaseRetainsScopeForExplicitRetry() {
    QFETCH(QString, failure);
    MediaDaemon daemon;
    daemon.features = {QStringLiteral("MediaWindow1")};
    daemon.page1 = {session(QStringLiteral("live"), QStringLiteral("Piece"), QStringLiteral("paused"), 0, monotonicUs(), 9000000, {QStringLiteral("stop")})};
    auto bus = QDBusConnection::sessionBus();
    QVERIFY(bus.registerService(QStringLiteral("org.arinn.Daemon1")));
    QVERIFY(bus.registerVirtualObject(QStringLiteral("/org/arinn"), &daemon, QDBusConnection::SubPath));
    MediaCatalog catalog;
    QTRY_VERIFY(catalog.windowAvailable());
    catalog.present(); QTRY_COMPARE(catalog.presentationState(), QStringLiteral("active"));
    daemon.rejectOperationRead = failure == QLatin1String("unobservable");
    daemon.holdOperation = failure == QLatin1String("timeout");
    daemon.rejectRelease = failure == QLatin1String("admission");
    daemon.operationState = failure == QLatin1String("operation") ? QStringLiteral("failed") : QStringLiteral("succeeded");
    catalog.release();
    QTRY_COMPARE_WITH_TIMEOUT(catalog.presentationState(), QStringLiteral("ReleaseFailed"), 8000);
    QVERIFY(!catalog.inputLayerActive());
    QCOMPARE(daemon.windowCommands, QStringList({QStringLiteral("Present"), QStringLiteral("Release")}));
    const auto originalRequest = daemon.lastRequest;
    daemon.rejectOperationRead = false; daemon.holdOperation = false; daemon.rejectRelease = false; daemon.operationState = QStringLiteral("succeeded");
    catalog.release();
    QTRY_COMPARE(daemon.windowCommands.count(QStringLiteral("Release")), 2);
    QVERIFY(daemon.lastRequest != originalRequest);
    QTRY_COMPARE(catalog.presentationState(), QString());
    QTest::qWait(300);
    catalog.release(); QTest::qWait(150);
    QCOMPARE(daemon.windowCommands.count(QStringLiteral("Release")), 2);
    QCOMPARE(daemon.lastLocal, QStringLiteral("live"));
    QVERIFY(daemon.commands.isEmpty());
  }
  void newPresentationSupersedesFailedReleaseScope() {
    MediaDaemon daemon;
    daemon.features = {QStringLiteral("MediaWindow1")};
    daemon.page1 = {session(QStringLiteral("old"), QStringLiteral("Old"), QStringLiteral("paused"), 0, monotonicUs(), 9000000, {QStringLiteral("stop")}), session(QStringLiteral("new"), QStringLiteral("New"), QStringLiteral("paused"), 0, monotonicUs(), 9000000, {QStringLiteral("stop")})};
    auto bus = QDBusConnection::sessionBus();
    QVERIFY(bus.registerService(QStringLiteral("org.arinn.Daemon1")));
    QVERIFY(bus.registerVirtualObject(QStringLiteral("/org/arinn"), &daemon, QDBusConnection::SubPath));
    MediaCatalog catalog; QTRY_VERIFY(catalog.windowAvailable());
    catalog.present(); QTRY_COMPARE(catalog.presentationState(), QStringLiteral("active"));
    daemon.rejectOperationRead = true;
    catalog.release(); QTRY_COMPARE(catalog.presentationState(), QStringLiteral("ReleaseFailed"));
    daemon.rejectOperationRead = false;
    for (const auto &entry : catalog.continueEntries()) {
      const auto row = entry.toMap();
      if (row.value(QStringLiteral("title")).toString() == QLatin1String("New"))
        catalog.select(row.value(QStringLiteral("id")).toString());
    }
    catalog.present(); QTRY_COMPARE(catalog.presentationState(), QStringLiteral("active"));
    QCOMPARE(daemon.lastLocal, QStringLiteral("new"));
    catalog.release(); QTRY_COMPARE(daemon.windowCommands.count(QStringLiteral("Release")), 2);
    QCOMPARE(daemon.lastLocal, QStringLiteral("new"));
    QTest::qWait(300); catalog.release(); QTest::qWait(150);
    QCOMPARE(daemon.windowCommands.count(QStringLiteral("Release")), 2);
    QVERIFY(daemon.commands.isEmpty());
  }
  void oldPresentationCleanupDoesNotEraseAnotherReleaseRetry() {
    MediaDaemon daemon;
    daemon.features = {QStringLiteral("MediaWindow1")};
    daemon.page1 = {session(QStringLiteral("old"), QStringLiteral("Old"), QStringLiteral("paused"), 0, monotonicUs(), 9000000, {QStringLiteral("stop")}), session(QStringLiteral("new"), QStringLiteral("New"), QStringLiteral("paused"), 0, monotonicUs(), 9000000, {QStringLiteral("stop")})};
    auto bus = QDBusConnection::connectToBus(QDBusConnection::SessionBus, QStringLiteral("delayed-media-provider"));
    const auto disconnect = qScopeGuard([] { QDBusConnection::disconnectFromBus(QStringLiteral("delayed-media-provider")); });
    QVERIFY(bus.registerService(QStringLiteral("org.arinn.Daemon1")));
    QVERIFY(bus.registerVirtualObject(QStringLiteral("/org/arinn"), &daemon, QDBusConnection::SubPath));
    MediaCatalog catalog; QTRY_VERIFY(catalog.windowAvailable());
    for (const auto &entry : catalog.continueEntries()) {
      const auto row = entry.toMap();
      if (row.value(QStringLiteral("title")).toString() == QLatin1String("Old"))
        catalog.select(row.value(QStringLiteral("id")).toString());
    }
    QTRY_VERIFY(catalog.windowAvailable());
    daemon.delayPresentAdmission = true;
    catalog.present(); QTRY_COMPARE(daemon.windowCommands.count(QStringLiteral("Present")), 1);
    daemon.delayPresentAdmission = false;
    for (const auto &entry : catalog.continueEntries()) {
      const auto row = entry.toMap();
      if (row.value(QStringLiteral("title")).toString() == QLatin1String("New"))
        catalog.select(row.value(QStringLiteral("id")).toString());
    }
    QTRY_VERIFY(catalog.windowAvailable());
    catalog.present(); QTRY_COMPARE(daemon.windowCommands.count(QStringLiteral("Present")), 2);
    QTRY_COMPARE(catalog.presentationState(), QStringLiteral("active"));
    daemon.rejectOperationRead = true;
    catalog.release(); QTRY_COMPARE(catalog.presentationState(), QStringLiteral("ReleaseFailed"));
    daemon.rejectOperationRead = false;
    bus.send(daemon.delayedPresent.createReply(QVariant::fromValue(QDBusObjectPath(QStringLiteral("/org/arinn/Daemon1/ops/media")))));
    QTRY_COMPARE(daemon.windowCommands.count(QStringLiteral("Release")), 3);
    QTest::qWait(300);
    QCOMPARE(daemon.lastLocal, QStringLiteral("old"));
    catalog.release(); QTRY_COMPARE(daemon.windowCommands.count(QStringLiteral("Release")), 4);
    QCOMPARE(daemon.lastLocal, QStringLiteral("new"));
    QVERIFY(daemon.commands.isEmpty());
  }
  void homeBackRetriesFailedReleaseWithoutReturnTarget_data() {
    QTest::addColumn<int>("retryKey");
    QTest::newRow("escape") << int(Qt::Key_Escape);
    QTest::newRow("backspace") << int(Qt::Key_Backspace);
    QTest::newRow("native-back") << int(Qt::Key_Back);
  }
  void homeBackRetriesFailedReleaseWithoutReturnTarget() {
    QFETCH(int, retryKey);
    MediaDaemon daemon;
    daemon.features = {QStringLiteral("MediaWindow1")};
    daemon.page1 = {session(QStringLiteral("live"), QStringLiteral("Piece"), QStringLiteral("paused"), 0, monotonicUs(), 9000000, {QStringLiteral("stop")})};
    auto bus = QDBusConnection::sessionBus();
    QVERIFY(bus.registerService(QStringLiteral("org.arinn.Daemon1")));
    QVERIFY(bus.registerVirtualObject(QStringLiteral("/org/arinn"), &daemon, QDBusConnection::SubPath));
    ShellStandIn backend; SessionGuardStandIn guard;
    QQmlApplicationEngine engine; installAssetImageProvider(&engine);
    engine.rootContext()->setContextProperty(QStringLiteral("backend"), &backend);
    engine.rootContext()->setContextProperty(QStringLiteral("sessionGuard"), &guard);
    engine.load(QUrl::fromLocalFile(QStringLiteral(ARINN_MAIN_QML)));
    auto window = qobject_cast<QQuickWindow *>(engine.rootObjects().value(0));
    QVERIFY(window); window->show(); window->requestActivate(); QTRY_VERIFY(window->isActive());
    auto catalog = window->findChild<MediaCatalog *>(QStringLiteral("mediaCatalog"));
    auto home = window->findChild<QQuickItem *>(QStringLiteral("productionHome"));
    QVERIFY(catalog); QVERIFY(home); QTRY_VERIFY(catalog->windowAvailable());
    catalog->present(); QTRY_COMPARE(catalog->presentationState(), QStringLiteral("active"));
    daemon.rejectOperationRead = true;
    catalog->release(); QTRY_COMPARE(catalog->presentationState(), QStringLiteral("ReleaseFailed"));
    const auto originalRequest = daemon.lastRequest;
    daemon.rejectOperationRead = false;
    backend.m_editor = {{QStringLiteral("id"), QStringLiteral("fixture")}, {QStringLiteral("title"), QStringLiteral("Fixture")}, {QStringLiteral("url"), QStringLiteral("https://example.test/")}};
    emit backend.changed(); QTRY_VERIFY(home->property("shortcutEditorOpen").toBool());
    auto sheet = home->findChild<QQuickItem *>(QStringLiteral("shortcutSheet"));
    QVERIFY(sheet); QTRY_VERIFY(sheet->hasActiveFocus());
    QTest::keyClick(window, Qt::Key_Escape);
    QTRY_VERIFY(!home->property("shortcutEditorOpen").toBool());
    QCOMPARE(daemon.windowCommands.count(QStringLiteral("Release")), 1);
    QVERIFY(QMetaObject::invokeMethod(home, "openNavigation"));
    QTRY_VERIFY(home->property("navOpen").toBool());
    QTest::keyClick(window, Qt::Key_Escape);
    QTRY_VERIFY(!home->property("navOpen").toBool());
    QCOMPARE(daemon.windowCommands.count(QStringLiteral("Release")), 1);
    QTest::keyClick(window, Qt::Key(retryKey));
    QTRY_COMPARE_WITH_TIMEOUT(daemon.windowCommands.count(QStringLiteral("Release")), 2, 2000);
    QVERIFY(daemon.lastRequest != originalRequest);
    QCOMPARE(daemon.lastLocal, QStringLiteral("live"));
    QCOMPARE(daemon.lastGeneration, daemon.pageGeneration);
    QTRY_COMPARE(catalog->presentationState(), QString());
    QCOMPARE(home->property("navOpen").toBool(), false);
    QTest::qWait(300);
    QTest::keyClick(window, Qt::Key_Escape);
    QTRY_VERIFY(home->property("navOpen").toBool());
    QCOMPARE(daemon.windowCommands.count(QStringLiteral("Release")), 2);
    QVERIFY(daemon.commands.isEmpty());
  }
  void thumbnailRequestWhilePlaying() {
    MediaDaemon daemon;
    daemon.features = {QStringLiteral("MediaTimeline1")};
    const auto anchor = monotonicUs();
    daemon.page1 = {session(QStringLiteral("live"), QStringLiteral("Moving"),
                            QStringLiteral("playing"), 0, anchor, 120000000,
                            {QStringLiteral("pause"), QStringLiteral("seek")})};
    auto bus = QDBusConnection::sessionBus();
    QVERIFY(bus.registerService(QStringLiteral("org.arinn.Daemon1")));
    QVERIFY(bus.registerVirtualObject(QStringLiteral("/org/arinn"), &daemon,
                                      QDBusConnection::SubPath));
    MediaCatalog catalog;
    QTRY_COMPARE(catalog.sessionState(), QStringLiteral("ready"));
    QVERIFY(catalog.timelineAvailable());
    const auto start = catalog.positionUs();
    catalog.setThumbnailWanted(true);
    QTRY_VERIFY_WITH_TIMEOUT(daemon.thumbnailReads >= 1, 3000);
    QTest::qWait(250);
    QVERIFY(catalog.positionUs() > start);
    QCOMPARE(daemon.commands, QStringList());
  }
  void thumbnailResumesAfterTimelineFocusReturns() {
    MediaDaemon daemon;
    daemon.features = {QStringLiteral("MediaTimeline1")};
    daemon.page1 = {session(QStringLiteral("live"), QStringLiteral("Paused"),
                            QStringLiteral("paused"), 1000000, monotonicUs(),
                            20000000, {QStringLiteral("seek")})};
    auto bus = QDBusConnection::sessionBus();
    QVERIFY(bus.registerService(QStringLiteral("org.arinn.Daemon1")));
    QVERIFY(bus.registerVirtualObject(QStringLiteral("/org/arinn"), &daemon,
                                      QDBusConnection::SubPath));
    MediaCatalog catalog;
    QTRY_COMPARE(catalog.sessionState(), QStringLiteral("ready"));
    catalog.setThumbnailWanted(true);
    QTRY_VERIFY(daemon.thumbnailReads >= 1);
    const auto before = daemon.thumbnailReads;
    catalog.seek(8000000);
    catalog.setThumbnailWanted(false);
    QTest::qWait(500);
    QCOMPARE(daemon.thumbnailReads, before);
    catalog.setThumbnailWanted(true);
    QTRY_VERIFY(daemon.thumbnailReads > before);
    QCOMPARE(daemon.commands, QStringList({QStringLiteral("Seek")}));
  }
  void shellHidesMissingFlagControls() {
    const auto show = [](const QStringList &features, bool tracksOn, bool timelineOn) {
      MediaDaemon daemon;
      daemon.features = features;
      if (tracksOn) {
        daemon.tracks = {TrackWire{QStringLiteral("a1"), QStringLiteral("audio"),
                                   QStringLiteral("eng"), QStringLiteral("English"),
                                   QStringLiteral("aac"), 2, true, false}};
      }
      daemon.chapters = {ChapterWire{QStringLiteral("c1"), QStringLiteral("Open"),
                                     1000000}};
      daemon.page1 = {session(QStringLiteral("live"), QStringLiteral("Piece"),
                              QStringLiteral("paused"), 1000000, monotonicUs(),
                              9000000, {QStringLiteral("seek")})};
      auto bus = QDBusConnection::sessionBus();
      if (!bus.registerService(QStringLiteral("org.arinn.Daemon1")) ||
          !bus.registerVirtualObject(QStringLiteral("/org/arinn"), &daemon,
                                     QDBusConnection::SubPath))
        return QStringLiteral("bus");
      ShellStandIn backend;
      SessionGuardStandIn guard;
      QQmlApplicationEngine engine;
      QStringList problems;
      QObject::connect(&engine, &QQmlEngine::warnings, &engine,
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
      if (!problems.isEmpty())
        return problems.join(QLatin1Char('\n'));
      auto window = qobject_cast<QQuickWindow *>(engine.rootObjects().value(0));
      if (!window)
        return QStringLiteral("window");
      window->show();
      window->requestActivate();
      auto catalog = window->findChild<MediaCatalog *>(QStringLiteral("mediaCatalog"));
      auto player = window->findChild<QQuickItem *>(QStringLiteral("productionPlayer"));
      if (!catalog || !player)
        return QStringLiteral("player");
      if (!QTest::qWaitFor([&] { return catalog->playerSession(); }, 5000))
        return QStringLiteral("session");
      const auto entryId = catalog->continueEntries()
                               .value(0)
                               .toMap()
                               .value(QStringLiteral("id"))
                               .toString();
      if (!QMetaObject::invokeMethod(window, "openSession", Q_ARG(QVariant, entryId),
                                     Q_ARG(QVariant, false)))
        return QStringLiteral("open");
      if (!QTest::qWaitFor(
              [&] { return player->property("visible").toBool(); }, 5000))
        return QStringLiteral("visible");
      if (player->property("tracksSupported").toBool() != tracksOn)
        return QStringLiteral("tracksSupported");
      if (player->property("timelineSupported").toBool() != timelineOn)
        return QStringLiteral("timelineSupported");
      auto audio = window->findChild<QQuickItem *>(QStringLiteral("audioPill"));
      auto subtitles = window->findChild<QQuickItem *>(QStringLiteral("subtitlePill"));
      auto thumb = window->findChild<QQuickItem *>(QStringLiteral("timelineThumb"));
      auto more = window->findChild<QQuickItem *>(QStringLiteral("morePill"));
      if (!audio || !subtitles || !thumb || !more)
        return QStringLiteral("controls");
      if (audio->isVisible() != tracksOn || subtitles->isVisible() != tracksOn)
        return QStringLiteral("pills");
      if (thumb->isVisible() != timelineOn)
        return QStringLiteral("thumbnail");
      if (!more->isVisible())
        return QStringLiteral("more");
      return QString();
    };
    const auto tracksOnly = show({QStringLiteral("MediaTracks1")}, true, false);
    QVERIFY2(tracksOnly.isEmpty(), qPrintable(tracksOnly));
    cleanup();
    const auto timelineOnly = show({QStringLiteral("MediaTimeline1")}, false, true);
    QVERIFY2(timelineOnly.isEmpty(), qPrintable(timelineOnly));
  }
};

int main(int argc, char **argv) {
  QGuiApplication app(argc, argv);
  MediaCatalogTest test;
  return QTest::qExec(&test, argc, argv);
}
#include "media_catalog_test.moc"
