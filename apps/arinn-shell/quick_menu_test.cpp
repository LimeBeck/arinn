// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// The quick menu in the production shell (spec §5.7, §6.3, ADR-0106): the
// real Backend and Main.qml against a fake Daemon1 on a private bus.
#include "asset_images.h"
#include "backend.h"
#include <QQmlApplicationEngine>
#include <QQmlContext>
#include <QQuickItem>
#include <QQuickWindow>
#include <QSignalSpy>
#include <QTest>
#include <QTimer>
#include <QtDBus>

namespace {
const QString epoch = QStringLiteral("66666666-6666-4666-8666-666666666666");
const QString service = QStringLiteral("org.arinn.Daemon1");
const QString path = QStringLiteral("/org/arinn/Daemon1");
const QString control = QStringLiteral("org.arinn.SystemControl1");

// Home state as the platform reports it; Back hands the screen back.
class QuickMenuDaemon : public QDBusVirtualObject {
public:
  bool home = false;
  QString returnWindow;
  int backs = 0;
  // SystemControl1 (ADR-0108): the calls in order, the default output, and
  // a suspend the platform refuses with a block inhibitor.
  QStringList controlCalls;
  uint level = 30;
  bool muted = false;
  QString introspect(const QString &) const override { return {}; }
  bool handleMessage(const QDBusMessage &m,
                     const QDBusConnection &bus) override {
    if (m.interface() == control) {
      controlCalls << m.member();
      const QString member = m.member();
      if (member == QLatin1String("StepVolume"))
        level = uint(qBound(0, int(level) + 5 * m.arguments().value(0).toInt(),
                            150));
      else if (member == QLatin1String("ToggleMuted"))
        muted = !muted;
      else if (member == QLatin1String("SetMuted"))
        muted = m.arguments().value(0).toBool();
      if (member == QLatin1String("GetVolume") ||
          member == QLatin1String("StepVolume") ||
          member == QLatin1String("ToggleMuted") ||
          member == QLatin1String("SetMuted")) {
        QDBusArgument a;
        a.beginStructure();
        a << (muted ? QStringLiteral("muted") : QStringLiteral("unmuted"))
          << level;
        a.endStructure();
        bus.send(m.createReply(QVariant::fromValue(a)));
      } else if (member == QLatin1String("Suspend")) {
        bus.send(m.createErrorReply(QStringLiteral("org.arinn.Error1.Conflict"),
                                    QStringLiteral("inhibited")));
      } else {
        bus.send(m.createErrorReply(QDBusError::NotSupported, "fixture"));
      }
      return true;
    }
    if (m.interface() != service) {
      bus.send(m.createErrorReply(QDBusError::NotSupported, "fixture"));
      return true;
    }
    if (m.member() == QLatin1String("Status")) {
      QDBusArgument a;
      a.beginStructure();
      a << epoch << true << epoch << qulonglong(1) << home << returnWindow
        << QString() << QString() << false << true << QString() << QString();
      a.endStructure();
      bus.send(m.createReply(QVariant::fromValue(a)));
    } else if (m.member() == QLatin1String("Back")) {
      ++backs;
      bus.send(m.createReply());
      // The platform activates the Activity and reports Home hidden.
      QTimer::singleShot(50, [this] {
        home = false;
        returnWindow.clear();
        changed();
      });
    } else {
      bus.send(m.createErrorReply(QDBusError::NotSupported, "fixture"));
    }
    return true;
  }
  void changed() {
    QDBusConnection::sessionBus().send(
        QDBusMessage::createSignal(path, service, QStringLiteral("Changed")));
  }
  // The platform's explicit request: state first, then the intent.
  void request(const char *signal, const QString &requestEpoch = epoch) {
    home = true;
    changed();
    auto message = QDBusMessage::createSignal(path, service, signal);
    message << requestEpoch << returnWindow << QString();
    QDBusConnection::sessionBus().send(message);
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
QQuickItem *child(QQuickWindow *window, const char *name) {
  return findItem(window->contentItem(), QString::fromLatin1(name));
}
} // namespace

class QuickMenuTest : public QObject {
  Q_OBJECT
  QuickMenuDaemon daemon;
  SessionGuardStandIn guard;
  std::unique_ptr<Backend> backend;
  std::unique_ptr<QQmlApplicationEngine> engine;
  QQuickWindow *window = nullptr;
  QQuickItem *menu = nullptr, *screens = nullptr, *home = nullptr;

