# SPDX-FileCopyrightText: 2026 Arinn contributors
#
# SPDX-License-Identifier: GPL-3.0-or-later

# Ubuntu 25.04 CI images for the Debian package (ADR-0091, ADR-0104), built by
# .github/workflows/ci-images.yml.
#
#   smoke  Python and D-Bus only: the install test starts from a clean
#          system, so apt resolves every dependency the .deb declares.
#   build  the .deb build dependencies. The Rust toolchain stays a job step
#          (rustup, cached on the runner's disk, ADR-0101).

FROM ubuntu:25.04 AS smoke
ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update \
 && apt-get install -y --no-install-recommends python3 dbus \
 && rm -rf /var/lib/apt/lists/*

FROM smoke AS build
RUN apt-get update \
 && apt-get install -y --no-install-recommends \
      git python3 ca-certificates curl debhelper-compat cmake g++ ninja-build \
      pkg-config qt6-base-dev qt6-declarative-dev qt6-svg-dev qt6-tools-dev \
      libcurl4-openssl-dev libunwind-dev libssl-dev devscripts \
      dbus-daemon libglib2.0-bin python-is-python3 \
 && rm -rf /var/lib/apt/lists/*
