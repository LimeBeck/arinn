// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick

FocusScope {
    id: root
    property string kind: "empty" // empty, loading, error, unavailable
    property string title: ""
    property string description: ""
    property string actionText: ""
    property bool pointerMode: false
    property bool hoverFocusEnabled: true
    signal actionRequested()
    signal dpadRequested()
    implicitWidth: 820
    implicitHeight: stack.implicitHeight
    Column {
        id: stack
        width: parent.width
        spacing: 24
        Rectangle {
            anchors.horizontalCenter: parent.horizontalCenter
            width: 160; height: 160; radius: 48
            color: Theme.plate
            UiText {
                anchors.centerIn: parent
                text: root.kind === "loading" ? "…" : root.kind === "empty" ? "—" : "!"
                font.pixelSize: 88
                color: root.kind === "error" || root.kind === "unavailable" ? Theme.error : Theme.textSecondary
            }
        }
        UiText { width: parent.width; role: "title"; font.pixelSize: 52; text: root.title; horizontalAlignment: Text.AlignHCenter }
        UiText { width: parent.width; text: root.description; color: Theme.textSecondary; horizontalAlignment: Text.AlignHCenter }
        ActionButton {
            objectName: "statusAction"
            anchors.horizontalCenter: parent.horizontalCenter
            text: root.actionText
            visible: root.kind !== "loading" && text.length > 0
            focus: visible
            pointerMode: root.pointerMode
            hoverFocusEnabled: root.hoverFocusEnabled
            onDpadRequested: root.dpadRequested()
            onActivated: root.actionRequested()
        }
    }
}
