// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// #387: SystemStatus reads org.arinn.SystemStatus1 from a fake daemon on a
// private bus: Changed and re-read, the revision floor, field-by-field
// fallback, failures and a daemon restart (ADR-0100).
#include "system_status.h"
#include <QSignalSpy>
#include <QTest>
#include <QtDBus>

namespace {
const QString service = QStringLiteral("org.arinn.Daemon1");
const QString path = QStringLiteral("/org/arinn/Daemon1");
const QString interface = QStringLiteral("org.arinn.SystemStatus1");
const QString epoch1 = QStringLiteral("10000000-0000-4000-8000-000000000001");
const QString epoch2 = QStringLiteral("20000000-0000-4000-8000-000000000002");

class FakeDaemon : public QDBusVirtualObject {
public:
  QString epoch = epoch1;
  qulonglong revision = 1;
  QString audio = QStringLiteral("muted");
  QString network = QStringLiteral("connected");
  QString link = QStringLiteral("wifi");
  uchar level = 3;
  bool fail = false;
  // Fail this many more reads, then answer.
  int failNext = 0;
  QString errorName = QStringLiteral("org.arinn.Error1.Internal");
  // Hold the next Get unanswered; `held` answers it later.
  bool hold = false;
  QDBusMessage held;
  int reads = 0;
  bool handleMessage(const QDBusMessage &message,
                     const QDBusConnection &connection) override {
    if (message.interface() != interface ||
        message.member() != QLatin1String("Get"))
      return false;
    ++reads;
    if (hold) {
      hold = false;
      held = message;
      return true;
    }
    if (fail || failNext > 0) {
      failNext = qMax(0, failNext - 1);
      connection.send(message.createErrorReply(errorName, QString()));
      return true;
    }
    QDBusArgument status;
    status.beginStructure();
    status << epoch << revision << audio << network << link << level;
    status.endStructure();
    connection.send(message.createReply(QVariant::fromValue(status)));
    return true;
  }
  QString introspect(const QString &) const override { return {}; }
};
} // namespace

class SystemStatusTest : public QObject {
  Q_OBJECT
  QDBusConnection daemon =
      QDBusConnection::connectToBus(QDBusConnection::SessionBus,
                                    QStringLiteral("fake-daemon"));
  FakeDaemon fake;
  void up() {
    QVERIFY(daemon.registerVirtualObject(path, &fake));
    QVERIFY(daemon.registerService(service));
  }
  void down() {
    daemon.unregisterService(service);
    daemon.unregisterObject(path);
  }
  void changed(const QString &epoch, qulonglong revision) {
    auto signal = QDBusMessage::createSignal(path, interface,
                                             QStringLiteral("Changed"));
    signal << epoch << revision;
    QVERIFY(daemon.send(signal));
  }
  static bool shows(const SystemStatus &s, bool muted, bool wifi, int level) {
    return s.muted() == muted && s.wifi() == wifi && s.wifiLevel() == level;
  }
private slots:
  void readsChangesAndFallsBackFieldByField() {
    QVERIFY(daemon.isConnected());
    SystemStatus status;
    // No daemon yet: nothing is shown.
    QTest::qWait(200);
    QVERIFY(shows(status, false, false, 0));

    up();
    QTRY_VERIFY(shows(status, true, true, 3));

    // Changed for a newer revision re-reads.
    fake.revision = 2;
    fake.audio = QStringLiteral("unmuted");
    fake.link = QStringLiteral("wired");
    fake.level = 0;
    changed(epoch1, 2);
    QTRY_VERIFY(shows(status, false, false, 0));
    // The quick menu reads the known values themselves (ADR-0106).
    QCOMPARE(status.audio(), QStringLiteral("unmuted"));
    QCOMPARE(status.network(), QStringLiteral("wired"));

    // A Changed for an applied revision is not read again.
    const int reads = fake.reads;
    changed(epoch1, 2);
    QTest::qWait(200);
    QCOMPARE(fake.reads, reads);

    // A reply older than the applied revision never applies.
    fake.revision = 1;
    fake.audio = QStringLiteral("muted");
    changed(epoch1, 3);
    QTRY_COMPARE(fake.reads, reads + 1);
    QTest::qWait(100);
    QVERIFY(shows(status, false, false, 0));

    // Values this build does not know show nothing, field by field.
    fake.revision = 4;
    fake.audio = QStringLiteral("ducked");
    fake.link = QStringLiteral("wifi");
    fake.level = 9;
    changed(epoch1, 4);
    QTRY_VERIFY(shows(status, false, true, 0));
    QCOMPARE(status.audio(), QString());
    QCOMPARE(status.network(), QStringLiteral("wifi"));
    fake.revision = 5;
    fake.audio = QStringLiteral("muted");
    fake.network = QStringLiteral("limited");
    fake.level = 2;
    changed(epoch1, 5);
    QTRY_VERIFY(shows(status, true, false, 0));
    QCOMPARE(status.audio(), QStringLiteral("muted"));
    QCOMPARE(status.network(), QString());

    // A failed read clears what was shown.
    fake.fail = true;
    changed(epoch1, 6);
    QTRY_VERIFY(shows(status, false, false, 0));
    QCOMPARE(status.network(), QString());
    fake.fail = false;
    fake.revision = 6;
    fake.network = QStringLiteral("connected");
    changed(epoch1, 6);
    QTRY_VERIFY(shows(status, true, true, 2));

    // An invalid epoch or revision 0 is not a snapshot.
    fake.revision = 7;
    fake.epoch = QStringLiteral("not-a-uuid");
    changed(epoch1, 7);
    QTRY_VERIFY(shows(status, false, false, 0));
    fake.epoch = epoch1;
    changed(epoch1, 7);
    QTRY_VERIFY(shows(status, true, true, 2));

    // Overlapping reads: a superseded read that fails late never clears the
    // newer state it was overtaken by.
    fake.hold = true;
    fake.revision = 8;
    changed(epoch1, 8);
    QTRY_VERIFY(fake.held.type() == QDBusMessage::MethodCallMessage);
    fake.revision = 9;
    fake.level = 1;
    changed(epoch1, 9);
    QTRY_VERIFY(shows(status, true, true, 1));
    QVERIFY(daemon.send(fake.held.createErrorReply(
        QStringLiteral("org.arinn.Error1.Internal"), QString())));
    QTest::qWait(200);
    QVERIFY(shows(status, true, true, 1));

    // A daemon restart: the state clears at once, and the new daemon's
    // first revision applies although it is lower.
    down();
    QTRY_VERIFY(shows(status, false, false, 0));
    fake.epoch = epoch2;
    fake.revision = 1;
    fake.level = 4;
    up();
    QTRY_VERIFY(shows(status, true, true, 4));
    fake.revision = 2;
    fake.network = QStringLiteral("disconnected");
    changed(epoch2, 2);
    QTRY_COMPARE(status.network(), QStringLiteral("disconnected"));
    QVERIFY(shows(status, true, false, 0));
    down();
  }

