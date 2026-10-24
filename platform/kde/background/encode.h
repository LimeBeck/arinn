// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once
#include "pixels.h"
#include <QImage>

namespace arinn::background {
// Call on the producer worker, never in a compositor grab callback. Reject
// physical/DPR-sensitive extents before conversion or allocation. An untagged
// Qt grab is interpreted as sRGB; tagged images are converted to sRGB explicitly.
Pixels encodeGrab(const QImage &grab);
// Scales a grab larger than the physical bounds (a high-DPR output) down to
// fit them, keeping its aspect ratio. Returns a null image for an empty grab
// or one whose storage exceeds 64 MiB, before any scaling. Worker only.
QImage boundGrab(const QImage &grab);
} // namespace arinn::background
