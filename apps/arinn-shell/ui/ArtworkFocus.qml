// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick

// Focus on artwork (ADR-0136, D-7): a 5 px Charcoal mat, then a 2 px rim
// outside it, and a faint warm bounce inside the bottom edge. The parent is
// the image. The mat and the rim paint outside it, so the parent must not
// clip them. The cast sits behind the image; see CastLight.
Item {
    id: artFocus
    property bool shown: false
    property real artworkRadius: Theme.stillRadius
    anchors.fill: parent
    z: 30

    Rectangle {
        objectName: "focusMat"
        anchors.fill: parent
        anchors.margins: -Theme.matThickness
        radius: artFocus.artworkRadius + Theme.matThickness
        color: "transparent"
        border.width: Theme.matThickness
        border.color: Theme.mat
        opacity: artFocus.shown ? 1 : 0
        Behavior on opacity { NumberAnimation { duration: Theme.focusDuration } }
    }
    FocusRing {
        anchors.fill: parent
        anchors.margins: -(Theme.matThickness + Theme.rimThickness)
        radius: artFocus.artworkRadius + Theme.matThickness + Theme.rimThickness
        shown: artFocus.shown
    }
    Rectangle {
        objectName: "focusBounce"
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        height: 28
        opacity: artFocus.shown ? 1 : 0
        z: 2
        gradient: Gradient {
            GradientStop { position: 0; color: "#00000000" }
            GradientStop { position: 1; color: Theme.bounce }
        }
        Behavior on opacity { NumberAnimation { duration: Theme.focusDuration } }
    }
}
