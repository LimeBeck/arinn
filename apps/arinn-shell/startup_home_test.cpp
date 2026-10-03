// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

#include <QProcess>
#include <QtDBus>
#include <QtTest>

class StartupDaemon : public QDBusVirtualObject {
public:
  bool platformReady = false;
  bool home = false;
  int attempts = 0;
  int accepted = 0;
  QString introspect(const QString &) const override { return {}; }
  bool handleMessage(const QDBusMessage &message,
                     const QDBusConnection &bus) override {
    if (message.member() == "Home") {
      ++attempts;
      if (!platformReady) {
        bus.send(message.createErrorReply("org.arinn.Error.Unavailable", "fixture"));
      } else {
        ++accepted;
        home = true;
        bus.send(message.createReply());
      }
    } else if (message.member() == "Status") {
      QDBusArgument status;
      status.beginStructure();
      status << QString("synthetic-epoch") << false << QString("synthetic-epoch")
             << qulonglong(0) << home << QString() << QString() << QString()
             << false << platformReady << QString() << QString();
      status.endStructure();
      bus.send(message.createReply(QVariant::fromValue(status)));
    } else {
      bus.send(message.createErrorReply(QDBusError::NotSupported, "fixture"));
    }
    return true;
  }
};

class StartupScreenSaver : public QDBusVirtualObject {
public:
  bool locked = false;
  QString introspect(const QString &) const override { return {}; }
  bool handleMessage(const QDBusMessage &message,
                     const QDBusConnection &bus) override {
    if (message.member() == "GetActive")
      bus.send(message.createReply(locked));
    else
      bus.send(message.createErrorReply(QDBusError::NotSupported, "fixture"));
    return true;
  }
  void setLocked(bool value) {
    locked = value;
    auto signal = QDBusMessage::createSignal(
        "/ScreenSaver", "org.freedesktop.ScreenSaver", "ActiveChanged");
    signal << value;
    QDBusConnection::sessionBus().send(signal);
  }
};

// Stop only the synthetic process launched on this test's private buses,
// including when a Qt assertion returns early.
class ShellProcess : public QProcess {
public:
  ~ShellProcess() override {
    terminate();
    if (!waitForFinished(3000)) {
      kill();
      waitForFinished(3000);
    }
  }
};

class StartupHomeTest : public QObject {
  Q_OBJECT
private slots:
  void cleanup() {
    auto bus = QDBusConnection::sessionBus();
    bus.unregisterService("org.arinn.Daemon1");
    bus.unregisterObject("/org/arinn", QDBusConnection::UnregisterTree);
    bus.unregisterService("org.freedesktop.ScreenSaver");
    bus.unregisterObject("/ScreenSaver");
  }
  void productionStartup_data() {
    QTest::addColumn<QString>("initialLock");
    QTest::newRow("unlocked-adapter-not-ready") << QString("unlocked");
    QTest::newRow("unlocked-daemon-not-ready") << QString("daemon-absent");
    QTest::newRow("locked") << QString("locked");
    QTest::newRow("unknown-lock-service") << QString("unknown");
  }
  void productionStartup() {
    QFETCH(QString, initialLock);
    auto bus = QDBusConnection::sessionBus();
    StartupDaemon daemon;
    StartupScreenSaver lock;
    if (initialLock != "daemon-absent")
      QVERIFY(bus.registerService("org.arinn.Daemon1"));
    lock.locked = initialLock == "locked";
    QVERIFY(bus.registerVirtualObject("/org/arinn", &daemon,
                                      QDBusConnection::SubPath));
    QVERIFY(bus.registerVirtualObject("/ScreenSaver", &lock));
    if (initialLock != "unknown")
      QVERIFY(bus.registerService("org.freedesktop.ScreenSaver"));

    ShellProcess shell;
    auto environment = QProcessEnvironment::systemEnvironment();
    // Lifecycle queries must never reach the real system bus or display.
    environment.insert("DBUS_SYSTEM_BUS_ADDRESS",
                       environment.value("DBUS_SESSION_BUS_ADDRESS"));
    environment.insert("QT_QPA_PLATFORM", "offscreen");
    environment.insert("QT_QUICK_BACKEND", "software");
    environment.insert("ARINN_SESSION_BACKEND", "kde");
    shell.setProcessEnvironment(environment);
    shell.start(QString::fromUtf8(ARINN_SHELL_BINARY));
    QVERIFY(shell.waitForStarted());
    QTRY_VERIFY_WITH_TIMEOUT(bus.interface()->isServiceRegistered("org.arinn.Shell1"), 5000);
    if (initialLock == "unlocked" || initialLock == "daemon-absent") {
      if (initialLock == "daemon-absent") {
        QTest::qWait(1200);
        QCOMPARE(daemon.attempts, 0);
        QVERIFY(bus.registerService("org.arinn.Daemon1"));
      }
      QTRY_VERIFY_WITH_TIMEOUT(daemon.attempts >= 1, 5000);
      QCOMPARE(daemon.accepted, 0);
    } else {
      QTest::qWait(1200);
      QCOMPARE(daemon.attempts, 0);
      if (initialLock == "unknown")
        QVERIFY(bus.registerService("org.freedesktop.ScreenSaver"));
      else
        lock.setLocked(false);
    }
    daemon.platformReady = true;
    QTRY_COMPARE_WITH_TIMEOUT(daemon.accepted, 1, 5000);
    QDBusInterface properties("org.arinn.Shell1", "/org/arinn/Shell1",
                              "org.freedesktop.DBus.Properties", bus);
    auto visible = [&] {
      QDBusReply<QDBusVariant> reply = properties.call(
          "Get", "org.arinn.Shell1", "homeVisible");
      return reply.isValid() && reply.value().variant().toBool();
    };
    QTRY_VERIFY_WITH_TIMEOUT(visible(), 5000);
    const int attempts = daemon.attempts;

    // A later Back/state refresh, lock/unlock and daemon reconnection must
    // not replay the accepted startup presentation.
    daemon.home = false;
    QTRY_VERIFY_WITH_TIMEOUT(!visible(), 5000);
    lock.setLocked(true);
    QTest::qWait(100);
    lock.setLocked(false);
    QVERIFY(bus.unregisterService("org.arinn.Daemon1"));
    QTest::qWait(100);
    QVERIFY(bus.registerService("org.arinn.Daemon1"));
    QTest::qWait(1500);
    QCOMPARE(daemon.attempts, attempts);
    QCOMPARE(daemon.accepted, 1);
    QCOMPARE(shell.state(), QProcess::Running);
  }
};

QTEST_GUILESS_MAIN(StartupHomeTest)
#include "startup_home_test.moc"
