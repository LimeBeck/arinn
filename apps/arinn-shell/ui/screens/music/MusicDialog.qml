// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import "../.." as Ui

// The shell's confirm dialog, as the close flow draws it (handoff confirm
// layout, #756 M6): a scrim over the whole screen and a centred plate,
// 900 wide with radius 48, holding the question, an optional explanation
// and the actions. The screen owns focus and keys; the scrim only blocks
// the pointer, and a click outside the plate dismisses.
Item {
    id: root

    property string title: ""
    property string body: ""
    // The action buttons, in reading order.
    default property alias actions: actionRow.data

    signal dismissed()

    anchors.fill: parent
    visible: false

    Rectangle {
        objectName: "musicDialogScrim"
        anchors.fill: parent
        color: Ui.Theme.scrim
        MouseArea {
            anchors.fill: parent
            acceptedButtons: Qt.AllButtons
            onClicked: root.dismissed()
        }
    }
    Rectangle {
        objectName: "musicDialogPlate"
        anchors.centerIn: parent
        width: 900
        height: content.implicitHeight + 112
        radius: Ui.Theme.dialogRadius
        color: Ui.Theme.dialogSurface
        border.width: 1
        border.color: Ui.Theme.dialogLine
        MouseArea {
            anchors.fill: parent
            acceptedButtons: Qt.AllButtons
        }
        Column {
            id: content
            x: 56
            y: 56
            width: parent.width - 112
            spacing: 24
            Ui.UiText {
                objectName: "musicDialogTitle"
                width: parent.width
                text: root.title
                role: "title"
                font.pixelSize: 48
                wrapMode: Text.WordWrap
            }
            Ui.UiText {
                width: parent.width
                visible: text.length > 0
                text: root.body
                color: Ui.Theme.textSecondary
                wrapMode: Text.WordWrap
            }
            Row {
                id: actionRow
                spacing: 20
            }
        }
    }
}
