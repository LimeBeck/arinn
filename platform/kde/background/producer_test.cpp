// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// The KWin producer against a fake daemon on a private bus (ADR-0087). A
// plain Qt Quick item stands in for KWin's WindowThumbnail; synthetic pixels.
#include "producer.h"
#include <QQmlComponent>
#include <QQmlEngine>
#include <QQuickWindow>
#include <QtDBus>
#include <QScopeGuard>
#include <QSemaphore>
#include <QtTest>

namespace {
const QString epoch = QStringLiteral("11111111-1111-4111-8111-111111111111");
const QString first = QStringLiteral("22222222-2222-4222-8222-222222222222");
const QString second = QStringLiteral("44444444-4444-4444-8444-444444444444");
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
  QString state = QStringLiteral("pending"), request = first;
  bool reject = false;
  qulonglong revision = 2;
  QStringList cancels;
  struct Published {
    QString sender, request, target, token;
    quint32 width = 0, height = 0;
    std::optional<std::vector<std::uint8_t>> rgba;
  };
  QList<Published> published;
  ContextWire context() const {
    return {epoch, request, QStringLiteral("activity"), QStringLiteral("screen"),
            {generation, QStringLiteral("window-token")}};
  }
  void changed() {
    auto message = QDBusMessage::createSignal(QStringLiteral("/org/arinn/Daemon1"),
                                              QStringLiteral("org.arinn.Background1"),
                                              QStringLiteral("Changed"));
    message << epoch << ++revision;
    bus.send(message);
  }
  QString introspect(const QString &) const override { return {}; }
  bool handleMessage(const QDBusMessage &message, const QDBusConnection &connection) override {
    const auto member = message.member();
    if (member == QLatin1String("Get")) {
      QDBusArgument body;
      body.beginStructure();
      body << epoch << revision;
      body.beginArray(QMetaType::fromType<ContextWire>());
      if (state != QLatin1String("idle")) body << context();
      body.endArray();
      body << state;
      body.endStructure();
      connection.send(message.createReply(QVariant::fromValue(body)));
      return true;
    }
    if (member == QLatin1String("Cancel")) {
      cancels.append(message.arguments().first().toString());
      connection.send(message.createReply());
      return true;
    }
    if (member == QLatin1String("Publish")) {
      Published p;
      p.sender = message.service();
      const auto a = qvariant_cast<QDBusArgument>(message.arguments().first());
      QString e, output, gen;
      QDBusUnixFileDescriptor fd;
      a.beginStructure();
      a.beginStructure();
      a >> e >> p.request >> p.target >> output;
      a.beginStructure();
      a >> gen >> p.token;
      a.endStructure();
      a.endStructure();
      a >> p.width >> p.height >> fd;
      a.endStructure();
      // The consumer-side checks: seals, exact size, positional read.
      auto pixels = arinn::background::Pixels::duplicate(fd.fileDescriptor(), p.width, p.height);
      if (pixels) p.rgba = pixels.read(p.width, p.height);
      published.append(p);
      if (reject) {
        connection.send(message.createErrorReply(QStringLiteral("org.arinn.Error1.Conflict"),
                                                 QStringLiteral("Conflict")));
      } else {
        state = QStringLiteral("ready");
        connection.send(message.createReply());
      }
      return true;
    }
    return false;
  }
};

