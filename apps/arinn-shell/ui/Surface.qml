// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import QtQuick.Templates as T

T.AbstractButton {
    id: root
    property bool busy: false
    // The enclosing focus/input scope owns these, including scroll suppression.
    property bool pointerMode: false
    property bool hoverFocusEnabled: true
    property real focusScale: 1.06
    property real cornerRadius: Theme.cardRadius
    property color restingColor: Theme.plate
    property color focusedColor: Theme.plateFocus
    readonly property bool focusVisible: activeFocus
    readonly property real lift: activeFocus ? (pointerMode ? -2 : -6) : 0
    signal activated()
    signal dpadRequested()
    onClicked: if (!busy) activated()
    // AbstractButton maps Space by default; remotes also send Return/Enter.
    Keys.onPressed: event => {
        if ([Qt.Key_Left, Qt.Key_Right, Qt.Key_Up, Qt.Key_Down].includes(event.key)) {
            dpadRequested();
            event.accepted = false;
            return;
        }
        if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) {
            if (!event.isAutoRepeat && !busy) down = true;
            event.accepted = true;
        }
    }
    Keys.onReleased: event => {
        if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) {
            if (!event.isAutoRepeat) {
                const invoke = down && enabled && !busy;
                down = false;
                if (invoke) clicked();
            }
            event.accepted = true;
        }
    }
    focusPolicy: Qt.StrongFocus
    hoverEnabled: true
    Accessible.name: text
    Accessible.description: busy ? qsTr("Loading") : ""
    opacity: enabled ? 1 : 0.4
    scale: (activeFocus ? (pointerMode ? 1 + (focusScale - 1) / 2 : focusScale) : 1) - (down && !busy ? 0.04 : 0)
    transform: Translate { y: root.lift }
    z: activeFocus ? 2 : 0
    background: Rectangle {
        radius: root.cornerRadius
        color: root.activeFocus ? root.focusedColor : root.restingColor
        border.width: 1
        border.color: Qt.rgba(1, 1, 1, 0.08)
        Behavior on color { ColorAnimation { duration: Theme.focusDuration } }
    }
    // A sibling overlay: artwork/content can never cover the focus ring.
    Rectangle {
        objectName: "focusRing"
        anchors.fill: parent
        z: 100
        radius: root.cornerRadius
        color: "transparent"
        border.width: 3
        border.color: Theme.ring
        opacity: root.activeFocus ? 1 : 0
        Behavior on opacity { NumberAnimation { duration: Theme.focusDuration } }
    }
    HoverHandler {
        onHoveredChanged: if (hovered && root.pointerMode && root.hoverFocusEnabled && root.enabled)
            root.forceActiveFocus(Qt.MouseFocusReason)
    }
    Behavior on scale {
        NumberAnimation {
            duration: root.down ? Theme.pressDuration : Theme.focusDuration
            easing.type: Easing.BezierSpline
            easing.bezierCurve: Theme.easing
        }
    }
}
