// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// The application launch screen in the production shell (spec §6.1, §6.5,
// ADR-0110): Main.qml, LaunchOverlay and the real Backend against a fake
// daemon on a private bus that launches two applications. It drives the
// overlay's reports to the Backend (setLaunchHidden, setLaunchConfirming)
// through real keys.
#include "asset_images.h"
#include "backend.h"
#include <QQmlApplicationEngine>
#include <QQmlContext>
#include <QQmlProperty>
#include <QQuickItem>
#include <QQuickWindow>
#include <QTest>
#include <QtDBus>

struct ProviderWire {
  QString id;
};
struct AppWire {
  QString id, title;
};
struct RunningWire {};
struct TargetWire {};
Q_DECLARE_METATYPE(ProviderWire)
Q_DECLARE_METATYPE(AppWire)
Q_DECLARE_METATYPE(RunningWire)
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
  a << p.id << p.title << true << QString("games") << QString();
  a.endStructure();
  return a;
}
// Only empty lists of these are sent: their D-Bus signatures matter.
QDBusArgument &operator<<(QDBusArgument &a, const RunningWire &) {
  a.beginStructure();
  a << QString() << QString() << QString() << QString() << false << false;
  a.endStructure();
  return a;
}
QDBusArgument &operator<<(QDBusArgument &a, const TargetWire &) {
  a.beginStructure();
  a << QString() << QString() << QString();
  a.endStructure();
  return a;
}
// Sent only, never received.
const QDBusArgument &operator>>(const QDBusArgument &a, ProviderWire &) {
  return a;
}
const QDBusArgument &operator>>(const QDBusArgument &a, AppWire &) { return a; }
const QDBusArgument &operator>>(const QDBusArgument &a, RunningWire &) {
  return a;
}
const QDBusArgument &operator>>(const QDBusArgument &a, TargetWire &) {
  return a;
}

namespace {
const QString epoch = QStringLiteral("55555555-5555-4555-8555-555555555555");
const QString service = QStringLiteral("org.arinn.Daemon1");
const QString daemonPath = QStringLiteral("/org/arinn/Daemon1");
const QString operations = QStringLiteral("/org/arinn/Daemon1/operations/launch_");

// Two applications whose launches stay running until the test ends them.
class LaunchDaemon : public QDBusVirtualObject {
public:
  QHash<QString, QString> states;
  QStringList cancelled;
  QDBusConnection bus = QDBusConnection::connectToBus(
      QDBusConnection::SessionBus, QStringLiteral("launch-screen-fake"));
  QString introspect(const QString &) const override { return {}; }
  bool handleMessage(const QDBusMessage &m,
                     const QDBusConnection &) override {
    QDBusArgument a;
    if (m.member() == QLatin1String("Status")) {
      a.beginStructure();
      a << epoch << true << epoch << qulonglong(1) << true << QString()
        << QString() << QString() << false << true << QString() << QString();
      a.endStructure();
    } else if (m.member() == QLatin1String("Providers")) {
      a.beginStructure();
      a << QString("snapshot")
        << QList<ProviderWire>{ProviderWire{QStringLiteral("test")}} << QString();
      a.endStructure();
    } else if (m.member() == QLatin1String("GetApplications")) {
      a.beginStructure();
      a << QString("generation") << qulonglong(1)
        << QList<AppWire>{AppWire{QStringLiteral("alpha"), QStringLiteral("Alpha")},
                          AppWire{QStringLiteral("beta"), QStringLiteral("Beta")}}
        << QList<RunningWire>{} << QString();
      a.endStructure();
    } else if (m.member() == QLatin1String("Launch")) {
      const auto id = m.arguments().value(1).toString();
      states.insert(id, QStringLiteral("running"));
      bus.send(m.createReply(
          QVariant::fromValue(QDBusObjectPath(operations + id))));
      return true;
    } else if (m.path().startsWith(operations)) {
      const auto id = m.path().mid(operations.size());
      if (m.member() == QLatin1String("Cancel")) {
        cancelled << id;
        states[id] = QStringLiteral("cancelled");
        bus.send(m.createReply());
        return true;
      }
      const auto state = states.value(id);
      const bool running = state == QLatin1String("running");
      a.beginStructure();
      a << state
        << (state == QLatin1String("failed")      ? QStringLiteral("Unavailable")
            : state == QLatin1String("cancelled") ? QStringLiteral("Cancelled")
                                                  : QString())
        << false << qulonglong(1) << qulonglong(running ? 0 : 2)
        << QList<TargetWire>{} << running;
      a.endStructure();
    } else {
      bus.send(m.createErrorReply(QDBusError::NotSupported, "fixture"));
      return true;
    }
    bus.send(m.createReply(QVariant::fromValue(a)));
    return true;
  }
  // The platform's Home request: Home on screen with the shell.
  void showHome() {
    bus.send(QDBusMessage::createSignal(daemonPath, service,
                                        QStringLiteral("Changed")));
    auto message = QDBusMessage::createSignal(daemonPath, service,
                                              QStringLiteral("HomeRequested"));
    message << epoch << QString() << QString();
    bus.send(message);
  }
};
class SessionGuardStandIn : public QObject {
  Q_OBJECT
  Q_PROPERTY(bool blocked MEMBER m_blocked NOTIFY changed)
public:
  bool m_blocked = false;
signals:
  void changed();
};
QQuickItem *findItem(QQuickItem *root, const QString &name) {
  if (root->objectName() == name)
    return root;
  for (auto *item : root->childItems())
    if (auto *found = findItem(item, name))
      return found;
  return nullptr;
}
} // namespace

