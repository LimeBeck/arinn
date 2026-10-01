// SPDX-FileCopyrightText: 2026 Arinn contributors
//
// SPDX-License-Identifier: GPL-3.0-or-later

#include "asset_images.h"
#include "icon_references.h"
#include <QTemporaryDir>
#include <QDir>
#include <QFile>
#include <sys/stat.h>
#include <QBuffer>
#include <QCoreApplication>
#include <QElapsedTimer>
#include <QGuiApplication>
#include <QImage>
#include <QMutex>
#include <QMutexLocker>
#include <QQmlApplicationEngine>
#include <QQmlContext>
#include <QQuickItem>
#include <QQuickItemGrabResult>
#include <QQuickWindow>
#include <QRunnable>
#include <QTest>
#include <QSignalSpy>
#include <QQuickImageProvider>
#include <QQmlExpression>
#include <atomic>
#include <QThreadPool>
#include <QThread>
#include <QUrl>
#include <QtDBus>
#include <memory>

namespace {
constexpr auto kPath = "/org/arinn/Daemon1/providers/p_media";
constexpr auto kProvider = "media-provider";
constexpr auto kGeneration = "g-media";

QByteArray pngBytes(const QColor &color) {
  QImage image(8, 8, QImage::Format_RGB32);
  image.fill(color);
  QByteArray bytes;
  QBuffer buffer(&bytes);
  buffer.open(QIODevice::WriteOnly);
  image.save(&buffer, "PNG");
  return bytes;
}

class AssetDaemon : public QDBusVirtualObject {
public:
  int calls = 0;
  bool hold = false;
  QByteArray png;
  QString lastId;
  QList<QDBusMessage> held;

