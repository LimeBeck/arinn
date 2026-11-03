# SPDX-FileCopyrightText: 2026 Arinn contributors
#
# SPDX-License-Identifier: GPL-3.0-or-later

"""Package smoke must receive the XFCE lock-provider stub (#749).

The install images have no Cargo tree. The stub is a test helper, built in
the package jobs and passed as ARINN_XFCE_SCREENSAVER_STUB. It is not
installed, and enabling test-support must not replace the binaries that are.
"""
import pathlib
import re
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[2]
PKGBUILD = (ROOT / "packaging/arch/PKGBUILD").read_text()
PACKAGE = (ROOT / ".github/workflows/package.yml").read_text()
DEB_RULES = (ROOT / "packaging/debian/debian/rules").read_text()
STUB_BUILD = (
    "cargo build --frozen --release -p arinn-platform-x11 "
    "--features test-support --bin arinn-xfce-test-screensaver\n"
)
RELEASE_BUILD = "cargo build --frozen --release\n"
STUB_ENV = (
    "ARINN_XFCE_SCREENSAVER_STUB: ${{ github.workspace }}/pkg/arinn-xfce-test-screensaver"
)


def pkgbuild_function(name: str) -> str:
    match = re.search(rf"^{name}\(\) \{{\n(.*?)^\}}", PKGBUILD, re.M | re.S)
    assert match, name
    return match.group(1)


def jobs() -> dict[str, str]:
    section = PACKAGE[PACKAGE.index("\njobs:\n"):]
    names = re.findall(r"^  (\w[\w-]*):\n", section, re.M)
    bodies = re.split(r"^  \w[\w-]*:\n", section, flags=re.M)[1:]
    return dict(zip(names, bodies))


class XfceScreensaverStubPackageTest(unittest.TestCase):
    def test_arch_builds_the_stub_before_the_installed_release_binaries(self):
        build = pkgbuild_function("build")
        self.assertIn(STUB_BUILD, build)
        self.assertLess(build.index(STUB_BUILD), build.index(RELEASE_BUILD))
        self.assertNotIn("arinn-xfce-test-screensaver", pkgbuild_function("package"))

    def test_debian_builds_the_stub_before_the_installed_release_binaries(self):
        build = re.search(r"^override_dh_auto_build:\n(.*?)\n\n", DEB_RULES, re.M | re.S).group(1)
        self.assertIn(STUB_BUILD, build + "\n")
        self.assertLess((build + "\n").index(STUB_BUILD), (build + "\n").index(RELEASE_BUILD))
        install = re.search(r"^override_dh_auto_install:\n(.*?)\n\n", DEB_RULES, re.M | re.S).group(1)
        self.assertNotIn("arinn-xfce-test-screensaver", install)

    def test_smoke_jobs_receive_the_built_stub(self):
        built = jobs()
        self.assertIn(
            'cp "$target/release/arinn-xfce-test-screensaver" pkg/arinn-xfce-test-screensaver',
            built["build"],
        )
        self.assertIn("            pkg/arinn-xfce-test-screensaver\n", PACKAGE)
        script = (ROOT / "packaging/debian/build-deb.sh").read_text()
        self.assertIn(
            'cp "${target_dir}/release/arinn-xfce-test-screensaver" '
            '"$root/packaging/debian/arinn-xfce-test-screensaver"',
            script,
        )
        self.assertIn("            packaging/debian/arinn-xfce-test-screensaver\n", PACKAGE)
        self.assertIn(
            "packaging/debian/arinn-xfce-test-screensaver\n",
            (ROOT / ".gitignore").read_text(),
        )
        for name in ("smoke", "smoke-deb"):
            job = built[name]
            with self.subTest(job=name):
                self.assertIn(STUB_ENV, job)
                chmod = 'chmod +x "$ARINN_XFCE_SCREENSAVER_STUB"'
                self.assertLess(job.index(chmod), job.index("tests/package/session.py"))


if __name__ == "__main__":
    unittest.main()