class LaunchScreenTest : public QObject {
  Q_OBJECT
  LaunchDaemon daemon;
  SessionGuardStandIn guard;
  std::unique_ptr<Backend> backend;
  std::unique_ptr<QQmlApplicationEngine> engine;
  QQuickWindow *window = nullptr;
  QQuickItem *overlay = nullptr, *home = nullptr;

  QQuickItem *part(const char *name) {
    return findItem(overlay, QString::fromLatin1(name));
  }
  QString title() { return part("launchTitle")->property("text").toString(); }
  bool dialog() { return overlay->property("showDialog").toBool(); }
  bool focusIn(QQuickItem *scope) {
    for (auto *item = window->activeFocusItem(); item; item = item->parentItem())
      if (item == scope)
        return true;
    return false;
  }
  // A tile press: the launch's "Starting <app>…" comes on screen.
  // The shell's entry ID of a fixture application.
  QString entry(const QString &localId) {
    for (const auto &item : backend->applications())
      if (item.toMap().value("localId").toString() == localId)
        return item.toMap().value("id").toString();
    return {};
  }
  bool starting(const QString &id, const QString &name) {
    backend->launch(entry(id));
    const bool ok = QTest::qWaitFor([&] {
      return overlay->isVisible() && !dialog() &&
             title() == QStringLiteral("Starting %1…").arg(name) &&
             part("launchCancel")->hasActiveFocus();
    });
    if (!ok)
      qWarning() << "not starting" << id << overlay->isVisible() << title()
                 << window->activeFocusItem();
    return ok;
  }
  // Home on the starting screen: hidden, still running.
  bool pressHome() {
    QTest::keyClick(window, Qt::Key_Home);
    return QTest::qWaitFor([&] { return !overlay->isVisible(); }) &&
           backend->busyEntryIds().size() > 0;
  }
  bool failed(const QString &id, const QString &name) {
    daemon.states[id] = QStringLiteral("failed");
    return QTest::qWaitFor([&] {
      return overlay->isVisible() && dialog() &&
             part("launchDialogTitle")->property("text").toString() ==
                 QStringLiteral("%1 did not open").arg(name) &&
             part("launchRetry")->hasActiveFocus();
    });
  }

private slots:
  void initTestCase() {
    qDBusRegisterMetaType<ProviderWire>();
    qDBusRegisterMetaType<AppWire>();
    qDBusRegisterMetaType<RunningWire>();
    qDBusRegisterMetaType<TargetWire>();
  }
  void init() {
    daemon.states.clear();
    daemon.cancelled.clear();
    guard.m_blocked = false;
    auto bus = daemon.bus;
    QVERIFY(bus.isConnected());
    bus.unregisterObject(QStringLiteral("/org/arinn"),
                         QDBusConnection::UnregisterTree);
    QVERIFY(bus.registerVirtualObject(QStringLiteral("/org/arinn"), &daemon,
                                      QDBusConnection::SubPath));
    QVERIFY(bus.registerService(service));
    backend = std::make_unique<Backend>();
    QTRY_VERIFY(backend->connected());
    QTRY_COMPARE(backend->applications().size(), 2);
    engine = std::make_unique<QQmlApplicationEngine>();
    installAssetImageProvider(engine.get());
    engine->rootContext()->setContextProperty("backend", backend.get());
    engine->rootContext()->setContextProperty("sessionGuard", &guard);
    engine->load(QUrl::fromLocalFile(QStringLiteral(ARINN_MAIN_QML)));
    window = qobject_cast<QQuickWindow *>(engine->rootObjects().value(0));
    QVERIFY(window);
    overlay = findItem(window->contentItem(), QStringLiteral("applicationLaunch"));
    home = findItem(window->contentItem(), QStringLiteral("productionHome"));
    QVERIFY(overlay && home);
    // The starting stage at once, so Cancel is there from the start.
    QVERIFY(QQmlProperty::write(overlay, QStringLiteral("revealDelay"), 0));
    QVERIFY(QQmlProperty::write(overlay, QStringLiteral("slowDelay"), 0));
    daemon.showHome();
    QTRY_VERIFY(window->isVisible());
    window->requestActivate();
    QTRY_VERIFY(window->isActive());
    QTRY_VERIFY(focusIn(home));
  }
  void cleanup() {
    engine.reset();
    backend.reset();
    auto bus = daemon.bus;
    bus.unregisterService(service);
    bus.unregisterObject(QStringLiteral("/org/arinn"),
                         QDBusConnection::UnregisterTree);
  }

