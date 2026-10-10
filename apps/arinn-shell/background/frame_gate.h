// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once
#include <cstdint>
#include <optional>
#include <span>
#include <string>
#include <vector>

namespace arinn::background {
struct Context {
  std::string epoch, request, target, output, generation, token;
  bool valid() const;
  bool operator==(const Context &) const = default;
};
struct Evidence {
  std::string owner, epoch, target, output, generation, token;
  bool home = false, sessionSafe = false, providerLive = false,
       producerLive = false, shellLive = false;
  bool accepts(const Context &context) const;
  bool operator==(const Evidence &) const = default;
};
struct Snapshot {
  std::string epoch, state;
  std::uint64_t revision = 0;
  std::optional<Context> context;
  bool valid() const;
  bool operator==(const Snapshot &) const = default;
};
// Consumer-side admission only. The daemon alone arms capture; this class never
// generates Home intent or asks a provider for pixels. Transport must pin owner,
// validate signatures/cardinality and authenticate before supplying snapshots.
class FrameGate {
public:
  struct Ticket {
    std::uint64_t serial, revision;
    Context context;
    bool operator==(const Ticket &) const = default;
  };
  void observe(Evidence evidence);
  // Read-only metadata reconciliation. Reject regressed/conflicting snapshots.
  bool accept(const Snapshot &snapshot);
  // Invalidate immediately when a newer notification arrives, before IPC.
  void notified(const std::string &epoch, std::uint64_t revision);
  // Failed/invalid IPC clears evidence. The next valid snapshot can reconcile.
  void failed();
  std::optional<Ticket> beginRead();
  // Off-thread copying is performed by transport using platform Pixels. Even
  // cancelled work must call completeRead once, releasing the single worker slot.
  bool completeRead(const Ticket &ticket, const Context &echoed,
                    std::uint32_t width, std::uint32_t height,
                    std::optional<std::vector<std::uint8_t>> rgba);
  bool ready() const { return !m_pixels.empty(); }
  bool busy() const { return m_worker.has_value(); }
  std::span<const std::uint8_t> pixels() const { return m_pixels; }
private:
  void clear();
  bool m_reconciled = false;
  Evidence m_evidence;
  std::optional<Snapshot> m_snapshot;
  std::optional<Ticket> m_worker;
  std::uint64_t m_serial = 1, m_floor = 0, m_retiredRevision = 0;
  std::string m_signalEpoch, m_retiredRequest;
  std::vector<std::uint8_t> m_pixels;
};
} // namespace arinn::background
