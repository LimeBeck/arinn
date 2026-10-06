// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

#include "backend.h"
#include "web_icon_references.h"
#include <QUuid>
#include <QtDBus>
#include <QtTest>

struct ProviderWire {
  QString id;
};
struct AppWire {
  QString id;
  bool enabled = true;
  QString group = "games";
  QString icon;
};
struct RunningWire {
  QString id;
};
struct HomeWire {};
struct ShortcutWire {
  QString id, title, url, icon;
};
struct TargetWire {
  QString provider, capability, id;
};
Q_DECLARE_METATYPE(ProviderWire)
Q_DECLARE_METATYPE(AppWire)
Q_DECLARE_METATYPE(RunningWire)
Q_DECLARE_METATYPE(HomeWire)
Q_DECLARE_METATYPE(ShortcutWire)
Q_DECLARE_METATYPE(TargetWire)
QDBusArgument &operator<<(QDBusArgument &a, const ProviderWire &p) {
  a.beginStructure();
  a << p.id << QString("plugin") << QString() << QString("Provider")
    << QString("1") << QDBusObjectPath("/org/arinn/Provider/" + p.id)
    << QStringList{"application"} << QString("ready") << QString("available")
    << QString("generation") << qulonglong(1) << QString() << qulonglong(0)
    << qulonglong(0) << qulonglong(0) << qulonglong(0);
  a.endStructure();
  return a;
}
QDBusArgument &operator<<(QDBusArgument &a, const AppWire &p) {
  a.beginStructure();
  a << p.id << QString("Application") << p.enabled << p.group << p.icon;
  a.endStructure();
  return a;
}
QDBusArgument &operator<<(QDBusArgument &a, const RunningWire &p) {
  a.beginStructure();
  a << p.id << QString("application") << QString("Window")
    << QString("window-token") << true << false;
  a.endStructure();
  return a;
}
QDBusArgument &operator<<(QDBusArgument &a, const HomeWire &) {
  a.beginStructure();
  a << QString() << QString() << QString() << QString() << QString();
  a.endStructure();
  return a;
}
// These fixture-only records are sent, never received.
const QDBusArgument &operator>>(const QDBusArgument &a, ProviderWire &) {
  return a;
}
const QDBusArgument &operator>>(const QDBusArgument &a, AppWire &) { return a; }
const QDBusArgument &operator>>(const QDBusArgument &a, RunningWire &) {
  return a;
}
const QDBusArgument &operator>>(const QDBusArgument &a, HomeWire &) {
  return a;
}
QDBusArgument &operator<<(QDBusArgument &a, const ShortcutWire &p) {
  a.beginStructure();
  a << p.id << p.title << p.url << p.icon;
  a.endStructure();
  return a;
}
QDBusArgument &operator<<(QDBusArgument &a, const TargetWire &p) {
  a.beginStructure();
  a << p.provider << p.capability << p.id;
  a.endStructure();
  return a;
}
const QDBusArgument &operator>>(const QDBusArgument &a, ShortcutWire &) {
  return a;
}
const QDBusArgument &operator>>(const QDBusArgument &a, TargetWire &v) {
  a.beginStructure();
  a >> v.provider >> v.capability >> v.id;
  a.endStructure();
  return a;
}

