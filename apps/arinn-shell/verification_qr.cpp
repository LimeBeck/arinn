// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "verification_qr.h"

#include <QCoreApplication>
#include <QPainter>
#include <QUrl>
#include <QtQml/qqml.h>
#include <algorithm>
#include <memory>
#include <qrencode.h>

namespace {
constexpr int quietZone = 4;
// Covers the provider's canonical verification URL limit and fits byte-mode
// QR codes with medium error correction.
constexpr int maximumUrlBytes = 2048;

QImage encode(const QString &address) {
  if (address.size() > maximumUrlBytes)
    return {};
  const QUrl url(address, QUrl::StrictMode);
  if (!url.isValid() || url.scheme() != QLatin1String("https") ||
      url.host().isEmpty() || !url.userInfo().isEmpty() || url.hasQuery() ||
      url.hasFragment())
    return {};
  const auto data = url.toEncoded();
  if (data.size() > maximumUrlBytes)
    return {};
  const std::unique_ptr<QRcode, decltype(&QRcode_free)> code(
      QRcode_encodeData(
          static_cast<int>(data.size()),
          reinterpret_cast<const unsigned char *>(data.constData()), 0,
          QR_ECLEVEL_M),
      &QRcode_free);
  if (!code)
    return {};
  const int size = code->width + 2 * quietZone;
  QImage image(size, size, QImage::Format_RGB32);
  if (image.isNull())
    return {};
  image.fill(Qt::white);
  for (int y = 0; y < code->width; ++y)
    for (int x = 0; x < code->width; ++x)
      if (code->data[y * code->width + x] & 1)
        image.setPixel(x + quietZone, y + quietZone, qRgb(0, 0, 0));
  return image;
}

void registerVerificationQr() {
  qmlRegisterType<VerificationQr>("Arinn.Shell", 1, 0, "VerificationQr");
}
} // namespace
Q_COREAPP_STARTUP_FUNCTION(registerVerificationQr)

void VerificationQr::setVerificationUrl(const QString &value) {
  if (m_url == value)
    return;
  m_url = value;
  m_image = encode(value);
  emit verificationUrlChanged();
  update();
}

void VerificationQr::paint(QPainter *painter) {
  if (m_image.isNull())
    return;
  const int scale =
      static_cast<int>(std::min(width(), height())) / m_image.width();
  if (scale < 1)
    return;
  const int size = m_image.width() * scale;
  painter->setRenderHint(QPainter::SmoothPixmapTransform, false);
  painter->drawImage(QRect((static_cast<int>(width()) - size) / 2,
                           (static_cast<int>(height()) - size) / 2, size, size),
                     m_image);
}
