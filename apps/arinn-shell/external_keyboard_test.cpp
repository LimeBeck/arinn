// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// The shell's side of the global on-screen keyboard (ADR-0105): the
// ExternalKeyboard1 client, the panel's external mode, and Main.qml hiding
// the panel on Back, Home and a lock, all against a fake daemon on a private
// session bus.
#include "asset_images.h"
#include "external_keyboard.h"
#include <QGuiApplication>
#include <QQmlApplicationEngine>
#include <QQmlComponent>
#include <QQmlContext>
#include <QQuickItem>
#include <QQuickWindow>
#include <QSignalSpy>
#include <QProcess>
#include <QtDBus>
#include <QtTest>
#ifdef ARINN_TEST_XCB
#include <xcb/xcb.h>
#endif

namespace {
const QString session = QStringLiteral("10000000-0000-4000-8000-000000000001");
const QString other = QStringLiteral("20000000-0000-4000-8000-000000000002");
const QString interface = QStringLiteral("org.arinn.ExternalKeyboard1");
} // namespace

// Records ExternalKeyboard1 calls; answers Conflict when asked to.
class FakeDaemon : public QDBusVirtualObject {
public:
  QStringList calls;
  bool conflict = false;
  QString introspect(const QString &) const override { return {}; }
  bool handleMessage(const QDBusMessage &message,
                     const QDBusConnection &bus) override {
    if (message.interface() != interface) {
      bus.send(message.createErrorReply(QDBusError::NotSupported, "fixture"));
      return true;
    }
    QStringList fields{message.member()};
    for (const auto &argument : message.arguments())
      fields << argument.toString();
    calls << fields.join(QLatin1Char('|'));
    if (conflict)
      bus.send(message.createErrorReply(
          QStringLiteral("org.arinn.Error1.Conflict"), QStringLiteral("Conflict")));
    else
      bus.send(message.createReply());
    return true;
  }
  static void emitSignal(const char *member, const QVariantList &args) {
    auto message = QDBusMessage::createSignal(QStringLiteral("/org/arinn/Daemon1"),
                                              interface, QString::fromLatin1(member));
    message.setArguments(args);
    QDBusConnection::sessionBus().send(message);
  }
};

class GuardStandIn : public QObject {
  Q_OBJECT
  Q_PROPERTY(bool blocked READ blocked NOTIFY changed)
public:
  bool blocked() const { return m_blocked; }
  void setBlocked(bool value) {
    m_blocked = value;
    emit changed();
  }
  bool m_blocked = false;
signals:
  void changed();
};

// Just enough of Backend for Main.qml; Home starts hidden, as it is while an
// external application has focus.
class BackendStandIn : public QObject {
  Q_OBJECT
  Q_PROPERTY(bool connected READ connected CONSTANT)
  Q_PROPERTY(QString daemonEpoch READ daemonEpoch CONSTANT)
  Q_PROPERTY(bool homeVisible READ homeVisible NOTIFY changed)
  Q_PROPERTY(QString returnWindow MEMBER m_return NOTIFY changed)
  Q_PROPERTY(QVariantList items MEMBER m_list CONSTANT)
  Q_PROPERTY(QVariantList continueEntries MEMBER m_list CONSTANT)
  Q_PROPERTY(QVariantList applications MEMBER m_list CONSTANT)
  Q_PROPERTY(QVariantList favorites MEMBER m_list CONSTANT)
  Q_PROPERTY(QString homeState MEMBER m_homeState NOTIFY changed)
  Q_PROPERTY(QStringList busyEntryIds MEMBER m_busy CONSTANT)
  Q_PROPERTY(QString message MEMBER m_message CONSTANT)
  // The application launch overlay (ADR-0110).
  Q_PROPERTY(bool launchActive MEMBER m_launchActive NOTIFY changed)
  Q_PROPERTY(bool launchStopping MEMBER m_launchStopping NOTIFY changed)
  Q_PROPERTY(QString launchTitle MEMBER m_launchTitle NOTIFY changed)
public:
  bool connected() const { return true; }
  QString daemonEpoch() const {
    return QStringLiteral("77777777-7777-4777-8777-777777777777");
  }
  bool homeVisible() const { return m_home; }
  void setHome(bool home) {
    m_home = home;
    emit changed();
  }
  Q_INVOKABLE void back() {}
  Q_INVOKABLE void launch(const QString &) {}
  Q_INVOKABLE void activate(const QString &) {}
  Q_INVOKABLE void refresh() {}
  void setLaunch(bool active, bool stopping) {
    m_launchActive = active;
    m_launchStopping = stopping;
    m_launchTitle = active ? QStringLiteral("Demo") : QString();
    emit changed();
  }
  bool m_home = false, m_launchActive = false, m_launchStopping = false;
  QString m_launchTitle;
  QString m_return, m_homeState = QStringLiteral("ready"), m_message;
  QVariantList m_list;
  QStringList m_busy;
signals:
  void changed();
};