  // A failure dialog that covered a visible "Starting B…" returns to it on
  // Back (§6.1).
  void aCoveredStartingScreenComesBack() {
    QVERIFY(starting("alpha", "Alpha"));
    QVERIFY(pressHome());
    QVERIFY(starting("beta", "Beta"));
    QVERIFY(failed("alpha", "Alpha"));
    QTest::keyClick(window, Qt::Key_Escape);
    QTRY_VERIFY(overlay->isVisible() && !dialog());
    QTRY_COMPARE(title(), QStringLiteral("Starting Beta…"));
    QTRY_VERIFY(part("launchCancel")->hasActiveFocus());
  }

  // One that Home hid stays hidden after the dialog (§6.5).
  void aHomeHiddenStartingScreenStaysHidden() {
    QVERIFY(starting("alpha", "Alpha"));
    QVERIFY(pressHome());
    QVERIFY(starting("beta", "Beta"));
    QVERIFY(pressHome());
    QVERIFY(failed("alpha", "Alpha"));
    QTest::keyClick(window, Qt::Key_Escape);
    QTRY_VERIFY(!overlay->isVisible());
    QTest::qWait(300);
    QVERIFY(!overlay->isVisible());
    QVERIFY(backend->launchActive());
    QVERIFY(backend->busyEntryIds().contains(entry(QStringLiteral("beta"))));
    // Its tile shows it again.
    QVERIFY(starting("beta", "Beta"));
  }

  // Home on a dialog that covered a visible starting screen leaves Home on
  // screen, with that launch hidden and still running.
  void homeOnTheDialogLeavesHome() {
    QVERIFY(starting("alpha", "Alpha"));
    QVERIFY(pressHome());
    QVERIFY(starting("beta", "Beta"));
    QVERIFY(failed("alpha", "Alpha"));
    QTest::keyClick(window, Qt::Key_Home);
    QTRY_VERIFY(!overlay->isVisible());
    QTRY_VERIFY(focusIn(home));
    QTest::qWait(300);
    QVERIFY(!overlay->isVisible());
    QVERIFY(backend->busyEntryIds().contains(entry(QStringLiteral("beta"))));
  }

  // A failure that waited behind "Stop starting B?" does not swallow the
  // confirmed Stop: B is stopped, then the failure shows.
  void stopReachesTheConfirmedLaunch() {
    QVERIFY(starting("alpha", "Alpha"));
    QVERIFY(pressHome());
    QVERIFY(starting("beta", "Beta"));
    QTest::keyClick(window, Qt::Key_Return);
    QTRY_VERIFY(part("launchKeepStarting")->hasActiveFocus());
    daemon.states[QStringLiteral("alpha")] = QStringLiteral("failed");
    QTRY_VERIFY(!backend->busyEntryIds().contains(entry(QStringLiteral("alpha"))));
    QTest::qWait(200);
    // The confirmation stays; the failure waits.
    QVERIFY(part("launchConfirm")->isVisible());
    QVERIFY(!dialog());
    QTest::keyClick(window, Qt::Key_Right);
    QTRY_VERIFY(part("launchConfirmStop")->hasActiveFocus());
    QTest::keyClick(window, Qt::Key_Return);
    QTRY_COMPARE(daemon.cancelled, QStringList{QStringLiteral("beta")});
    QTRY_VERIFY(dialog());
    QCOMPARE(part("launchDialogTitle")->property("text").toString(),
             QStringLiteral("Alpha did not open"));
  }
};

QTEST_MAIN(LaunchScreenTest)
#include "launch_screen_test.moc"
