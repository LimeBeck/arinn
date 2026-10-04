// SPDX-FileCopyrightText: 2026 Arinn contributors
//
// SPDX-License-Identifier: GPL-3.0-or-later

// ADR-0079: bound keys become the action's canonical key before the shell
// window sees them; text keys and modified keys are left alone; bindings are
// read from Daemon1.GetInputBindings.
#include "input_bindings.h"

#include <QDBusConnection>
#include <QDBusMessage>
#include <QDBusMetaType>
#include <QDBusVirtualObject>
#include <QGuiApplication>
#include <QKeyEvent>
#include <QTest>
#include <QWindow>

namespace {
// Records the keys that reach the window.
class Window : public QWindow {
public:
  QList<int> pressed;
  QList<int> released;

protected:
  void keyPressEvent(QKeyEvent *event) override { pressed.append(event->key()); }
  void keyReleaseEvent(QKeyEvent *event) override { released.append(event->key()); }
};

// A key as the platform delivers it: Qt key, XKB keycode (evdev + 8), text.
void press(QWindow *window, int key, quint32 xkb, const QString &text = {},
           Qt::KeyboardModifiers modifiers = Qt::NoModifier) {
  QKeyEvent down(QEvent::KeyPress, key, modifiers, xkb, 0, 0, text);
  QCoreApplication::sendEvent(window, &down);
  QKeyEvent up(QEvent::KeyRelease, key, modifiers, xkb, 0, 0, text);
  QCoreApplication::sendEvent(window, &up);
}

// One (action, keys) entry for the fake daemon's a(sas) reply.
struct WireBinding {
  QString action;
  QStringList keys;
};
QDBusArgument &operator<<(QDBusArgument &argument, const WireBinding &binding) {
  argument.beginStructure();
  argument << binding.action << binding.keys;
  argument.endStructure();
  return argument;
}
const QDBusArgument &operator>>(const QDBusArgument &argument, WireBinding &binding) {
  argument.beginStructure();
  argument >> binding.action >> binding.keys;
  argument.endStructure();
  return argument;
}

// org.arinn.Daemon1 with only GetInputBindings.
class Daemon : public QDBusVirtualObject {
public:
  QList<WireBinding> bindings;
  QString introspect(const QString &) const override { return {}; }
  bool handleMessage(const QDBusMessage &message,
                     const QDBusConnection &connection) override {
    if (message.member() != QLatin1String("GetInputBindings"))
      return false;
    auto reply = message.createReply();
    reply << QVariant::fromValue(bindings);
    connection.send(reply);
    return true;
  }
};
} // namespace
Q_DECLARE_METATYPE(WireBinding)

class InputBindingsTest : public QObject {
  Q_OBJECT
private slots:
  void boundKeysBecomeCanonical() {
    Window window;
    InputBindings bindings;
    qApp->installEventFilter(&bindings);
    bindings.setBindings({{QStringLiteral("menu"), {QStringLiteral("evdev:582")}},
                          {QStringLiteral("select"), {QStringLiteral("F5")}},
                          {QStringLiteral("jump"), {QStringLiteral("F6")}}});
    // A remote's voice button: Qt has no key for it, only the keycode.
    press(&window, 0, 582 + 8);
    press(&window, Qt::Key_F5, 71);
    press(&window, Qt::Key_F6, 72);
    QCOMPARE(window.pressed, (QList<int>{Qt::Key_Menu, Qt::Key_Return, Qt::Key_F6}));
    QCOMPARE(window.released, (QList<int>{Qt::Key_Menu, Qt::Key_Return, Qt::Key_F6}));
    qApp->removeEventFilter(&bindings);
  }

