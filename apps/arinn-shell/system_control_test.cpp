// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// #148: SystemControl calls org.arinn.SystemControl1 (ADR-0108) on a fake
// daemon on a private bus: power availability and inhibitors, typed
// refusals, no success without the platform's answer, and volume replies
// for the HUD. #438: the session switch read and Switch to desktop, which
// shares the one-request rule. Nothing here suspends, logs out or touches a
// sound server.
#include "system_control.h"
#include <QSignalSpy>
#include <memory>
#include <QTest>
#include <QtDBus>

namespace {
const QString service = QStringLiteral("org.arinn.Daemon1");
const QString path = QStringLiteral("/org/arinn/Daemon1");
const QString interface = QStringLiteral("org.arinn.SystemControl1");

struct Inhibitor {
  QString what, who, why;
};
QDBusArgument &operator<<(QDBusArgument &arg, const Inhibitor &inhibitor) {
  arg.beginStructure();
  arg << inhibitor.what << inhibitor.who << inhibitor.why;
  arg.endStructure();
  return arg;
}
const QDBusArgument &operator>>(const QDBusArgument &arg, Inhibitor &inhibitor) {
  arg.beginStructure();
  arg >> inhibitor.what >> inhibitor.who >> inhibitor.why;
  arg.endStructure();
  return arg;
}

class FakeDaemon : public QDBusVirtualObject {
public:
  QString suspend = QStringLiteral("available");
  QString reboot = QStringLiteral("inhibited");
  QString powerOff = QStringLiteral("hibernating"); // not in this contract
  QList<Inhibitor> inhibitors = {
      {QStringLiteral("shutdown"), QStringLiteral("Fixture player"),
       QStringLiteral("Playing")},
      {QStringLiteral("idle"), QStringLiteral("Ignored"), QStringLiteral("-")}};
  // GetSessionSwitch's reply; empty answers UnknownMethod (an older daemon).
  QString sessionSwitch = QStringLiteral("available");
  // Error name for the next power request; empty accepts it.
  QString refuse;
  // Hold power requests unanswered; `held` answers later.
  bool hold = false;
  QDBusMessage held;
  QStringList calls;
  QString audio = QStringLiteral("unmuted");
  uint level = 40;
  QString volumeError;
  bool handleMessage(const QDBusMessage &message,
                     const QDBusConnection &connection) override {
    if (message.interface() != interface)
      return false;
    const QString member = message.member();
    calls.append(member);
    if (member == QLatin1String("GetPower")) {
      connection.send(message.createReply(QVariant::fromValue(power())));
      return true;
    }
    if (member == QLatin1String("GetSessionSwitch")) {
      connection.send(
          sessionSwitch.isEmpty()
              ? message.createErrorReply(
                    QStringLiteral("org.freedesktop.DBus.Error.UnknownMethod"),
                    QString())
              : message.createReply(sessionSwitch));
      return true;
    }
    if (member == QLatin1String("Suspend") ||
        member == QLatin1String("Reboot") ||
        member == QLatin1String("PowerOff") ||
        member == QLatin1String("SwitchToDesktop")) {
      if (hold) {
        held = message;
        return true;
      }
      connection.send(refuse.isEmpty()
                          ? message.createReply()
                          : message.createErrorReply(refuse, QString()));
      return true;
    }
    if (!volumeError.isEmpty()) {
      connection.send(message.createErrorReply(volumeError, QString()));
      return true;
    }
    if (member == QLatin1String("StepVolume"))
      level = qBound<int>(0, int(level) + 5 * message.arguments().at(0).toInt(),
                          100);
    else if (member == QLatin1String("ToggleMuted"))
      audio = audio == QLatin1String("muted") ? QStringLiteral("unmuted")
                                              : QStringLiteral("muted");
    else if (member == QLatin1String("SetMuted"))
      audio = message.arguments().at(0).toBool() ? QStringLiteral("muted")
                                                 : QStringLiteral("unmuted");
    else if (member != QLatin1String("GetVolume"))
      return false;
    QDBusArgument volume;
    volume.beginStructure();
    volume << audio << level;
    volume.endStructure();
    connection.send(message.createReply(QVariant::fromValue(volume)));
    return true;
  }
  QDBusArgument power() const {
    QDBusArgument state;
    state.beginStructure();
    state << suspend << reboot << powerOff;
    state << inhibitors;
    state.endStructure();
    return state;
  }
  QString introspect(const QString &) const override { return {}; }
};
} // namespace
Q_DECLARE_METATYPE(Inhibitor)

