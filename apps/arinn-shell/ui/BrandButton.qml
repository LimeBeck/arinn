// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick

// Home's top-left menu button (handoff 01: mark 36 + "Arinn", no plate).
// At rest only the lockup shows; focus adds the plate and the ring (#564).
// The lockup starts `lockupInset` px inside the button, so a button placed at
// x 100 puts the mark at the handoff's x 120.
Surface {
    id: root
    readonly property int lockupInset: 20
    text: "Arinn"
    implicitWidth: lockup.implicitWidth + 2 * lockupInset
    implicitHeight: 64
    cornerRadius: height / 2
    focusScale: 1.02
    restingColor: "transparent"
    focusedColor: Theme.plateFocus
    background: Rectangle {
        objectName: "brandPlate"
        radius: root.cornerRadius
        color: root.activeFocus ? root.focusedColor : root.restingColor
        border.width: 1
        border.color: root.activeFocus ? Qt.rgba(1, 1, 1, 0.08) : "transparent"
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