class ExternalKeyboardTest : public QObject {
  Q_OBJECT
  FakeDaemon daemon;

  // The keys of the panel's model, to find a key by its label.
  static QPoint keyLabelled(QObject *keyboard, const QString &label) {
    auto model = keyboard->property("model").value<QObject *>();
    const auto rows = model->property("rows").toList();
    for (int r = 0; r < rows.size(); ++r) {
      const auto keys = rows.at(r).toList();
      for (int c = 0; c < keys.size(); ++c)
        if (keys.at(c).toMap().value(QStringLiteral("label")).toString() == label)
          return {c, r};
    }
    return {-1, -1};
  }

private slots:
  void init() {
    auto bus = QDBusConnection::sessionBus();
    QVERIFY(bus.registerService(QStringLiteral("org.arinn.Daemon1")));
    QVERIFY(bus.registerVirtualObject(QStringLiteral("/org/arinn/Daemon1"), &daemon));
    daemon.calls.clear();
    daemon.conflict = false;
  }
  void cleanup() {
    auto bus = QDBusConnection::sessionBus();
    bus.unregisterObject(QStringLiteral("/org/arinn/Daemon1"));
    bus.unregisterService(QStringLiteral("org.arinn.Daemon1"));
  }

  void sessionsFollowTheDaemon() {
    ExternalKeyboard keyboard;
    QSignalSpy input(&keyboard, &ExternalKeyboard::input);
    QVERIFY(!keyboard.open());
    // Input and Closed of no session, or another one, change nothing.
    FakeDaemon::emitSignal("Input", {session, QStringLiteral("up")});
    FakeDaemon::emitSignal("Opened", {QStringLiteral("not-a-uuid")});
    QTest::qWait(100);
    QVERIFY(!keyboard.open());
    QCOMPARE(input.count(), 0);
    FakeDaemon::emitSignal("Opened", {session});
    QTRY_VERIFY(keyboard.open());
    FakeDaemon::emitSignal("Input", {other, QStringLiteral("up")});
    FakeDaemon::emitSignal("Input", {session, QStringLiteral("home")});
    FakeDaemon::emitSignal("Input", {session, QStringLiteral("down")});
    QTRY_COMPARE(input.count(), 1);
    QCOMPARE(input.at(0).at(0).toString(), QStringLiteral("down"));
    // Typed keys go to the daemon for the session; invalid ones never leave.
    keyboard.type(QStringLiteral("Я"));
    keyboard.press(QStringLiteral("backspace"));
    keyboard.type(QStringLiteral("a\nb"));
    keyboard.type(QString(300, QLatin1Char('x')));
    keyboard.press(QStringLiteral("tab"));
    QTRY_COMPARE(daemon.calls.size(), 2);
    QCOMPARE(daemon.calls,
             (QStringList{QStringLiteral("Type|") + session + QStringLiteral("|Я"),
                          QStringLiteral("Press|") + session + QStringLiteral("|backspace")}));
    // A stale Closed does nothing; the session's own ends it.
    FakeDaemon::emitSignal("Closed", {other});
    QTest::qWait(100);
    QVERIFY(keyboard.open());
    FakeDaemon::emitSignal("Closed", {session});
    QTRY_VERIFY(!keyboard.open());
    keyboard.type(QStringLiteral("late"));
    QTest::qWait(100);
    QCOMPARE(daemon.calls.size(), 2);

    // The shell hides the panel: Close for the session.
    FakeDaemon::emitSignal("Opened", {other});
    QTRY_VERIFY(keyboard.open());
    keyboard.close();
    QVERIFY(!keyboard.open());
    QTRY_COMPARE(daemon.calls.last(), QStringLiteral("Close|") + other);

    // A session the daemon already ended hides the panel on Conflict.
    FakeDaemon::emitSignal("Opened", {session});
    QTRY_VERIFY(keyboard.open());
    daemon.conflict = true;
    keyboard.type(QStringLiteral("x"));
    QTRY_VERIFY(!keyboard.open());

    // A daemon owner change ends the session.
    daemon.conflict = false;
    FakeDaemon::emitSignal("Opened", {other});
    QTRY_VERIFY(keyboard.open());
    QDBusConnection::sessionBus().unregisterService(QStringLiteral("org.arinn.Daemon1"));
    QTRY_VERIFY(!keyboard.open());
  }

