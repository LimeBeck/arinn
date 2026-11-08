// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import "../.." as Ui

Ui.IconButton {
    id: root
    hoverMotionRevision: Ui.InputMode.pointerRevision
    onDpadRequested: Ui.InputMode.keyPressed()
    iconName: "play"
    implicitWidth: 80
    implicitHeight: 80
    width: 80
    height: 80
    // A circle at every size: 96 and 120 in Now playing's row (#840, MH-3).
    cornerRadius: width / 2
    focusScale: 1.06
    focusLift: -6
    iconSize: width >= 96 ? 48 : 32
    Ui.UiText {
        anchors.top: parent.bottom
        anchors.topMargin: 14
        anchors.horizontalCenter: parent.horizontalCenter
        width: 260
        visible: root.activeFocus
        text: root.text
        // Labels under the round buttons appear in focus at 26 (M14).
        role: "meta"
        horizontalAlignment: Text.AlignHCenter
        maximumLineCount: 1
        elide: Text.ElideRight
    }
}
