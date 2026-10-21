// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// #148: SystemControl calls org.arinn.SystemControl1 (ADR-0108) on a fake
// daemon on a private bus: power availability and inhibitors, typed
// refusals, no success without the platform's answer, and volume replies
// for the HUD. Nothing here suspends or touches a sound server.
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
    if (member == QLatin1String("Suspend") ||
        member == QLatin1String("Reboot") ||
        member == QLatin1String("PowerOff")) {
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

    // A failure shows no level and reports why.
    fake->volumeError = QStringLiteral("org.arinn.Error1.Unavailable");
    control.stepVolume(-1);
    QTRY_COMPARE(failed.count(), 1);
    QCOMPARE(failed.first().first().toString(), QStringLiteral("unavailable"));
    QCOMPARE(control.volumeLevel(), 50);
    // An unknown state is not a level either.
    fake->volumeError.clear();
    fake->audio = QStringLiteral("unknown");
    control.stepVolume(1);
    QTRY_COMPARE(failed.count(), 2);
    QVERIFY(!control.volumeKnown());
    QCOMPARE(control.volumeLevel(), 0);
  }
};

QTEST_GUILESS_MAIN(SystemControlTest)
#include "system_control_test.moc"
