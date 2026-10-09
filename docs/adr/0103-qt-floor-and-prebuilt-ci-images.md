# ADR-0103: Minimum supported Qt 6.8 and prebuilt CI images

- **Status:** Accepted
- **Date:** 2026-10-09
- **Deciders:** project owner. On 2026-10-09 the owner merged #416 and #413 into one task and chose the Qt 6.8 LTS floor, an SDK in the CI image (approach A), public images on ghcr.io and prebuilt images for every Arch Linux and Ubuntu job. Mechanism under delegated authority ([ADR-0037](0037-agent-autonomy-and-owner-gated-decisions.md)).
- **Amends / Supersedes:** amends [ADR-0033](0033-ci-cd-and-releases.md) (the Arch package is no longer built against the current Arch Qt; jobs no longer install their dependencies), [ADR-0049](0049-ci-minutes.md) (one job runs on the runner host) and [ADR-0091](0091-debian-package-ci-artifact.md) (the Qt minimum is 6.8; the Ubuntu jobs use prebuilt images)
- **Amended by / Superseded by:** —
- **Specification:** [§17.1](../specifications/arinn-specification.md#171-continuous-integration-and-delivery), [§18](../specifications/arinn-specification.md#18-packaging-distribution-and-licensing)

## Context

**The package needed the newest Qt.** The Arch package was built in
`archlinux:latest`, so `arinn-shell` linked against whatever Qt the image had
that day. The 2026-10-08 snapshot (`0.1.0.r223`, also `r225`) was installed
on a host with Qt 6.11.2. pacman accepted it, because `depends` named
`qt6-base` without a version. The shell then failed at every start (#416):

```text
/usr/lib/libQt6Core.so.6: version `Qt_6.12' not found (required by /usr/bin/arinn-shell)
```

The source needs nothing newer than the `find_package(Qt6 6.6)` floor in
`CMakeLists.txt`, but the binary does. Qt's ELF symbol versioning adds a
`qt_version_tag@Qt_6.N` import for the Qt the binary was built against. The
compiler also picks overloads that the newer Qt added, such as rvalue
constructors for temporaries. Against Qt 6.8.3, the `r225` shell imports 11
such symbols: `QVariant(QString&&)`, `QString::arg_impl`, the `QBasicMutex`
futex helpers and others. Defining `QT_NO_VERSION_TAGGING` would only turn
the clear error into `undefined symbol`. In short, a package's runtime Qt
floor equals the Qt it was built against.

**Every job installed its dependencies (#413).** Each Arch and Ubuntu job ran
`pacman -Syu` or `apt-get install` into a bare distribution image. This took
35–60 s per job on the self-hosted runner and depended on reachable mirrors.

## Decision

### Minimum supported Qt

- **Arinn supports Qt 6.8 (LTS) and newer.** `CMakeLists.txt` requires
  `find_package(Qt6 6.8 …)`.
- **The Arch package declares the floor:** `qt6-base>=6.8`,
  `qt6-declarative>=6.8`, `qt6-svg>=6.8`. pacman refuses to install the package
  on an older Qt instead of the shell failing at start.
- **CI builds the Arch package's shell against Qt 6.8.** `PKGBUILD`'s
  `build()` passes `ARINN_QT_FLOOR_PREFIX` to CMake as `CMAKE_PREFIX_PATH`,
  and the package build job sets it to the Qt SDK in the Arch build image.
  - At run time the shell loads the system's `libQt6*.so.6` by soname, which
    works on any Qt ≥ 6.8.
  - Build RPATHs are off, so the SDK path never reaches the package.
  - Without the variable, a local build uses the system Qt as before.
- **Everything else in the package still builds on current Arch Linux:** Rust,
  glibc, glib2 and the other C dependencies.
- **The CI build job keeps the newest Qt.** "Rust, Qt shell and integration"
  builds and tests the shell against the current Arch system Qt, the Qt most
  users run.
- **The `.deb` is unchanged.** It builds against Ubuntu 25.04's Qt 6.8.3,
  which is the floor. `dpkg-shlibdeps` derives its versioned dependency.

### Verification in the package build

After `makepkg`, the Arch package build job runs two checks.

**`tests/package/qt_floor.py`** checks every ELF file in the extracted package:

- it imports no `Qt_6.N` version tag above `Qt_6.8` and no `Qt_6_PRIVATE_API`
  symbol;
- every Qt symbol it imports is exported by the SDK's Qt 6.8 libraries;
- it has no RPATH or RUNPATH.

**A floor-Qt smoke run** installs the package into the throwaway build
container. It then runs `tests/package/smoke.py` with `LD_LIBRARY_PATH` set to
the SDK's `lib` directory, so the packaged shell loads Qt 6.8 instead of the
system Qt. `smoke.py` now waits until the shell owns `org.arinn.Shell1`.

### Prebuilt CI images

- **Image sources.** `ci/images/arch/Dockerfile` and
  `ci/images/ubuntu/Dockerfile` each define two targets:
  - `smoke`: only Python and D-Bus. Install tests start from a clean system,
    so the package manager still resolves every declared dependency.
  - `build`: every dependency of the CI build job and the package build. The
    Arch build target also holds the builder user and the Qt 6.8 SDK in
    `/opt/qt/<version>/gcc_64` (`ARINN_QT_FLOOR_PREFIX`).
- **The Qt SDK** is the official Qt 6.8.3 Linux binaries (`qtbase`,
  `qtdeclarative`, `qtsvg`, `qttools`, `icu`), downloaded in a separate stage:
  - aqtinstall is pinned with hashes for every distribution file
    (`ci/images/arch/aqtinstall.txt`); aqtinstall verifies each archive against
    the Qt repository's SHA-256;
  - `ci/images/arch/qt-archives.sha256` pins those archives as well. The image
    build fails when the downloaded set differs and prints the actual list, so
    a Qt version bump updates the file.
- **Registry and tags.** Images are published as
  `ghcr.io/<owner>/arinn-ci-<arch|ubuntu>:<target>-<tree>`, where `<tree>` is
  the first 16 hex digits of the Git tree hash of `ci/images/<distro>`.
  - A commit always runs on the images built from its own Dockerfiles, and a
    pull request that changes a Dockerfile tests the new image.
  - The packages are public. Jobs still pull with the job's `GITHUB_TOKEN`, so
    they also work while a package is private.
- **The build workflow, `.github/workflows/ci-images.yml`:**
  - CI calls it first. It builds and pushes only the tags that do not exist
    yet; everything else waits on it.
  - A weekly run and manual runs rebuild every tag in place from the current
    base image and packages.
  - It prunes the runner's copies of these images that are older than two
    weeks.
- **One job runs on the runner host.** The image job drives the host's Docker
  and runs on the host, not in a container. It checks out only `ci/images`
  into its own directory, which no container job uses, and keeps its registry
  login in a temporary Docker configuration directory. Every other job still
  runs in a container ([ADR-0049](0049-ci-minutes.md)).
- **Jobs that use the images:**
  - the CI build job and the Arch package build: Arch `build`;
  - the Arch install test: Arch `smoke`;
  - the Debian package build: Ubuntu `build`;
  - the Debian install test: Ubuntu `smoke`.
- **Package upgrades inside jobs:**
  - The nightly runs `pacman -Syu` in both Arch build jobs, so it still tests
    the latest Arch Linux, Qt and Rust ([ADR-0033](0033-ci-cd-and-releases.md)).
  - The Arch install test upgrades its image before `pacman -U`, because
    pacman cannot mix a week-old package set with current dependencies.
  - The Debian install test runs `apt-get update`, as before.
  - The rustup toolchain of the Debian build stays a job step cached on the
    runner ([ADR-0101](0101-host-local-ci-caches.md)).

## Consequences

**Packages and Qt:**

- Arch packages run on every Arch system whose Qt is 6.8 or newer. pacman
  rejects them on an older Qt.
- Code that needs a Qt API newer than 6.8 now fails the Arch and Debian
  package builds. Pull requests build only against the newest Qt and never
  build the packages ([ADR-0102](0102-packages-built-for-master-only.md)), so
  such a break shows on `master` or the nightly unless the author runs CI
  manually on the branch.
- Raising the floor is an owner decision. It changes `CMakeLists.txt`,
  `PKGBUILD`, `QT_VERSION` and `qt-archives.sha256` in the Arch Dockerfile,
  and `FLOOR` in `tests/package/qt_floor.py`; `tests/ci/test_qt_floor.py`
  keeps them in step.

**CI images:**

- Jobs no longer install their dependencies. The Arch image is at most a week
  old outside the nightly.
- Building an image needs download.qt.io, PyPI, the distribution mirrors and
  ghcr.io. Ordinary CI runs need only ghcr.io.
- A pull request from a fork cannot push a new image. It works as long as its
  Dockerfiles match existing tags.
- After the first publication the owner sets the `arinn-ci-arch` and
  `arinn-ci-ubuntu` packages to public on GitHub.

## Alternatives considered

- **Floor Qt 6.6 (the previous CMake minimum):** rejected. Nothing built or
  tested against it, and Qt 6.6 is out of upstream support.
- **Floor at the current Arch Qt minus one minor:** rejected. The floor, the
  image and `depends` would move every few months.
- **Build the whole package in an Arch Linux Archive snapshot from the Qt 6.8
  era:** rejected. It also pins glibc, glib2 and Rust to that date. `PKGBUILD`
  already requires `glib2>=2.86`, which such a snapshot does not have.
- **Downgrade only Qt inside `archlinux:latest`:** rejected. It is a partial
  downgrade, and Qt's ICU and other sonames diverge from the rest of the
  system.
- **`QT_NO_VERSION_TAGGING` alone:** rejected. The rvalue overloads would
  still be missing at run time.
- **Install test in an Arch image whose system Qt is 6.8:** not feasible. An
  Arch Linux Archive system from the Qt 6.8 era has an older glibc and glib2
  than the package needs. The floor-Qt smoke run with the SDK libraries and
  the symbol-coverage check replace it.
- **One mutable `:latest` image per distribution, rebuilt on `master`:**
  rejected. A pull request that changes a Dockerfile would test the old
  image, and the first pull request could not run at all.
- **Build Qt 6.8 from source in the image:** rejected. It is slow on the
  5-vCPU runner and risks build failures with a newer GCC.

## Verification

- `tests/ci/test_qt_floor.py` compiles fake Qt libraries and binaries with
  gcc:
  - a binary built against a "newer Qt" with `qt_version_tag@Qt_6.12`, an
    rvalue overload and private API fails `qt_floor.py`;
  - a binary built against the floor passes;
  - an RPATH into the SDK fails.

  It also checks that `CMakeLists.txt`, `PKGBUILD`, the Arch Dockerfile and
  `qt_floor.py` name the same floor, and that the package job builds and
  checks against the SDK.
- `tests/ci/test_ci_images.py` checks:
  - the image tags, the reuse and rebuild rules and the token handling;
  - that no Arch or Ubuntu job installs into a bare distribution image;
  - that only the nightly upgrades the build images;
  - that the install tests start from the `smoke` images.
- **Regression evidence:** `qt_floor.py` fails on the `r225` snapshot with
  `qt_version_tag@Qt_6.12` and 11 symbols missing from Ubuntu 25.04's Qt 6.8.3
  libraries.
- **Not tested:** a `pacman -U` refusal on a system whose Qt is older than
  6.8. It follows from the versioned `depends`, which
  `tests/ci/test_qt_floor.py` checks.
- **CI evidence**, recorded in the pull request:
  - the *Package* job on `master` or a manual run passes the floor check and
    the floor-Qt smoke run;
  - the images publish to ghcr.io.
