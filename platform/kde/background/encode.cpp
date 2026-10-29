// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "encode.h"
#include <QColorSpace>
#include <cstring>

namespace arinn::background {
Pixels encodeGrab(const QImage &grab) {
  if (grab.isNull() || grab.width() <= 0 || grab.height() <= 0) return {};
  const auto bytes = pixelBytes(grab.width(), grab.height());
  // A wrapped image can have arbitrarily large stride despite small dimensions.
  if (!bytes || grab.sizeInBytes() <= 0 ||
      static_cast<std::uint64_t>(grab.sizeInBytes()) > maxBytes) return {};
  QImage image = grab.convertToFormat(QImage::Format_RGBA8888);
  if (image.isNull()) return {};
  if (image.colorSpace().isValid() && image.colorSpace() != QColorSpace::SRgb) {
    image = image.convertedToColorSpace(QColorSpace::SRgb);
    if (image.isNull()) return {};
  }
  // Qt's conversion unpremultiplies alpha. DPR is metadata, never a size multiplier.
  // RGBA8888 with normal stride is already tight; custom input strides are packed.
  const auto rowBytes = static_cast<std::size_t>(image.width()) * 4;
  if (static_cast<std::size_t>(image.bytesPerLine()) == rowBytes)
    return Pixels::create(image.width(), image.height(), {image.constBits(), *bytes});
  std::vector<std::uint8_t> packed(*bytes);
  for (int y = 0; y < image.height(); ++y)
    std::memcpy(packed.data() + y * rowBytes, image.constScanLine(y), rowBytes);
  return Pixels::create(image.width(), image.height(), packed);
}
QImage boundGrab(const QImage &grab) {
  constexpr qsizetype maxInputBytes = 64 * 1024 * 1024;
  if (grab.isNull() || grab.width() <= 0 || grab.height() <= 0 || grab.sizeInBytes() <= 0 ||
      grab.sizeInBytes() > maxInputBytes)
    return {};
  if (std::uint32_t(grab.width()) <= maxWidth && std::uint32_t(grab.height()) <= maxHeight)
    return grab;
  QImage scaled = grab.scaled(int(maxWidth), int(maxHeight), Qt::KeepAspectRatio,
                              Qt::SmoothTransformation);
  // Physical pixels: DPR metadata never enlarges admission.
  scaled.setDevicePixelRatio(1.0);
  return scaled;
}
} // namespace arinn::background