  void thePanelTypesIntoTheApplication() {
    QQmlEngine engine;
    QQmlComponent component(&engine, QUrl::fromLocalFile(QStringLiteral(ARINN_PANEL_QML)));
    std::unique_ptr<QObject> object(component.create());
    QVERIFY2(object, qPrintable(component.errorString()));
    auto window = qobject_cast<QQuickWindow *>(object.get());
    QVERIFY(window);
    QSignalSpy typed(object.get(), SIGNAL(typed(QString)));
    QSignalSpy edited(object.get(), SIGNAL(edited(QString)));
    QSignalSpy hide(object.get(), SIGNAL(hideRequested()));
    QVERIFY(!window->isVisible());
    // Keys before the panel shows do nothing.
    QMetaObject::invokeMethod(object.get(), "press", Q_ARG(QVariant, QStringLiteral("select")));
    QCOMPARE(typed.count(), 0);
    object->setProperty("active", true);
    QTRY_VERIFY(window->isVisible());
    auto keyboard = window->findChild<QQuickItem *>(QStringLiteral("externalOnScreenKeyboard"));
    QVERIFY(keyboard);
    QVERIFY(keyboard->property("external").toBool());
    // No suggestions and no context row outside the shell.
    QCOMPARE(keyboard->property("suggestionCount").toInt(), 0);
    auto none = window->findChild<QQuickItem *>(QStringLiteral("keyboardNoSuggestions"));
    QVERIFY(none);
    QCOMPARE(none->property("text").toString(), QString());
    // OK types the focused key; arrows move; Up from the top row stays.
    const auto press = [&](const char *key) {
      QMetaObject::invokeMethod(object.get(), "press", Q_ARG(QVariant, QString::fromLatin1(key)));
    };
    press("select");
    QCOMPARE(typed.count(), 1);
    QCOMPARE(typed.at(0).at(0).toString().size(), 1);
    press("right");
    press("select");
    QCOMPARE(typed.count(), 2);
    QVERIFY(typed.at(1).at(0).toString() != typed.at(0).at(0).toString());
    for (int i = 0; i < 6; ++i)
      press("up");
    QVERIFY(window->isVisible());
    QCOMPARE(hide.count(), 0);
    // The main key is Enter and sends Return; Backspace and the cursor keys
    // are editing keys for the application.
    const QPoint enter = keyLabelled(keyboard, QStringLiteral("Enter"));
    QVERIFY(enter.x() >= 0);
    auto model = keyboard->property("model").value<QObject *>();
    QMetaObject::invokeMethod(model, "focusKey", Q_ARG(int, enter.y()), Q_ARG(int, enter.x()));
    press("select");
    QCOMPARE(edited.count(), 1);
    QCOMPARE(edited.at(0).at(0).toString(), QStringLiteral("return"));
    const auto apply = [&](const char *action) {
      const QVariantMap result{{QStringLiteral("action"), QString::fromLatin1(action)}};
      QMetaObject::invokeMethod(keyboard, "apply", Q_ARG(QVariant, QVariant(result)));
    };
    apply("backspace");
    apply("left");
    QCOMPARE(edited.count(), 3);
    QCOMPARE(edited.at(1).at(0).toString(), QStringLiteral("backspace"));
    QCOMPARE(edited.at(2).at(0).toString(), QStringLiteral("left"));
    // Back asks to hide the panel.
    press("back");
    QCOMPARE(hide.count(), 1);
  }

