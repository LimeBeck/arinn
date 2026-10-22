// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// The shell's Activity background consumer against a fake daemon on a private
// session bus (ADR-0087). Synthetic pixels only.
#include "activity_background.h"
#include "pixels.h"
#include <QGuiApplication>
#include <QQmlEngine>
#include <QtDBus>
#include <QtTest>
#include <sys/mman.h>
#include <unistd.h>

namespace {
const QString epoch = QStringLiteral("11111111-1111-4111-8111-111111111111");
const QString request = QStringLiteral("22222222-2222-4222-8222-222222222222");
const QString generation = QStringLiteral("33333333-3333-4333-8333-333333333333");
} // namespace

struct WindowWire {
  QString generation, token;
};
struct ContextWire {
  QString epoch, request, target, output;
  WindowWire window;
};
Q_DECLARE_METATYPE(WindowWire)
Q_DECLARE_METATYPE(ContextWire)
QDBusArgument &operator<<(QDBusArgument &a, const WindowWire &w) {
  a.beginStructure();
  a << w.generation << w.token;
  a.endStructure();
  return a;
}
const QDBusArgument &operator>>(const QDBusArgument &a, WindowWire &) { return a; }
QDBusArgument &operator<<(QDBusArgument &a, const ContextWire &c) {
  a.beginStructure();
  a << c.epoch << c.request << c.target << c.output << c.window;
  a.endStructure();
  return a;
}
const QDBusArgument &operator>>(const QDBusArgument &a, ContextWire &) { return a; }

class FakeDaemon : public QDBusVirtualObject {
public:
  explicit FakeDaemon(QDBusConnection bus) : bus(bus) {}
  QDBusConnection bus;
  bool home = true, platform = true, background = true, sealPixels = true;
  QString target = QStringLiteral("activity"), output = QStringLiteral("screen");
  QString token = QStringLiteral("window-token");
  QString state = QStringLiteral("ready");
  qulonglong associationRevision = 1, revision = 2;
  ContextWire echoed;
  int reads = 0, gets = 0;
  ContextWire context() const {
    return {epoch, request, target, output, {generation, QStringLiteral("window-token")}};
  }
  QString introspect(const QString &) const override {
    return QStringLiteral("<interface name=\"org.arinn.ReturnAssociation1\"/>"
                          "<interface name=\"org.arinn.Background1\"/>");
  }
  void notify(const QString &interface, const QVariantList &args = {}) {
    auto message = QDBusMessage::createSignal(QStringLiteral("/org/arinn/Daemon1"),
                                              interface, QStringLiteral("Changed"));
    message.setArguments(args);
    bus.send(message);
  }
  void changed() { notify(QStringLiteral("org.arinn.Background1"), {epoch, ++revision}); }
  void homeChanged() {
    notify(QStringLiteral("org.arinn.Daemon1"));
    notify(QStringLiteral("org.arinn.ReturnAssociation1"), {epoch, ++associationRevision});
  }
  bool handleMessage(const QDBusMessage &message, const QDBusConnection &connection) override {
    QDBusArgument body;
    const auto member = message.member();
    const auto iface = message.interface();
    if (iface == QLatin1String("org.freedesktop.DBus.Introspectable")) {
      connection.send(message.createReply(QStringLiteral("<node>") + introspect({}) +
                                          QStringLiteral("</node>")));
      return true;
    }
    if (member == QLatin1String("Status")) {
      body.beginStructure();
      body << epoch << true << epoch << qulonglong(1) << home << target << QString()
           << QString() << false << platform << output << QString();
      body.endStructure();
    } else if (iface == QLatin1String("org.arinn.ReturnAssociation1")) {
      body.beginStructure();
      body << epoch << associationRevision << target;
      body.beginArray(QMetaType::fromType<WindowWire>());
      if (!token.isEmpty()) body << WindowWire{generation, token};
      body.endArray();
      body.endStructure();
    } else if (iface == QLatin1String("org.arinn.Background1") && !background) {
      connection.send(message.createErrorReply(
          QStringLiteral("org.freedesktop.DBus.Error.UnknownMethod"), QStringLiteral("no")));
      return true;
    } else if (member == QLatin1String("Get")) {
      ++gets;
      body.beginStructure();
      body << epoch << revision;
      body.beginArray(QMetaType::fromType<ContextWire>());
      if (state != QLatin1String("idle")) body << context();
      body.endArray();
      body << state;
      body.endStructure();
    } else if (member == QLatin1String("ReadFrame")) {
      ++reads;
      if (state != QLatin1String("ready")) {
        connection.send(message.createErrorReply(QStringLiteral("org.arinn.Error1.Unavailable"),
                                                 QStringLiteral("Unavailable")));
        return true;
      }
      std::vector<std::uint8_t> rgba(2 * 2 * 4);
      for (std::size_t i = 0; i < rgba.size(); ++i) rgba[i] = std::uint8_t(i * 10);
      auto pixels = arinn::background::Pixels::create(2, 2, rgba);
      int fd = pixels.descriptor();
      int plain = -1;
      if (!sealPixels) {
        // Same bytes in an ordinary unsealed memfd.
        plain = memfd_create("plain", MFD_CLOEXEC);
        (void)!write(plain, rgba.data(), rgba.size());
        fd = plain;
      }
      body.beginStructure();
      body << (echoed.epoch.isEmpty() ? context() : echoed) << quint32(2) << quint32(2)
           << QDBusUnixFileDescriptor(fd);
      body.endStructure();
      if (plain >= 0) close(plain);
    } else {
      return false;
    }
    connection.send(message.createReply(QVariant::fromValue(body)));
    return true;
  }
};