class CatalogDaemon : public QDBusVirtualObject {
public:
  QString mode;
  QString returnedWindow;
  QString invokedMethod, invokedId, openInterface, openRequestId;
  QList<ShortcutWire> shortcuts;
  qulonglong shortcutRevision = 1;
  bool listError = false;
  bool operationDone = true;
  bool operationSuccess = false;
  bool platform = true;
  QString operationError = QStringLiteral("Unavailable");
  int pages = 0;
  int launches = 0;
  int opens = 0;
  int mutations = 0;
  QString mutationError;
  QString mutationMethod;
  QVariantList mutationArgs;
  int lists = 0;
  int providerCalls = 0;
  QList<TargetWire> favorites;
  qulonglong favoriteRevision = 1;
  int favoriteWrites = 0;
  QString favoriteFailure;
  bool favoriteListError = false;
  QString introspect(const QString &) const override { return {}; }
  bool handleMessage(const QDBusMessage &m,
                     const QDBusConnection &bus) override {
    QDBusArgument a;
    if (m.member() == "Status") {
      a.beginStructure();
      const QString epoch = "77777777-7777-4777-8777-777777777777";
      a << epoch << true << epoch << qulonglong(1)
        << true << returnedWindow << QString() << QString() << false << platform
        << QString() << QString();
      a.endStructure();
    } else if (m.member() == "HomeItems") {
      a << QList<HomeWire>{};
    } else if (m.member() == "Providers") {
      ++providerCalls;
      if (mode == "error") {
        bus.send(m.createErrorReply(QDBusError::Failed, "Unavailable"));
        return true;
      }
      a.beginStructure();
      a << QString("snapshot")
        << (mode == "empty"
                ? QList<ProviderWire>{}
                : (mode == "partial" ? QList<ProviderWire>{{"broken"}, {"test"}}
                                     : QList<ProviderWire>{{"test"}}))
        << QString();
      a.endStructure();
    } else if (m.member() == "GetApplications") {
      if (m.path().endsWith("/broken")) {
        bus.send(m.createErrorReply(QDBusError::Failed, "Unavailable"));
        return true;
      }
      ++pages;
      bool first = m.arguments().at(0).toString().isEmpty();
      a.beginStructure();
      a << (mode == "generation" && !first ? QString("stale")
                                           : QString("generation"))
        << qulonglong(mode == "revision" && !first ? 2 : 1)
        << QList<AppWire>{{first ? "first" : "second", first,
                           first ? "games" : "future-group",
                           first ? QStringLiteral("applications-games")
                                 : QStringLiteral("/usr/share/icons/hicolor/"
                                                  "48x48/apps/fixture.png")}}
        << QList<RunningWire>{{first ? "window-first" : "window-second"}}
        << (first || mode == "cycle" ? QString("next") : QString());
      a.endStructure();
    } else if (m.member() == "Activate") {
      invokedMethod = m.member();
      invokedId = m.arguments().at(1).toString();
      bus.send(m.createReply(QVariant::fromValue(
          QDBusObjectPath("/org/arinn/Daemon1/operations/1"))));
      return true;
    } else if (m.member() == "Launch") {
      ++launches;
      invokedMethod = m.member();
      invokedId = m.arguments().at(1).toString();
      bus.send(
          m.createErrorReply(QDBusError::Failed, "fixture operation failure"));
      return true;
    } else if (m.interface() == "org.arinn.Favorites1" &&
               m.member() == "List") {
      if (favoriteListError) {
        bus.send(m.createErrorReply("org.arinn.Error.Unavailable", "fixture"));
        return true;
      }
      a.beginStructure();
      a << favoriteRevision << favorites;
      a.endStructure();
    } else if (m.interface() == "org.arinn.Favorites1" && m.member() == "Set") {
      ++favoriteWrites;
      if (!favoriteFailure.isEmpty() ||
          m.arguments().first().toULongLong() != favoriteRevision) {
        bus.send(
            m.createErrorReply("org.arinn.Error." + (favoriteFailure.isEmpty()
                                                         ? QString("Conflict")
                                                         : favoriteFailure),
                               "synthetic private payload"));
        return true;
      }
      favorites = qdbus_cast<QList<TargetWire>>(m.arguments().at(1));
      ++favoriteRevision;
      bus.send(m.createReply(QVariant::fromValue(favoriteRevision)));
      auto signal = QDBusMessage::createSignal(
          "/org/arinn/Daemon1", "org.arinn.Favorites1", "Changed");
      signal << favoriteRevision;
      bus.send(signal);
      return true;
    } else if (m.member() == "List") {
      ++lists;
      if (listError) {
        bus.send(m.createErrorReply(QDBusError::Failed, "list failed"));
        return true;
      }
      a.beginStructure();
      a << shortcutRevision << shortcuts;
      a.endStructure();
    } else if (m.member() == "Add" || m.member() == "Update" || m.member() == "Remove") {
      ++mutations;
      mutationMethod = m.member();
      mutationArgs = m.arguments();
      if (!mutationError.isEmpty())
        bus.send(m.createErrorReply("org.arinn.Error." + mutationError, "synthetic private payload"));
      else if (m.member() == "Add")
        bus.send(m.createReply(QString("new-shortcut")));
      else
        bus.send(m.createReply());
      return true;
    } else if (m.member() == "Open") {
      ++opens;
      openInterface = m.interface();
      invokedMethod = m.member();
      invokedId = m.arguments().at(0).toString();
      openRequestId = m.arguments().at(1).toString();
      bus.send(m.createReply(QVariant::fromValue(
          QDBusObjectPath("/org/arinn/Daemon1/operations/1"))));
      return true;
    } else if (m.member() == "Get") {
      const bool done = operationDone;
      a.beginStructure();
      a << (done ? (operationSuccess ? QString("succeeded") : QString("failed"))
                 : QString("running"))
        << (done && !operationSuccess ? operationError : QString())
        << (done && !operationSuccess) << qulonglong(1)
        << qulonglong(done ? 2 : 0) << QList<TargetWire>{} << false;
      a.endStructure();
    } else {
      bus.send(m.createErrorReply(QDBusError::NotSupported, "fixture"));
      return true;
    }
    bus.send(m.createReply(QVariant::fromValue(a)));
    return true;
  }
};
class CatalogTest : public QObject {
  Q_OBJECT
private slots:
  void savedFavoritesMutationAndUnavailableRetention() {
    auto bus = QDBusConnection::sessionBus();
    CatalogDaemon daemon;
    daemon.mode = "populated";
    daemon.shortcuts = {
        {"saved-site", "Saved site", "https://example.test/saved", ""}};
    QVERIFY(bus.registerService("org.arinn.Daemon1"));
    QVERIFY(bus.registerVirtualObject("/org/arinn", &daemon,
                                      QDBusConnection::SubPath));
    {
      Backend backend;
      QTRY_VERIFY(backend.favoritesReady());
      QTRY_COMPARE(backend.applications().size(), 3);
      QVERIFY(backend.favorites().isEmpty());
      const auto app =
          backend.applications().first().toMap().value("id").toString();
      backend.setFavorite(app, true);
      backend.setFavorite(app, true);
      QTRY_COMPARE(backend.favorites().size(), 1);
      QCOMPARE(daemon.favoriteWrites, 1);
      QCOMPARE(daemon.favorites.first().provider, QString("test"));
      QCOMPARE(daemon.favorites.first().capability, QString("application"));
      QCOMPARE(daemon.favorites.first().id, QString("first"));
      QVERIFY(backend.applications().first().toMap().value("pinned").toBool());
      backend.setFavorite("saved-site", true);
      QTRY_COMPARE(backend.favorites().size(), 2);
      QCOMPARE(backend.favorites().last().toMap().value("id").toString(),
               QString("saved-site"));
      daemon.favoriteFailure = "Conflict";
      backend.setFavorite(app, false);
      QTRY_VERIFY(!backend.favoriteError().isEmpty());
      QTRY_VERIFY(!backend.favoriteSaving());
      QCOMPARE(backend.favorites().size(), 2);
      QVERIFY(!backend.favoriteError().contains("private payload"));
      const int writes = daemon.favoriteWrites;
      QTest::qWait(100);
      QCOMPARE(daemon.favoriteWrites, writes);
      daemon.favoriteFailure.clear();
      daemon.mode = "empty";
      backend.refresh();
      QTRY_COMPARE(backend.applications().size(), 1);
      QTRY_VERIFY(
          backend.favorites().first().toMap().value("unavailable").toBool());
      const auto missing =
          backend.favorites().first().toMap().value("id").toString();
      backend.setFavorite(missing, false);
      QTRY_COMPARE(backend.favorites().size(), 1);
      backend.setFavorite("saved-site", false);
      QTRY_VERIFY(backend.favorites().isEmpty());
      QVERIFY(daemon.favorites.isEmpty());
      // Reject malformed/regressing snapshots without replacing the saved
      // state.
      daemon.favoriteRevision = 0;
      backend.refresh();
      QTRY_VERIFY(!backend.favoritesReady());
      bus.unregisterService("org.arinn.Daemon1");
      QTRY_VERIFY(!backend.connected());
      daemon.favoriteRevision = 1;
      daemon.favorites = {{"arinn", "web-shortcut", "saved-site"}};
      QVERIFY(bus.registerService("org.arinn.Daemon1"));
      QTRY_VERIFY(backend.favoritesReady());
      QTRY_COMPARE(backend.favorites().size(), 1);
    }
    bus.unregisterObject("/org/arinn", QDBusConnection::UnregisterTree);
    bus.unregisterService("org.arinn.Daemon1");
  }