  void mainHidesThePanelOnBackHomeAndLock() {
    BackendStandIn backend;
    GuardStandIn guard;
    QQmlApplicationEngine engine;
    engine.rootContext()->setContextProperty(QStringLiteral("backend"), &backend);
    engine.rootContext()->setContextProperty(QStringLiteral("sessionGuard"), &guard);
    installAssetImageProvider(&engine);
    engine.load(QUrl::fromLocalFile(QStringLiteral(ARINN_MAIN_QML)));
    QVERIFY(!engine.rootObjects().isEmpty());
    auto shell = qobject_cast<QQuickWindow *>(engine.rootObjects().first());
    QVERIFY(shell);
    auto panel = shell->findChild<QQuickWindow *>(QStringLiteral("keyboardPanelWindow"));
    QVERIFY(panel);
    QVERIFY(!panel->transientParent());
    QVERIFY(!shell->isVisible());
    QVERIFY(!panel->isVisible());
    // The platform setup ran before the first show: the panel never takes
    // focus and is the window KWin's rule and script recognise.
    QVERIFY(panel->flags().testFlag(Qt::WindowDoesNotAcceptFocus));
    QVERIFY(panel->flags().testFlag(Qt::FramelessWindowHint));
    QCOMPARE(panel->title(), QStringLiteral("org.arinn.KeyboardPanel"));

    // An application window has focus when the key is pressed.
    QWindow application;
    application.resize(200, 200);
    application.show();
    application.requestActivate();
    QTRY_COMPARE(QGuiApplication::focusWindow(), &application);

    FakeDaemon::emitSignal("Opened", {session});
    QTRY_VERIFY(panel->isVisible());
    QVERIFY(!shell->isVisible());
    QTest::qWait(100);
    // The offscreen platform activates every window it shows; a real X
    // server (the X11 run of this test) keeps focus where it is.
    const bool realFocus = QGuiApplication::platformName() == QLatin1String("xcb");
    if (realFocus)
      QCOMPARE(QGuiApplication::focusWindow(), &application);
#ifdef ARINN_TEST_XCB
    if (realFocus) {
      // Override-redirect and WM_HINTS input = False: no window manager
      // focuses, lists or stacks the panel.
      auto *x11 = qGuiApp->nativeInterface<QNativeInterface::QX11Application>();
      QVERIFY(x11);
      xcb_connection_t *c = x11->connection();
      const auto id = xcb_window_t(panel->winId());
      auto *attributes = xcb_get_window_attributes_reply(
          c, xcb_get_window_attributes(c, id), nullptr);
      QVERIFY(attributes);
      QVERIFY(attributes->override_redirect);
      free(attributes);
      auto *hints = xcb_get_property_reply(
          c, xcb_get_property(c, 0, id, XCB_ATOM_WM_HINTS, XCB_ATOM_WM_HINTS, 0, 9), nullptr);
      QVERIFY(hints);
      QVERIFY(xcb_get_property_value_length(hints) >= 8);
      const auto *words = static_cast<const quint32 *>(xcb_get_property_value(hints));
      QVERIFY(words[0] & 1);     // InputHint present
      QCOMPARE(words[1], 0u);    // input = False
      free(hints);
    }
#endif
    // Remote keys reach the panel through the daemon, never as key events.
    FakeDaemon::emitSignal("Input", {session, QStringLiteral("select")});
    QTRY_COMPARE(daemon.calls.size(), 1);
    QVERIFY(daemon.calls.at(0).startsWith(QStringLiteral("Type|") + session + QLatin1Char('|')));
    // Menu sent at the panel is not one of its keys: nothing happens.
    QTest::keyClick(panel, Qt::Key_Menu);
    QTest::qWait(50);
    QCOMPARE(daemon.calls.size(), 1);
    QVERIFY(panel->isVisible());
    // Back hides the panel and closes the session; the application keeps
    // focus.
    FakeDaemon::emitSignal("Input", {session, QStringLiteral("back")});
    QTRY_VERIFY(!panel->isVisible());
    QTRY_COMPARE(daemon.calls.last(), QStringLiteral("Close|") + session);
    if (realFocus)
      QCOMPARE(QGuiApplication::focusWindow(), &application);

    // Home hides the panel at once and closes the session.
    FakeDaemon::emitSignal("Opened", {other});
    QTRY_VERIFY(panel->isVisible());
    backend.setHome(true);
    QTRY_VERIFY(!panel->isVisible());
    QTRY_COMPARE(daemon.calls.last(), QStringLiteral("Close|") + other);
    backend.setHome(false);

    // The shell on screen for anything else hides it too: the quick menu's
    // sheet, the power scope, ambient, the launch overlay and the player all
    // live in the shell window (ADR-0106, ADR-0108).
    FakeDaemon::emitSignal("Opened", {session});
    QTRY_VERIFY(panel->isVisible());
    shell->setVisibility(QWindow::FullScreen);
    QTRY_VERIFY(!panel->isVisible());
    QTRY_COMPARE(daemon.calls.last(), QStringLiteral("Close|") + session);
    // A session that opens while the shell is on screen has no panel and is
    // closed at once.
    FakeDaemon::emitSignal("Opened", {other});
    QTRY_COMPARE(daemon.calls.last(), QStringLiteral("Close|") + other);
    QVERIFY(!panel->isVisible());
    shell->setVisibility(QWindow::Hidden);
    QTRY_VERIFY(!shell->isVisible());

    // An application launch on screen, also in its "Stopping…" state
    // (ADR-0110): the shell carries it, so the panel closes, and a session
    // opened meanwhile is closed at once without a panel.
    FakeDaemon::emitSignal("Opened", {session});
    QTRY_VERIFY(panel->isVisible());
    backend.setLaunch(true, false);
    backend.setHome(true);
    QTRY_VERIFY(!panel->isVisible());
    QTRY_COMPARE(daemon.calls.last(), QStringLiteral("Close|") + session);
    auto launch = shell->findChild<QQuickItem *>(QStringLiteral("applicationLaunch"));
    QVERIFY(launch);
    QTRY_VERIFY(launch->isVisible());
    backend.setLaunch(true, true);
    QTRY_COMPARE(launch->property("progressTitle").toString(), QStringLiteral("Stopping Demo…"));
    FakeDaemon::emitSignal("Opened", {other});
    QTRY_COMPARE(daemon.calls.last(), QStringLiteral("Close|") + other);
    QVERIFY(!panel->isVisible());
    backend.setLaunch(false, false);
    backend.setHome(false);
    QTRY_VERIFY(!shell->isVisible());

    // A lock hides it and closes the session; nothing is typed after it.
    FakeDaemon::emitSignal("Opened", {session});
    QTRY_VERIFY(panel->isVisible());
    guard.setBlocked(true);
    QTRY_VERIFY(!panel->isVisible());
    QTRY_COMPARE(daemon.calls.last(), QStringLiteral("Close|") + session);
    const auto calls = daemon.calls.size();
    FakeDaemon::emitSignal("Input", {session, QStringLiteral("select")});
    QTest::qWait(100);
    QCOMPARE(daemon.calls.size(), calls);
  }
};

