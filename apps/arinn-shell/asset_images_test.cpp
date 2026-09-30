// SPDX-FileCopyrightText: 2026 Arinn contributors
//
// SPDX-License-Identifier: GPL-3.0-or-later

#include "asset_images.h"
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
#include <QQuickWindow>
#include <QRunnable>
#include <QTest>
#include <QThreadPool>
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
    if (id == QLatin1String("missing")) {
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
  QThreadPool::globalInstance()->start(job);
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

class AssetImagesTest : public QObject {
  Q_OBJECT
  AssetDaemon daemon;
  QByteArray redPng;
  QByteArray bluePng;
private slots:
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
    QVERIFY(plate->isVisible());
    QVERIFY(sessionImage->property("status").toInt() != 1);
    QCOMPARE(sessionImage->property("visible").toBool(), false);
    QCOMPARE(thumb->property("visible").toBool(), false);
    QCOMPARE(label->property("visible").toBool(), true);
    QCOMPARE(poster->property("visible").toBool(), false);
    QCOMPARE(posterPlate->property("visible").toBool(), true);
    QCOMPARE(card->property("artworkReady").toBool(), false);

    const auto callsWhileHeld = daemon.calls;
    daemon.hold = false;
    daemon.release();
    QTRY_COMPARE(sessionImage->property("status").toInt(), 1);
    QCOMPARE(sessionImage->property("visible").toBool(), true);
    QVERIFY(plate->isVisible());
    QTRY_COMPARE(thumb->property("status").toInt(), 1);
    QCOMPARE(thumb->property("visible").toBool(), true);
    QCOMPARE(label->property("visible").toBool(), false);
    QTRY_COMPARE(poster->property("status").toInt(), 1);
    QCOMPARE(poster->property("visible").toBool(), true);
    QCOMPARE(posterPlate->property("visible").toBool(), false);
    QTRY_COMPARE(card->property("artworkReady").toBool(), true);
    QVERIFY(daemon.calls >= callsWhileHeld);
  }
};

int main(int argc, char **argv) {
  QGuiApplication app(argc, argv);
  AssetImagesTest test;
  return QTest::qExec(&test, argc, argv);
}
#include "asset_images_test.moc"