class ActivityBackgroundTest : public QObject {
  Q_OBJECT
  QDBusConnection daemonBus{QString()};
  FakeDaemon *daemon = nullptr;
private slots:
  void init() {
    qDBusRegisterMetaType<WindowWire>();
    qDBusRegisterMetaType<ContextWire>();
    daemonBus = QDBusConnection::connectToBus(QDBusConnection::SessionBus,
                                              QStringLiteral("fake-daemon"));
    QVERIFY(daemonBus.isConnected());
    daemon = new FakeDaemon(daemonBus);
    QVERIFY(daemonBus.registerVirtualObject(QStringLiteral("/org/arinn/Daemon1"), daemon));
    QVERIFY(daemonBus.registerService(QStringLiteral("org.arinn.Daemon1")));
  }
  void cleanup() {
    daemonBus.unregisterService(QStringLiteral("org.arinn.Daemon1"));
    daemonBus.unregisterObject(QStringLiteral("/org/arinn/Daemon1"));
    delete daemon;
    daemon = nullptr;
    QDBusConnection::disconnectFromBus(QStringLiteral("fake-daemon"));
  }

  void readyFrameReachesTheImageProvider() {
    QQmlEngine engine;
    ActivityBackgroundSource source;
    source.install(&engine);
    source.setBlocked(false);
    QTRY_VERIFY_WITH_TIMEOUT(source.available(), 5000);
    QCOMPARE(source.frameSize(), QSize(2, 2));
    QVERIFY(source.source().toString().startsWith(QLatin1String("image://arinn-activity/")));
    auto provider = static_cast<QQuickImageProvider *>(
        engine.imageProvider(QStringLiteral("arinn-activity")));
    QVERIFY(provider);
    QSize size;
    const auto image = provider->requestImage(source.source().path().mid(1), &size, {});
    QCOMPARE(size, QSize(2, 2));
    QCOMPARE(image.format(), QImage::Format_RGBA8888);
    QCOMPARE(image.pixel(1, 0), qRgba(40, 50, 60, 70));
    QCOMPARE(daemon->reads, 1);
    // ReturnAssociation polls every second; unchanged evidence asks the
    // daemon nothing.
    const auto gets = daemon->gets;
    QTest::qWait(2500);
    QCOMPARE(daemon->gets, gets);
    // A repeated Home clears the association until the adapter reports it
    // again; the frame stays for the same Home context.
    daemon->token.clear();
    daemon->homeChanged();
    QTest::qWait(300);
    QVERIFY(source.available());
    daemon->token = QStringLiteral("window-token");
    daemon->homeChanged();
    QTest::qWait(300);
    QVERIFY(source.available());
    QCOMPARE(daemon->reads, 1);
    // A stale image URL resolves to nothing once the frame is released.
    const auto old = source.source().path().mid(1);
    daemon->state = QStringLiteral("unavailable");
    daemon->changed();
    QTRY_VERIFY(!source.available());
    QVERIFY(provider->requestImage(old, &size, {}).isNull());
  }

  void sessionGuardClearsWithoutWaitingForTheDaemon() {
    ActivityBackgroundSource source;
    source.setBlocked(false);
    QTRY_VERIFY_WITH_TIMEOUT(source.available(), 5000);
    source.setBlocked(true);
    // Synchronous: no event loop or daemon reply between lock and clear.
    QVERIFY(!source.available());
    QVERIFY(source.frameSize().isEmpty());
    // Unlock never resurrects the retired request.
    source.setBlocked(false);
    QTest::qWait(500);
    QVERIFY(!source.available());
  }

  void returnAndRemapClearTheFrame() {
    ActivityBackgroundSource source;
    source.setBlocked(false);
    QTRY_VERIFY_WITH_TIMEOUT(source.available(), 5000);
    daemon->token = QStringLiteral("other-window");
    daemon->homeChanged();
    QTRY_VERIFY(!source.available());
    daemon->token = QStringLiteral("window-token");
    daemon->home = false;
    daemon->homeChanged();
    QTest::qWait(300);
    QVERIFY(!source.available());
  }

  void invalidReadsNeverInstallPixels() {
    {
      daemon->echoed = daemon->context();
      daemon->echoed.target = QStringLiteral("other");
      ActivityBackgroundSource source;
      source.setBlocked(false);
      QTRY_VERIFY_WITH_TIMEOUT(daemon->reads > 0, 5000);
      QTest::qWait(300);
      QVERIFY(!source.available());
    }
    daemon->echoed = {};
    daemon->sealPixels = false;
    daemon->reads = 0;
    ActivityBackgroundSource source;
    source.setBlocked(false);
    QTRY_VERIFY_WITH_TIMEOUT(daemon->reads > 0, 5000);
    QTest::qWait(300);
    QVERIFY(!source.available());
  }

  void unsupportedOrIdleDaemonsKeepTheNormalBackground() {
    daemon->background = false;
    {
      ActivityBackgroundSource source;
      source.setBlocked(false);
      QTest::qWait(500);
      QVERIFY(!source.available());
    }
    daemon->background = true;
    daemon->state = QStringLiteral("pending");
    ActivityBackgroundSource source;
    source.setBlocked(false);
    QTRY_VERIFY_WITH_TIMEOUT(daemon->gets > 0, 5000);
    QTest::qWait(300);
    QVERIFY(!source.available());
    QCOMPARE(daemon->reads, 0);
    daemon->state = QStringLiteral("ready");
    daemon->changed();
    QTRY_VERIFY_WITH_TIMEOUT(source.available(), 5000);
  }
};

QTEST_MAIN(ActivityBackgroundTest)
#include "activity_background_test.moc"
