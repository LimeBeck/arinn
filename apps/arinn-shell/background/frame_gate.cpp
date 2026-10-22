// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "frame_gate.h"
#include "pixels.h"
#include <algorithm>
#include <limits>
#include <utility>

namespace arinn::background {
namespace {
bool uuid(const std::string &s) {
  if (s.size() != 36) return false;
  bool nonzero = false;
  for (std::size_t i = 0; i < s.size(); ++i) {
    if (i == 8 || i == 13 || i == 18 || i == 23) {
      if (s[i] != '-') return false;
    } else {
      const char c = s[i];
      if (!((c >= '0' && c <= '9') || (c >= 'a' && c <= 'f') || (c >= 'A' && c <= 'F')))
        return false;
      nonzero |= c != '0';
    }
  }
  return nonzero;
}
bool text(const std::string &s, std::size_t limit) {
  return !s.empty() && s.size() <= limit && s.find('\0') == std::string::npos;
}
}
bool Context::valid() const {
  return uuid(epoch) && uuid(request) && uuid(generation) &&
         text(target, 8192) && text(output, 8192) && text(token, 256);
}
bool Evidence::accepts(const Context &c) const {
  return home && sessionSafe && providerLive && producerLive && shellLive &&
         !owner.empty() && owner.front() == ':' && c.valid() &&
         epoch == c.epoch && target == c.target && output == c.output &&
         generation == c.generation && token == c.token;
}
bool Snapshot::valid() const {
  return uuid(epoch) && revision > 0 &&
         (state == "idle" || state == "pending" || state == "ready" || state == "unavailable") &&
         (state != "idle" || !context) && (state != "ready" || context) &&
         (!context || (context->valid() && context->epoch == epoch));
}
void FrameGate::clear() {
  // Release capacity too; invalidation must not retain a previous frame's bytes.
  std::vector<std::uint8_t>().swap(m_pixels);
  if (m_serial != std::numeric_limits<std::uint64_t>::max()) ++m_serial;
}
void FrameGate::observe(Evidence evidence) {
  if (evidence == m_evidence) return;
  clear();
  if (evidence.owner != m_evidence.owner || evidence.epoch != m_evidence.epoch) {
    m_floor = m_retiredRevision = 0;
    m_signalEpoch.clear(); m_retiredRequest.clear(); m_snapshot.reset();
  } else if (m_snapshot && m_snapshot->context &&
             (m_evidence.accepts(*m_snapshot->context) ||
              (!m_evidence.sessionSafe && evidence.sessionSafe))) {
    // A restored guard/Home/provider is a new lifetime, never permission to
    // reuse the old capture. Keep one retired request and revision floor.
    // Evidence that only completes (the Return window resolving after the
    // snapshot) never admitted the request, so it does not retire it.
    m_retiredRequest = m_snapshot->context->request;
    m_retiredRevision = m_snapshot->revision;
  }
  m_reconciled = false;
  m_evidence = std::move(evidence);
}
void FrameGate::failed() {
  m_reconciled = false;
  clear();
  // Preserve installed revision/content as an anti-regression floor; never
  // allow a failed read to make an older metadata reply fresh again.
}
bool FrameGate::accept(const Snapshot &s) {
  if (!s.valid() || s.epoch != m_evidence.epoch ||
      (s.state == "ready" && (s.revision <= m_retiredRevision ||
                             s.context->request == m_retiredRequest)) ||
      (!m_signalEpoch.empty() && (s.epoch != m_signalEpoch || s.revision < m_floor)) ||
      (m_snapshot && (s.revision < m_snapshot->revision ||
                     (s.revision == m_snapshot->revision && s != *m_snapshot)))) {
    failed(); return false;
  }
  if (!m_snapshot || s != *m_snapshot) clear();
  m_snapshot = s;
  m_reconciled = true;
  if (s.state != "ready" || !m_evidence.accepts(*s.context)) clear();
  return true;
}
void FrameGate::notified(const std::string &epoch, std::uint64_t revision) {
  if (!uuid(epoch) || revision == 0 || epoch != m_evidence.epoch) {
    // Invalid data clears pixels but cannot replace a validated signal floor.
    failed(); return;
  }
  if (epoch == m_signalEpoch && revision <= m_floor) return;
  m_signalEpoch = epoch; m_floor = revision;
  if (!m_snapshot || revision > m_snapshot->revision) { m_reconciled = false; clear(); }
}
std::optional<FrameGate::Ticket> FrameGate::beginRead() {
  if (!m_reconciled || m_worker || ready() || !m_snapshot || m_snapshot->state != "ready" ||
      (!m_signalEpoch.empty() && (m_signalEpoch != m_snapshot->epoch || m_floor > m_snapshot->revision)) ||
      !m_evidence.accepts(*m_snapshot->context) ||
      m_serial == std::numeric_limits<std::uint64_t>::max()) return std::nullopt;
  m_worker = Ticket{m_serial, m_snapshot->revision, *m_snapshot->context};
  return m_worker;
}
bool FrameGate::completeRead(const Ticket &ticket, const Context &echoed,
                             std::uint32_t width, std::uint32_t height,
                             std::optional<std::vector<std::uint8_t>> rgba) {
  if (!m_worker || ticket != *m_worker) return false;
  m_worker.reset();
  const auto bytes = pixelBytes(width, height);
  if (!rgba || !bytes || rgba->size() != *bytes || ticket.serial != m_serial ||
      !m_snapshot || m_snapshot->state != "ready" ||
      ticket.revision != m_snapshot->revision || ticket.context != *m_snapshot->context ||
      echoed != ticket.context || !m_evidence.accepts(echoed)) {
    // A late cancelled result cannot clear a replacement's metadata or pixels.
    if (ticket.serial == m_serial) failed();
    return false;
  }
  m_pixels = std::move(*rgba);
  return true;
}
} // namespace arinn::background
