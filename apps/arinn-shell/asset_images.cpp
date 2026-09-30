// SPDX-FileCopyrightText: 2026 Arinn contributors
//
// SPDX-License-Identifier: GPL-3.0-or-later

#include "asset_images.h"
#include <QList>
#include <QMetaObject>
#include <QMutex>
#include <QMutexLocker>
#include <QQuickAsyncImageProvider>
#include <QQuickImageResponse>
#include <QRunnable>
#include <QThread>
#include <QThreadPool>
#include <QUrl>
#include <QtDBus>

namespace {
constexpr auto kDaemon = "org.arinn.Daemon1";
constexpr auto kInterface = "org.arinn.Assets1";
constexpr int kCallTimeoutMs = 5000;
constexpr int kMaxBytes = 8 * 1024 * 1024;
constexpr auto kPathPrefix = "/org/arinn/";

bool validProviderPath(const QString &path) {
  if (!path.startsWith(QLatin1String(kPathPrefix)) ||
      path.contains(QLatin1String("..")))
    return false;
  for (const auto &segment : path.split(QLatin1Char('/'))) {
    if (segment.isEmpty())
      continue;
    for (const auto ch : segment) {
      const auto u = ch.unicode();
      const auto ok = (u >= 'A' && u <= 'Z') || (u >= 'a' && u <= 'z') ||
                      (u >= '0' && u <= '9') || u == '_' || u == '-';
      if (!ok)
        return false;
    }
  }
  return true;
}

QByteArray encodeFields(const QStringList &fields) {
  QByteArray payload;
  for (const auto &field : fields) {
    const auto bytes = field.toUtf8();
    payload.append(QByteArray::number(bytes.size()));
    payload.append(':');
    payload.append(bytes);
  }
  return payload.toHex();
}

bool decodeFields(const QByteArray &hex, QStringList *fields) {
  auto raw = QByteArray::fromHex(hex);
  QStringList decoded;
  while (!raw.isEmpty()) {
    const auto colon = raw.indexOf(':');
    if (colon <= 0)
      return false;
    bool ok = false;
    const auto len = raw.left(colon).toInt(&ok);
    if (!ok || len < 0 || raw.size() < colon + 1 + len)
      return false;
    decoded.append(QString::fromUtf8(raw.mid(colon + 1, len)));
    raw = raw.mid(colon + 1 + len);
  }
  if (decoded.size() != 4)
    return false;
  *fields = decoded;
  return true;
}

bool signatureMatches(const QString &mime, const QByteArray &bytes) {
  if (mime == QLatin1String("image/png"))
    return bytes.startsWith(QByteArrayLiteral("\x89PNG\r\n\x1A\n"));
  if (mime == QLatin1String("image/jpeg"))
    return bytes.size() >= 3 &&
           static_cast<unsigned char>(bytes.at(0)) == 0xFF &&
           static_cast<unsigned char>(bytes.at(1)) == 0xD8 &&
           static_cast<unsigned char>(bytes.at(2)) == 0xFF;
  if (mime == QLatin1String("image/webp"))
    return bytes.size() >= 12 && bytes.startsWith(QByteArrayLiteral("RIFF")) &&
           bytes.mid(8, 4) == QByteArrayLiteral("WEBP");
  return false;
}

QImage decodeImage(const QString &mime, const QByteArray &bytes) {
  if (bytes.isEmpty() || bytes.size() > kMaxBytes ||
      !signatureMatches(mime, bytes))
    return {};
  const char *format = nullptr;
  if (mime == QLatin1String("image/png"))
    format = "PNG";
  else if (mime == QLatin1String("image/jpeg"))
    format = "JPEG";
  else if (mime == QLatin1String("image/webp"))
    format = "WEBP";
  else
    return {};
  QImage image;
  if (!image.loadFromData(bytes, format) || image.isNull())
    return {};
  return image;
}

AssetImages::Status statusFromError(const QString &name) {
  if (name.endsWith(QLatin1String("NotFound")))
    return AssetImages::Status::NotFound;
  if (name.endsWith(QLatin1String("Unsupported")))
    return AssetImages::Status::Unsupported;
  return AssetImages::Status::Error;
}

// One connection per pool thread. The pool does not retire threads, so a long
// session does not open a new bus connection for every fetch. The connection
// is not the session bus: a blocking call cannot invoke a local object on the
// same connection, and the tests register Assets1 there.
QThreadPool &assetPool() {
  static struct Holder {
    QThreadPool pool;
    Holder() {
      pool.setMaxThreadCount(4);
      pool.setExpiryTimeout(-1);
    }
  } holder;
  return holder.pool;
}

QDBusConnection workerBus() {
  thread_local QDBusConnection bus = QDBusConnection::connectToBus(
      QDBusConnection::SessionBus,
      QStringLiteral("arinn-assets-%1")
          .arg(reinterpret_cast<quintptr>(QThread::currentThreadId())));
  return bus;
}

class AssetResponse : public QQuickImageResponse {
public:
  AssetResponse(const QString &providerId, const QString &path,
                const QString &generation, const QString &assetId) {
    auto *job = QRunnable::create([this, providerId, path, generation, assetId] {
      const auto result =
          AssetImages::shared().fetch(providerId, path, generation, assetId);
      const auto image = result.image;
      QMetaObject::invokeMethod(
          this,
          [this, image] {
            m_image = image;
            emit finished();
          },
          Qt::QueuedConnection);
    });
    assetPool().start(job);
  }
  QQuickTextureFactory *textureFactory() const override {
    if (m_image.isNull())
      return nullptr;
    return QQuickTextureFactory::textureFactoryForImage(m_image);
  }
  // A null texture with an empty error is a successful 0×0 load, which hides
  // the plate. The text stays fixed so a missing id or URL is not logged.
  QString errorString() const override {
    return m_image.isNull() ? QStringLiteral("Artwork is unavailable")
                            : QString();
  }

private:
  QImage m_image;
};

class AssetProvider : public QQuickAsyncImageProvider {
public:
  QQuickImageResponse *requestImageResponse(const QString &id,
                                            const QSize &) override {
    QString providerId, path, generation, assetId;
    if (!AssetImages::parseImageId(id, &providerId, &path, &generation,
                                   &assetId))
      return new AssetResponse({}, {}, {}, {});
    return new AssetResponse(providerId, path, generation, assetId);
  }
};
} // namespace

