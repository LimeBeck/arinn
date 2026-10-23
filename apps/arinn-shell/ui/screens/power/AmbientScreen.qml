// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import "../.." as Ui

// Ambient (screensaver) from handoff 20 and spec §5.7: a deep background
// with a dim clock and date that drift to a new place every 8 s. It starts
// by an explicit action or after the idle timeout, and any key or pointer
// movement wakes it. The first wake event is consumed: it never reaches the
// screen beneath. Ambient never stops applications, pauses playback or
// requests OS suspend; it only draws.
FocusScope {
    id: root
    objectName: "ambientScreen"
    width: 1920
    height: 1080
    visible: active
    enabled: active

    property bool active: false
    property string clockText: ""
    property string dateText: ""
    // Seconds between drifts and the drift duration (handoff: 4 s every 8 s).
    property int driftInterval: 8000
    property int driftDuration: 4000
    // True once the opening event has gone by: only then does input wake.
    property bool armed: false
    property bool waking: false
    property bool anchorKnown: false
    property real anchorX: 0
    property real anchorY: 0

    signal wakeRequested()

    function start() {
        if (active)
            return;
        armed = false;
        waking = false;
        anchorKnown = false;
        if (pointerTracker.hovered)
            remember(pointerTracker.point.position.x, pointerTracker.point.position.y);
        active = true;
        // Take focus after the opening key release, so that release is not
        // also the wake.
        Qt.callLater(() => {
            if (!active)
                return;
            Ui.Focus.take(root);
            Qt.callLater(() => { if (active) armed = true; });
        });
    }
    // Leaves without a wake event, e.g. when the session locks.
    function stop() {
        active = false;
        armed = false;
        waking = false;
        anchorKnown = false;
    }
    function remember(x, y) {
        anchorX = x;
        anchorY = y;
        anchorKnown = true;
    }
    function wake() {
        if (!active)
            return;
        stop();
        wakeRequested();
    }
    function drift() {
        block.x = Math.random() * Math.max(0, width - block.width);
        block.y = Math.random() * Math.max(0, height - block.height);
    }

    Rectangle {
        anchors.fill: parent
        color: "#050506" // bg-deep
    }
    Column {
        id: block
        objectName: "ambientBlock"
        x: (root.width - width) / 2
        y: (root.height - height) / 2
        width: 900
        spacing: 8
        Behavior on x { NumberAnimation { duration: root.driftDuration; easing.type: Easing.InOutQuad } }
        Behavior on y { NumberAnimation { duration: root.driftDuration; easing.type: Easing.InOutQuad } }
        Ui.UiText {
            objectName: "ambientClock"
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            font.pixelSize: 160
            font.weight: 500
            wrapMode: Text.NoWrap
            color: "#8A8C90" // oklch(0.60 0.004 260)
            text: root.clockText
        }
        Ui.UiText {
            objectName: "ambientDate"
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            font.pixelSize: 32
            wrapMode: Text.NoWrap
            color: "#5B5D61" // oklch(0.45 0.004 260)
            text: root.dateText
        }
    }
    Timer {
        interval: root.driftInterval
        running: root.active
        repeat: true
        onTriggered: root.drift()
    }

    Keys.onPressed: event => {
        event.accepted = true;
        if (root.armed)
            root.waking = true;
    }
    // The wake happens on release, so the release never reaches the screen
    // beneath either.
    Keys.onReleased: event => {
        event.accepted = true;
        if (root.waking && root.armed)
            root.wake();
        else
            root.waking = false;
    }
    MouseArea {
        anchors.fill: parent
        hoverEnabled: true
        acceptedButtons: Qt.AllButtons
        onPressed: mouse => {
            mouse.accepted = true;
            if (root.armed)
                root.wake();
        }
        onWheel: wheel => {
            wheel.accepted = true;
            if (root.armed)
                root.wake();
        }
        onPositionChanged: mouse => {
            if (!root.anchorKnown) {
                root.remember(mouse.x, mouse.y);
                return;
            }
            if (root.armed && (mouse.x !== root.anchorX || mouse.y !== root.anchorY))
                root.wake();
        }
    }
    HoverHandler { id: pointerTracker }
}
