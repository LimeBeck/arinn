// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick

// Home's top-left menu button. At rest only the lockup shows. Focus adds the
// container plate and the rim (ADR-0136). The lockup starts `lockupInset` px
// inside the button, so a button placed at x 100 puts the mark at x 120.
Surface {
    id: root
    readonly property int lockupInset: 20
    text: "Arinn"
    implicitWidth: lockup.implicitWidth + 2 * lockupInset
    implicitHeight: 64
    shapeRole: "container"
    cornerRadius: height / 2
    focusScale: 1.02
    focusLift: 0
    liftFromBottom: false
    restingColor: "transparent"
    focusedColor: Theme.plateContainer
    background: Rectangle {
        objectName: "brandPlate"
        radius: root.cornerRadius
        color: root.activeFocus ? root.focusedColor : root.restingColor
        Behavior on color { ColorAnimation { duration: Theme.focusDuration } }
    }
    contentItem: Item {
        BrandLockup {
            id: lockup
            objectName: "headerBrand"
            x: root.lockupInset
            anchors.verticalCenter: parent.verticalCenter
            markSize: 36
        }
    }
}
