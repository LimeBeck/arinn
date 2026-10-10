// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once
// Test helper: records the shell flows a model reports (flow_trace.h).
#include "flow_trace.h"
#include <QList>
#include <QString>
#include <QStringList>

struct RecordedFlow {
  QString flow, outcome;
  QStringList steps; // "name:outcome", in start order.
};
inline QList<RecordedFlow> &recordedFlows() {
  static QList<RecordedFlow> flows;
  return flows;
}
// Installs the recording sink and clears earlier records.
inline void recordFlows() {
  recordedFlows().clear();
  FlowTrace::sink = [](const FlowTrace::Record &record) {
    RecordedFlow flow{QString::fromLatin1(record.flow), QString::fromLatin1(record.outcome), {}};
    for (int i = 0; i < record.stepCount; ++i)
      flow.steps << QStringLiteral("%1:%2").arg(QLatin1String(record.steps[i].name),
                                               QLatin1String(record.steps[i].outcome));
    recordedFlows() << flow;
  };
}
// The recorded flows with this name, oldest first.
inline QList<RecordedFlow> flowsNamed(const QString &name) {
  QList<RecordedFlow> found;
  for (const auto &flow : recordedFlows())
    if (flow.flow == name) found << flow;
  return found;
}
