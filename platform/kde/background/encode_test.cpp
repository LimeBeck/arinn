// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "encode.h"
#include <QColorSpace>
#include <cstdlib>
#include <iostream>
namespace {
void check(bool ok, const char *message) {
  if (!ok) { std::cerr << message << '\n'; std::exit(1); }
}
}
int main() {
  using namespace arinn::background;
  check(!encodeGrab(QImage()), "empty grab unavailable");
  QImage large(1921, 1, QImage::Format_ARGB32_Premultiplied);
  check(!encodeGrab(large), "reject physical width before conversion");
  QImage tall(1, 1081, QImage::Format_ARGB32_Premultiplied);
  check(!encodeGrab(tall), "reject physical height before conversion");
  QImage image(2, 2, QImage::Format_ARGB32_Premultiplied);
  image.fill(QColor(200, 100, 50, 128));
  image.setDevicePixelRatio(2.0);
  auto frame = encodeGrab(image);
  auto bytes = frame.read(2, 2);
  check(bytes.has_value() && bytes->size() == 16, "DPR does not multiply physical extents");
  check((*bytes)[0] >= 198 && (*bytes)[0] <= 200 && (*bytes)[1] >= 99 &&
        (*bytes)[1] <= 101 && (*bytes)[2] >= 49 && (*bytes)[2] <= 51 && (*bytes)[3] == 128,
        "straight alpha and RGBA ordering after Qt unpremultiplication");
  QImage transparent(1, 1, QImage::Format_ARGB32_Premultiplied);
  transparent.fill(Qt::transparent);
  check(encodeGrab(transparent).read(1, 1) == std::vector<std::uint8_t>({0, 0, 0, 0}), "transparent pixel");
  QImage linear(1, 1, QImage::Format_RGBA8888);
  linear.fill(QColor(128, 128, 128));
  linear.setColorSpace(QColorSpace::SRgbLinear);
  auto converted = encodeGrab(linear).read(1, 1);
  check(converted && (*converted)[0] >= 187 && (*converted)[0] <= 189 && (*converted)[3] == 255,
        "tagged linear colour converted to sRGB");
  const std::uint8_t rows[16]{1, 2, 3, 255, 99, 99, 99, 99, 4, 5, 6, 255, 99, 99, 99, 99};
  QImage padded(rows, 1, 2, 8, QImage::Format_RGBA8888);
  check(encodeGrab(padded).read(1, 2) == std::vector<std::uint8_t>({1, 2, 3, 255, 4, 5, 6, 255}),
        "row padding is not transported");
  std::vector<std::uint8_t> padding(maxBytes + 4);
  QImage hostile(padding.data(), 1, 1, static_cast<int>(padding.size()), QImage::Format_RGBA8888);
  check(!encodeGrab(hostile), "reject oversized wrapped image storage");
  std::cout << "Qt background encoding checks passed\n";
}
