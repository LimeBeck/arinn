# SPDX-FileCopyrightText: 2026 Arinn contributors
#
# SPDX-License-Identifier: GPL-3.0-or-later

"""Every Qt Quick module the shipped shell imports is a Debian dependency.

Debian splits the QML modules into separate packages. A screen importing a
module the package does not depend on makes Main.qml fail to load on a clean
install (review of #710: QtQuick.Controls).
"""

import pathlib
import re
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[2]
SHELL = ROOT / "apps/arinn-shell"
IMPORT = re.compile(r"^import\s+(Qt[A-Za-z.]*)", re.MULTILINE)


def debian_package(module: str) -> str:
    # QtQuick -> qml6-module-qtquick, QtQuick.Window -> qml6-module-qtquick-window
    return "qml6-module-" + module.lower().replace(".", "-")


def shipped_qml():
    # The development gallery is not installed.
    return [path for path in SHELL.rglob("*.qml") if "gallery" not in path.parts]


class QmlModulePackagingTest(unittest.TestCase):
    def test_debian_depends_on_every_imported_qt_quick_module(self):
        control = (ROOT / "packaging/debian/debian/control").read_text()
        depends = control.split("\nDepends:", 1)[1].split("\n\n", 1)[0]
        packages = set(re.findall(r"qml6-module-[a-z0-9-]+", depends))
        modules = {}
        for path in shipped_qml():
            for module in IMPORT.findall(path.read_text()):
                if module.startswith("QtQuick"):
                    modules.setdefault(module, path.relative_to(ROOT))
        self.assertIn("QtQuick", modules)
        for module, path in sorted(modules.items()):
            with self.subTest(module=module):
                self.assertIn(debian_package(module), packages,
                              f"{path} imports {module}")


if __name__ == "__main__":
    unittest.main()