  bool focusIn(QQuickItem *scope) {
    for (auto *item = window->activeFocusItem(); item; item = item->parentItem())
      if (item == scope)
        return true;
    return false;
  }
  bool menuOpen() { return menu->isVisible() && focusIn(menu); }

private slots:
  void init() {
    daemon.home = false;
    daemon.returnWindow.clear();
    daemon.backs = 0;
    daemon.controlCalls.clear();
    daemon.level = 30;
    daemon.muted = false;
    guard.m_blocked = false;
    auto bus = QDBusConnection::sessionBus();
    bus.unregisterObject(path);
    QVERIFY(bus.registerVirtualObject(path, &daemon));
    QVERIFY(bus.registerService(service));
    backend = std::make_unique<Backend>();
    QTRY_VERIFY(backend->connected());
    engine = std::make_unique<QQmlApplicationEngine>();
    installAssetImageProvider(engine.get());
    engine->rootContext()->setContextProperty("backend", backend.get());
    engine->rootContext()->setContextProperty("sessionGuard", &guard);
    engine->load(QUrl::fromLocalFile(QStringLiteral(ARINN_MAIN_QML)));
    window = qobject_cast<QQuickWindow *>(engine->rootObjects().value(0));
    QVERIFY(window);
    menu = child(window, "quickMenu");
    screens = child(window, "shellScreens");
    home = child(window, "productionHome");
    QVERIFY(menu && screens && home);
    QTRY_VERIFY(!window->isVisible());
  }
  void cleanup() {
    engine.reset();
    backend.reset();
    auto bus = QDBusConnection::sessionBus();
    bus.unregisterService(service);
    bus.unregisterObject(path);
  }

  // Over an external Activity: the sheet alone, the live window behind it,
  // and Back activates that same Activity (§6.3).
  void backReturnsToTheSameActivity() {
    daemon.returnWindow = QStringLiteral("activity");
    daemon.request("QuickMenuRequested");
    QTRY_VERIFY(window->isVisible());
    window->requestActivate();
    QTRY_VERIFY(menuOpen());
    QVERIFY(!screens->isVisible());
    QCOMPARE(window->color(), QColor(Qt::transparent));
    // Nothing plays, so focus starts on Sound, the first available row.
    QVERIFY(child(window, "quickMenuVolume")->hasActiveFocus());
    QTest::keyClick(window, Qt::Key_Escape);
    QTRY_COMPARE(daemon.backs, 1);
    // Home never shows on the way back.
    QVERIFY(!screens->isVisible());
    QTRY_VERIFY(!window->isVisible());
    QVERIFY(!menu->isVisible());
    // The next Home is the ordinary Home.
    daemon.returnWindow = QStringLiteral("activity");
    daemon.request("HomeRequested");
    QTRY_VERIFY(window->isVisible());
    QVERIFY(screens->isVisible());
    QVERIFY(!menu->isVisible());
  }

  // Open Home and the Home key replace the sheet with the fullscreen Home.
  void homeReplacesTheSheet() {
    daemon.returnWindow = QStringLiteral("activity");
    daemon.request("QuickMenuRequested");
    QTRY_VERIFY(window->isVisible());
    window->requestActivate();
    QTRY_VERIFY(menuOpen());
    for (int i = 0; i < 3; ++i)
      QTest::keyClick(window, Qt::Key_Down);
    QTRY_VERIFY(child(window, "quickMenuHome")->hasActiveFocus());
    QTest::keyClick(window, Qt::Key_Return);
    QTRY_VERIFY(!menu->isVisible());
    QVERIFY(screens->isVisible());
    QCOMPARE(window->color(), QColor(QStringLiteral("#131416")));
    QTRY_VERIFY(focusIn(home));
    QCOMPARE(daemon.backs, 0);
    // Over Home: the sheet opens on top and Back returns focus to Home.
    // (A shell shown in the last 1.5 s still counts as the Activity's.)
    QTest::qWait(1600);
    const auto opener = window->activeFocusItem();
    daemon.request("QuickMenuRequested");
    QTRY_VERIFY(menuOpen());
    QVERIFY(screens->isVisible());
    QTest::keyClick(window, Qt::Key_Backspace);
    QTRY_VERIFY(!menu->isVisible());
    QTRY_COMPARE(window->activeFocusItem(), opener);
    QCOMPARE(daemon.backs, 0);
    // The global Home key while the sheet is open shows Home.
    daemon.request("QuickMenuRequested");
    QTRY_VERIFY(menuOpen());
    daemon.request("HomeRequested");
    QTRY_VERIFY(!menu->isVisible());
    QTRY_VERIFY(focusIn(home));
  }