  void textAndModifiedKeysAreNotRebound() {
    Window window;
    InputBindings bindings;
    qApp->installEventFilter(&bindings);
    // Neither a letter by name nor a code that types text is rebound.
    bindings.setBindings({{QStringLiteral("back"), {QStringLiteral("A")}},
                          {QStringLiteral("menu"), {QStringLiteral("evdev:30"),
                                                    QStringLiteral("F5")}}});
    press(&window, Qt::Key_A, 30 + 8, QStringLiteral("a"));
    press(&window, Qt::Key_F5, 71, {}, Qt::ControlModifier);
    QCOMPARE(window.pressed, (QList<int>{Qt::Key_A, Qt::Key_F5}));
    qApp->removeEventFilter(&bindings);
  }

  // Review of #288: defaults keep their meaning, modifiers and unknown or
  // ambiguous names never match.
  void defaultsModifiersAndAmbiguityAreKept() {
    Window window;
    InputBindings bindings;
    qApp->installEventFilter(&bindings);
    bindings.setBindings({{QStringLiteral("select"), {QStringLiteral("Menu")}},
                          {QStringLiteral("menu"), {QStringLiteral("evdev:582"),
                                                    QStringLiteral("NotAQtKey")}},
                          {QStringLiteral("up"), {QStringLiteral("F5")}},
                          {QStringLiteral("down"), {QStringLiteral("f5")}}});
    press(&window, Qt::Key_Menu, 135);                            // stays Menu
    press(&window, 0, 582 + 8, {}, Qt::ControlModifier);          // modified: unchanged
    press(&window, Qt::Key_unknown, 0);                           // unknown name ignored
    press(&window, Qt::Key_F5, 71);                               // F5 vs f5: dropped
    press(&window, 0, 582 + 8);                                   // still bound
    QCOMPARE(window.pressed,
             (QList<int>{Qt::Key_Menu, 0, Qt::Key_unknown, Qt::Key_F5, Qt::Key_Menu}));
    qApp->removeEventFilter(&bindings);
  }

  // #328: F5 by name and by its evdev code is one key; bound to two actions
  // it is ambiguous and stays F5.
  void nameAndCodeOfOneKeyAreAmbiguous() {
    Window window;
    InputBindings bindings;
    qApp->installEventFilter(&bindings);
    bindings.setBindings({{QStringLiteral("up"), {QStringLiteral("F5")}},
                          {QStringLiteral("down"), {QStringLiteral("evdev:63")}},
                          {QStringLiteral("select"), {QStringLiteral("F6"),
                                                      QStringLiteral("evdev:64")}}});
    press(&window, Qt::Key_F5, 63 + 8);
    // Name and code of one key on the same action still map.
    press(&window, Qt::Key_F6, 64 + 8);
    QCOMPARE(window.pressed, (QList<int>{Qt::Key_F5, Qt::Key_Return}));
    qApp->removeEventFilter(&bindings);
  }

  void bindingsComeFromTheDaemon() {
    auto bus = QDBusConnection::sessionBus();
    QVERIFY(bus.isConnected());
    qDBusRegisterMetaType<WireBinding>();
    qDBusRegisterMetaType<QList<WireBinding>>();
    Daemon daemon;
    daemon.bindings = {{QStringLiteral("menu"), {QStringLiteral("evdev:582")}}};
    QVERIFY(bus.registerVirtualObject(QStringLiteral("/org/arinn/Daemon1"), &daemon));
    QVERIFY(bus.registerService(QStringLiteral("org.arinn.Daemon1")));
    Window window;
    InputBindings bindings;
    qApp->installEventFilter(&bindings);
    bindings.reload();
    QTRY_VERIFY_WITH_TIMEOUT(
        [&] {
          window.pressed.clear();
          press(&window, 0, 582 + 8);
          return window.pressed == QList<int>{Qt::Key_Menu};
        }(),
        3000);
    qApp->removeEventFilter(&bindings);
    bus.unregisterService(QStringLiteral("org.arinn.Daemon1"));
    bus.unregisterObject(QStringLiteral("/org/arinn/Daemon1"));
  }
};

QTEST_MAIN(InputBindingsTest)
#include "input_bindings_test.moc"