class ProducerTest : public QObject {
  Q_OBJECT
  QDBusConnection daemonBus{QString()};
  FakeDaemon *daemon = nullptr;
  QQmlEngine *engine = nullptr;
  QQuickWindow *window = nullptr;
  QQuickItem *item() { return window->findChild<QQuickItem *>(QStringLiteral("thumbnail")); }
private slots:
  void initTestCase() {
    qDBusRegisterMetaType<WindowWire>();
    qDBusRegisterMetaType<ContextWire>();
    engine = new QQmlEngine(this);
    QQmlComponent component(engine);
    component.setData("import QtQuick\nimport QtQuick.Window\n"
                      "Window { width: 64; height: 36; visible: true\n"
                      "  Rectangle { objectName: 'thumbnail'; anchors.fill: parent;"
                      " color: '#ff0000' } }",
                      QUrl());
    window = qobject_cast<QQuickWindow *>(component.create());
    QVERIFY2(window, qPrintable(component.errorString()));
    QVERIFY(QTest::qWaitForWindowExposed(window));
  }
  void cleanupTestCase() { delete window; }
  void init() {
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

  void publishesOneSealedFrameFromOwnConnection() {
    BackgroundProducer producer;
    QSignalSpy finished(&producer, &BackgroundProducer::finished);
    QTRY_COMPARE_WITH_TIMEOUT(producer.request(), first, 5000);
    QCOMPARE(producer.target(), QStringLiteral("activity"));
    QCOMPARE(producer.output(), QStringLiteral("screen"));
    producer.capture(item());
    // The grab result and its image are released once the image is taken.
    QPointer<QQuickItemGrabResult> grab = producer.pendingGrab();
    QVERIFY(grab);
    QTRY_COMPARE_WITH_TIMEOUT(daemon->published.size(), 1, 5000);
    const auto &p = daemon->published.first();
    // Sent from the producer's own (KWin's) connection, never a helper.
    QCOMPARE(p.sender, QDBusConnection::sessionBus().baseService());
    QCOMPARE(p.request, first);
    QCOMPARE(p.target, QStringLiteral("activity"));
    QCOMPARE(p.token, QStringLiteral("window-token"));
    QCOMPARE(p.width * p.height > 0, true);
    QVERIFY(p.width <= 1920 && p.height <= 1080);
    QVERIFY2(p.rgba.has_value(), "sealed descriptor of exact size");
    QCOMPARE((*p.rgba)[0], std::uint8_t(255));
    QCOMPARE((*p.rgba)[1], std::uint8_t(0));
    QCOMPARE((*p.rgba)[3], std::uint8_t(255));
    QTRY_COMPARE(finished.size(), 1);
    QCOMPARE(finished.first().at(1).toBool(), true);
    QVERIFY(producer.request().isEmpty());
    QTRY_VERIFY(grab.isNull());
    // The same request is never captured twice.
    daemon->state = QStringLiteral("pending");
    daemon->changed();
    QTest::qWait(200);
    QVERIFY(producer.request().isEmpty());
    producer.capture(item());
    QTest::qWait(200);
    QCOMPARE(daemon->published.size(), 1);
  }

  void deadlineAndMissingWindowCancel() {
    BackgroundProducer producer;
    producer.setDeadline(100);
    QSignalSpy finished(&producer, &BackgroundProducer::finished);
    QTRY_COMPARE_WITH_TIMEOUT(producer.request(), first, 5000);
    QTRY_COMPARE_WITH_TIMEOUT(daemon->cancels, QStringList{first}, 2000);
    QVERIFY(producer.request().isEmpty());
    QCOMPARE(finished.first().at(1).toBool(), false);
    daemon->request = second;
    daemon->changed();
    QTRY_COMPARE_WITH_TIMEOUT(producer.request(), second, 5000);
    producer.setDeadline(5000);
    producer.fail();
    QTRY_COMPARE(daemon->cancels.size(), 2);
    QCOMPARE(daemon->cancels.last(), second);
    QCOMPARE(daemon->published.size(), 0);
  }

  void supersededAndInvalidatedRequestsAreDropped() {
    BackgroundProducer producer;
    QTRY_COMPARE_WITH_TIMEOUT(producer.request(), first, 5000);
    // A grab still outstanding when its request is superseded is released.
    // Hide the window right after the grab starts, before the event loop
    // runs: an unexposed window renders nothing, so the grab stays
    // outstanding until the supersede is seen.
    producer.capture(item());
    QPointer<QQuickItemGrabResult> grab = producer.pendingGrab();
    window->hide();
    QVERIFY(grab);
    daemon->request = second;
    daemon->changed();
    QTRY_COMPARE_WITH_TIMEOUT(producer.request(), second, 5000);
    QTRY_VERIFY(grab.isNull());
    QVERIFY(!producer.pendingGrab());
    window->show();
    QVERIFY(QTest::qWaitForWindowExposed(window));
    daemon->state = QStringLiteral("unavailable");
    daemon->changed();
    QTRY_VERIFY_WITH_TIMEOUT(producer.request().isEmpty(), 5000);
    producer.capture(item());
    QTest::qWait(200);
    QCOMPARE(daemon->published.size(), 0);
    daemon->state = QStringLiteral("idle");
    daemon->changed();
    QTest::qWait(200);
    QVERIFY(producer.request().isEmpty());
  }

  void aReplacementWhileEncodingIsCancelledAsBusy() {
    // The encoding worker is held: the first image stays on it.
    QSemaphore entered, release;
    BackgroundProducer producer;
    producer.setDeadline(10000);
    producer.setEncodeHook([&] {
      entered.release();
      release.acquire();
    });
    // Never leave the worker blocked, also when an assertion fails: the
    // producer waits for its worker when destroyed.
    auto unblock = qScopeGuard([&] { release.release(100); });
    QTRY_COMPARE_WITH_TIMEOUT(producer.request(), first, 5000);
    producer.capture(item());
    QTRY_VERIFY_WITH_TIMEOUT(entered.available() == 1, 5000);
    // A replacement request arrives while that image is still encoding.
    daemon->request = second;
    daemon->changed();
    QTRY_COMPARE_WITH_TIMEOUT(producer.request(), second, 5000);
    producer.capture(item());
    // Refused at once instead of queueing a second image behind the first.
    QTRY_COMPARE_WITH_TIMEOUT(daemon->cancels, QStringList{second}, 1000);
    QVERIFY(producer.request().isEmpty());
    QVERIFY(!producer.pendingGrab());
    unblock.dismiss();
    release.release();
    QTest::qWait(300);
    // The superseded first image is dropped, never published.
    QCOMPARE(daemon->published.size(), 0);
  }

  void stageSizeBoundsTheBackingSurface() {
    BackgroundProducer producer;
    const auto physical = [&](qreal w, qreal h, qreal dpr) {
      const auto size = producer.stageSize(w, h, dpr);
      return QSizeF(size.width() * dpr, size.height() * dpr);
    };
    QCOMPARE(producer.stageSize(3840, 2160, 1), QSize(1920, 1080));
    QCOMPARE(producer.stageSize(800, 450, 1), QSize(800, 450));
    QCOMPARE(producer.stageSize(1000, 3000, 1), QSize(360, 1080));
    // A 1920×1080 logical window on a DPR 1.75 output: the backing surface,
    // not just the logical size, stays within 1920×1080.
    const auto hidpi = physical(1920, 1080, 1.75);
    QVERIFY2(hidpi.width() <= 1920 && hidpi.height() <= 1080, "DPR 1.75");
    QCOMPARE(producer.stageSize(1920, 1080, 1.75), QSize(1097, 617));
    const auto two = physical(800, 600, 2);
    QVERIFY(two.width() <= 1920 && two.height() <= 1080);
    QCOMPARE(producer.stageSize(800, 600, 2), QSize(720, 540));
    QCOMPARE(producer.stageSize(0, 100, 1), QSize(1, 1));
  }

  void rejectedPublicationEnds() {
    daemon->reject = true;
    BackgroundProducer producer;
    QSignalSpy finished(&producer, &BackgroundProducer::finished);
    QTRY_COMPARE_WITH_TIMEOUT(producer.request(), first, 5000);
    producer.capture(item());
    QTRY_COMPARE_WITH_TIMEOUT(finished.size(), 1, 5000);
    QCOMPARE(finished.first().at(1).toBool(), false);
    QVERIFY(producer.request().isEmpty());
  }

  void daemonOwnerChangeAbandonsTheRequest() {
    BackgroundProducer producer;
    QTRY_COMPARE_WITH_TIMEOUT(producer.request(), first, 5000);
    daemonBus.unregisterService(QStringLiteral("org.arinn.Daemon1"));
    QTRY_VERIFY_WITH_TIMEOUT(producer.request().isEmpty(), 5000);
  }
};

QTEST_MAIN(ProducerTest)
#include "producer_test.moc"
