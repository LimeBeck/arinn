// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick

Surface {
    id: root
    identityGlow: true
    property string monogram: text.trim().slice(0, 1).toUpperCase()
    property url iconSource: ""
    readonly property bool iconReady: icon.status === Image.Ready
    property string subtitle: ""
    property bool running: false
    property bool serviceOpen: false
    property bool unavailable: false
    implicitWidth: 264
    implicitHeight: 132
    focusScale: 1.08
    opacity: !enabled || unavailable ? 0.4 : 1
    contentItem: Item {
        Rectangle {
            x: 24; y: (parent.height - height) / 2
            width: 60; height: 60; radius: 18
            color: root.unavailable ? Qt.rgba(1, 1, 1, 0.06) : root.accentColor
            Image {
                id: icon
                objectName: "applicationIcon"
                anchors.centerIn: parent
                width: 44; height: 44
                sourceSize: Qt.size(128, 128)
                source: root.iconSource
                asynchronous: true
                fillMode: Image.PreserveAspectFit
                visible: status === Image.Ready
            }
            UiText {
                objectName: "applicationFallback"
                visible: !root.iconReady
                anchors.centerIn: parent
                text: root.monogram
                role: "control"
                color: root.unavailable ? Theme.textHint : Theme.textPrimary
            }
        }
        Column {
            x: 100; width: parent.width - 120
            anchors.verticalCenter: parent.verticalCenter
            spacing: 4
            UiText {
                width: parent.width
                role: "control"; font.pixelSize: 28
                text: root.busy ? qsTr("Loading…") : root.text
                maximumLineCount: root.subtitle.length ? 1 : 2
                elide: Text.ElideRight
                color: root.unavailable ? Theme.textHint : Theme.textPrimary
            }
            UiText {
                width: parent.width
                visible: root.subtitle.length > 0
                text: root.subtitle
                role: "caption"
                color: Theme.textHint
                maximumLineCount: 1
                elide: Text.ElideRight
            }
        }
        Rectangle {
            objectName: "emberDot"
            anchors.top: parent.top; anchors.right: parent.right; anchors.margins: 16
            width: 12; height: 12; radius: 6; color: Theme.ember
            visible: root.running || root.serviceOpen
        }
    }
}
