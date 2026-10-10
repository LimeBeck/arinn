# SPDX-FileCopyrightText: 2026 Arinn contributors
#
# SPDX-License-Identifier: GPL-3.0-or-later

"""The Arch package runs on the minimum supported Qt (#416, ADR-0104)."""

import contextlib
import io
import pathlib
import re
import subprocess
import sys
import tempfile
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "tests" / "package"))

import qt_floor  # noqa: E402

PKGBUILD = (ROOT / "packaging" / "arch" / "PKGBUILD").read_text()
CMAKE = (ROOT / "CMakeLists.txt").read_text()
DOCKERFILE = (ROOT / "ci" / "images" / "arch" / "Dockerfile").read_text()
PACKAGE = (ROOT / ".github" / "workflows" / "package.yml").read_text()

# Fake Qt Core libraries. The newer one is what Arch shipped on 2026-10-08:
# it adds a Qt_6.12 version tag, an rvalue overload the compiler picks for
# temporaries, and private API.
FLOOR_SCRIPT = """
Qt_6 { global: qvariant_from_const_ref; local: *; };
Qt_6.8 { global: qt_version_tag; } Qt_6;
"""
NEWER_SCRIPT = """
Qt_6 { global: qvariant_from_const_ref; qvariant_from_rvalue; local: *; };
Qt_6_PRIVATE_API { global: qt_private_hook; };
Qt_6.12 { global: qt_version_tag; } Qt_6;
"""
LIBRARY = """
const char qt_version_tag = 0;
int qvariant_from_const_ref(void) { return 1; }
int qvariant_from_rvalue(void) { return 2; }
int qt_private_hook(void) { return 3; }
"""
# Like qglobal.h: every Qt binary refers to qt_version_tag from .qtversion.
VERSION_TAG = """
extern const char qt_version_tag;
__attribute__((used, section(".qtversion"))) static const void *const version_tag = &qt_version_tag;
"""
FLOOR_PROGRAM = VERSION_TAG + """
int qvariant_from_const_ref(void);
int main(void) { return qvariant_from_const_ref(); }
"""
NEWER_PROGRAM = VERSION_TAG + """
int qvariant_from_rvalue(void);
int qt_private_hook(void);
int main(void) { return qvariant_from_rvalue() + qt_private_hook(); }
"""


def compile_library(directory: pathlib.Path, script: str) -> pathlib.Path:
    directory.mkdir(parents=True)
    (directory / "core.c").write_text(LIBRARY)
    (directory / "core.map").write_text(script)
    subprocess.run(["gcc", "-shared", "-fPIC", "-o", str(directory / "libQt6Core.so.6"),
                    "-Wl,-soname,libQt6Core.so.6", f"-Wl,--version-script={directory / 'core.map'}",
                    str(directory / "core.c")], check=True, capture_output=True)
    return directory


def compile_program(root: pathlib.Path, library: pathlib.Path, source: str, *flags: str) -> None:
    (root / "usr" / "bin").mkdir(parents=True)
    (root / "main.c").write_text(source)
    subprocess.run(["gcc", "-o", str(root / "usr" / "bin" / "arinn-shell"), str(root / "main.c"),
                    str(library / "libQt6Core.so.6"), *flags], check=True, capture_output=True)
    (root / "main.c").unlink()
    (root / "usr" / "share").mkdir()
    (root / "usr" / "share" / "README").write_text("not an ELF file\n")


def run(*args: str) -> tuple[int, str]:
    output = io.StringIO()
    with contextlib.redirect_stdout(output), contextlib.redirect_stderr(output):
        code = qt_floor.main(list(args))
    return code, output.getvalue()


class ElfCheckTest(unittest.TestCase):
    def setUp(self):
        self.folder = tempfile.TemporaryDirectory()
        self.addCleanup(self.folder.cleanup)
        work = pathlib.Path(self.folder.name)
        self.floor = compile_library(work / "floor", FLOOR_SCRIPT)
        self.newer = compile_library(work / "newer", NEWER_SCRIPT)
        self.work = work

    def test_binary_built_against_a_newer_qt_fails(self):
        # Regression: the 2026-10-08 snapshot imported qt_version_tag@Qt_6.12
        # and rvalue overloads missing from older Qt, and failed at start.
        package = self.work / "newer-package"
        compile_program(package, self.newer, NEWER_PROGRAM)
        code, output = run("--floor", "6.8", "--qt-lib-dir", str(self.floor), str(package))
        self.assertEqual(code, 1, output)
        self.assertIn("qt_version_tag@Qt_6.12: Qt version tag above the floor Qt_6.8", output)
        self.assertIn("qt_private_hook@Qt_6_PRIVATE_API: private Qt API", output)
        self.assertIn("qvariant_from_rvalue@Qt_6: not exported by the floor Qt libraries", output)

    def test_tags_alone_are_checked_without_floor_libraries(self):
        package = self.work / "newer-package"
        compile_program(package, self.newer, NEWER_PROGRAM)
        code, output = run("--floor", "6.8", str(package))
        self.assertEqual(code, 1, output)
        self.assertIn("Qt_6.12", output)
        self.assertNotIn("not exported", output)

    def test_binary_built_against_the_floor_passes(self):
        package = self.work / "floor-package"
        compile_program(package, self.floor, FLOOR_PROGRAM)
        code, output = run("--floor", "6.8", "--qt-lib-dir", str(self.floor), str(package))
        self.assertEqual(code, 0, output)
        self.assertIn("PASS: 1 ELF files, 2 Qt symbol imports", output)

    def test_runpath_into_the_sdk_fails(self):
        package = self.work / "rpath-package"
        compile_program(package, self.floor, FLOOR_PROGRAM, f"-Wl,-rpath,{self.floor}")
        code, output = run("--floor", "6.8", "--qt-lib-dir", str(self.floor), str(package))
        self.assertEqual(code, 1, output)
        self.assertIn(f"usr/bin/arinn-shell: RPATH/RUNPATH {self.floor}", output)

    def test_a_tree_without_elf_files_fails(self):
        empty = self.work / "empty"
        empty.mkdir()
        code, output = run(str(empty))
        self.assertEqual(code, 1)
        self.assertIn("no ELF files", output)


