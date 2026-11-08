// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick

// Explicit selection without decorating a translated label. Distinct roles
// retain the accepted Library tab/container and Music action treatments.
Surface {
    id: root
    property bool selected: false
    property string presentation: "tab" // tab, pill, action
    property string labelName: objectName + "Label"
    property string markerName: objectName + "Selection"
    readonly property alias labelItem: label
    implicitWidth: Math.max(presentation === "action" ? 180 : 0,
        label.implicitWidth + (presentation === "action" ? 80 : 56))
    implicitHeight: presentation === "action" ? 88 : 64
    cornerRadius: height / 2
    focusScale: presentation === "action" ? Theme.contentFocusScale : 1
    focusLift: 0
    liftFromBottom: false
    shapeRole: presentation === "pill" ? "container" : "action"
    rimEnabled: presentation === "pill"
    restingColor: presentation === "action" ? Theme.plateAction : "transparent"
    focusedColor: presentation === "pill" ? Theme.plateContainer : Theme.textPrimary
    Accessible.role: Accessible.PageTab
    Accessible.selected: selected
    contentItem: UiText {
        id: label
        objectName: root.labelName
        text: root.text
        textFormat: Text.PlainText
        font.pixelSize: root.presentation === "action" ? 30 : 28
        font.weight: 600
        color: root.activeFocus && root.presentation !== "pill" ? Theme.ink
            : root.activeFocus || root.selected || root.presentation === "action" ? Theme.textPrimary : Theme.textSecondary
        horizontalAlignment: Text.AlignHCenter
        verticalAlignment: Text.AlignVCenter
        maximumLineCount: 1
        elide: Text.ElideRight
    }
    Rectangle {
        objectName: root.markerName
        width: root.presentation === "pill" ? parent.width - 56 : 8
        height: root.presentation === "pill" ? 4 : 8
        radius: height / 2
        anchors.horizontalCenter: parent.horizontalCenter
        y: parent.height - (root.presentation === "pill" ? 11 : 6)
        color: Theme.ember
        visible: root.selected && !root.activeFocus
    }
}