  QString introspect(const QString &) const override { return {}; }
  bool handleMessage(const QDBusMessage &message,
                     const QDBusConnection &bus) override {
    if (message.interface() != QLatin1String("org.arinn.Assets1") ||
        message.member() != QLatin1String("GetAsset")) {
      bus.send(message.createErrorReply(QDBusError::NotSupported,
                                        QStringLiteral("fixture")));
      return true;
    }
    ++calls;
    const auto id = message.arguments().value(1).toString();
    lastId = id;
    if (hold) {
      message.setDelayedReply(true);
      held.append(message);
      return true;
    }
    send(message, bus, id);
    return true;
  }
  void send(const QDBusMessage &message, const QDBusConnection &bus,
            const QString &id) {
    if (id == QLatin1String("missing") || id == QLatin1String("absent-asset")) {
      bus.send(message.createErrorReply(QStringLiteral("org.arinn.Error1.NotFound"),
                                        QStringLiteral("NotFound")));
      return;
    }
    if (id == QLatin1String("unsupported")) {
      bus.send(message.createErrorReply(
          QStringLiteral("org.arinn.Error1.Unsupported"),
          QStringLiteral("Unsupported")));
      return;
    }
    if (id == QLatin1String("broken")) {
      bus.send(message.createErrorReply(QStringLiteral("org.arinn.Error1.Internal"),
                                        QStringLiteral("Internal")));
      return;
    }
    QDBusArgument body;
    body.beginStructure();
    if (id == QLatin1String("disguised")) {
      body << QStringLiteral("image/png")
           << QByteArrayLiteral("<svg xmlns=\"http://www.w3.org/2000/svg\"/>");
    } else if (id == QLatin1String("mismatch")) {
      body << QStringLiteral("image/jpeg") << png;
    } else {
      body << QStringLiteral("image/png") << png;
    }
    body.endStructure();
    bus.send(message.createReply(QVariant::fromValue(body)));
  }
  void release() {
    const auto pending = held;
    held.clear();
    auto bus = QDBusConnection::sessionBus();
    for (const auto &message : pending) {
      const auto id = message.arguments().value(1).toString();
      send(message, bus, id);
    }
  }
};

AssetImages::Result fetchOnWorker(const QString &provider, const QString &path,
                                  const QString &generation, const QString &id) {
  struct Box {
    QMutex mutex;
    bool done = false;
    AssetImages::Result result;
  };
  auto box = std::make_shared<Box>();
  auto *job = QRunnable::create([box, provider, path, generation, id] {
    auto result = AssetImages::shared().fetch(provider, path, generation, id);
    QMutexLocker lock(&box->mutex);
    box->result = result;
    box->done = true;
  });
  static QThreadPool pool;
  static const bool keepThreads = [] {
    pool.setMaxThreadCount(2);
    pool.setExpiryTimeout(-1);
    return true;
  }();
  Q_UNUSED(keepThreads);
  pool.start(job);
  QElapsedTimer timer;
  timer.start();
  while (timer.elapsed() < 8000) {
    {
      QMutexLocker lock(&box->mutex);
      if (box->done)
        return box->result;
    }
    QCoreApplication::processEvents(QEventLoop::AllEvents, 20);
  }
  return {};
}

constexpr int kImageReady = 1;
constexpr int kImageError = 3;
QStringList imageLogs;

void collectImageLogs(QtMsgType, const QMessageLogContext &, const QString &message) {
  imageLogs.append(message);
}

bool continueCardShowsRed(QQuickItem *card) {
  const auto grab = card->grabToImage();
  if (grab.isNull())
    return false;
  QElapsedTimer timer;
  timer.start();
  while (grab->image().isNull() && timer.elapsed() < 500)
    QCoreApplication::processEvents(QEventLoop::AllEvents, 20);
  if (grab->image().isNull())
    return false;
  const auto pixel = grab->image().pixelColor(100, 30);
  return pixel.red() > 180 && pixel.red() > pixel.green() + 80 &&
         pixel.red() > pixel.blue() + 80;
}

QQuickItem *findItem(QQuickItem *root, const QString &name) {
  if (!root)
    return nullptr;
  if (root->objectName() == name)
    return root;
  for (auto *child : root->childItems())
    if (auto *found = findItem(child, name))
      return found;
  return nullptr;
}
bool registerDaemon(AssetDaemon *daemon) {
  auto bus = QDBusConnection::sessionBus();
  return bus.registerService(QStringLiteral("org.arinn.Daemon1")) &&
         bus.registerVirtualObject(QStringLiteral("/org/arinn"), daemon,
                                   QDBusConnection::SubPath);
}
} // namespace

namespace {
struct IconPixelGate {
  std::atomic<bool> hold{true}, fail{false};
  std::atomic<int> pixelCalls{0};
};
class GatedIconProvider : public QQuickImageProvider {
public:
  explicit GatedIconProvider(std::shared_ptr<IconPixelGate> gate)
      : QQuickImageProvider(Image, ForceAsynchronousImageLoading), m_gate(gate) {}
  QImage requestImage(const QString &, QSize *size, const QSize &requestedSize) override {
    if (!requestedSize.isValid()) {
      ++m_gate->pixelCalls;
      QElapsedTimer timeout;
      timeout.start();
      while (m_gate->hold && timeout.elapsed() < 3000) QThread::msleep(1);
      if (m_gate->hold || m_gate->fail) return {};
    }
    QImage image(32, 32, QImage::Format_ARGB32);
    image.fill(Qt::red);
    if (size) *size = image.size();
    return image;
  }
private:
  std::shared_ptr<IconPixelGate> m_gate;
};
}

