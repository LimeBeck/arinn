/*
 * SPDX-FileCopyrightText: 2026 Arinn contributors
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

// Build identity (ADR-0097): this shell's build id and the comparison the
// daemon's BuildInfo answer is judged by.
#pragma once
#include "arinn_build_id.h"
#include <QString>

namespace BuildIdentity {
/// The exit status that asks the platform to restart Arinn (ADR-0098).
constexpr int RestartExitStatus = 75;

inline QString shell() { return QStringLiteral(ARINN_BUILD_ID); }

/// Whether two build ids name the same build: equal without `-dirty`, as
/// arinn_protocol::build::same_build.
inline bool same(QString a, QString b) {
  const auto dirty = QStringLiteral("-dirty");
  if (a.endsWith(dirty))
    a.chop(dirty.size());
  if (b.endsWith(dirty))
    b.chop(dirty.size());
  return a == b;
}
} // namespace BuildIdentity
