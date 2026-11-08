// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick

Surface {
    id: root
    property bool destructive: false
    property bool primary: false
    property bool selected: false
    // Some scopes (the Player OSD) keep their own local focus selection.
    property bool highlighted: false
    property string iconName: ""
    property real iconSize: 32
    property bool iconOnly: false
    property string busyText: qsTr("Loading…")
    property var labels: []
    property real widestLabel: 0
    property real minimumWidth: 180
    property real maximumWidth: Number.POSITIVE_INFINITY
    readonly property alias iconItem: glyph
    readonly property alias labelItem: label
    readonly property bool visuallyFocused: activeFocus || highlighted
    readonly property real labelWidth: Math.max(label.implicitWidth, widestLabel)
    readonly property real naturalContentWidth: (iconOnly ? 0 : labelWidth)
        + (iconOnly || iconName.length ? iconSize : 0)
        + (!iconOnly && iconName.length ? spacing : 0)
    readonly property color contentColor: visuallyFocused ? Theme.ink : Theme.textPrimary

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
        UiText {
            required property var modelData
            visible: false
            role: "control"
            text: modelData
            textFormat: Text.PlainText
            onImplicitWidthChanged: root.measure()
        }
        onItemAdded: root.measure()
        onItemRemoved: root.measure()
    }
    shapeRole: "action"
    leftPadding: 40
    rightPadding: 40
    spacing: 16
    implicitWidth: iconOnly ? implicitHeight
        : Math.min(maximumWidth, Math.max(minimumWidth, naturalContentWidth + leftPadding + rightPadding))
    implicitHeight: 88
    cornerRadius: height / 2
    focusScale: Theme.contentFocusScale
    focusLift: 0
    liftFromBottom: false
    restingColor: highlighted ? Theme.textPrimary
            : destructive ? Qt.rgba(0.91, 0.28, 0.31, 0.22)
            : primary || selected ? Theme.platePrimary : Theme.plateAction
    focusedColor: Theme.textPrimary
    contentItem: Item {
        implicitWidth: root.naturalContentWidth
        implicitHeight: Math.max(label.implicitHeight, glyph.visible ? root.iconSize : 0)
        Row {
            anchors.centerIn: parent
            spacing: root.spacing
            LineIcon {
                id: glyph
                objectName: "actionButtonIcon"
                anchors.verticalCenter: parent.verticalCenter
                width: root.iconSize
                height: width
                visible: root.iconOnly || root.iconName.length > 0
                name: root.iconOnly && root.busy ? "refresh" : root.iconName
                color: root.contentColor
                NumberAnimation on rotation {
                    running: root.visible && root.iconOnly && root.busy
                    from: 0; to: 360
                    duration: 1200
                    loops: Animation.Infinite
                    onStopped: glyph.rotation = 0
                }
            }
            UiText {
                id: label
                objectName: "actionButtonLabel"
                anchors.verticalCenter: parent.verticalCenter
                visible: !root.iconOnly
                width: Math.max(0, Math.min(root.labelWidth,
                    root.availableWidth - (glyph.visible ? root.iconSize + root.spacing : 0)))
                text: root.busy ? root.busyText : root.text
                role: "control"
                textFormat: Text.PlainText
                color: root.contentColor
                horizontalAlignment: Text.AlignHCenter
                verticalAlignment: Text.AlignVCenter
                wrapMode: Text.NoWrap
                elide: Text.ElideRight
            }
        }
    }
}
