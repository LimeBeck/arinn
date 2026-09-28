// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick

Surface {
    id: root
    property string monogram: ""
    property bool running: false
    implicitWidth: 264
    implicitHeight: 132
    focusScale: 1.08
    contentItem: Item {
        Rectangle {
            x: 24; y: (parent.height - height) / 2
            width: 60; height: 60; radius: 18
            color: Qt.rgba(1, 1, 1, 0.1)
            UiText { anchors.centerIn: parent; text: root.monogram; role: "control" }
        }
        UiText {
            x: 100; width: parent.width - 120
            anchors.verticalCenter: parent.verticalCenter
            role: "control"; font.pixelSize: 28
            text: root.busy ? qsTr("Loading…") : root.text
            maximumLineCount: 2; elide: Text.ElideRight
        }
        Rectangle {
            anchors.top: parent.top; anchors.right: parent.right; anchors.margins: 16
            width: 12; height: 12; radius: 6; color: Theme.ember
            visible: root.running
        }
    }
}