  void initTestCase() {
    qDBusRegisterMetaType<ProviderWire>();
    qDBusRegisterMetaType<AppWire>();
    qDBusRegisterMetaType<RunningWire>();
    qDBusRegisterMetaType<HomeWire>();
    qDBusRegisterMetaType<ShortcutWire>();
    qDBusRegisterMetaType<TargetWire>();
  }
  void catalog_data() {
    QTest::addColumn<QString>("mode");
    for (const auto &mode : {"populated", "empty", "error", "generation",
                             "revision", "cycle", "partial"})
      QTest::newRow(mode) << QString(mode);
  }
  void catalog() {
    QFETCH(QString, mode);
    auto bus = QDBusConnection::sessionBus();
    CatalogDaemon daemon;
    daemon.mode = mode;
    QVERIFY(bus.registerService("org.arinn.Daemon1"));
    QVERIFY(bus.registerVirtualObject("/org/arinn", &daemon,
                                      QDBusConnection::SubPath));
    {
      Backend backend;
      QTRY_VERIFY(backend.connected());
      const auto state =
          (mode == "populated" || mode == "empty") ? "empty" : "error";
      const auto applicationsState = mode == "populated" ? "ready"
                                     : mode == "empty"   ? "empty"
                                                         : "error";
      QTRY_COMPARE(backend.homeState(), QString(state));
      QTRY_COMPARE(backend.applicationsState(), QString(applicationsState));
      if (mode == "populated" || mode == "partial") {
        QTRY_COMPARE(backend.applications().size(), 2);
        QCOMPARE(backend.items().size(), 2);
        QCOMPARE(backend.items()[1].toMap()["localId"].toString(),
                 QString("window-second"));
        QCOMPARE(daemon.pages, 2);
        QCOMPARE(backend.applications()[1].toMap()["enabled"].toBool(), false);
        // Provider groups pass through; unknown groups display as "other".
        QCOMPARE(backend.applications()[0].toMap()["group"].toString(),
                 QString("games"));
        QCOMPARE(backend.applications()[1].toMap()["group"].toString(),
                 QString("other"));
        // ADR-0059: QML gets opaque image handles, never local references.
        for (const auto &entry : backend.applications()) {
          const auto map = entry.toMap();
          QVERIFY(!map.contains("icon"));
          QVERIFY(map.value("iconSource").toString().startsWith("image://arinn-icons/"));
          QVERIFY(!map.value("visualIdentity").toString().isEmpty());
          for (const auto &value : map) {
            QVERIFY(!value.toString().contains(QStringLiteral("applications-games")));
            QVERIFY(!value.toString().contains(QStringLiteral("fixture.png")));
          }
        }
        QCOMPARE(daemon.launches, 0);
        QCOMPARE(daemon.opens, 0);
        // An empty saved list never substitutes the installed catalog.
        QVERIFY(backend.favorites().isEmpty());
        QSignalSpy changes(&backend, &Backend::applicationsChanged);
        backend.refresh();
        QTest::qWait(200);
        QCOMPARE(changes.count(), 0);
        backend.launch(backend.applications()[1].toMap()["id"].toString());
        QCOMPARE(daemon.launches, 0);
        backend.launch(backend.applications()[0].toMap()["id"].toString());
        backend.launch(backend.applications()[0].toMap()["id"].toString());
        QTRY_COMPARE(daemon.invokedId, QString("first"));
        QCOMPARE(daemon.launches, 1);
        QCOMPARE(daemon.opens, 0);
        QTRY_COMPARE(backend.message(), QString("fixture operation failure"));
        // Return is a KWin internalId UUID, not an inventory
        // generation:counter.
        daemon.returnedWindow = "{aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa}";
        backend.refresh();
        QTRY_COMPARE(backend.continueEntries().size(), 1);
        QCOMPARE(backend.continueEntries().first().toMap().value("id").toString(),
                 daemon.returnedWindow);
        QVERIFY(!backend.continueEntries().first().toMap().contains("platformGeneration"));
        daemon.returnedWindow.clear();
        backend.refresh();
        QTRY_VERIFY(backend.continueEntries().isEmpty());
      } else
        QVERIFY(backend.applications().isEmpty());
      if (mode == "error") {
        // A failed catalog stays an application error while shortcuts load.
        // It must not be reported as an empty shortcut list.
        QCOMPARE(backend.homeState(), QString("error"));
        QCOMPARE(backend.applicationsState(), QString("error"));
        QVERIFY(backend.webShortcuts().isEmpty());
        daemon.shortcuts = {{QStringLiteral("keep"), QStringLiteral("Keep"),
                             QStringLiteral("https://example.test/keep"),
                             QString()}};
        daemon.shortcutRevision = 3;
        const int providersBefore = daemon.providerCalls;
        auto signal = QDBusMessage::createSignal(
            "/org/arinn/Daemon1", "org.arinn.WebShortcuts1", "Changed");
        signal << qulonglong(3);
        QVERIFY(bus.send(signal));
        QTRY_COMPARE(backend.webShortcuts().size(), 1);
        QCOMPARE(backend.homeState(), QString("error"));
        QCOMPARE(backend.applicationsState(), QString("error"));
        QCOMPARE(backend.applications().size(), 1);
        const auto online = backend.applications().at(0).toMap();
        QCOMPARE(online.value("id").toString(), QString("keep"));
        QCOMPARE(online.value("group").toString(), QString("online"));
        QVERIFY(online.value("webShortcut").toBool());
        QVERIFY(online.value("enabled").toBool());
        QVERIFY(!online.contains("url"));
        QVERIFY(backend.favorites().isEmpty());
        QCOMPARE(daemon.providerCalls, providersBefore);
        daemon.shortcuts.clear();
        daemon.shortcutRevision = 4;
        auto cleared = QDBusMessage::createSignal(
            "/org/arinn/Daemon1", "org.arinn.WebShortcuts1", "Changed");
        cleared << qulonglong(4);
        QVERIFY(bus.send(cleared));
        QTRY_VERIFY(backend.webShortcuts().isEmpty());
        QVERIFY(backend.applications().isEmpty());
        QCOMPARE(backend.homeState(), QString("error"));
      }
      daemon.mode = "empty";
      backend.refresh();
      QTRY_COMPARE(backend.homeState(), QString("empty"));
      bus.unregisterService("org.arinn.Daemon1");
      QTRY_COMPARE(backend.homeState(), QString("unavailable"));
      QVERIFY(backend.applications().isEmpty());
      QVERIFY(backend.returnWindow().isEmpty());
      QVERIFY(bus.registerService("org.arinn.Daemon1"));
      QTRY_COMPARE(backend.homeState(), QString("empty"));
    }
    bus.unregisterObject("/org/arinn", QDBusConnection::UnregisterTree);
    bus.unregisterService("org.arinn.Daemon1");
  }
  void shortcutEditorAndRevisionValidation() {
    auto bus = QDBusConnection::sessionBus();
    CatalogDaemon daemon;
    daemon.mode = "empty";
    daemon.shortcuts = {{"one", "Original", "https://example.test/one", "packaged"}};
    QVERIFY(bus.registerVirtualObject("/org/arinn", &daemon, QDBusConnection::SubPath));
    QVERIFY(bus.registerService("org.arinn.Daemon1"));
    {
      Backend backend;
      QTRY_COMPARE(backend.webShortcuts().size(), 1);
      backend.editShortcut("one");
      QCOMPARE(backend.shortcutEditor().value("url").toString(), QString("https://example.test/one"));
      daemon.shortcutRevision = 2;
      QMetaObject::invokeMethod(&backend, "reloadShortcuts");
      QTest::qWait(100);
      daemon.mutationError = "Conflict";
      backend.saveShortcut("Edited", "http://nas.local/item");
      backend.saveShortcut("Duplicate", "https://example.test/duplicate");
      QTRY_COMPARE(daemon.mutations, 1);
      QTRY_VERIFY(!backend.shortcutSaving());
      QCOMPARE(daemon.mutationMethod, QString("Update"));
      QCOMPARE(daemon.mutationArgs.at(1).toULongLong(), qulonglong(1));
      QCOMPARE(daemon.mutationArgs.at(4).toString(), QString("packaged"));
      QCOMPARE(backend.shortcutEditor().value("title").toString(), QString("Edited"));
      QVERIFY(backend.shortcutError().contains("changed"));
      QVERIFY(!backend.shortcutError().contains("payload"));
      backend.closeShortcutEditor();
      backend.editShortcut("one");
      // Remove must use the editor's captured revision even after a newer
      // complete snapshot is published. Observe its data, not a fixed delay.
      daemon.shortcutRevision = 3;
      daemon.shortcuts[0].title = "Changed elsewhere";
      auto changed = QDBusMessage::createSignal(
          "/org/arinn/Daemon1", "org.arinn.WebShortcuts1", "Changed");
      changed << qulonglong(3);
      QVERIFY(bus.send(changed));
      QTRY_COMPARE(backend.webShortcuts().at(0).toMap().value("title").toString(),
                   QString("Changed elsewhere"));
      backend.removeShortcut();
      QTRY_COMPARE(daemon.mutations, 2);
      QTRY_VERIFY(!backend.shortcutSaving());
      QCOMPARE(daemon.mutationMethod, QString("Remove"));
      QCOMPARE(daemon.mutationArgs.at(1).toULongLong(), qulonglong(2));
      QCOMPARE(backend.shortcutEditor().value("title").toString(), QString("Original"));
      QVERIFY(backend.shortcutError().contains("changed"));
      QVERIFY(!backend.shortcutError().contains("payload"));
      backend.closeShortcutEditor();
      backend.editShortcut("one");
      daemon.mutationError.clear();
      backend.removeShortcut();
      QTRY_COMPARE(daemon.mutations, 3);
      QTRY_VERIFY(backend.shortcutEditor().isEmpty());
      QCOMPARE(daemon.mutationArgs.at(1).toULongLong(), qulonglong(3));
      backend.editShortcut();
      backend.saveShortcut("New", "https://example.test/new");
      QTRY_COMPARE(daemon.mutations, 4);
      QTRY_VERIFY(backend.shortcutEditor().isEmpty());
      QCOMPARE(daemon.mutationMethod, QString("Add"));
      QVERIFY(!QUuid(daemon.mutationArgs.at(3).toString()).isNull());
      // A duplicate or regressing snapshot cannot publish a partial list.
      daemon.shortcutRevision = 4;
      daemon.shortcuts.append(daemon.shortcuts.first());
      QMetaObject::invokeMethod(&backend, "reloadShortcuts");
      QTRY_VERIFY(backend.webShortcuts().isEmpty());
      backend.editShortcut("one");
      QVERIFY(backend.shortcutEditor().isEmpty());
      daemon.shortcuts.removeLast();
      daemon.shortcutRevision = 1;
      QMetaObject::invokeMethod(&backend, "reloadShortcuts");
      QTest::qWait(100);
      QVERIFY(backend.webShortcuts().isEmpty());
      daemon.shortcutRevision = 5;
      QMetaObject::invokeMethod(&backend, "reloadShortcuts");
      QTRY_COMPARE(backend.webShortcuts().size(), 1);
      backend.editShortcut("one");
      QVERIFY(!backend.shortcutEditor().isEmpty());
      backend.saveShortcut("Pending", "https://example.test/pending");
      backend.closeShortcutEditor();
      backend.editShortcut("one");
      QVERIFY(backend.shortcutEditor().isEmpty());
      QTRY_VERIFY(!backend.shortcutSaving());
      backend.editShortcut("one");
      bus.unregisterService("org.arinn.Daemon1");
      QTRY_VERIFY(backend.shortcutEditor().isEmpty());
      QVERIFY(!backend.shortcutSaving());
    }
    bus.unregisterObject("/org/arinn", QDBusConnection::UnregisterTree);
    bus.unregisterService("org.arinn.Daemon1");
  }
  void webShortcuts() {
    auto bus = QDBusConnection::sessionBus();
    CatalogDaemon daemon;
    daemon.mode = "empty";
    {
      Backend backend;
      QTRY_COMPARE(backend.homeState(), QString("unavailable"));
      QStringList states;
      QObject::connect(&backend, &Backend::changed, &backend,
                       [&] { states << backend.homeState(); });
      QVERIFY(bus.registerVirtualObject("/org/arinn", &daemon,
                                        QDBusConnection::SubPath));
      QVERIFY(bus.registerService("org.arinn.Daemon1"));
      QTRY_COMPARE(backend.homeState(), QString("empty"));
      QVERIFY(states.contains(QString("loading")));
      QVERIFY(backend.webShortcuts().isEmpty());

      const auto north = QStringLiteral("https://example.test/north");
      const QString emoji = QString(QChar(0xD83C)) + QChar(0xDFAE);
      const QString broken = QStringLiteral("?");
      daemon.shortcuts = {{QStringLiteral("north"), QStringLiteral("north"),
                           north, QStringLiteral("packaged-mark")},
                          {QStringLiteral("south"), QStringLiteral("South"),
                           QStringLiteral("https://example.test/south"),
                           QString()},
                          {QStringLiteral("emoji"), emoji,
                           QStringLiteral("https://example.test/emoji"),
                           QString()},
                          {QStringLiteral("broken"), broken,
                           QStringLiteral("https://example.test/broken"),
                           QString()}};
      daemon.shortcutRevision = 2;
      const int listsBefore = daemon.lists;
      const int providersBefore = daemon.providerCalls;
      auto signal = QDBusMessage::createSignal(
          "/org/arinn/Daemon1", "org.arinn.WebShortcuts1", "Changed");
      signal << qulonglong(2);
      QVERIFY(bus.send(signal));
      QTRY_COMPARE(backend.webShortcuts().size(), 4);
      QCOMPARE(backend.applications().size(), 4);
      for (const auto &entry : backend.applications()) {
        const auto map = entry.toMap();
        QCOMPARE(map.value("group").toString(), QString("online"));
        QVERIFY(map.value("webShortcut").toBool());
        QVERIFY(!map.contains("url"));
        QVERIFY(!map.contains("icon"));
        for (const auto &value : map)
          QVERIFY(!value.toString().contains(QStringLiteral("example.test")));
      }
      QCOMPARE(daemon.lists, listsBefore + 1);
      QCOMPARE(daemon.providerCalls, providersBefore);
      const auto first = backend.webShortcuts().at(0).toMap();
      QVERIFY(first.value("iconSource").toString().isEmpty());
      const auto siteIcon=backend.webShortcuts().at(1).toMap().value("iconSource").toString();
      QVERIFY(siteIcon.startsWith("image://arinn-web-icons/"));
      const auto request=WebIconReferences::lookup(siteIcon.mid(QString("image://arinn-web-icons/").size()));
      QCOMPARE(request.owner,bus.baseService());
      QCOMPARE(request.revision,quint64(2));
      QCOMPARE(request.id,QString("south"));
      QCOMPARE(first.value("id").toString(), QString("north"));
      QCOMPARE(first.value("title").toString(), QString("north"));
      QCOMPARE(first.value("monogram").toString(), QString("N"));
      QCOMPARE(
          backend.webShortcuts().at(1).toMap().value("monogram").toString(),
          QString("S"));
      const auto emojiEntry = backend.webShortcuts().at(2).toMap();
      QCOMPARE(emojiEntry.value("monogram").toString(), emoji.toUpper());
      QVERIFY(emojiEntry.value("monogram").toString() !=
              QString(emoji.at(0)));
      QCOMPARE(
          backend.webShortcuts().at(3).toMap().value("monogram").toString(),
          QString("?"));
      for (const auto &entry : backend.webShortcuts()) {
        const auto map = entry.toMap();
        QVERIFY(!map.contains("url"));
        QVERIFY(!map.contains("icon"));
        for (const auto &value : map)
          QVERIFY(!value.toString().contains(QStringLiteral("example.test")));
      }
      QCOMPARE(backend.homeState(), QString("empty"));

      QSignalSpy same(&backend, &Backend::webShortcutsChanged);
      backend.refresh();
      QTest::qWait(200);
      QCOMPARE(same.count(), 0);

      daemon.operationDone = false;
      const auto id = first.value("id").toString();
      backend.launch(id);
      backend.launch(id);
      QTRY_COMPARE(daemon.opens, 1);
      QCOMPARE(daemon.launches, 0);
      QCOMPARE(daemon.invokedMethod, QString("Open"));
      QCOMPARE(daemon.openInterface, QString("org.arinn.WebShortcuts1"));
      QCOMPARE(daemon.invokedId, id);
      QVERIFY(!QUuid::fromString(daemon.openRequestId).isNull());
      QTRY_VERIFY(backend.busyEntryIds().contains(id));
      daemon.operationDone = true;
      QTRY_COMPARE(backend.message(), QString("Unavailable"));
      QTRY_VERIFY(backend.busyEntryIds().isEmpty());

      daemon.operationSuccess = true;
      backend.launch(
          backend.webShortcuts().at(1).toMap().value("id").toString());
      QTRY_COMPARE(backend.message(), QString("Done"));
      QCOMPARE(daemon.opens, 2);
      QCOMPARE(daemon.launches, 0);
      // #258: an action result is transient.
      QTRY_COMPARE_WITH_TIMEOUT(backend.message(), QString(), 5000);

      backend.launch(QStringLiteral("missing"));
      QCOMPARE(backend.message(), QString("Shortcut is no longer available"));
      QCOMPARE(daemon.opens, 2);

      const int listsAfterOpen = daemon.lists;
      auto focus = QDBusMessage::createSignal(
          "/org/arinn/Daemon1", "org.arinn.Daemon1", "StateChanged");
      focus << QStringLiteral("test") << QStringLiteral("browser")
            << QStringLiteral("generation") << qulonglong(1);
      QVERIFY(bus.send(focus));
      QTest::qWait(300);
      QCOMPARE(daemon.lists, listsAfterOpen);

      daemon.listError = true;
      backend.refresh();
      QTRY_COMPARE(backend.homeState(), QString("error"));
      QCOMPARE(backend.applicationsState(), QString("empty"));
      QVERIFY(backend.webShortcuts().isEmpty());

      bus.unregisterService("org.arinn.Daemon1");
      QTRY_COMPARE(backend.homeState(), QString("unavailable"));
      QVERIFY(backend.webShortcuts().isEmpty());
    }
    bus.unregisterObject("/org/arinn", QDBusConnection::UnregisterTree);
    bus.unregisterService("org.arinn.Daemon1");
  }
  void operationResultSurvivesStatusRefresh() {
    auto bus = QDBusConnection::sessionBus();
    CatalogDaemon daemon;
    daemon.platform = false;
    daemon.operationError = QStringLiteral("Timeout");
    QVERIFY(bus.registerService("org.arinn.Daemon1"));
    QVERIFY(bus.registerVirtualObject("/org/arinn", &daemon,
                                      QDBusConnection::SubPath));
    Backend backend;
    QTRY_COMPARE(backend.message(), QString("KWin adapter is not connected"));
    QTRY_VERIFY(!backend.items().isEmpty());
    backend.activate(backend.items().at(0).toMap().value("id").toString());
    QTRY_COMPARE(backend.message(), QString("Timeout"));
    const auto polls = daemon.providerCalls;
    backend.refresh();
    QTRY_VERIFY(daemon.providerCalls > polls);
    QTest::qWait(200);
    QCOMPARE(backend.message(), QString("Timeout"));
    bus.unregisterObject("/org/arinn", QDBusConnection::UnregisterTree);
    bus.unregisterService("org.arinn.Daemon1");
  }
};
QTEST_GUILESS_MAIN(CatalogTest)
#include "catalog_test.moc"
