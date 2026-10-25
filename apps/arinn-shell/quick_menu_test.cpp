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
#include <QQmlProperty>
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
  // Holds Suspend until releaseSuspend() refuses it: a request in flight.
  // The fake has its own connection, so a held call is really pending (a
  // call within one connection is answered when the handler returns).
  bool holdSuspend = false;
  QDBusMessage heldSuspend;
  // GetSessionSwitch (#438); empty answers NotSupported as an older daemon.
  // SwitchToDesktop is refused: the desktop did not accept the logout.
  QString sessionSwitch;
  QDBusConnection bus = QDBusConnection::connectToBus(
      QDBusConnection::SessionBus, QStringLiteral("quick-menu-fake"));
  void releaseSuspend() {
    bus.send(heldSuspend.createErrorReply(
        QStringLiteral("org.arinn.Error1.Conflict"), QStringLiteral("inhibited")));
    heldSuspend = {};
  }
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
      } else if (member == QLatin1String("GetSessionSwitch") &&
                 !sessionSwitch.isEmpty()) {
        bus.send(m.createReply(sessionSwitch));
      } else if (member == QLatin1String("SwitchToDesktop")) {
        bus.send(m.createErrorReply(
            QStringLiteral("org.arinn.Error1.Unavailable"), QStringLiteral("refused")));
      } else if (member == QLatin1String("Suspend")) {
        if (holdSuspend)
          heldSuspend = m;
        else
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
    bus.send(
        QDBusMessage::createSignal(path, service, QStringLiteral("Changed")));
  }
  // The platform's explicit request: state first, then the intent.
  void request(const char *signal, const QString &requestEpoch = epoch) {
    home = true;
    changed();
    auto message = QDBusMessage::createSignal(path, service, signal);
    message << requestEpoch << returnWindow << QString();
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
    daemon.holdSuspend = false;
    daemon.heldSuspend = {};
    daemon.sessionSwitch.clear();
    guard.m_blocked = false;
    auto bus = daemon.bus;
    QVERIFY(bus.isConnected());
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
    auto bus = daemon.bus;
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

  // Opens the sheet over an Activity, or over Home when `overShell`.
  void openSheet(bool overShell) {
    daemon.returnWindow = QStringLiteral("activity");
    if (overShell) {
      daemon.request("HomeRequested");
      QTRY_VERIFY(window->isVisible());
      window->requestActivate();
      QTRY_VERIFY(window->isActive());
      QTRY_VERIFY(focusIn(home));
    }
    daemon.request("QuickMenuRequested");
    QTRY_VERIFY(window->isVisible());
    window->requestActivate();
    QTRY_VERIFY(menuOpen());
    QCOMPARE(screens->isVisible(), overShell);
  }
  QQuickItem *row(const char *name) { return child(window, name); }
  void moveTo(const char *name) {
    for (int i = 0; i < 6 && !row(name)->hasActiveFocus(); ++i)
      QTest::keyClick(window, Qt::Key_Down);
    QTRY_VERIFY(row(name)->hasActiveFocus());
  }

  // Power opens the power scope over the sheet (§6.1: power › sheet). The
  // Activity stays behind; Back closes the scope and focus returns to the
  // Power row (§6.3). The same over the shell.
  void powerOpensOverTheSheetAndBackReturnsToIt_data() {
    QTest::addColumn<bool>("overShell");
    QTest::newRow("over an Activity") << false;
    QTest::newRow("over the shell") << true;
  }
  void powerOpensOverTheSheetAndBackReturnsToIt() {
    QFETCH(bool, overShell);
    openSheet(overShell);
    moveTo("quickMenuPower");
    QTest::keyClick(window, Qt::Key_Return);
    auto *power = child(window, "powerOverlay");
    QTRY_VERIFY(child(window, "powerMenu")->isVisible());
    QTRY_VERIFY(focusIn(power));
    QVERIFY(menu->isVisible());
    QCOMPARE(screens->isVisible(), overShell);
    QCOMPARE(window->color() == QColor(Qt::transparent), !overShell);
    QCOMPARE(daemon.controlCalls.count(QStringLiteral("Suspend")), 0);
    QTest::keyClick(window, Qt::Key_Escape);
    QTRY_VERIFY(!child(window, "powerMenu")->isVisible());
    QTRY_VERIFY(row("quickMenuPower")->hasActiveFocus());
    QVERIFY(menuOpen());
    QCOMPARE(screens->isVisible(), overShell);
    QCOMPARE(daemon.backs, 0);
    // A repeated key under the power scope keeps that scope on top.
    QTest::keyClick(window, Qt::Key_Return);
    QTRY_VERIFY(focusIn(power));
    daemon.request("QuickMenuRequested");
    QTest::qWait(200);
    QVERIFY(focusIn(power));
    QTest::keyClick(window, Qt::Key_Escape);
    QTRY_VERIFY(row("quickMenuPower")->hasActiveFocus());
    // Back from the sheet then leaves as usual.
    QTest::keyClick(window, Qt::Key_Escape);
    QTRY_VERIFY(!menu->isVisible());
    QTRY_COMPARE(daemon.backs, overShell ? 0 : 1);
  }

  // #438: in the dedicated session the power scope over the sheet offers
  // Switch to desktop with its confirmation (Cancel first, the live list);
  // a refusal is an error over the sheet and Back walks back to the
  // sheet's Power row. Outside the dedicated session it is not offered.
  void switchToDesktopFromTheSheet() {
    openSheet(false);
    moveTo("quickMenuPower");
    QTest::keyClick(window, Qt::Key_Return);
    QTRY_VERIFY(child(window, "powerMenu")->isVisible());
    QTRY_VERIFY(daemon.controlCalls.contains(QStringLiteral("GetSessionSwitch")));
    QTest::qWait(100);
    QVERIFY(!child(window, "desktopButton")->isVisible());
    QTest::keyClick(window, Qt::Key_Escape);
    QTRY_VERIFY(row("quickMenuPower")->hasActiveFocus());

    daemon.sessionSwitch = QStringLiteral("available");
    QTest::keyClick(window, Qt::Key_Return);
    QTRY_VERIFY(child(window, "desktopButton")->isVisible());
    QTRY_VERIFY(child(window, "sleepButton")->hasActiveFocus());
    for (int i = 0; i < 3; ++i)
      QTest::keyClick(window, Qt::Key_Down);
    QTRY_VERIFY(child(window, "desktopButton")->hasActiveFocus());
    QTest::keyClick(window, Qt::Key_Return);
    QTRY_VERIFY(child(window, "cancelButton")->hasActiveFocus());
    QCOMPARE(child(window, "powerTitle")->property("text").toString(),
             QStringLiteral("Switch to desktop?"));
    QVERIFY(child(window, "closingList")->isVisible());
    QVERIFY(!screens->isVisible());
    QCOMPARE(daemon.controlCalls.count(QStringLiteral("SwitchToDesktop")), 0);
    QTest::keyClick(window, Qt::Key_Right);
    QTRY_VERIFY(child(window, "confirmButton")->hasActiveFocus());
    QTest::keyClick(window, Qt::Key_Return);
    QTRY_VERIFY(child(window, "powerError")->isVisible());
    QCOMPARE(child(window, "powerError")->property("title").toString(),
             QStringLiteral("Could not switch to the desktop"));
    QCOMPARE(daemon.controlCalls.count(QStringLiteral("SwitchToDesktop")), 1);
    QVERIFY(menu->isVisible());
    QTest::keyClick(window, Qt::Key_Escape);
    QTRY_VERIFY(child(window, "cancelButton")->hasActiveFocus());
    QTest::keyClick(window, Qt::Key_Escape);
    QTRY_VERIFY(child(window, "desktopButton")->hasActiveFocus());
    QTest::keyClick(window, Qt::Key_Escape);
    QTRY_VERIFY(!child(window, "powerMenu")->isVisible());
    QTRY_VERIFY(row("quickMenuPower")->hasActiveFocus());
    QCOMPARE(daemon.controlCalls.count(QStringLiteral("SwitchToDesktop")), 1);
    QCOMPARE(daemon.backs, 0);
  }

  // Sleep sends one request at once, without confirmation; the refusal is
  // shown in the power scope over the sheet, never a success, and Back
  // returns to the Sleep row with the Activity still behind.
  void sleepRequestsSuspendOverTheSheet() {
    openSheet(false);
    moveTo("quickMenuSleep");
    QTest::keyClick(window, Qt::Key_Return);
    QTRY_VERIFY(child(window, "powerError")->isVisible());
    QTRY_VERIFY(focusIn(child(window, "powerOverlay")));
    QVERIFY(menu->isVisible());
    QVERIFY(!screens->isVisible());
    QCOMPARE(daemon.controlCalls.count(QStringLiteral("Suspend")), 1);
    QTest::keyClick(window, Qt::Key_Escape);
    QTRY_VERIFY(!child(window, "powerError")->isVisible());
    QVERIFY(child(window, "powerMenu")->isVisible());
    QTest::keyClick(window, Qt::Key_Escape);
    QTRY_VERIFY(!child(window, "powerMenu")->isVisible());
    QTRY_VERIFY(row("quickMenuSleep")->hasActiveFocus());
    QVERIFY(!screens->isVisible());
    QCOMPARE(daemon.controlCalls.count(QStringLiteral("Suspend")), 1);
    QCOMPARE(daemon.backs, 0);
  }

  // Home leaves the power scope while Sleep is in flight (#477): the sheet
  // gives way to Home, the request goes on, and its late refusal reopens the
  // power scope over Home. Back on it returns to Home.
  void lateRefusalAfterLeavingTheSheet() {
    daemon.holdSuspend = true;
    openSheet(false);
    moveTo("quickMenuSleep");
    QTest::keyClick(window, Qt::Key_Return);
    QTRY_COMPARE(daemon.controlCalls.count(QStringLiteral("Suspend")), 1);
    QTRY_VERIFY(focusIn(child(window, "powerOverlay")));
    // In flight: Back does nothing, Home leaves.
    QTest::keyClick(window, Qt::Key_Escape);
    QTest::qWait(100);
    QVERIFY(child(window, "powerMenu")->isVisible());
    QTest::keyClick(window, Qt::Key_Home);
    QTRY_VERIFY(!child(window, "powerMenu")->isVisible());
    QTRY_VERIFY(!menu->isVisible());
    QVERIFY(screens->isVisible());
    QTRY_VERIFY(focusIn(home));
    daemon.releaseSuspend();
    QTRY_VERIFY(child(window, "powerError")->isVisible());
    QTRY_VERIFY(focusIn(child(window, "powerOverlay")));
    QVERIFY(!menu->isVisible());
    QTest::keyClick(window, Qt::Key_Escape);
    QTRY_VERIFY(!child(window, "powerMenu")->isVisible());
    QTRY_VERIFY(focusIn(home));
    QCOMPARE(daemon.controlCalls.count(QStringLiteral("Suspend")), 1);
    QCOMPARE(daemon.backs, 0);
  }

  // Regression (review of #502): the quick-menu key right after Home is
  // over the shell, however recently the shell was shown. Back closes the
  // sheet and focus returns to Home; the Activity is not activated.
  void quickMenuRightAfterHomeIsOverTheShell() {
    daemon.returnWindow = QStringLiteral("activity");
    daemon.request("HomeRequested");
    QTRY_VERIFY(window->isVisible());
    window->requestActivate();
    QTRY_VERIFY(window->isActive());
    QTRY_VERIFY(focusIn(home));
    // Home settles its focus on Return; still well inside 1.5 s.
    QTRY_COMPARE(window->activeFocusItem()->objectName(), QStringLiteral("entry-activity"));
    const auto opener = window->activeFocusItem();
    daemon.request("QuickMenuRequested");
    QTRY_VERIFY(menuOpen());
    QVERIFY(screens->isVisible());
    QCOMPARE(window->color(), QColor(QStringLiteral("#131416")));
    QTest::keyClick(window, Qt::Key_Escape);
    QTRY_VERIFY(!menu->isVisible());
    QTRY_COMPARE(window->activeFocusItem(), opener);
    QTest::qWait(200);
    QCOMPARE(daemon.backs, 0);
    QVERIFY(window->isVisible());
  }

  // During ambient the quick-menu key only wakes, and that first event
  // triggers nothing: the same for Meta+F11 (a request) and for the
  // remote's Menu, which reaches the shell while it has focus (§5.7).
  void ambientOnlyWakesForBothQuickMenuKeys() {
    daemon.returnWindow = QStringLiteral("activity");
    daemon.request("HomeRequested");
    QTRY_VERIFY(window->isVisible());
    window->requestActivate();
    QTRY_VERIFY(window->isActive());
    QTRY_VERIFY(focusIn(home));
    auto *ambient = child(window, "ambientScreen");
    for (const bool remote : {true, false}) {
      QVERIFY(QMetaObject::invokeMethod(window, "startAmbient"));
      QTRY_VERIFY(ambient->property("armed").toBool());
      if (remote)
        QTest::keyClick(window, Qt::Key_Menu);
      else
        daemon.request("QuickMenuRequested");
      QTRY_VERIFY(!ambient->property("active").toBool());
      QTest::qWait(200);
      QVERIFY2(!menu->isVisible(), remote ? "Menu" : "request");
      QVERIFY2(!home->property("navOpen").toBool(), remote ? "Menu" : "request");
      QTRY_VERIFY(focusIn(home));
    }
    // Awake, the key opens the sheet as usual.
    daemon.request("QuickMenuRequested");
    QTRY_VERIFY(menuOpen());
  }

  // Automatic ambient never counts while the sheet is over an Activity: the
  // Activity is in the foreground behind it (§5.7). Over the shell it does.
  void ambientIdleWaitsOverAnActivity() {
    auto *idle = engine->rootObjects().value(0)->findChild<QObject *>(
        QStringLiteral("ambientIdle"));
    QVERIFY(idle);
    openSheet(false);
    QTest::qWait(100);
    QVERIFY(!idle->property("eligible").toBool());
    QVERIFY(!idle->property("counting").toBool());
    QTest::keyClick(window, Qt::Key_Escape);
    QTRY_VERIFY(!window->isVisible());
    openSheet(true);
    QTRY_VERIFY(idle->property("eligible").toBool());
  }

  // Automatic ambient waits while an application's starting screen is on
  // screen, not while the launch runs hidden behind Home: an application
  // launch has no time limit (§5.7, §6.5).
  void ambientWaitsOnlyForAShownApplicationLaunch() {
    daemon.request("HomeRequested");
    QTRY_VERIFY(window->isVisible());
    window->requestActivate();
    QTRY_VERIFY(window->isActive());
    QTRY_VERIFY(focusIn(home));
    auto *idle = engine->rootObjects().value(0)->findChild<QObject *>(
        QStringLiteral("ambientIdle"));
    auto *launch = child(window, "applicationLaunch");
    QVERIFY(idle && launch);
    QTRY_VERIFY(idle->property("eligible").toBool());
    QVERIFY(QQmlProperty::write(launch, QStringLiteral("revealDelay"), 0));
    QVERIFY(QQmlProperty::write(launch, QStringLiteral("slowDelay"), 0));
    QVERIFY(QQmlProperty::write(launch, QStringLiteral("cancellable"), true));
    QVERIFY(QQmlProperty::write(launch, QStringLiteral("serviceTitle"), QStringLiteral("Game")));
    QVERIFY(QQmlProperty::write(launch, QStringLiteral("active"), true));
    QTRY_VERIFY(launch->isVisible());
    QVERIFY(!idle->property("eligible").toBool());
    // Home hides the starting screen; the launch goes on, ambient may count.
    QTRY_VERIFY(findItem(launch, QStringLiteral("launchCancel"))->hasActiveFocus());
    QTest::keyClick(window, Qt::Key_Home);
    QTRY_VERIFY(!launch->isVisible());
    QVERIFY(launch->property("active").toBool());
    QTRY_VERIFY(idle->property("eligible").toBool());
    QVERIFY(QQmlProperty::write(launch, QStringLiteral("active"), false));
  }

  // A shell that is shown but has no focus when the request arrives is not
  // the screen the user is on: the sheet is over the Activity (the guard
  // against a Status read that showed the shell first).
  void shownButInactiveShellIsNotTheContext() {
    daemon.returnWindow = QStringLiteral("activity");
    daemon.request("HomeRequested");
    QTRY_VERIFY(window->isVisible());
    // Another window has focus: the shell was shown by a Status read and
    // is not yet activated by the platform.
    QQuickWindow other;
    other.resize(64, 64);
    other.show();
    QVERIFY(QTest::qWaitForWindowExposed(&other));
    other.requestActivate();
    QTRY_VERIFY(other.isActive() && !window->isActive());
    // The request's handler, in the same turn: the shell is shown, not active.
    QVERIFY(QMetaObject::invokeMethod(window, "openQuickMenu"));
    QVERIFY(menu->isVisible());
    QVERIFY(!screens->isVisible());
    window->requestActivate();
    QTRY_VERIFY(menuOpen());
    QTest::keyClick(window, Qt::Key_Escape);
    QTRY_COMPARE(daemon.backs, 1);
  }

  // The global Home key with the power scope over the sheet leaves as Home
  // inside the scope does (#477): a request in flight goes on, and its late
  // refusal reopens the scope over Home.
  void globalHomeLeavesThePowerScopeOverTheSheet() {
    daemon.holdSuspend = true;
    openSheet(false);
    moveTo("quickMenuSleep");
    QTest::keyClick(window, Qt::Key_Return);
    QTRY_COMPARE(daemon.controlCalls.count(QStringLiteral("Suspend")), 1);
    QTRY_VERIFY(focusIn(child(window, "powerOverlay")));
    daemon.request("HomeRequested");
    QTRY_VERIFY(!child(window, "powerMenu")->isVisible());
    QTRY_VERIFY(!menu->isVisible());
    QVERIFY(screens->isVisible());
    QTRY_VERIFY(focusIn(home));
    daemon.releaseSuspend();
    QTRY_VERIFY(child(window, "powerError")->isVisible());
    QTRY_VERIFY(focusIn(child(window, "powerOverlay")));
    QTest::keyClick(window, Qt::Key_Escape);
    QTRY_VERIFY(!child(window, "powerMenu")->isVisible());
    QTRY_VERIFY(focusIn(home));
    QCOMPARE(daemon.backs, 0);
  }

  // The "Opening…" progress never hides a focused sheet: the sheet takes
  // its place, as Home does, and the launch goes on. Only the launch dialog
  // is drawn above the sheet, and Home on it replaces the sheet with Home.
  void launchProgressGivesWayToTheSheet() {
    daemon.returnWindow = QStringLiteral("activity");
    daemon.request("HomeRequested");
    QTRY_VERIFY(window->isVisible());
    window->requestActivate();
    QTRY_VERIFY(window->isActive());
    QTRY_VERIFY(focusIn(home));
    auto *launch = child(window, "launchOverlay");
    // A web service is opening (the binding to the browser catalog is
    // replaced for the test).
    QVERIFY(QQmlProperty::write(launch, QStringLiteral("serviceTitle"), QStringLiteral("Service")));
    QVERIFY(QQmlProperty::write(launch, QStringLiteral("active"), true));
    QTRY_VERIFY(launch->isVisible());
    daemon.request("QuickMenuRequested");
    QTRY_VERIFY(menuOpen());
    QVERIFY(!launch->isVisible());
    QVERIFY(launch->property("active").toBool());
    // The launch fails: its dialog comes up above the sheet with focus.
    QVERIFY(QQmlProperty::write(launch, QStringLiteral("failure"), QStringLiteral("failed")));
    QTRY_VERIFY(launch->isVisible());
    QTRY_VERIFY(focusIn(launch));
    QVERIFY(menu->isVisible());
    QTest::keyClick(window, Qt::Key_Home);
    QTRY_VERIFY(!menu->isVisible());
    QVERIFY(!window->property("quickMenuOpen").toBool());
    QVERIFY(screens->isVisible());
    QVERIFY(QQmlProperty::write(launch, QStringLiteral("failure"), QString()));
    QVERIFY(QQmlProperty::write(launch, QStringLiteral("active"), false));
    QTRY_VERIFY(!launch->isVisible());
  }

  // "Starting <app>…" of a slow application launch (spec §6.5, ADR-0110)
  // gives way to the sheet as the "Opening…" progress does, and its stop
  // confirmation closes without stopping the launch. While it is on screen
  // a late power refusal waits; its failure dialog is drawn above the sheet.
  void applicationLaunchGivesWayToTheSheet() {
    daemon.returnWindow = QStringLiteral("activity");
    daemon.request("HomeRequested");
    QTRY_VERIFY(window->isVisible());
    window->requestActivate();
    QTRY_VERIFY(window->isActive());
    QTRY_VERIFY(focusIn(home));
    auto *launch = child(window, "applicationLaunch");
    auto *power = child(window, "powerOverlay");
    QVERIFY(launch && power);
    QVERIFY(!power->property("holdLateAnswer").toBool());
    // An application is starting (the bindings to the backend are replaced
    // for the test, and the slow stage comes at once).
    QVERIFY(QQmlProperty::write(launch, QStringLiteral("revealDelay"), 0));
    QVERIFY(QQmlProperty::write(launch, QStringLiteral("slowDelay"), 0));
    QVERIFY(QQmlProperty::write(launch, QStringLiteral("cancellable"), true));
    QVERIFY(QQmlProperty::write(launch, QStringLiteral("serviceTitle"), QStringLiteral("Game")));
    QVERIFY(QQmlProperty::write(launch, QStringLiteral("active"), true));
    QTRY_VERIFY(launch->isVisible());
    QTRY_VERIFY(findItem(launch, QStringLiteral("launchCancel"))->hasActiveFocus());
    QCOMPARE(findItem(launch, QStringLiteral("launchTitle"))->property("text").toString(),
             QStringLiteral("Starting Game…"));
    QVERIFY(power->property("holdLateAnswer").toBool());
    // Cancel asks first, with Keep starting focused.
    QSignalSpy cancels(launch, SIGNAL(cancelRequested()));
    QTest::keyClick(window, Qt::Key_Return);
    QTRY_VERIFY(findItem(launch, QStringLiteral("launchKeepStarting"))->hasActiveFocus());
    QVERIFY(launch->property("confirming").toBool());
    daemon.request("QuickMenuRequested");
    QTRY_VERIFY(menuOpen());
    QVERIFY(!launch->isVisible());
    QVERIFY(!launch->property("confirming").toBool());
    QVERIFY(launch->property("active").toBool());
    QCOMPARE(cancels.count(), 0);
    QVERIFY(!power->property("holdLateAnswer").toBool());
    // The launch fails: its dialog comes up above the sheet with focus.
    QVERIFY(QQmlProperty::write(launch, QStringLiteral("failure"), QStringLiteral("Unavailable")));
    QTRY_VERIFY(launch->isVisible());
    QTRY_VERIFY(focusIn(launch));
    QVERIFY(menu->isVisible());
    QTest::keyClick(window, Qt::Key_Home);
    QTRY_VERIFY(!menu->isVisible());
    QVERIFY(!window->property("quickMenuOpen").toBool());
    QVERIFY(screens->isVisible());
    QCOMPARE(cancels.count(), 0);
    QVERIFY(QQmlProperty::write(launch, QStringLiteral("failure"), QString()));
    QVERIFY(QQmlProperty::write(launch, QStringLiteral("active"), false));
    QTRY_VERIFY(!launch->isVisible());
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
    daemon.bus.unregisterService(service);
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
