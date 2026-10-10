# SPDX-FileCopyrightText: 2026 Arinn contributors
#
# SPDX-License-Identifier: CC-BY-SA-4.0

# Debian package (CI artifact)

Arinn targets Arch Linux for releases and nightly qualification ([ADR-0033](../../docs/adr/0033-ci-cd-and-releases.md)). The `.deb` built here is an **additional** CI artifact and install smoke target only ([ADR-0091](../../docs/adr/0091-debian-package-ci-artifact.md)); it is not published to a public apt repository.

## CI suite

Package jobs use **Ubuntu 25.04** images built from [`ci/images/ubuntu`](../../ci/images/ubuntu/Dockerfile), whose Qt 6.8 is the minimum supported Qt ([ADR-0104](../../docs/adr/0104-qt-floor-and-prebuilt-ci-images.md)). The image is a build and smoke stand only; Ubuntu is not the product target distribution.

## Build locally

On a Debian or Ubuntu system with the `Build-Depends` from `debian/control` installed:

```sh
./packaging/debian/build-deb.sh
```

The script creates a `git archive` tarball (same layout as `packaging/arch/PKGBUILD`), injects the Cargo workspace version into `debian/changelog`, and runs `dpkg-buildpackage`. The resulting `arinn_<version>-1_amd64.deb` is written to `packaging/debian/`.

## Smoke test

After `sudo apt install ./packaging/debian/arinn_*.deb`:

```sh
ARINN_TEST_MANAGER=/path/to/private-systemd dbus-run-session -- python tests/package/smoke.py
python tests/package/session.py
python tests/package/uninstall.py
```

Build `private-systemd` with `cargo build --release --example private-systemd` from the source tree; it is not installed by the package.
