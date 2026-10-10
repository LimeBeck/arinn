// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef _GNU_SOURCE
#define _GNU_SOURCE
#endif
#include "pixels.h"
#include <cerrno>
#include <fcntl.h>
#include <sys/mman.h>
#include <sys/stat.h>
#include <unistd.h>
#include <utility>

namespace arinn::background {
namespace {
constexpr int requiredSeals = F_SEAL_WRITE | F_SEAL_GROW | F_SEAL_SHRINK;
bool valid(int fd, std::size_t bytes) {
  struct stat info {};
  if (fd < 0 || fstat(fd, &info) != 0 || !S_ISREG(info.st_mode) ||
      info.st_size < 0 || static_cast<std::uint64_t>(info.st_size) != bytes)
    return false;
  // Linux regular files, devices, pipes and sockets cannot satisfy this memfd
  // sealing contract. F_SEAL_WRITE also rejects existing shared writable maps.
  const int seals = fcntl(fd, F_GET_SEALS);
  return seals >= 0 && (seals & requiredSeals) == requiredSeals;
}
}
std::optional<std::size_t> pixelBytes(std::uint32_t width, std::uint32_t height) {
  if (width == 0 || height == 0 || width > maxWidth || height > maxHeight)
    return std::nullopt;
  const auto bytes = std::uint64_t(width) * height * 4;
  if (bytes > maxBytes) return std::nullopt;
  return static_cast<std::size_t>(bytes);
}
Pixels::~Pixels() { reset(); }
Pixels::Pixels(Pixels &&other) noexcept : m_fd(std::exchange(other.m_fd, -1)) {}
Pixels &Pixels::operator=(Pixels &&other) noexcept {
  if (this != &other) {
    reset();
    m_fd = std::exchange(other.m_fd, -1);
  }
  return *this;
}
void Pixels::reset() {
  if (m_fd >= 0) close(std::exchange(m_fd, -1));
}
Pixels Pixels::create(std::uint32_t width, std::uint32_t height,
                      std::span<const std::uint8_t> rgba) {
  const auto bytes = pixelBytes(width, height);
  if (!bytes || rgba.size() != *bytes) return {};
  Pixels result(memfd_create("arinn-background", MFD_CLOEXEC | MFD_ALLOW_SEALING));
  if (!result) return {};
  std::size_t offset = 0;
  while (offset < *bytes) {
    const auto count = pwrite(result.m_fd, rgba.data() + offset, *bytes - offset,
                              static_cast<off_t>(offset));
    if (count < 0 && errno == EINTR) continue;
    if (count <= 0) return {};
    offset += static_cast<std::size_t>(count);
  }
  if (fcntl(result.m_fd, F_ADD_SEALS, requiredSeals | F_SEAL_SEAL) != 0 ||
      !valid(result.m_fd, *bytes)) return {};
  return result;
}
Pixels Pixels::duplicate(int fd, std::uint32_t width, std::uint32_t height) {
  const auto bytes = pixelBytes(width, height);
  if (!bytes || fd < 0) return {};
  Pixels result(fcntl(fd, F_DUPFD_CLOEXEC, 0));
  if (!valid(result.m_fd, *bytes)) return {};
  return result;
}
std::optional<std::vector<std::uint8_t>> Pixels::read(std::uint32_t width,
                                                     std::uint32_t height) const {
  const auto bytes = pixelBytes(width, height);
  if (!bytes || !valid(m_fd, *bytes)) return std::nullopt;
  std::vector<std::uint8_t> rgba(*bytes);
  std::size_t offset = 0;
  while (offset < *bytes) {
    const auto count = pread(m_fd, rgba.data() + offset, *bytes - offset,
                             static_cast<off_t>(offset));
    if (count < 0 && errno == EINTR) continue;
    if (count <= 0) return std::nullopt;
    offset += static_cast<std::size_t>(count);
  }
  return rgba;
}
} // namespace arinn::background