struct AssetImages::Cache {
  struct Entry {
    QString key;
    QString mime;
    QByteArray bytes;
  };
  QMutex mutex;
  qint64 budget = AssetImages::defaultBudget;
  qint64 used = 0;
  QList<Entry> entries;

  void trim() {
    while (used > budget && !entries.isEmpty()) {
      used -= entries.last().bytes.size();
      entries.removeLast();
    }
  }
};

AssetImages::AssetImages() : m_cache(new Cache) {}
AssetImages::~AssetImages() { delete m_cache; }

AssetImages &AssetImages::shared() {
  static AssetImages images;
  return images;
}

void AssetImages::setMemoryBudget(qint64 bytes) {
  QMutexLocker lock(&m_cache->mutex);
  m_cache->budget = qMax<qint64>(0, bytes);
  m_cache->trim();
}
qint64 AssetImages::memoryBudget() const {
  QMutexLocker lock(&m_cache->mutex);
  return m_cache->budget;
}
void AssetImages::clear() {
  QMutexLocker lock(&m_cache->mutex);
  m_cache->entries.clear();
  m_cache->used = 0;
}
int AssetImages::cachedEntries() const {
  QMutexLocker lock(&m_cache->mutex);
  return static_cast<int>(m_cache->entries.size());
}

QString AssetImages::imageUrl(const QString &providerId, const QString &objectPath,
                              const QString &generation, const QString &assetId) {
  if (providerId.isEmpty() || generation.isEmpty() || assetId.isEmpty() ||
      !validProviderPath(objectPath))
    return {};
  const auto token =
      encodeFields({providerId, objectPath, generation, assetId});
  return QStringLiteral("image://arinn-assets/") + QString::fromLatin1(token);
}