class AssetImagesTest : public QObject {
  Q_OBJECT
  AssetDaemon daemon;
  QByteArray redPng;
  QByteArray bluePng;
private slots:
  void iconFallbackWaitsForCanvasAndReleasesPreviousSource() {
    auto gate = std::make_shared<IconPixelGate>();
    QQmlApplicationEngine engine;
    engine.addImageProvider("controlled-icons", new GatedIconProvider(gate));
    const QString first = "image://controlled-icons/first";
    const QString second = "image://controlled-icons/second";
    engine.rootContext()->setContextProperty("iconUrl", first);
    engine.load(QUrl::fromLocalFile(QStringLiteral(ARINN_PLACEHOLDER_QML))
                    .resolved(QUrl("icon_tile_fixture.qml")));
    QVERIFY(!engine.rootObjects().isEmpty());
    auto window = qobject_cast<QQuickWindow *>(engine.rootObjects().first());
    QVERIFY(window);
    auto tile = findItem(window->contentItem(), "iconTile");
    auto picture = findItem(window->contentItem(), "applicationIcon");
    auto frame = findItem(window->contentItem(), "applicationIconCanvas");
    auto fallback = findItem(window->contentItem(), "applicationFallback");
    QVERIFY(tile && picture && frame && fallback);
    QTRY_COMPARE(picture->property("status").toInt(), kImageReady);
    QTRY_COMPARE(gate->pixelCalls.load(), 1);
    QVERIFY(!tile->property("iconReady").toBool());
    QVERIFY(fallback->isVisible());
    gate->hold = false;
    QTRY_VERIFY(tile->property("iconReady").toBool());
    QVERIFY(!fallback->isVisible());
    const auto cached = [&](const QString &source) {
      QQmlExpression expression(engine.rootContext(), frame,
          "isImageLoaded('" + source + "')");
      return expression.evaluate().toBool();
    };
    QVERIFY(cached(first));
    gate->hold = true;
    gate->fail = true;
    engine.rootContext()->setContextProperty("iconUrl", second);
    QTRY_COMPARE(gate->pixelCalls.load(), 2);
    QCOMPARE(picture->property("status").toInt(), kImageReady);
    QVERIFY(!tile->property("iconReady").toBool());
    QVERIFY(fallback->isVisible());
    QVERIFY(!cached(first));
    gate->hold = false;
    QQmlExpression failed(engine.rootContext(), frame,
        "isImageError('" + second + "')");
    QTRY_VERIFY(failed.evaluate().toBool());
    QVERIFY(!tile->property("iconReady").toBool());
    QVERIFY(fallback->isVisible());
    gate->fail = false;
    engine.rootContext()->setContextProperty("iconUrl", QString("image://controlled-icons/third"));
    QTRY_VERIFY(tile->property("iconReady").toBool());
    QVERIFY(!cached(first));
    engine.rootContext()->setContextProperty("iconUrl", QString());
    QTRY_VERIFY(!tile->property("iconReady").toBool());
    QVERIFY(fallback->isVisible());
    QVERIFY(!cached("image://controlled-icons/third"));
    // Changing source while a Canvas request is pending must not resurrect it.
    gate->hold = true;
    const int before = gate->pixelCalls.load();
    engine.rootContext()->setContextProperty("iconUrl", QString("image://controlled-icons/obsolete"));
    QTRY_COMPARE(gate->pixelCalls.load(), before + 1);
    engine.rootContext()->setContextProperty("iconUrl", QString("image://controlled-icons/latest"));
    QTRY_COMPARE(frame->property("requestedSource").toString(), QString("image://controlled-icons/latest"));
    QVERIFY(!tile->property("iconReady").toBool());
    gate->hold = false;
    QTRY_VERIFY(tile->property("iconReady").toBool());
    QVERIFY(!cached("image://controlled-icons/obsolete"));
    QVERIFY(cached("image://controlled-icons/latest"));
  }
  void localIconsAreBoundedAndPrivate() {
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    const auto icon = dir.path() + "/fixture.png";
    QImage fixture(32, 32, QImage::Format_RGB32);
    fixture.fill(Qt::blue);
    QVERIFY(fixture.save(icon));
    const auto url = IconReferences::imageUrl(icon);
    QVERIFY(url.startsWith("image://arinn-icons/"));
    QVERIFY(!url.contains(dir.path()));
    QCOMPARE(IconReferences::lookup(url.mid(QString("image://arinn-icons/").size())), icon);
    QVERIFY(!loadLocalIcon(icon, {}, {}).isNull());
    for (const auto &bad : {QString("https://example.test/icon.png"), QString("../icon.png"), dir.path() + "/../icon.png"}) {
      QVERIFY(IconReferences::imageUrl(bad).isEmpty());
      QVERIFY(loadLocalIcon(bad, {}, {}).isNull());
    }
    const auto fifo = dir.path() + "/pipe";
    QCOMPARE(::mkfifo(QFile::encodeName(fifo).constData(), 0600), 0);
    QVERIFY(loadLocalIcon(fifo, {}, {}).isNull());
    QFile file(dir.path() + "/bad.png");
    QVERIFY(file.open(QIODevice::WriteOnly));
    file.write(QByteArray(2 * 1024 * 1024 + 1, 'x')); file.close();
    QVERIFY(loadLocalIcon(file.fileName(), {}, {}).isNull());
    QVERIFY(QImage(4097, 1, QImage::Format_RGB32).save(icon));
    QVERIFY(loadLocalIcon(icon, {}, {}).isNull());
    const auto svg = dir.path() + "/external.svg";
    QFile external(svg); QVERIFY(external.open(QIODevice::WriteOnly));
    external.write("<svg xmlns='http://www.w3.org/2000/svg' width='32' height='32'><image href='file:///tmp/fixture.png'/></svg>"); external.close();
    QVERIFY(loadLocalIcon(svg, {}, {}).isNull());
    QVERIFY(external.open(QIODevice::WriteOnly | QIODevice::Truncate));
    external.write("<svg xmlns='http://www.w3.org/2000/svg' width='32' height='32'><rect width='32' height='32' fill='#ff0000'/></svg>"); external.close();
    QCOMPARE(loadLocalIcon(svg, {}, {}).pixelColor(0, 0), QColor(Qt::red));
    QQmlApplicationEngine engine;
    installAssetImageProvider(&engine);
    engine.rootContext()->setContextProperty("iconUrl", IconReferences::imageUrl(svg));
    engine.load(QUrl::fromLocalFile(QStringLiteral(ARINN_PLACEHOLDER_QML))
                    .resolved(QUrl("icon_tile_fixture.qml")));
    QVERIFY(!engine.rootObjects().isEmpty());
    auto window = qobject_cast<QQuickWindow *>(engine.rootObjects().first());
    QVERIFY(window);
    auto picture = findItem(window->contentItem(), "applicationIcon");
    QVERIFY(picture);
    QTRY_COMPARE(picture->property("status").toInt(), kImageReady);
    auto tile = findItem(window->contentItem(), "iconTile");
    QVERIFY(tile);
    QTRY_VERIFY(([&] {
      const auto grab = tile->grabToImage(QSize(528, 264));
      if (grab.isNull()) return false;
      QSignalSpy ready(grab.data(), &QQuickItemGrabResult::ready);
      if (grab->image().isNull() && !ready.wait(500)) return false;
      const auto image = grab->image();
      if (image.isNull()) return false;
      const auto edge = image.pixelColor(56, 132);
      const auto corner = image.pixelColor(57, 81);
      const auto outside = image.pixelColor(176, 132);
      return edge.red() > 200 && edge.green() < 40 &&
          !(corner.red() > 200 && corner.green() < 40) &&
          !(outside.red() > 200 && outside.green() < 40);
    })());
    QVERIFY(loadLocalIcon("unknown-theme-icon", {dir.path()}, "fixture").isNull());
  }
  void localThemeInheritance() {
    QTemporaryDir dir; QVERIFY(dir.isValid());
    QVERIFY(QDir().mkpath(dir.path() + "/child"));
    QVERIFY(QDir().mkpath(dir.path() + "/parent/48x48/apps"));
    QVERIFY(QDir().mkpath(dir.path() + "/parent/128x128/apps"));
    QVERIFY(QDir().mkpath(dir.path() + "/hicolor"));
    QFile child(dir.path() + "/child/index.theme"); QVERIFY(child.open(QIODevice::WriteOnly));
    child.write("[Icon Theme]\nInherits=parent\n"); child.close();
    QFile parent(dir.path() + "/parent/index.theme"); QVERIFY(parent.open(QIODevice::WriteOnly));
    parent.write("[Icon Theme]\nDirectories=48x48/apps,128x128/apps\nInherits=child\n[48x48/apps]\nSize=48\n[128x128/apps]\nSize=128\n"); parent.close();
    QImage image(48, 48, QImage::Format_RGB32); image.fill(Qt::red);
    QVERIFY(image.save(dir.path() + "/parent/48x48/apps/fixture.png"));
    QVERIFY(image.save(dir.path() + "/parent/128x128/apps/fixture.png"));
    image.fill(Qt::blue);
    QVERIFY(image.save(dir.path() + "/parent/48x48/apps/fixture.png"));
    QFile fallback(dir.path() + "/hicolor/index.theme"); QVERIFY(fallback.open(QIODevice::WriteOnly));
    fallback.write("[Icon Theme]\nDirectories=apps\n"); fallback.close();
    QVERIFY(QDir().mkpath(dir.path() + "/hicolor/apps"));
    QVERIFY(image.save(dir.path() + "/hicolor/apps/fixture.png"));
    const auto loaded = loadLocalIcon("fixture", {dir.path()}, "child");
    QVERIFY(!loaded.isNull()); QCOMPARE(loaded.pixelColor(0, 0), QColor(Qt::red));
  }
  void init() {
    redPng = pngBytes(Qt::red);
    bluePng = pngBytes(Qt::blue);
    daemon.calls = 0;
    daemon.hold = false;
    daemon.png = redPng;
    daemon.lastId.clear();
    daemon.held.clear();
    AssetImages::shared().clear();
    AssetImages::shared().setMemoryBudget(AssetImages::defaultBudget);
    auto bus = QDBusConnection::sessionBus();
    bus.unregisterObject(QStringLiteral("/org/arinn"),
                         QDBusConnection::UnregisterTree);
    bus.unregisterService(QStringLiteral("org.arinn.Daemon1"));
  }
  void cleanup() {
    daemon.hold = false;
    daemon.release();
    QElapsedTimer timer;
    timer.start();
    while (timer.elapsed() < 500)
      QCoreApplication::processEvents(QEventLoop::AllEvents, 20);
    auto bus = QDBusConnection::sessionBus();
    bus.unregisterObject(QStringLiteral("/org/arinn"),
                         QDBusConnection::UnregisterTree);
    bus.unregisterService(QStringLiteral("org.arinn.Daemon1"));
    AssetImages::shared().clear();
    AssetImages::shared().setMemoryBudget(AssetImages::defaultBudget);
  }
  void imageUrlRoundTrips() {
    const auto url = AssetImages::imageUrl(
        QStringLiteral("prov"), QString::fromLatin1(kPath), QStringLiteral("gen"),
        QStringLiteral("id/with %2F"));
    QVERIFY(url.startsWith(QStringLiteral("image://arinn-assets/")));
    QString provider, path, generation, id;
    QVERIFY(AssetImages::parseImageId(QUrl(url).path(), &provider, &path,
                                      &generation, &id));
    QCOMPARE(provider, QStringLiteral("prov"));
    QCOMPARE(path, QString::fromLatin1(kPath));
    QCOMPARE(generation, QStringLiteral("gen"));
    QCOMPARE(id, QStringLiteral("id/with %2F"));
    QVERIFY(AssetImages::imageUrl({}, QString::fromLatin1(kPath),
                                   QStringLiteral("gen"), QStringLiteral("id"))
                .isEmpty());
    QVERIFY(AssetImages::imageUrl(QStringLiteral("prov"), QStringLiteral("/tmp/x"),
                                   QStringLiteral("gen"), QStringLiteral("id"))
                .isEmpty());
    QVERIFY(AssetImages::imageUrl(QStringLiteral("prov"), QString::fromLatin1(kPath),
                                   QStringLiteral("gen"), {})
                .isEmpty());
  }
  void cacheHitBudgetEvictionAndKey() {
    QVERIFY(registerDaemon(&daemon));
    QVERIFY(!redPng.isEmpty());
    QVERIFY(!bluePng.isEmpty());
    AssetImages::shared().setMemoryBudget(qMax(redPng.size(), bluePng.size()));
    daemon.png = redPng;
    auto first = fetchOnWorker(QString::fromLatin1(kProvider), QString::fromLatin1(kPath),
                               QString::fromLatin1(kGeneration), QStringLiteral("red"));
    QVERIFY(first.status == AssetImages::Status::Ready);
    QVERIFY(!first.cacheHit);
    QCOMPARE(first.image.pixelColor(0, 0), QColor(Qt::red));
    QCOMPARE(daemon.calls, 1);
    QCOMPARE(AssetImages::shared().cachedEntries(), 1);
    auto again = fetchOnWorker(QString::fromLatin1(kProvider), QString::fromLatin1(kPath),
                               QString::fromLatin1(kGeneration), QStringLiteral("red"));
    QVERIFY(again.status == AssetImages::Status::Ready);
    QVERIFY(again.cacheHit);
    QCOMPARE(daemon.calls, 1);

    daemon.png = bluePng;
    auto otherProvider =
        fetchOnWorker(QStringLiteral("other"), QString::fromLatin1(kPath),
                      QString::fromLatin1(kGeneration), QStringLiteral("red"));
    QVERIFY(otherProvider.status == AssetImages::Status::Ready);
    QVERIFY(!otherProvider.cacheHit);
    QCOMPARE(daemon.calls, 2);
    auto otherGeneration =
        fetchOnWorker(QString::fromLatin1(kProvider), QString::fromLatin1(kPath),
                      QStringLiteral("g-next"), QStringLiteral("red"));
    QVERIFY(!otherGeneration.cacheHit);
    QCOMPARE(daemon.calls, 3);

    AssetImages::shared().clear();
    AssetImages::shared().setMemoryBudget(qMax(redPng.size(), bluePng.size()));
    daemon.calls = 0;
    daemon.png = redPng;
    QVERIFY(fetchOnWorker(QString::fromLatin1(kProvider), QString::fromLatin1(kPath),
                          QString::fromLatin1(kGeneration), QStringLiteral("red"))
                .status == AssetImages::Status::Ready);
    daemon.png = bluePng;
    QVERIFY(fetchOnWorker(QString::fromLatin1(kProvider), QString::fromLatin1(kPath),
                          QString::fromLatin1(kGeneration), QStringLiteral("blue"))
                .status == AssetImages::Status::Ready);
    QCOMPARE(AssetImages::shared().cachedEntries(), 1);
    daemon.png = redPng;
    auto evicted =
        fetchOnWorker(QString::fromLatin1(kProvider), QString::fromLatin1(kPath),
                      QString::fromLatin1(kGeneration), QStringLiteral("red"));
    QVERIFY(evicted.status == AssetImages::Status::Ready);
    QVERIFY(!evicted.cacheHit);
    QCOMPARE(daemon.calls, 3);
  }
  void fallbacksDoNotCache() {
    QVERIFY(registerDaemon(&daemon));
    daemon.png = redPng;
    auto empty = fetchOnWorker(QString::fromLatin1(kProvider), QString::fromLatin1(kPath),
                               QString::fromLatin1(kGeneration), {});
    QVERIFY(empty.status == AssetImages::Status::Empty);
    QVERIFY(empty.image.isNull());
    QCOMPARE(daemon.calls, 0);

    const struct {
      const char *id;
      AssetImages::Status status;
    } cases[] = {{"missing", AssetImages::Status::NotFound},
                 {"unsupported", AssetImages::Status::Unsupported},
                 {"broken", AssetImages::Status::Error},
                 {"disguised", AssetImages::Status::Error},
                 {"mismatch", AssetImages::Status::Error}};
    for (const auto &item : cases) {
      AssetImages::shared().clear();
      const auto before = daemon.calls;
      const auto assetId = QString::fromLatin1(item.id);
      auto result = fetchOnWorker(QString::fromLatin1(kProvider), QString::fromLatin1(kPath),
                                  QString::fromLatin1(kGeneration), assetId);
      QVERIFY(result.status == item.status);
      QVERIFY(result.image.isNull());
      QVERIFY(!result.cacheHit);
      QCOMPARE(daemon.calls, before + 1);
      auto retry = fetchOnWorker(QString::fromLatin1(kProvider), QString::fromLatin1(kPath),
                                 QString::fromLatin1(kGeneration), assetId);
      QVERIFY(retry.status == item.status);
      QVERIFY(!retry.cacheHit);
      QCOMPARE(daemon.calls, before + 2);
      QCOMPARE(AssetImages::shared().cachedEntries(), 0);
    }
  }
  void placeholderStaysUntilTheImageArrives() {
    QVERIFY(registerDaemon(&daemon));
    daemon.hold = true;
    daemon.png = redPng;
    const auto url = AssetImages::imageUrl(
        QString::fromLatin1(kProvider), QString::fromLatin1(kPath),
        QString::fromLatin1(kGeneration), QStringLiteral("cover"));
    QVERIFY(!url.isEmpty());
    QQmlApplicationEngine engine;
    installAssetImageProvider(&engine);
    engine.rootContext()->setContextProperty(QStringLiteral("assetUrl"), url);
    engine.load(QUrl::fromLocalFile(
        QStringLiteral(ARINN_PLACEHOLDER_QML)));
    QVERIFY(!engine.rootObjects().isEmpty());
    auto window = qobject_cast<QQuickWindow *>(engine.rootObjects().first());
    QVERIFY(window);
    window->show();
    QTRY_VERIFY(daemon.calls >= 1);
    auto sessionImage = findItem(window->contentItem(), QStringLiteral("artworkImage"));
    auto plate = findItem(window->contentItem(), QStringLiteral("artworkPlate"));
    auto thumb = findItem(window->contentItem(), QStringLiteral("thumbnailImage"));
    auto label = findItem(window->contentItem(), QStringLiteral("thumbnailLabel"));
    auto poster = findItem(window->contentItem(), QStringLiteral("posterImage-north"));
    auto posterPlate = findItem(window->contentItem(), QStringLiteral("posterPlate-north"));
    auto card = findItem(window->contentItem(), QStringLiteral("card"));
    QVERIFY(sessionImage);
    QVERIFY(plate);
    QVERIFY(thumb);
    QVERIFY(label);
    QVERIFY(poster);
    QVERIFY(posterPlate);
    QVERIFY(card);
    QVERIFY(sessionImage->property("status").toInt() != kImageReady);
    QCOMPARE(sessionImage->property("visible").toBool(), false);
    QCOMPARE(thumb->property("visible").toBool(), false);
    QCOMPARE(label->property("visible").toBool(), true);
    QCOMPARE(poster->property("visible").toBool(), false);
    QCOMPARE(posterPlate->property("visible").toBool(), true);
    QCOMPARE(card->property("artworkReady").toBool(), false);

    daemon.hold = false;
    daemon.release();
    QTRY_COMPARE(sessionImage->property("status").toInt(), kImageReady);
    QCOMPARE(sessionImage->property("visible").toBool(), true);
    QTRY_COMPARE(thumb->property("status").toInt(), kImageReady);
    QCOMPARE(thumb->property("visible").toBool(), true);
    QCOMPARE(label->property("visible").toBool(), false);
    QTRY_COMPARE(poster->property("status").toInt(), kImageReady);
    QCOMPARE(poster->property("visible").toBool(), true);
    QCOMPARE(posterPlate->property("visible").toBool(), false);
    QTRY_COMPARE(card->property("artworkReady").toBool(), true);
    QTRY_VERIFY(continueCardShowsRed(card));
  }
  void missingAssetKeepsThePlate() {
    QVERIFY(registerDaemon(&daemon));
    const auto url = AssetImages::imageUrl(
        QString::fromLatin1(kProvider), QString::fromLatin1(kPath),
        QString::fromLatin1(kGeneration), QStringLiteral("absent-asset"));
    QVERIFY(url.contains(QStringLiteral("image://arinn-assets/")));
    imageLogs.clear();
    const auto previousLog = qInstallMessageHandler(collectImageLogs);
    struct RestoreLog {
      QtMessageHandler previous;
      ~RestoreLog() { qInstallMessageHandler(previous); }
    } restore{previousLog};

    QQmlApplicationEngine engine;
    installAssetImageProvider(&engine);
    engine.rootContext()->setContextProperty(QStringLiteral("assetUrl"), url);
    engine.load(QUrl::fromLocalFile(QStringLiteral(ARINN_PLACEHOLDER_QML)));
    QVERIFY(!engine.rootObjects().isEmpty());
    auto window = qobject_cast<QQuickWindow *>(engine.rootObjects().first());
    QVERIFY(window);
    window->show();

    auto sessionImage = findItem(window->contentItem(), QStringLiteral("artworkImage"));
    auto thumb = findItem(window->contentItem(), QStringLiteral("thumbnailImage"));
    auto label = findItem(window->contentItem(), QStringLiteral("thumbnailLabel"));
    auto poster = findItem(window->contentItem(), QStringLiteral("posterImage-north"));
    auto posterPlate = findItem(window->contentItem(), QStringLiteral("posterPlate-north"));
    auto card = findItem(window->contentItem(), QStringLiteral("card"));
    auto cardArtwork = findItem(window->contentItem(), QStringLiteral("cardArtwork"));
    auto cardPlate = findItem(window->contentItem(), QStringLiteral("cardPlate"));
    QVERIFY(sessionImage);
    QVERIFY(thumb);
    QVERIFY(label);
    QVERIFY(poster);
    QVERIFY(posterPlate);
    QVERIFY(card);
    QVERIFY(cardArtwork);
    QVERIFY(cardPlate);

    QTRY_COMPARE(sessionImage->property("status").toInt(), kImageError);
    QCOMPARE(sessionImage->property("visible").toBool(), false);
    QTRY_COMPARE(thumb->property("status").toInt(), kImageError);
    QCOMPARE(thumb->property("visible").toBool(), false);
    QCOMPARE(label->property("visible").toBool(), true);
    QTRY_COMPARE(poster->property("status").toInt(), kImageError);
    QCOMPARE(poster->property("visible").toBool(), false);
    QCOMPARE(posterPlate->property("visible").toBool(), true);
    QTRY_COMPARE(cardArtwork->property("status").toInt(), kImageError);
    QCOMPARE(card->property("artworkReady").toBool(), false);
    QCOMPARE(cardPlate->property("visible").toBool(), true);
    QTRY_VERIFY(daemon.calls >= 4);
    QCOMPARE(daemon.lastId, QStringLiteral("absent-asset"));

    for (const auto &line : imageLogs) {
      QVERIFY(!line.contains(QStringLiteral("absent-asset")));
      QVERIFY(!line.contains(QStringLiteral("image://")));
      QVERIFY(!line.contains(QString::fromLatin1(kPath)));
    }
  }
};

int main(int argc, char **argv) {
  QGuiApplication app(argc, argv);
  AssetImagesTest test;
  return QTest::qExec(&test, argc, argv);
}
#include "asset_images_test.moc"
