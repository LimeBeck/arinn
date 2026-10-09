# SPDX-FileCopyrightText: 2026 Arinn contributors
#
# SPDX-License-Identifier: GPL-3.0-or-later

"""CI jobs run in prebuilt images from ci/images/** (#413, ADR-0103)."""

import pathlib
import re
import unittest

from test_ci_build_scope import evaluate, jobs, value

ROOT = pathlib.Path(__file__).resolve().parents[2]
WORKFLOWS = ROOT / ".github" / "workflows"
CI = (WORKFLOWS / "ci.yml").read_text()
PACKAGE = (WORKFLOWS / "package.yml").read_text()
IMAGES = (WORKFLOWS / "ci-images.yml").read_text()
RELEASE = (WORKFLOWS / "release.yml").read_text()
ARCH = (ROOT / "ci" / "images" / "arch" / "Dockerfile").read_text()
UBUNTU = (ROOT / "ci" / "images" / "ubuntu" / "Dockerfile").read_text()


def stage(dockerfile: str, name: str) -> str:
    match = re.search(rf"^FROM \S+ AS {name}\n(.*?)(?=^FROM |\Z)", dockerfile, re.M | re.S)
    assert match, name
    return match.group(1)


class ImageWorkflowTest(unittest.TestCase):
    def test_tags_follow_the_image_sources(self):
        # A commit runs on the images built from its own Dockerfiles.
        self.assertIn('tree="$(git -C ci-images-source rev-parse "HEAD:ci/images/$distro")"', IMAGES)
        self.assertIn('image="ghcr.io/$owner/arinn-ci-$distro:$target-${tree:0:16}"', IMAGES)
        self.assertIn("for distro in arch ubuntu; do", IMAGES)
        self.assertIn("for target in smoke build; do", IMAGES)
        for output in ("arch_build", "arch_smoke", "ubuntu_build", "ubuntu_smoke"):
            with self.subTest(output=output):
                self.assertIn(f"value: ${{{{ jobs.images.outputs.{output} }}}}", IMAGES)
                self.assertIn(f"{output}: ${{{{ steps.images.outputs.{output} }}}}", IMAGES)

    def test_ci_reuses_tags_and_scheduled_runs_rebuild(self):
        reuse = value(IMAGES, "REUSE")
        body = re.fullmatch(r"\$\{\{(.*)\}\}", reuse).group(1)

        def evaluated(given):
            text = body.replace("inputs.reuse", repr(given)).replace("&&", " and ").replace("||", " or ")
            return eval(text, {"__builtins__": {}})  # noqa: S307 - fixed workflow text

        # Called from CI the input defaults to true; the weekly and manual
        # runs of ci-images.yml have no inputs.
        self.assertEqual(evaluated(True), "true")
        self.assertEqual(evaluated(None), "false")
        self.assertIn('if [ "$REUSE" = true ] && docker manifest inspect "$image"', IMAGES)
        self.assertIn("docker build --pull --no-cache", IMAGES)
        self.assertRegex(IMAGES, r"schedule:\n\s+- cron:")

    def test_ghcr_token_stays_out_of_the_runner_home(self):
        self.assertIn('DOCKER_CONFIG=$RUNNER_TEMP/ci-images-docker', IMAGES)
        self.assertLess(IMAGES.index("DOCKER_CONFIG="), IMAGES.index("docker login"))
        self.assertIn('rm -rf "$RUNNER_TEMP/ci-images-docker"', IMAGES)

    def test_ci_builds_images_before_the_jobs_that_use_them(self):
        ci = jobs()
        self.assertIn("uses: ./.github/workflows/ci-images.yml", ci["images"])
        self.assertIn("packages: write", ci["images"])
        for name in ("build", "package"):
            with self.subTest(job=name):
                self.assertRegex(ci[name], r"needs: \[scope, images\]")
        for key in ("arch_build_image", "arch_smoke_image", "deb_build_image", "deb_smoke_image"):
            with self.subTest(input=key):
                self.assertIn(f"{key}: ${{{{ needs.images.outputs.", ci["package"])
                self.assertIn(f"      {key}:\n", PACKAGE)
        # release.yml calls CI, which needs to push new images.
        self.assertRegex(RELEASE, r"ci:\n(?:.*\n)*?    permissions:\n      contents: read\n      packages: write\n")

    def test_no_job_installs_into_a_bare_distribution_image(self):
        for name, text in (("ci.yml", CI), ("package.yml", PACKAGE)):
            with self.subTest(workflow=name):
                self.assertNotIn("archlinux:latest", text)
                self.assertNotIn("ubuntu:25.04", text)
                self.assertNotIn("apt-get install -y --no-install-recommends", text)
                self.assertNotIn("useradd", text)
        # Every ghcr.io image is pulled with the job's token, so the images
        # work whether the packages are public or private.
        for text in (CI, PACKAGE):
            for image in re.findall(r"^      image: (\$\{\{ (?:needs\.images\.outputs|inputs)\..*\}\})$", text, re.M):
                block = text[text.index(f"image: {image}"):]
                with self.subTest(image=image):
                    self.assertRegex(block, r"^image: .*\n      credentials:\n        username: \$\{\{ github.actor \}\}\n"
                                            r"        password: \$\{\{ secrets.GITHUB_TOKEN \}\}\n")

    def test_only_the_nightly_upgrades_the_build_images(self):
        # ADR-0033: the nightly still runs against the latest Arch packages.
        for text in (jobs()["build"], jobs(PACKAGE)["build"]):
            step = text[text.index("Update to the latest Arch Linux"):]
            condition = value(step, "if")
            for event, expected in (("schedule", True), ("push", False), ("pull_request", False),
                                    ("workflow_dispatch", False)):
                with self.subTest(event=event):
                    self.assertEqual(evaluate(condition, event_name=event), expected)
            self.assertRegex(step, re.compile(r"^\s+run: pacman -Syu --noconfirm$", re.M))

    def test_install_tests_start_from_clean_images(self):
        package = jobs(PACKAGE)
        self.assertIn("image: ${{ inputs.arch_smoke_image }}", package["smoke"])
        self.assertIn("image: ${{ inputs.deb_smoke_image }}", package["smoke-deb"])
        self.assertIn("image: ${{ inputs.arch_build_image }}", package["build"])
        self.assertIn("image: ${{ inputs.deb_build_image }}", package["build-deb"])
        # The smoke images hold only Python and D-Bus, so the package manager
        # resolves every dependency the package declares.
        self.assertRegex(stage(ARCH, "smoke"), r"pacman -Syu --noconfirm --needed python dbus \\\n")
        self.assertRegex(stage(UBUNTU, "smoke"), r"apt-get install -y --no-install-recommends python3 dbus \\\n")
        self.assertIn("pacman -Syu --noconfirm\n          pacman -U --noconfirm pkg/*.pkg.tar.zst", package["smoke"])