class ReadelfParsingTest(unittest.TestCase):
    def test_symbol_lines(self):
        text = """
Symbol table '.dynsym' contains 3 entries:
   Num:    Value          Size Type    Bind   Vis      Ndx Name
     0: 0000000000000000     0 NOTYPE  LOCAL  DEFAULT  UND
   610: 0000000000000000     0 OBJECT  GLOBAL DEFAULT  UND qt_version_tag@Qt_6.12 (32)
    12: 0000000000000000     0 FUNC    GLOBAL DEFAULT  UND _ZN8QVariantC1EO7QString@Qt_6 (5)
    40: 00000000001a2b30   120 FUNC    GLOBAL DEFAULT   14 _ZN7QStringC1Ev@@Qt_6
    41: 00000000001a2b30 0x1000 OBJECT GLOBAL DEFAULT   22 qt_version_tag@Qt_6.0
    13: 0000000000000000     0 FUNC    GLOBAL DEFAULT  UND malloc@GLIBC_2.2.5 (2)
"""
        self.assertEqual(qt_floor.parse_symbols(text), [
            ("qt_version_tag", "Qt_6.12", False),
            ("_ZN8QVariantC1EO7QString", "Qt_6", False),
            ("_ZN7QStringC1Ev", "Qt_6", True),
            ("qt_version_tag", "Qt_6.0", True),
            ("malloc", "GLIBC_2.2.5", False),
        ])

    def test_tag_problems(self):
        floor = (6, 8)
        self.assertIsNone(qt_floor.tag_problem("Qt_6", floor))
        self.assertIsNone(qt_floor.tag_problem("Qt_6.8", floor))
        self.assertIsNone(qt_floor.tag_problem("Qt_6.2", floor))
        self.assertIsNotNone(qt_floor.tag_problem("Qt_6.9", floor))
        self.assertIsNotNone(qt_floor.tag_problem("Qt_6.12", floor))
        self.assertIsNotNone(qt_floor.tag_problem("Qt_6_PRIVATE_API", floor))


class FloorConsistencyTest(unittest.TestCase):
    """CMake, the PKGBUILD, the CI image and the check name the same floor."""

    def test_cmake_requires_the_floor(self):
        # The shell and the standalone performance harness.
        for path in (ROOT / "CMakeLists.txt", ROOT / "scripts" / "perf" / "CMakeLists.txt"):
            with self.subTest(path=path.relative_to(ROOT)):
                self.assertRegex(path.read_text(), rf"find_package\(Qt6 {re.escape(qt_floor.FLOOR)} REQUIRED")

    def test_build_requirements_name_the_floor(self):
        # Contributors who follow the documented setup can configure the build.
        for name in ("CONTRIBUTING.md", "README.md", "README.ru.md",
                     "docs/architecture/phase-0-runbook.md", "docs/architecture/performance-harness.md"):
            with self.subTest(document=name):
                text = (ROOT / name).read_text()
                self.assertIn(f"≥ {qt_floor.FLOOR}", text)
                self.assertNotRegex(text, r"Qt[^.\n]*≥ 6\.[0-7]\b")

    def test_pkgbuild_depends_on_the_floor(self):
        depends = re.search(r"^depends=\((.*)\)$", PKGBUILD, re.M).group(1)
        for name in ("qt6-base", "qt6-declarative", "qt6-svg"):
            with self.subTest(package=name):
                self.assertIn(f"'{name}>={qt_floor.FLOOR}'", depends)
        self.assertNotRegex(depends, r"'qt6-[a-z]+'")

    def test_ci_image_installs_a_floor_sdk(self):
        versions = set(re.findall(r"^ARG QT_VERSION=(\S+)$", DOCKERFILE, re.M))
        self.assertEqual(len(versions), 1, versions)
        version = versions.pop()
        self.assertEqual(version.rsplit(".", 1)[0], qt_floor.FLOOR)
        self.assertIn("ENV ARINN_QT_FLOOR_PREFIX=/opt/qt/${QT_VERSION}/gcc_64", DOCKERFILE)

    def test_package_builds_and_checks_against_the_floor_sdk(self):
        build = re.search(r"^build\(\) \{\n(.*?)^\}", PKGBUILD, re.M | re.S).group(1)
        self.assertIn('${ARINN_QT_FLOOR_PREFIX:+"-DCMAKE_PREFIX_PATH=$ARINN_QT_FLOOR_PREFIX"}', build)
        self.assertIn("-DCMAKE_SKIP_BUILD_RPATH=ON", build)
        self.assertIn("ARINN_QT_FLOOR_PREFIX='$ARINN_QT_FLOOR_PREFIX' makepkg", PACKAGE)
        self.assertIn('python tests/package/qt_floor.py --qt-lib-dir "$ARINN_QT_FLOOR_PREFIX/lib" pkgroot',
                      PACKAGE)
        self.assertIn('export LD_LIBRARY_PATH="$ARINN_QT_FLOOR_PREFIX/lib"', PACKAGE)
        # The floor check and the floor-Qt smoke run after makepkg.
        self.assertLess(PACKAGE.index("makepkg --noconfirm"), PACKAGE.index("qt_floor.py"))
        self.assertLess(PACKAGE.index("qt_floor.py"), PACKAGE.index("LD_LIBRARY_PATH="))


if __name__ == "__main__":
    unittest.main()