class SystemControlTest : public QObject {
  Q_OBJECT
  QDBusConnection daemon =
      QDBusConnection::connectToBus(QDBusConnection::SessionBus,
                                    QStringLiteral("fake-daemon"));
  std::unique_ptr<FakeDaemon> fake;
  void up() {
    QVERIFY(daemon.registerVirtualObject(path, fake.get()));
    QVERIFY(daemon.registerService(service));
  }
  void down() {
    daemon.unregisterService(service);
    daemon.unregisterObject(path);
  }
private slots:
  void initTestCase() { qDBusRegisterMetaType<Inhibitor>(); }
  void init() { fake = std::make_unique<FakeDaemon>(); }
  void cleanup() { down(); }

  void readsPowerWithReaderRules() {
    up();
    SystemControl control;
    QCOMPARE(control.powerState(), QString());
    control.readPower();
    QCOMPARE(control.powerState(), QStringLiteral("loading"));
    QTRY_COMPARE(control.powerState(), QStringLiteral("ready"));
    QCOMPARE(control.suspend(), QStringLiteral("available"));
    QCOMPARE(control.reboot(), QStringLiteral("inhibited"));
    // A value this build does not know is unknown.
    QCOMPARE(control.powerOff(), QStringLiteral("unknown"));
    // Only sleep and shutdown inhibitors are kept.
    QCOMPARE(control.inhibitors().size(), 1);
    const auto inhibitor = control.inhibitors().first().toMap();
    QCOMPARE(inhibitor.value("what").toString(), QStringLiteral("shutdown"));
    QCOMPARE(inhibitor.value("who").toString(), QStringLiteral("Fixture player"));
    QCOMPARE(inhibitor.value("why").toString(), QStringLiteral("Playing"));
  }

  void withoutTheInterfaceEverythingIsUnknown() {
    // No daemon at all: the menu keeps its actions, the daemon answers each.
    SystemControl control;
    control.readPower();
    QTRY_COMPARE(control.powerState(), QStringLiteral("ready"));
    QCOMPARE(control.suspend(), QStringLiteral("unknown"));
    QCOMPARE(control.inhibitors().size(), 0);
    control.request(QStringLiteral("suspend"));
    QTRY_COMPARE(control.refusal(), QStringLiteral("unavailable"));
    QCOMPARE(control.refusalAction(), QStringLiteral("suspend"));
    QCOMPARE(control.acceptedAction(), QString());
  }

  void acceptanceNeedsTheDaemonsReply() {
    up();
    fake->hold = true;
    SystemControl control;
    QSignalSpy changes(&control, &SystemControl::requestChanged);
    control.request(QStringLiteral("suspend"));
    QCOMPARE(control.pendingAction(), QStringLiteral("suspend"));
    // A second request while one is in flight is ignored.
    control.request(QStringLiteral("reboot"));
    QTRY_VERIFY(fake->held.type() == QDBusMessage::MethodCallMessage);
    // Waiting never turns into success.
    QTest::qWait(500);
    QCOMPARE(control.acceptedAction(), QString());
    QCOMPARE(control.pendingAction(), QStringLiteral("suspend"));
    QCOMPARE(fake->calls.count(QStringLiteral("Reboot")), 0);
    daemon.send(fake->held.createReply());
    QTRY_COMPARE(control.acceptedAction(), QStringLiteral("suspend"));
    QCOMPARE(control.pendingAction(), QString());
    QCOMPARE(control.refusal(), QString());
    control.reset();
    QCOMPARE(control.acceptedAction(), QString());
    QVERIFY(changes.count() >= 3);
    // Unknown actions are not sent.
    control.request(QStringLiteral("hibernate"));
    QCOMPARE(control.pendingAction(), QString());
  }

  void refusalsAreTypedAndInhibitorsReread() {
    up();
    SystemControl control;
    const QList<QPair<QString, QString>> cases = {
        {QStringLiteral("org.arinn.Error1.Conflict"), QStringLiteral("inhibited")},
        {QStringLiteral("org.arinn.Error1.PermissionDenied"), QStringLiteral("denied")},
        {QStringLiteral("org.arinn.Error1.Unsupported"), QStringLiteral("unsupported")},
        {QStringLiteral("org.arinn.Error1.Unavailable"), QStringLiteral("unavailable")},
        {QStringLiteral("org.freedesktop.DBus.Error.UnknownMethod"), QStringLiteral("unavailable")},
        {QStringLiteral("org.arinn.Error1.Timeout"), QStringLiteral("timeout")},
        {QStringLiteral("org.arinn.Error1.Internal"), QStringLiteral("failed")},
    };
    for (const auto &[error, refusal] : cases) {
      fake->refuse = error;
      fake->calls.clear();
      control.request(QStringLiteral("power-off"));
      QTRY_COMPARE(control.refusal(), refusal);
      QCOMPARE(control.refusalAction(), QStringLiteral("power-off"));
      QCOMPARE(control.acceptedAction(), QString());
      QCOMPARE(control.pendingAction(), QString());
      // An inhibitor or permission refusal reads who holds it now.
      if (refusal == QLatin1String("inhibited") ||
          refusal == QLatin1String("denied"))
        QTRY_VERIFY(fake->calls.contains(QStringLiteral("GetPower")));
    }
    QTRY_COMPARE(control.powerState(), QStringLiteral("ready"));
    QCOMPARE(control.inhibitors().size(), 1);
  }

