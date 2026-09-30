// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

#include <QCommandLineParser>
#include <QDBusConnection>
#include <QDBusMessage>
#include <QDBusPendingCallWatcher>
#include <QDBusVirtualObject>
#include <QElapsedTimer>
#include <QGuiApplication>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QKeyEvent>
#include <QQuickItem>
#include <QQuickView>
#include <QSGRendererInterface>
#include <QScreen>
#include <QSysInfo>
#include <QTextStream>
#include <QTimer>
#include <atomic>
#include <memory>

// Delayed replies use the event loop, so they cannot block the UI thread.
// Only the private dbus-run-session bus is used by the runner.
class SlowProvider : public QDBusVirtualObject {
public:
  QString introspect(const QString &) const override {
    return QStringLiteral("<interface name=\"org.arinn.PerfFixture1\">"
                          "<method name=\"Read\"/></interface>");
  }
  bool handleMessage(const QDBusMessage &message,
                     const QDBusConnection &connection) override {
    if (message.member() != "Read")
      return false;
    message.setDelayedReply(true);
    QTimer::singleShot(3000, this, [message, connection] {
      connection.send(message.createReply());
    });
    return true;
  }
};

QQuickItem *findItem(QQuickItem *root, const QString &name) {
  if (root->objectName() == name)
    return root;
  for (auto *child : root->childItems())
    if (auto *found = findItem(child, name))
      return found;
  return nullptr;
}

