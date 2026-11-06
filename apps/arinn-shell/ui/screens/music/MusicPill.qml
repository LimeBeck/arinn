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
    // Every label the pill can show: it keeps the width of the widest, so a
    // state change never moves its neighbours (review of #821).
    property var labels: []
    property real widestLabel: 0
    function measure() {
        let widest = 0;
        for (let i = 0; i < measures.count; ++i)
            if (measures.itemAt(i))
                widest = Math.max(widest, measures.itemAt(i).implicitWidth);
        widestLabel = widest;
    }
    Repeater {
        id: measures
        model: root.labels
        Ui.UiText {
            required property var modelData
            visible: false
            role: "control"
            text: modelData
            onImplicitWidthChanged: root.measure()
        }
        onItemAdded: root.measure()
        onItemRemoved: root.measure()
    }
    shapeRole: "action"
    implicitWidth: Math.max(180, pillRow.implicitWidth - label.implicitWidth
        + Math.max(label.implicitWidth, widestLabel) + 56)
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
                id: label
                anchors.verticalCenter: parent.verticalCenter
                width: Math.max(implicitWidth, root.widestLabel)
                horizontalAlignment: Text.AlignHCenter
                text: root.text
                role: "control"
                textFormat: Text.PlainText
                color: root.activeFocus ? Ui.Theme.ink : Ui.Theme.textPrimary
                wrapMode: Text.NoWrap
            }
        }
    }
}
