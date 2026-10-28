// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick

// The pill inside a text field that opens and hides the on-screen keyboard
// (spec §6.2, handoff KB-1…8). It is the only way to open the panel.
Surface {
    id: root
    property bool panelOpen: false
    text: panelOpen ? qsTr("Hide") : qsTr("Keyboard")
    implicitWidth: content.implicitWidth + 54
    implicitHeight: 84
    cornerRadius: height / 2
    focusScale: 1.02
    restingColor: Qt.rgba(1, 1, 1, 0.10)
    focusedColor: Theme.textPrimary
    Accessible.name: panelOpen ? qsTr("Hide the on-screen keyboard") : qsTr("Open the on-screen keyboard")
    contentItem: Item {
        Row {
            id: content
            anchors.centerIn: parent
            spacing: 12
            LineIcon {
                width: 38
                height: 38
                anchors.verticalCenter: parent.verticalCenter
                name: root.panelOpen ? "keyboard_hide" : "keyboard"
                color: root.activeFocus ? Theme.ink : Theme.textPrimary
            }
            UiText {
                anchors.verticalCenter: parent.verticalCenter
                text: root.text
                font.pixelSize: 28
                font.weight: 600
                wrapMode: Text.NoWrap
                color: root.activeFocus ? Theme.ink : Theme.textPrimary
            }
        }
    }
}
