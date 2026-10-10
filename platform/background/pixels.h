// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once
#include <cstddef>
#include <cstdint>
#include <optional>
#include <span>
#include <vector>

namespace arinn::background {
inline constexpr std::uint32_t maxWidth = 1920;
inline constexpr std::uint32_t maxHeight = 1080;
inline constexpr std::size_t maxBytes = 8 * 1024 * 1024;

// Validate physical extents before allocating or multiplying untrusted values.
std::optional<std::size_t> pixelBytes(std::uint32_t width, std::uint32_t height);

// Move-only ownership. Never exports a filename or retains a writable mapping.
class Pixels {
public:
  Pixels() = default;
  ~Pixels();
  Pixels(const Pixels &) = delete;
  Pixels &operator=(const Pixels &) = delete;
  Pixels(Pixels &&other) noexcept;
  Pixels &operator=(Pixels &&other) noexcept;
  int descriptor() const { return m_fd; }
  explicit operator bool() const { return m_fd >= 0; }
  void reset();

  // Straight-alpha, tightly packed RGBA8888 in sRGB, already converted by caller.
  static Pixels create(std::uint32_t width, std::uint32_t height,
                       std::span<const std::uint8_t> rgba);
  // Duplicate first, then validate that private copy. Caller retains ownership.
  static Pixels duplicate(int descriptor, std::uint32_t width,
                          std::uint32_t height);
  // Uses pread: duplicates and repeated consumers never share a file offset.
  std::optional<std::vector<std::uint8_t>> read(std::uint32_t width,
                                               std::uint32_t height) const;
private:
  explicit Pixels(int fd) : m_fd(fd) {}
  int m_fd = -1;
};
} // namespace arinn::background