  // Sound: the platform's level when the sheet opens, and the volume keys'
  // semantics on the row (SystemControl1, ADR-0108).
  void soundRowControlsThePlatformVolume() {
    daemon.returnWindow = QStringLiteral("activity");
    daemon.request("QuickMenuRequested");
    QTRY_VERIFY(window->isVisible());
    window->requestActivate();
    QTRY_VERIFY(menuOpen());
    auto *volume = child(window, "quickMenuVolume");
    QVERIFY(volume->hasActiveFocus());
    auto description = [this] {
      return child(window, "quickMenuVolumeDescription")
          ->property("text")
          .toString();
    };
    QTRY_VERIFY(description().startsWith(QStringLiteral("Sound on · 30%")));
    QCOMPARE(daemon.controlCalls, QStringList{QStringLiteral("GetVolume")});
    // Right turns the sound back on, then raises it.
    daemon.muted = true;
    QTest::keyClick(window, Qt::Key_Right);
    QTRY_VERIFY(description().startsWith(QStringLiteral("Sound on · 35%")));
    QTest::keyClick(window, Qt::Key_Left);
    QTRY_VERIFY(description().startsWith(QStringLiteral("Sound on · 30%")));
    QTest::keyClick(window, Qt::Key_Return);
    QTRY_VERIFY(description().startsWith(QStringLiteral("Muted")));
    QCOMPARE(daemon.controlCalls,
             (QStringList{"GetVolume", "SetMuted", "StepVolume", "StepVolume",
                          "ToggleMuted"}));
    // The sheet stays open over the Activity, focus on Sound.
    QVERIFY(menuOpen());
    QVERIFY(volume->hasActiveFocus());
    QVERIFY(!screens->isVisible());
  }

  // Sleep and Power hand over to the shell's power scope. Over an Activity
  // the shell comes up as for Home; Sleep sends one request and a refusal is
  // shown, never a success. Over the shell the power menu opens over it.
  void sleepAndPowerOpenThePowerMenu() {
    daemon.returnWindow = QStringLiteral("activity");
    daemon.request("QuickMenuRequested");
    QTRY_VERIFY(window->isVisible());
    window->requestActivate();
    QTRY_VERIFY(menuOpen());
    QTest::keyClick(window, Qt::Key_Down);
    QTRY_VERIFY(child(window, "quickMenuSleep")->hasActiveFocus());
    QTest::keyClick(window, Qt::Key_Return);
    QTRY_VERIFY(!menu->isVisible());
    QVERIFY(screens->isVisible());
    auto *power = child(window, "powerOverlay");
    QTRY_VERIFY(child(window, "powerError")->isVisible());
    QTRY_VERIFY(focusIn(power));
    QCOMPARE(daemon.controlCalls.count(QStringLiteral("Suspend")), 1);
    QCOMPARE(daemon.backs, 0);
    // Back clears the refusal, Back again closes the menu onto Home, whose
    // Return is still that Activity.
    QTest::keyClick(window, Qt::Key_Escape);
    QTRY_VERIFY(!child(window, "powerError")->isVisible());
    QVERIFY(child(window, "powerMenu")->isVisible());
    QTest::keyClick(window, Qt::Key_Escape);
    QTRY_VERIFY(!child(window, "powerMenu")->isVisible());
    QTRY_VERIFY(focusIn(home));
    QCOMPARE(daemon.backs, 0);
    // Over the shell, Power opens the menu and sends nothing.
    QTest::qWait(1600);
    daemon.request("QuickMenuRequested");
    QTRY_VERIFY(menuOpen());
    QVERIFY(screens->isVisible());
    QTest::keyClick(window, Qt::Key_Down);
    QTest::keyClick(window, Qt::Key_Down);
    QTRY_VERIFY(child(window, "quickMenuPower")->hasActiveFocus());
    QTest::keyClick(window, Qt::Key_Return);
    QTRY_VERIFY(!menu->isVisible());
    QTRY_VERIFY(child(window, "powerMenu")->isVisible());
    QTRY_VERIFY(focusIn(power));
    QCOMPARE(daemon.controlCalls.count(QStringLiteral("Suspend")), 1);
    QTest::keyClick(window, Qt::Key_Escape);
    QTRY_VERIFY(!child(window, "powerMenu")->isVisible());
    QTRY_VERIFY(focusIn(home));
  }

