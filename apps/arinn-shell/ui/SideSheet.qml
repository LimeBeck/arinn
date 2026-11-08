// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick

// Matching right-hand sheets share framing; their feature focus scope owns
// header/content layout, navigation, scrolling and dismissal policy.
Item {
    id: root
    anchors.fill: parent
    property real panelWidth: 720
    property bool dismissible: true
    default property alias content: panel.data
    readonly property alias plate: panel
    readonly property alias scrim: scrim
    signal dismissed()

    Rectangle {
        id: scrim
        objectName: "sideSheetScrim"
        anchors.fill: parent
        color: Theme.scrim
        MouseArea {
            anchors.fill: parent
            acceptedButtons: Qt.AllButtons
            onClicked: if (root.dismissible) root.dismissed()
            onWheel: wheel => wheel.accepted = true
        }
    }
    Rectangle {
        id: panel
        objectName: "sideSheetPlate"
        x: parent.width - width
        width: root.panelWidth
        height: parent.height
        color: Theme.dialogSurface
        border.color: Theme.dialogLine
        MouseArea {
            anchors.fill: parent
            acceptedButtons: Qt.AllButtons
            onWheel: wheel => wheel.accepted = true
        }
    }
}
