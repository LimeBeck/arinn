/*
 * SPDX-FileCopyrightText: 2026 Arinn contributors
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

#pragma once
#include <QImage>
#include <QString>

class QQmlEngine;

// Loads artwork, posters, fanart and thumbnails through org.arinn.Assets1.
// The cache key is (provider, generation, id). Compressed bytes stay in this
// process, capped by a memory budget. Screens only receive an image:// URL.
class AssetImages {
public:
  enum class Status { Ready, Empty, NotFound, Unsupported, Error };
  struct Result {
    Status status = Status::Empty;
    bool cacheHit = false;
    QImage image;
  };

  static AssetImages &shared();
  // 64 MiB of compressed image bytes. Visible textures are separate.
  static constexpr qint64 defaultBudget = 64 * 1024 * 1024;

  void setMemoryBudget(qint64 bytes);
  qint64 memoryBudget() const;
  void clear();
  int cachedEntries() const;

  // Empty when any part is missing or the path is not a daemon object path.
  static QString imageUrl(const QString &providerId, const QString &objectPath,
                          const QString &generation, const QString &assetId);
  static bool parseImageId(const QString &id, QString *providerId,
                           QString *objectPath, QString *generation,
                           QString *assetId);

  // Blocks. Call it off the UI thread. A hit does not call the daemon.
  Result fetch(const QString &providerId, const QString &objectPath,
               const QString &generation, const QString &assetId);

private:
  AssetImages();
  ~AssetImages();
  AssetImages(const AssetImages &) = delete;
  AssetImages &operator=(const AssetImages &) = delete;
  struct Cache;
  Cache *m_cache;
};

void installAssetImageProvider(QQmlEngine *engine);

// Bounded image-only local/theme loading; call off the UI thread.
QImage loadLocalIcon(const QString &reference, const QStringList &roots,
                     const QString &theme);