class ImageContentTest(unittest.TestCase):
    def test_arch_build_image_has_every_build_dependency(self):
        build = stage(ARCH, "build")
        for name in ("base-devel", "git", "rust", "cmake", "ninja", "ccache", "qt6-base", "qt6-declarative",
                     "qt6-svg", "qt6-tools", "curl", "libunwind", "openssl", "nodejs", "systemd",
                     "xorg-server-xvfb"):
            with self.subTest(package=name):
                self.assertRegex(build, rf"(?<![\w-]){re.escape(name)}(?![\w-])")
        self.assertIn("useradd --create-home builder", build)
        self.assertIn("COPY --from=qt-sdk", build)

    def test_qt_sdk_is_hash_locked(self):
        sdk = stage(ARCH, "qt-sdk")
        self.assertIn("pip install --no-cache-dir --require-hashes -r /qt/aqtinstall.txt", sdk)
        self.assertIn("diff -u - /qt/actual.sha256", sdk)
        pins = (ROOT / "ci" / "images" / "arch" / "qt-archives.sha256").read_text()
        archives = re.findall(r"^[0-9a-f]{64}  \./(\S+)\.7z$", pins, re.M)
        self.assertEqual(len(archives), len([line for line in pins.splitlines()
                                             if line and not line.startswith("#")]))
        for name in ("qtbase", "qtdeclarative", "qtsvg", "qttools", "icu"):
            with self.subTest(archive=name):
                self.assertTrue(any(archive.startswith(name + "-") for archive in archives), archives)
        requirements = (ROOT / "ci" / "images" / "arch" / "aqtinstall.txt").read_text()
        pinned = re.findall(r"^([A-Za-z0-9_.-]+)==\S+ \\$", requirements, re.M)
        self.assertIn("aqtinstall", pinned)
        locked = requirements[re.search(r"^aqtinstall==", requirements, re.M).start():]
        for block in re.split(r"\n(?=[A-Za-z0-9_.-]+==)", locked):
            with self.subTest(requirement=block.split()[0]):
                self.assertIn("--hash=sha256:", block)

    def test_ubuntu_build_image_has_every_build_dependency(self):
        build = stage(UBUNTU, "build")
        for name in ("debhelper-compat", "qt6-base-dev", "qt6-declarative-dev", "qt6-svg-dev",
                     "qt6-tools-dev", "libcurl4-openssl-dev", "dbus-daemon", "devscripts"):
            with self.subTest(package=name):
                self.assertIn(name, build)


if __name__ == "__main__":
    unittest.main()