  // #438: the menu offers Switch to desktop only for "available"; every
  // other value, an error and no daemon read as unknown or unsupported.
  void readsTheSessionSwitchWithReaderRules() {
    {
      // No daemon: unknown, so the action is not offered.
      SystemControl control;
      QCOMPARE(control.sessionSwitch(), QStringLiteral("unknown"));
      control.readPower();
      QTRY_COMPARE(control.powerState(), QStringLiteral("ready"));
      QTest::qWait(50);
      QCOMPARE(control.sessionSwitch(), QStringLiteral("unknown"));
    }
    up();
    SystemControl control;
    QSignalSpy changes(&control, &SystemControl::sessionSwitchChanged);
    // The power menu's read asks for both.
    control.readPower();
    QTRY_COMPARE(control.sessionSwitch(), QStringLiteral("available"));
    QVERIFY(fake->calls.contains(QStringLiteral("GetPower")));
    QCOMPARE(fake->calls.count(QStringLiteral("GetSessionSwitch")), 1);
    QCOMPARE(changes.count(), 1);
    const QList<QPair<QString, QString>> cases = {
        {QStringLiteral("unsupported"), QStringLiteral("unsupported")},
        {QStringLiteral("available"), QStringLiteral("available")},
        // A value this build does not know.
        {QStringLiteral("pending"), QStringLiteral("unknown")},
        {QStringLiteral("available"), QStringLiteral("available")},
        // A daemon without the method.
        {QString(), QStringLiteral("unknown")},
    };
    for (const auto &[reply, state] : cases) {
      fake->sessionSwitch = reply;
      control.readPower();
      QTRY_COMPARE(control.sessionSwitch(), state);
    }
  }

  // #438: Switch to desktop is a power request like the others: one at a
  // time in either direction, no success before the daemon's reply, typed
  // refusals, and an Unsupported refusal reads the switch again.
  void switchToDesktopSharesTheOneRequestRule() {
    up();
    fake->hold = true;
    SystemControl control;
    control.readPower();
    QTRY_COMPARE(control.sessionSwitch(), QStringLiteral("available"));
    control.request(QStringLiteral("switch-to-desktop"));
    QCOMPARE(control.pendingAction(), QStringLiteral("switch-to-desktop"));
    control.request(QStringLiteral("suspend"));
    QTRY_VERIFY(fake->held.type() == QDBusMessage::MethodCallMessage);
    QCOMPARE(fake->held.member(), QStringLiteral("SwitchToDesktop"));
    QCOMPARE(fake->held.arguments().size(), 0);
    QTest::qWait(300);
    QCOMPARE(control.acceptedAction(), QString());
    QCOMPARE(fake->calls.count(QStringLiteral("Suspend")), 0);
    daemon.send(fake->held.createReply());
    QTRY_COMPARE(control.acceptedAction(), QStringLiteral("switch-to-desktop"));
    QCOMPARE(control.pendingAction(), QString());
    control.reset();

    // The other direction: a power request in flight holds the switch.
    fake->held = {};
    control.request(QStringLiteral("reboot"));
    control.request(QStringLiteral("switch-to-desktop"));
    QTRY_VERIFY(fake->held.type() == QDBusMessage::MethodCallMessage);
    QCOMPARE(fake->held.member(), QStringLiteral("Reboot"));
    QTest::qWait(100);
    QCOMPARE(fake->calls.count(QStringLiteral("SwitchToDesktop")), 1);
    daemon.send(fake->held.createErrorReply(
        QStringLiteral("org.arinn.Error1.Timeout"), QString()));
    QTRY_COMPARE(control.refusal(), QStringLiteral("timeout"));
    QCOMPARE(control.refusalAction(), QStringLiteral("reboot"));

    fake->hold = false;
    const QList<QPair<QString, QString>> cases = {
        {QStringLiteral("org.arinn.Error1.Unavailable"), QStringLiteral("unavailable")},
        {QStringLiteral("org.arinn.Error1.Timeout"), QStringLiteral("timeout")},
        {QStringLiteral("org.arinn.Error1.Internal"), QStringLiteral("failed")},
        {QStringLiteral("org.arinn.Error1.Conflict"), QStringLiteral("inhibited")},
        {QStringLiteral("org.arinn.Error1.PermissionDenied"), QStringLiteral("denied")},
        {QStringLiteral("org.arinn.Error1.Unsupported"), QStringLiteral("unsupported")},
    };
    for (const auto &[error, refusal] : cases) {
      fake->refuse = error;
      fake->calls.clear();
      if (refusal == QLatin1String("unsupported"))
        fake->sessionSwitch = QStringLiteral("unsupported");
      control.request(QStringLiteral("switch-to-desktop"));
      QTRY_COMPARE(control.refusal(), refusal);
      QCOMPARE(control.refusalAction(), QStringLiteral("switch-to-desktop"));
      QCOMPARE(control.acceptedAction(), QString());
      QCOMPARE(control.pendingAction(), QString());
      QTest::qWait(50);
      // No inhibitor holds the switch: a refusal never reads GetPower.
      QCOMPARE(fake->calls.count(QStringLiteral("GetPower")), 0);
    }
    // Unsupported: the menu stops offering it.
    QTRY_COMPARE(control.sessionSwitch(), QStringLiteral("unsupported"));
    QCOMPARE(fake->calls.count(QStringLiteral("GetSessionSwitch")), 1);
  }

