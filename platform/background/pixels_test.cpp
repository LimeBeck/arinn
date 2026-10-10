// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef _GNU_SOURCE
#define _GNU_SOURCE
#endif
#include "pixels.h"
#include <array>
#include <cstdlib>
#include <fcntl.h>
#include <iostream>
#include <limits>
#include <sys/mman.h>
#include <sys/socket.h>
#include <unistd.h>

namespace {
void check(bool ok, const char *message) {
  if (!ok) { std::cerr << message << '\n'; std::exit(1); }
}
int frame(int seals, int bytes = 16) {
  const int fd = memfd_create("synthetic-background-test", MFD_CLOEXEC | MFD_ALLOW_SEALING);
  check(fd >= 0 && ftruncate(fd, bytes) == 0, "fixture memfd");
  if (seals) check(fcntl(fd, F_ADD_SEALS, seals) == 0, "fixture seals");
  return fd;
}
}
int main() {
  using namespace arinn::background;
  constexpr int seals = F_SEAL_WRITE | F_SEAL_GROW | F_SEAL_SHRINK;
  check(pixelBytes(1920, 1080) == 8294400, "physical frame budget");
  for (const auto &[w, h] : std::array<std::pair<std::uint32_t, std::uint32_t>, 5>{
           {{0, 1}, {1, 0}, {1921, 1}, {1, 1081},
            {std::numeric_limits<std::uint32_t>::max(), std::numeric_limits<std::uint32_t>::max()}}})
    check(!pixelBytes(w, h), "reject invalid extent before multiplication");
  const std::vector<std::uint8_t> rgba{1, 2, 3, 4, 5, 6, 7, 8,
                                      9, 10, 11, 12, 13, 14, 15, 16};
  check(!Pixels::create(2, 2, std::span(rgba).first(15)), "reject mismatched payload");
  auto original = Pixels::create(2, 2, rgba);
  check(bool(original), "create immutable frame");
  check((fcntl(original.descriptor(), F_GETFD) & FD_CLOEXEC) != 0, "producer close-on-exec");
  check((fcntl(original.descriptor(), F_GET_SEALS) & seals) == seals, "required seals");
  check(pwrite(original.descriptor(), rgba.data(), 1, 0) == -1, "writes forbidden");
  check(ftruncate(original.descriptor(), 15) == -1, "shrink forbidden");
  check(ftruncate(original.descriptor(), 17) == -1, "growth forbidden");
  auto copy = Pixels::duplicate(original.descriptor(), 2, 2);
  check(bool(copy) && copy.descriptor() != original.descriptor(), "consumer owns duplicate");
  check((fcntl(copy.descriptor(), F_GETFD) & FD_CLOEXEC) != 0, "consumer close-on-exec");
  check(lseek(original.descriptor(), 16, SEEK_SET) == 16, "fixture changes shared offset");
  check(copy.read(2, 2) == rgba && copy.read(2, 2) == rgba, "reads independent of shared offset");
  check(!copy.read(1, 1), "read revalidates size");
  original.reset();
  check(copy.read(2, 2) == rgba, "producer release preserves consumer duplicate");
  const int held = copy.descriptor();
  auto moved = std::move(copy);
  check(!copy && moved.descriptor() == held, "move transfers descriptor");
  auto *alias = &moved;
  moved = std::move(*alias);
  check(moved.descriptor() == held, "self move preserves ownership");
  moved.reset();
  check(fcntl(held, F_GETFD) == -1, "reset closes descriptor");
  check(!Pixels::duplicate(-1, 2, 2), "reject invalid descriptor");
  for (const int missing : {0, F_SEAL_WRITE, F_SEAL_GROW | F_SEAL_SHRINK,
                            F_SEAL_WRITE | F_SEAL_GROW, F_SEAL_WRITE | F_SEAL_SHRINK}) {
    const int fd = frame(missing);
    check(!Pixels::duplicate(fd, 2, 2), "reject incomplete seals");
    close(fd);
  }
  for (const int bytes : {0, 15, 17}) {
    const int fd = frame(seals, bytes);
    check(!Pixels::duplicate(fd, 2, 2), "reject truncated or extra bytes");
    close(fd);
  }
  int fd = frame(F_SEAL_GROW | F_SEAL_SHRINK);
  void *map = mmap(nullptr, 16, PROT_WRITE, MAP_SHARED, fd, 0);
  check(map != MAP_FAILED, "fixture writable mapping");
  check(fcntl(fd, F_ADD_SEALS, F_SEAL_WRITE) == -1, "kernel forbids writable mapping seal");
  check(!Pixels::duplicate(fd, 2, 2), "reject writable mapping frame");
  munmap(map, 16); close(fd);
  int pipes[2];
  check(pipe(pipes) == 0, "fixture pipe");
  check(!Pixels::duplicate(pipes[0], 2, 2), "reject pipe without blocking");
  close(pipes[0]); close(pipes[1]);
  int sockets[2];
  check(socketpair(AF_UNIX, SOCK_STREAM, 0, sockets) == 0, "fixture sockets");
  check(!Pixels::duplicate(sockets[0], 2, 2), "reject socket");
  close(sockets[0]); close(sockets[1]);
  fd = open("/dev/zero", O_RDONLY | O_CLOEXEC);
  check(fd >= 0 && !Pixels::duplicate(fd, 2, 2), "reject device"); close(fd);
  char path[] = "/tmp/arinn-background-test-XXXXXX";
  fd = mkstemp(path); unlink(path);
  check(fd >= 0 && ftruncate(fd, 16) == 0, "fixture ordinary file");
  check(!Pixels::duplicate(fd, 2, 2), "reject ordinary unlinked file"); close(fd);
  std::vector<std::uint8_t> maximum(*pixelBytes(maxWidth, maxHeight), 42);
  auto large = Pixels::create(maxWidth, maxHeight, maximum);
  check(bool(large) && large.read(maxWidth, maxHeight) == maximum, "maximum admitted frame round trip");
  std::cout << "background pixel descriptor checks passed\n";
}
