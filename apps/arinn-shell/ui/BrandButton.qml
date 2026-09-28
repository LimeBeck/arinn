// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick

Surface {
    text: "Arinn"
    implicitWidth: Math.max(180, lockup.implicitWidth + 40)
    implicitHeight: 64
    cornerRadius: height / 2
    focusScale: 1.02
    focusedColor: Theme.plateFocus
    contentItem: Item {
        BrandLockup {
            id: lockup
            objectName: "headerBrand"
            anchors.centerIn: parent
        }
    }
}