int main(int argc, char **argv) {
  QGuiApplication app(argc, argv);
  QCommandLineParser parser;
  parser.addHelpOption();
  parser.addOption(
      {"samples", "Focus samples per scenario (20..10000).", "count", "200"});
  parser.process(app);
  bool valid = false;
  const int samples = parser.value("samples").toInt(&valid);
  if (!valid || samples < 20 || samples > 10000)
    return 2;

  SlowProvider provider;
  auto bus = QDBusConnection::sessionBus();
  auto client = QDBusConnection::connectToBus(QDBusConnection::SessionBus,
                                              "arinn-perf-client");
  if (!bus.isConnected() || !client.isConnected() ||
      !bus.registerService("org.arinn.PerfFixture1") ||
      !bus.registerVirtualObject("/org/arinn/PerfFixture1", &provider))
    return 2;

  auto window = std::make_unique<QQuickView>();
  auto &view = *window;
  view.setSource(QUrl::fromLocalFile(QStringLiteral(ARINN_PERF_SCENE)));
  if (view.status() != QQuickView::Ready)
    return 2;
  view.setTitle("Arinn performance fixture");
  view.resize(1920, 1080);
  view.show(); // Windowed; no KWin shortcuts, screen lock or platform actions.

  QElapsedTimer clock;
  clock.start();
  std::atomic<qint64> previousFrame{0};
  std::atomic<bool> measuringFrames{false};
  std::atomic<int> frameScenario{0};
  std::atomic<qint64> frameStart{0};
  QJsonArray frames[2], focus[2];
  QJsonArray navigationOpen, homeReturn;
  QString failure;
  int scenario = 0, step = 0;
  qint64 inputStart = 0;
  QString expectedFocus;
  bool inputPending = false, slowPending = false;
  int pendingSamples = 0, completedReads = 0;

  // frameSwapped is presentation cadence, not GPU execution time. With the
  // threaded render loop it arrives on the render thread; copy timestamps
  // into a queued callback before touching the report arrays.
  QObject::connect(
      &view, &QQuickWindow::frameSwapped, &view,
      [&] {
        const auto now = clock.nsecsElapsed();
        const auto previous = previousFrame.exchange(now);
        const int index = frameScenario.load();
        if (measuringFrames.load() && previous &&
            previous >= frameStart.load()) {
          const double ms = double(now - previous) / 1e6;
          QMetaObject::invokeMethod(
              &view, [&, index, ms] { frames[index].append(ms); },
              Qt::QueuedConnection);
        }
        QMetaObject::invokeMethod(
            &view, [&] { view.update(); }, Qt::QueuedConnection);
      },
      Qt::DirectConnection);

  QObject::connect(&view, &QQuickWindow::activeFocusItemChanged, &view, [&] {
    if (!inputPending)
      return;
    auto *item = view.activeFocusItem();
    if (!item || item->objectName() != expectedFocus)
      return;
    focus[scenario].append(double(clock.nsecsElapsed() - inputStart) / 1e6);
    if (scenario == 1 && slowPending)
      ++pendingSamples;
    inputPending = false;
  });

  auto key = [&](int code) {
    QKeyEvent press(QEvent::KeyPress, code, Qt::NoModifier);
    QCoreApplication::sendEvent(&view, &press);
    QKeyEvent release(QEvent::KeyRelease, code, Qt::NoModifier);
    QCoreApplication::sendEvent(&view, &release);
  };
  auto startRead = [&] {
    slowPending = true;
    auto request = QDBusMessage::createMethodCall(
        "org.arinn.PerfFixture1", "/org/arinn/PerfFixture1",
        "org.arinn.PerfFixture1", "Read");
    auto *watcher =
        new QDBusPendingCallWatcher(client.asyncCall(request, 5000), &view);
    QObject::connect(watcher, &QDBusPendingCallWatcher::finished, &view,
                     [&, watcher] {
                       if (watcher->isError()) {
                         failure = "delayed_fixture_read_failed";
                         app.exit(2);
                       }
                       slowPending = false;
                       ++completedReads;
                       watcher->deleteLater();
                     });
  };

  auto finish = [&] {
    measuringFrames = false;
    QJsonArray scenarios;
    for (int i = 0; i < 2; ++i)
      scenarios.append(
          QJsonObject{{"name", i == 0 ? "warm_home" : "delayed_read"},
                      {"input_to_focus_ms", focus[i]},
                      {"frame_interval_ms", frames[i]}});
    QJsonObject result{
        {"schema_version", 1},
        {"scope", "production_components_fixture"},
        {"qt_version", QString::fromLatin1(qVersion())},
        {"build_type", QStringLiteral(ARINN_PERF_BUILD_TYPE)},
        {"cpu_architecture", QSysInfo::currentCpuArchitecture()},
        {"platform", QGuiApplication::platformName()},
        {"graphics_api", int(view.rendererInterface()->graphicsApi())},
        {"render_loop", qEnvironmentVariable("QSG_RENDER_LOOP", "default")},
        {"screen_refresh_hz", view.screen()->refreshRate()},
        {"width", view.width()},
        {"height", view.height()},
        {"warmup_ms", 2000},
        {"key_interval_ms", 32},
        {"delayed_read_ms", 3000},
        {"focus_samples_while_read_pending", pendingSamples},
        {"completed_delayed_reads", completedReads},
        {"scenarios", scenarios},
        {"navigation_open_ms", navigationOpen},
        {"home_return_ms", homeReturn},
        {"target_qualification", "not_run"}};
    QTextStream(stdout) << QJsonDocument(result).toJson(QJsonDocument::Compact)
                        << '\n';
    app.quit();
  };

  QTimer driver;
  driver.setInterval(32);
  QObject::connect(&driver, &QTimer::timeout, &view, [&] {
    if (inputPending) {
      // Keep delayed focus samples, including budget misses. A missing target
      // is an invalid measurement only after the bounded three-second wait.
      if (clock.nsecsElapsed() - inputStart > 3000000000LL) {
        failure = "focus_did_not_move";
        app.exit(2);
      }
      return;
    }
    if (step == samples) {
      // Exclude idle time while awaiting the final read.
      measuringFrames = false;
      if (inputPending || focus[scenario].size() != samples ||
          frames[scenario].isEmpty()) {
        failure = "missing_focus_or_frame_samples";
        app.exit(2);
        return;
      }
      if (scenario == 0) {
        frameScenario = ++scenario;
        step = 0;
        previousFrame = 0;
        startRead();
        frameStart = clock.nsecsElapsed();
        measuringFrames = true;
      } else {
        // A fast smoke run still waits for one complete delayed reply.
        if (slowPending)
          return;
        driver.stop();
        finish();
      }
      return;
    }
    if (scenario == 1 && !slowPending)
      startRead();
    inputPending = true;
    expectedFocus = step % 2 ? "entry-first" : "entry-second";
    inputStart = clock.nsecsElapsed();
    key(step % 2 ? Qt::Key_Left : Qt::Key_Right);
    ++step;
  });

  QTimer::singleShot(2000, &view, [&] {
    auto *first = findItem(view.rootObject(), "entry-first");
    if (!first) {
      failure = "home_fixture_not_ready";
      app.exit(2);
      return;
    }
    first->forceActiveFocus();
    // Local overlay entry and Home return: observations without an
    // invented budget. Verify the resulting state before recording time.
    for (int i = 0; i < 20; ++i) {
      auto start = clock.nsecsElapsed();
      key(Qt::Key_Menu);
      QCoreApplication::processEvents(); // HomeView schedules navigation focus.
      if (!view.rootObject()->property("navOpen").toBool() ||
          !view.activeFocusItem() ||
          view.activeFocusItem()->objectName() != "nav-0") {
        failure = "navigation_did_not_open";
        app.exit(2);
        return;
      }
      navigationOpen.append(double(clock.nsecsElapsed() - start) / 1e6);
      start = clock.nsecsElapsed();
      key(Qt::Key_Home);
      QCoreApplication::processEvents();
      if (view.rootObject()->property("navOpen").toBool() ||
          view.activeFocusItem() != first) {
        failure = "home_focus_not_restored";
        app.exit(2);
        return;
      }
      homeReturn.append(double(clock.nsecsElapsed() - start) / 1e6);
    }
    previousFrame = 0;
    frameStart = clock.nsecsElapsed();
    measuringFrames = true;
    driver.start();
  });
  const int result = app.exec();
  window.reset(); // Stop rendering while callback state is still alive.
  if (!failure.isEmpty())
    QTextStream(stderr) << failure << '\n';
  return result;
}
