// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once
// Timing of user-visible shell flows for diagnostics (ADR-0093). A flow is a
// fixed name, its fixed step names and an outcome: a public error code or
// "ok". Nothing else is recorded: no IDs, titles, URLs or paths. Without a
// sink, which the telemetry startup hook installs, recording is a no-op.
#include <QDateTime>
#include <QElapsedTimer>
#include <QString>
#include <array>
#include <cstring>

namespace FlowTrace {
struct Step {
  const char *name = nullptr;
  qint64 startMs = 0;
  qint64 endMs = -1;
  const char *outcome = nullptr;
};
struct Record {
  const char *flow = nullptr;
  qint64 startedAtMs = 0; // Wall clock, milliseconds since the epoch.
  qint64 durationMs = 0;  // Monotonic, like the step offsets.
  const char *outcome = nullptr;
  const Step *steps = nullptr;
  int stepCount = 0;
};
using Sink = void (*)(const Record &);
inline Sink sink = nullptr;

// The fixed outcome for an error code: a public code (ErrorCode) or "failed".
inline const char *outcome(const QString &code) {
  for (const char *known : {"ok", "NotFound", "Unavailable", "Unsupported", "PermissionDenied", "Timeout",
                            "InvalidArgument", "Conflict", "Internal", "Cancelled"})
    if (code == QLatin1String(known)) return known;
  return "failed";
}

class Flow {
public:
  static constexpr int MaxSteps = 8;
  bool active() const { return m_flow != nullptr; }
  qint64 elapsedMs() const { return m_flow ? m_timer.elapsed() : 0; }
  // Starts a flow; one already running is reported as cancelled first.
  void start(const char *flow) {
    if (m_flow) finish("Cancelled");
    m_flow = flow;
    m_startedAtMs = QDateTime::currentMSecsSinceEpoch();
    m_timer.start();
    m_count = 0;
  }
  void begin(const char *step) {
    if (!m_flow || m_count == MaxSteps) return;
    m_steps[m_count++] = Step{step, m_timer.elapsed(), -1, nullptr};
  }
  void end(const char *step, const char *outcome = "ok") {
    if (!m_flow) return;
    for (int i = m_count - 1; i >= 0; --i)
      if (m_steps[i].endMs < 0 && std::strcmp(m_steps[i].name, step) == 0) {
        m_steps[i].endMs = m_timer.elapsed();
        m_steps[i].outcome = outcome;
        return;
      }
  }
  // Reports the flow; a step still open ends with the flow and its outcome.
  void finish(const char *outcome) {
    if (!m_flow) return;
    const auto now = m_timer.elapsed();
    for (int i = 0; i < m_count; ++i)
      if (m_steps[i].endMs < 0) {
        m_steps[i].endMs = now;
        m_steps[i].outcome = outcome;
      }
    const Record record{m_flow, m_startedAtMs, now, outcome, m_steps.data(), m_count};
    m_flow = nullptr;
    if (sink) sink(record);
  }
  // Drops the flow without reporting it.
  void discard() { m_flow = nullptr; }

private:
  const char *m_flow = nullptr;
  qint64 m_startedAtMs = 0;
  QElapsedTimer m_timer;
  std::array<Step, MaxSteps> m_steps{};
  int m_count = 0;
};
}
