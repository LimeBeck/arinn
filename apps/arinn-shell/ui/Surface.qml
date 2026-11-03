// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import QtQuick.Templates as T

T.AbstractButton {
    id: root
    property string visualIdentity: text
    property bool identityGlow: false
    readonly property color accentColor: Theme.identityColor(visualIdentity)
    property bool busy: false
    // The enclosing focus/input scope owns these, including scroll suppression.
    property bool pointerMode: false
    property bool hoverFocusEnabled: true
    // "content", "action", "container", "object", or "" for a control that
    // sets its own plate. Content and objects rise from the bottom edge.
    property string shapeRole: ""
    property real focusScale: shapeRole === "content" || shapeRole === "object" ? Theme.contentFocusScale : 1.06
    property real focusLift: shapeRole === "content" || shapeRole === "object" ? Theme.contentFocusLift : -6
    property bool liftFromBottom: shapeRole === "content" || shapeRole === "object"
    // Actions are a white fill. Content draws its rim on the artwork.
    property bool rimEnabled: shapeRole !== "action" && shapeRole !== "content"
    property real cornerRadius: Theme.cardRadius
    property color restingColor: shapeRole === "container" || shapeRole === "content" ? "transparent"
            : shapeRole === "object" ? Theme.plateObject
            : shapeRole === "action" ? Theme.plateAction
            : Theme.plate
    property color focusedColor: shapeRole === "container" ? Theme.plateContainer
            : shapeRole === "object" ? Theme.plateObjectFocus
            : shapeRole === "action" ? Theme.textPrimary
            : shapeRole === "content" ? "transparent"
            : Theme.plateFocus
    // 1 on the focused item, 0.82 on the rest of its row, 0.62 on other rows.
    property real rowBrightness: 1
    readonly property bool focusVisible: activeFocus
    readonly property real lift: !activeFocus ? 0 : pointerMode ? (focusLift < 0 ? -2 : 0) : focusLift
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
    transformOrigin: liftFromBottom ? Item.Bottom : Item.Center
    transform: Translate { y: root.lift }
    z: activeFocus ? 2 : 0
    background: Rectangle {
        radius: root.cornerRadius
        color: root.activeFocus ? root.focusedColor : root.restingColor
        Behavior on color { ColorAnimation { duration: Theme.focusDuration } }
    }
    // Row light for tiles and other whole controls. Content shades only its
    // image, so a caption under the image stays at full brightness.
    Rectangle {
        objectName: root.shapeRole === "content" ? "" : "rowShade"
        anchors.fill: parent
        radius: root.cornerRadius
        color: "black"
        opacity: root.shapeRole === "content" ? 0 : Math.max(0, 1 - root.rowBrightness)
        z: 40
        Behavior on opacity { NumberAnimation { duration: Theme.focusDuration } }
    }
    // Containers and objects. Content puts the rim on the artwork instead.
    FocusRing {
        objectName: root.rimEnabled ? "focusRing" : ""
        shown: root.rimEnabled && root.activeFocus
        radius: root.cornerRadius
    }
    function focusFromHover() {
        if (hoverTracker.hovered && pointerMode && hoverFocusEnabled && enabled)
            Focus.take(root, Qt.MouseFocusReason);
    }
    // The parent input scope and this handler receive the same move. Defer the
    // check until its pointer-mode update has propagated, regardless of order.
    onPointerModeChanged: if (pointerMode) Qt.callLater(focusFromHover)
    HoverHandler {
        id: hoverTracker
        onHoveredChanged: Qt.callLater(root.focusFromHover)
        onPointChanged: Qt.callLater(root.focusFromHover)
    }
    Behavior on scale {
        NumberAnimation {
            duration: root.down ? Theme.pressDuration : Theme.focusDuration
            easing.type: Easing.BezierSpline
            easing.bezierCurve: Theme.easing
        }
    }
}
