// SPDX-FileCopyrightText: 2026 Arinn contributors
//
// SPDX-License-Identifier: GPL-3.0-or-later

#include "media_catalog.h"
#include <QGuiApplication>
#include <QQmlApplicationEngine>
#include <QQmlContext>
#include <QQuickItem>
#include <QQuickWindow>
#include <QSignalSpy>
#include <QTest>
#include <QtDBus>
#include <time.h>
namespace {
quint64 monotonicUs() {
  timespec ts{};
  if (clock_gettime(CLOCK_MONOTONIC, &ts) != 0)
    return 0;
  return static_cast<quint64>(ts.tv_sec) * 1000000ull +
         static_cast<quint64>(ts.tv_nsec) / 1000ull;
}
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
  QString operationState = QStringLiteral("succeeded");
  QString operationError;
  int sessionReads = 0;
  int operationReads = 0;
  QStringList commands;
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
      bus.send(message.createReply(QVariant::fromValue(
          QDBusObjectPath(QStringLiteral("/org/arinn/Daemon1/ops/media")))));
      return true;
    } else if (message.member() == QLatin1String("Get") &&
               message.interface() == QLatin1String("org.arinn.Operation1")) {
      ++operationReads;
      body.beginStructure();
      body << QString(holdOperation ? QStringLiteral("running") : operationState)
           << operationError << false << qulonglong(1) << qulonglong(2);
      body.beginArray(QMetaType::fromType<EntityWire>());
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
    qDBusRegisterMetaType<ContentWire>();
    qDBusRegisterMetaType<SessionWire>();
    qDBusRegisterMetaType<ProviderWire>();
    qDBusRegisterMetaType<ShortcutWire>();
    qDBusRegisterMetaType<EntityWire>();
  }
  void cleanup() {
    auto bus = QDBusConnection::sessionBus();
    bus.unregisterObject(QStringLiteral("/org/arinn"),
                         QDBusConnection::UnregisterTree);
    bus.unregisterService(QStringLiteral("org.arinn.Daemon1"));
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
    for (const auto &entry : entries) {
      QVERIFY(entry.toMap().value(QStringLiteral("kind")).toString() ==
              QLatin1String("media"));
      QVERIFY(!entry.toMap().contains(QStringLiteral("artwork")));
    }
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
};

int main(int argc, char **argv) {
  QGuiApplication app(argc, argv);
  MediaCatalogTest test;
  return QTest::qExec(&test, argc, argv);
}
#include "media_catalog_test.moc"
