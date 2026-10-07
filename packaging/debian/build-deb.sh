#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Arinn contributors
#
# SPDX-License-Identifier: GPL-3.0-or-later

# Build a .deb from the checked-out commit (ADR-0091). Version comes from the
# Cargo workspace via scripts/release.py, matching packaging/arch/PKGBUILD.

set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$root"
git config --global --add safe.directory "$root"

version="$(python3 scripts/release.py version)"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

git archive --format=tar.gz --prefix="arinn-${version}/" -o "${work}/src.tar.gz" HEAD
tar -C "$work" -xzf "${work}/src.tar.gz"
cp -a packaging/debian/debian "${work}/arinn-${version}/debian"

maintainer="Arinn contributors <arinn@packages.local>"
date="$(date -R)"
cat >"${work}/arinn-${version}/debian/changelog" <<EOF
arinn (${version}-1) unstable; urgency=medium

  * CI build from git.

 -- ${maintainer}  ${date}

EOF

cd "${work}/arinn-${version}"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-${work}/arinn-${version}/target}"
dpkg-buildpackage -us -uc -b -d
mv ../*.deb "$root/packaging/debian/"
if [ -f "../arinn_${version}-1_amd64.buildinfo" ]; then
  mv ../*.buildinfo "$root/packaging/debian/" || true
fi
target_dir="${CARGO_TARGET_DIR:-${work}/arinn-${version}/target}"
if [ -f "${target_dir}/release/examples/private-systemd" ]; then
  cp "${target_dir}/release/examples/private-systemd" "$root/packaging/debian/private-systemd"
fi