  // A repeated key keeps the sheet; a request of another daemon epoch is
  // not this daemon's intent.
  void repeatsAndForeignEpochs() {
    QSignalSpy requests(backend.get(), &Backend::quickMenuRequested);
    daemon.request("QuickMenuRequested", QStringLiteral("77777777-7777-4777-8777-777777777777"));
    QTRY_VERIFY(window->isVisible());
    QTest::qWait(200);
    QCOMPARE(requests.count(), 0);
    QVERIFY(!menu->isVisible());
    daemon.request("QuickMenuRequested");
    QTRY_COMPARE(requests.count(), 1);
    window->requestActivate();
    QTRY_VERIFY(menuOpen());
    QTest::keyClick(window, Qt::Key_Down);
    daemon.request("QuickMenuRequested");
    QTRY_COMPARE(requests.count(), 2);
    QTRY_VERIFY(child(window, "quickMenuVolume")->hasActiveFocus());
    QVERIFY(menu->isVisible());
  }

  // Degraded without the daemon: Open Home and Close Arinn stay available,
  // and Back without a daemon shows Home instead of leaving nothing.
  void degradedWithoutTheDaemon() {
    daemon.returnWindow = QStringLiteral("activity");
    daemon.request("QuickMenuRequested");
    QTRY_VERIFY(window->isVisible());
    window->requestActivate();
    QTRY_VERIFY(menuOpen());
    QVERIFY(child(window, "quickMenuSleep")->isEnabled());
    QDBusConnection::sessionBus().unregisterService(service);
    QTRY_VERIFY(!backend->connected());
    QTRY_VERIFY(!child(window, "quickMenuSleep")->isEnabled());
    QVERIFY(!child(window, "quickMenuPower")->isEnabled());
    QTRY_VERIFY(child(window, "quickMenuHome")->hasActiveFocus());
    QVERIFY(child(window, "quickMenuClose")->isEnabled());
    QTest::keyClick(window, Qt::Key_Escape);
    QTRY_VERIFY(!menu->isVisible());
    QVERIFY(screens->isVisible());
    QCOMPARE(daemon.backs, 0);
  }

  // The session lock hides the shell and ends the quick menu.
  void lockEndsTheSheet() {
    daemon.request("QuickMenuRequested");
    QTRY_VERIFY(window->isVisible());
    QTRY_VERIFY(menu->isVisible());
    guard.m_blocked = true;
    emit guard.changed();
    QTRY_VERIFY(!window->isVisible());
    QVERIFY(!menu->isVisible());
    guard.m_blocked = false;
    emit guard.changed();
    QTRY_VERIFY(window->isVisible());
    QVERIFY(!menu->isVisible());
    QVERIFY(screens->isVisible());
    // A request while locked does nothing.
    guard.m_blocked = true;
    emit guard.changed();
    daemon.request("QuickMenuRequested");
    QTest::qWait(200);
    QVERIFY(!menu->isVisible());
    guard.m_blocked = false;
    emit guard.changed();
  }
};

QTEST_MAIN(QuickMenuTest)
#include "quick_menu_test.moc"