  void volumeRepliesFeedTheHud() {
    up();
    SystemControl control;
    QSignalSpy shown(&control, &SystemControl::volumeShown);
    QSignalSpy failed(&control, &SystemControl::volumeFailed);
    control.readVolume();
    QTRY_VERIFY(control.volumeKnown());
    QCOMPARE(control.volumeLevel(), 40);
    // A read does not open the HUD.
    QCOMPARE(shown.count(), 0);
    control.stepVolume(1);
    control.stepVolume(1);
    QTRY_COMPARE(shown.count(), 2);
    QCOMPARE(control.volumeLevel(), 50);
    QVERIFY(!control.volumeMuted());
    control.toggleMuted();
    QTRY_VERIFY(control.volumeMuted());
    control.setMuted(false);
    QTRY_VERIFY(!control.volumeMuted());
    QCOMPARE(fake->calls.count(QStringLiteral("StepVolume")), 2);

    // Volume Up while muted: the sound comes back on and the level rises,
    // in that order, and the HUD shows the result (owner, 2026-10-21).
    fake->audio = QStringLiteral("muted");
    fake->calls.clear();
    const int before = shown.count();
    control.raiseVolume();
    QTRY_COMPARE(shown.count(), before + 1);
    QCOMPARE(fake->calls, (QStringList{QStringLiteral("SetMuted"),
                                       QStringLiteral("StepVolume")}));
    QVERIFY(!control.volumeMuted());
    QCOMPARE(control.volumeLevel(), 55);

    // Keys apply in the order pressed: Up (unmute, then +1) and an
    // immediate Down never swap (98 → 100 → 95, not 98 → 93 → 98).
    fake->level = 98;
    fake->calls.clear();
    const int queued = shown.count();
    control.raiseVolume();
    control.stepVolume(-1);
    QTRY_COMPARE(shown.count(), queued + 2);
    QCOMPARE(fake->calls, (QStringList{QStringLiteral("SetMuted"),
                                       QStringLiteral("StepVolume"),
                                       QStringLiteral("StepVolume")}));
    QCOMPARE(control.volumeLevel(), 95);
    fake->level = 55;
    control.readVolume();
    QTRY_COMPARE(control.volumeLevel(), 55);

    // A failure shows no level and reports why.
    fake->volumeError = QStringLiteral("org.arinn.Error1.Unavailable");
    control.stepVolume(-1);
    QTRY_COMPARE(failed.count(), 1);
    QCOMPARE(failed.first().first().toString(), QStringLiteral("unavailable"));
    QCOMPARE(control.volumeLevel(), 55);
    // A failed unmute does not go on to step the level.
    fake->calls.clear();
    control.raiseVolume();
    QTRY_COMPARE(failed.count(), 2);
    QCOMPARE(fake->calls, QStringList{QStringLiteral("SetMuted")});
    // An unknown state is not a level either.
    fake->volumeError.clear();
    fake->audio = QStringLiteral("unknown");
    control.stepVolume(1);
    QTRY_COMPARE(failed.count(), 3);
    QVERIFY(!control.volumeKnown());
    QCOMPARE(control.volumeLevel(), 0);
  }
};

QTEST_GUILESS_MAIN(SystemControlTest)
#include "system_control_test.moc"