// With ARINN_TEST_XVFB=1 the test runs on its own Xvfb server with Qt's xcb
// platform, never on an inherited DISPLAY. Exit status 77 (skipped) when
// Xvfb is not installed.
int main(int argc, char **argv) {
  QProcess server;
  if (qEnvironmentVariableIntValue("ARINN_TEST_XVFB") == 1) {
    server.setProgram(qEnvironmentVariable("ARINN_XVFB", QStringLiteral("Xvfb")));
    server.setArguments({QStringLiteral("-displayfd"), QStringLiteral("1"),
                         QStringLiteral("-screen"), QStringLiteral("0"),
                         QStringLiteral("1920x1080x24"), QStringLiteral("-nolisten"),
                         QStringLiteral("tcp")});
    server.setProcessChannelMode(QProcess::SeparateChannels);
    server.setStandardErrorFile(QProcess::nullDevice());
    server.start();
    if (!server.waitForStarted(5000) || !server.waitForReadyRead(10000))
      return 77;
    const QByteArray display = server.readLine().trimmed();
    if (display.isEmpty())
      return 77;
    qputenv("DISPLAY", ":" + display);
    qputenv("QT_QPA_PLATFORM", "xcb");
  }
  QGuiApplication app(argc, argv);
  ExternalKeyboardTest test;
  const int result = QTest::qExec(&test, argc, argv);
  if (server.state() != QProcess::NotRunning) {
    server.kill();
    server.waitForFinished();
  }
  return result;
}
#include "external_keyboard_test.moc"