bool AssetImages::parseImageId(const QString &id, QString *providerId,
                               QString *objectPath, QString *generation,
                               QString *assetId) {
  auto token = id;
  if (token.startsWith(QLatin1Char('/')))
    token = token.mid(1);
  QStringList fields;
  if (!decodeFields(token.toLatin1(), &fields))
    return false;
  *providerId = fields.at(0);
  *objectPath = fields.at(1);
  *generation = fields.at(2);
  *assetId = fields.at(3);
  return true;
}

AssetImages::Result AssetImages::fetch(const QString &providerId,
                                       const QString &objectPath,
                                       const QString &generation,
                                       const QString &assetId) {
  Result result;
  if (providerId.isEmpty() || generation.isEmpty() || assetId.isEmpty() ||
      !validProviderPath(objectPath)) {
    result.status = Status::Empty;
    return result;
  }
  const auto key = providerId + QLatin1Char('\n') + generation +
                   QLatin1Char('\n') + assetId;
  QString cachedMime;
  QByteArray cachedBytes;
  bool cached = false;
  {
    QMutexLocker lock(&m_cache->mutex);
    for (int i = 0; i < m_cache->entries.size(); ++i) {
      if (m_cache->entries.at(i).key != key)
        continue;
      auto entry = m_cache->entries.takeAt(i);
      m_cache->entries.prepend(entry);
      cachedMime = entry.mime;
      cachedBytes = entry.bytes;
      cached = true;
      break;
    }
  }
  if (cached) {
    auto image = decodeImage(cachedMime, cachedBytes);
    if (!image.isNull()) {
      result.status = Status::Ready;
      result.cacheHit = true;
      result.image = image;
      return result;
    }
    QMutexLocker lock(&m_cache->mutex);
    for (int i = 0; i < m_cache->entries.size(); ++i) {
      if (m_cache->entries.at(i).key == key &&
          m_cache->entries.at(i).bytes == cachedBytes) {
        m_cache->used -= m_cache->entries.at(i).bytes.size();
        m_cache->entries.removeAt(i);
        break;
      }
    }
  }
  auto bus = workerBus();
  if (!bus.isConnected()) {
    result.status = Status::Error;
    return result;
  }
  auto msg = QDBusMessage::createMethodCall(kDaemon, objectPath, kInterface,
                                            QStringLiteral("GetAsset"));
  msg << generation << assetId;
  const auto reply = bus.call(msg, QDBus::Block, kCallTimeoutMs);
  if (reply.type() != QDBusMessage::ReplyMessage || reply.arguments().isEmpty()) {
    result.status = statusFromError(reply.errorName());
    return result;
  }
  const auto argument = qvariant_cast<QDBusArgument>(reply.arguments().at(0));
  if (argument.currentType() != QDBusArgument::StructureType) {
    result.status = Status::Error;
    return result;
  }
  QString mime;
  QByteArray bytes;
  argument.beginStructure();
  argument >> mime >> bytes;
  argument.endStructure();
  if (bytes.size() > kMaxBytes) {
    result.status = Status::Error;
    return result;
  }
  auto image = decodeImage(mime, bytes);
  if (image.isNull()) {
    result.status = Status::Error;
    return result;
  }
  {
    QMutexLocker lock(&m_cache->mutex);
    for (int i = 0; i < m_cache->entries.size(); ++i) {
      if (m_cache->entries.at(i).key == key) {
        m_cache->used -= m_cache->entries.at(i).bytes.size();
        m_cache->entries.removeAt(i);
        break;
      }
    }
    if (bytes.size() <= m_cache->budget) {
      while (m_cache->used + bytes.size() > m_cache->budget &&
             !m_cache->entries.isEmpty()) {
        m_cache->used -= m_cache->entries.last().bytes.size();
        m_cache->entries.removeLast();
      }
      Cache::Entry entry;
      entry.key = key;
      entry.mime = mime;
      entry.bytes = bytes;
      m_cache->entries.prepend(entry);
      m_cache->used += bytes.size();
    }
  }
  result.status = Status::Ready;
  result.image = image;
  return result;
}

void installAssetImageProvider(QQmlEngine *engine) {
  if (!engine)
    return;
  engine->addImageProvider(QStringLiteral("arinn-assets"), new AssetProvider);
}