  // #409: a failed read is retried a bounded number of times without
  // waiting for the next Changed; a lasting error is not retried.
  void retriesAfterAFailedRead() {
    fake.epoch = epoch1;
    fake.revision = 1;
    fake.audio = QStringLiteral("muted");
    fake.network = QStringLiteral("connected");
    fake.link = QStringLiteral("wifi");
    fake.level = 3;
    fake.fail = false;
    fake.reads = 0;
    fake.failNext = 1;
    up();
    SystemStatus status;
    status.setRetryDelays({50, 100});
    // The first read fails; the retry, with no Changed, shows the state.
    QTRY_VERIFY(shows(status, true, true, 3));
    QCOMPARE(fake.reads, 2);

    // Every read fails: two retries, then it waits for the next Changed.
    fake.fail = true;
    changed(epoch1, 2);
    QTRY_COMPARE(fake.reads, 5);
    QTest::qWait(400);
    QCOMPARE(fake.reads, 5);
    QVERIFY(shows(status, false, false, 0));

    // A Changed starts a fresh round, and a good read ends it.
    fake.fail = false;
    fake.failNext = 2;
    fake.revision = 3;
    changed(epoch1, 3);
    QTRY_VERIFY(shows(status, true, true, 3));
    QCOMPARE(fake.reads, 8);

    // The daemon owns its name before it exports SystemStatus1, so a
    // missing interface is retried too.
    fake.errorName =
        QStringLiteral("org.freedesktop.DBus.Error.UnknownInterface");
    fake.failNext = 1;
    fake.revision = 4;
    fake.audio = QStringLiteral("unmuted");
    changed(epoch1, 4);
    QTRY_VERIFY(shows(status, false, true, 3));
    QCOMPARE(fake.reads, 10);

    // A name without an owner is not retried: the owner change brings the
    // next read.
    fake.errorName = QStringLiteral("org.freedesktop.DBus.Error.ServiceUnknown");
    fake.fail = true;
    changed(epoch1, 5);
    QTRY_COMPARE(fake.reads, 11);
    QTest::qWait(400);
    QCOMPARE(fake.reads, 11);
    fake.fail = false;
    fake.errorName = QStringLiteral("org.arinn.Error1.Internal");
    down();
  }
};

QTEST_GUILESS_MAIN(SystemStatusTest)
#include "system_status_test.moc"
