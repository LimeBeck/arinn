# SPDX-FileCopyrightText: 2026 Arinn contributors
#
# SPDX-License-Identifier: GPL-3.0-or-later

# Arch Linux CI images (ADR-0104), built by .github/workflows/ci-images.yml.
#
#   smoke  current Arch Linux with Python and D-Bus only: the package install
#          test starts from a clean system, so pacman resolves every
#          dependency the package declares.
#   build  everything the CI build job and the package build need, plus the
#          Qt SDK of the minimum supported Qt in $ARINN_QT_FLOOR_PREFIX. The
#          Arch package builds its shell against that SDK (#416); the CI
#          build job keeps using the system Qt.

# The Qt floor SDK, downloaded by a hash-locked aqtinstall. Its archives must
# match qt-archives.sha256; the step prints the actual list so a version bump
# can update the file.
FROM python:3.12-slim AS qt-sdk
ARG QT_VERSION=6.8.3
COPY aqtinstall.txt qt-archives.sha256 /qt/
RUN pip install --no-cache-dir --require-hashes -r /qt/aqtinstall.txt \
 && aqt install-qt linux desktop "$QT_VERSION" linux_gcc_64 \
      --archives qtbase qtdeclarative qtsvg qttools icu \
      --outputdir /opt/qt --keep --archive-dest /qt/archives \
 && cd /qt/archives \
 && find . -type f | sort | xargs sha256sum > /qt/actual.sha256 \
 && cat /qt/actual.sha256 \
 && grep -v -e '^#' -e '^$' /qt/qt-archives.sha256 | diff -u - /qt/actual.sha256 \
 && test -f "/opt/qt/$QT_VERSION/gcc_64/lib/cmake/Qt6/Qt6Config.cmake"

FROM archlinux:latest AS smoke
RUN pacman -Syu --noconfirm --needed python dbus \
 && rm -rf /var/cache/pacman/pkg/*

FROM smoke AS build
ARG QT_VERSION=6.8.3
# The union of the CI build job (Rust, Qt shell, integration suite) and the
# package build (makepkg as the unprivileged builder user).
RUN pacman -Syu --noconfirm --needed base-devel git rust cmake ninja ccache \
      qt6-base qt6-declarative qt6-svg qt6-tools curl libunwind openssl \
      nodejs systemd xorg-server-xvfb \
 && rm -rf /var/cache/pacman/pkg/* \
 && useradd --create-home builder
COPY --from=qt-sdk /opt/qt/${QT_VERSION}/gcc_64 /opt/qt/${QT_VERSION}/gcc_64
ENV ARINN_QT_FLOOR_PREFIX=/opt/qt/${QT_VERSION}/gcc_64
