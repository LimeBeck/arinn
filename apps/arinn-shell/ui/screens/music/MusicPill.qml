// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import "../.." as Ui

// A pill with a line icon and a label, as Now playing's Like and Queue
// (brief mh-03, #802). A focused pill is a white fill with ink text.
Ui.Surface {
    id: root
    property string iconName: ""
    property bool selected: false
    shapeRole: "action"
    implicitWidth: Math.max(180, pillRow.implicitWidth + 64)
    implicitHeight: 88
    width: implicitWidth
    height: implicitHeight
    cornerRadius: height / 2
    focusScale: Ui.Theme.contentFocusScale
    focusLift: 0
    liftFromBottom: false
    restingColor: selected ? Ui.Theme.platePrimary : Ui.Theme.plateAction
    focusedColor: Ui.Theme.textPrimary
    contentItem: Item {
        Row {
            id: pillRow
            anchors.centerIn: parent
            spacing: 14
            Ui.LineIcon {
                objectName: "musicPillIcon"
                anchors.verticalCenter: parent.verticalCenter
                width: 36
                height: 36
                visible: root.iconName.length > 0
                name: root.iconName
                color: root.activeFocus ? Ui.Theme.ink : Ui.Theme.textPrimary
            }
            Ui.UiText {
                anchors.verticalCenter: parent.verticalCenter
                text: root.text
                role: "control"
                textFormat: Text.PlainText
                color: root.activeFocus ? Ui.Theme.ink : Ui.Theme.textPrimary
                wrapMode: Text.NoWrap
            }
        }
    }
}
