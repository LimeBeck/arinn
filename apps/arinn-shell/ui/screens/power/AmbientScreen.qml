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
    // Where the drift puts the block, centred until the first drift. The
    // block's own position clamps it to the screen: the block is as wide as
    // its text, so a longer date (at midnight, or after a locale change)
    // never pushes it off screen where the last drift left it, also while a
    // drift is under way.
    property real driftX: (width - block.width) / 2
    property real driftY: (height - block.height) / 2
    Behavior on driftX { NumberAnimation { duration: root.driftDuration; easing.type: Easing.InOutQuad } }
    Behavior on driftY { NumberAnimation { duration: root.driftDuration; easing.type: Easing.InOutQuad } }

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
    // Drifts the block towards (x, y), kept on screen.
    function moveTo(x, y) {
        driftX = Math.max(0, Math.min(x, width - block.width));
        driftY = Math.max(0, Math.min(y, height - block.height));
    }
    // A random place where the whole block fits: sampling the screen and
    // clamping would pile places up on the right and bottom edges.
    function drift() {
        moveTo(Math.random() * Math.max(0, width - block.width),
               Math.random() * Math.max(0, height - block.height));
    }

    // Colours of handoff 20. The clock and date are the prototype's oklch
    // values converted exactly to sRGB. The background keeps bg-deep's hex
    // from the handoff token table: the token's oklch(0.07 0.003 260), which
    // the prototype markup also uses, converts exactly to #010101 (#487).
    readonly property color backgroundColor: "#050506" // bg-deep, token-table hex
    readonly property color clockColor: "#7F8083" // oklch(0.60 0.004 260)
    readonly property color dateColor: "#545557" // oklch(0.45 0.004 260)

    Rectangle {
        objectName: "ambientBackground"
        anchors.fill: parent
        color: root.backgroundColor
    }
    // As in the prototype: the date sits left-aligned under the clock,
    // 6 px in, 10 px below it.
    Column {
        id: block
        objectName: "ambientBlock"
        x: Math.max(0, Math.min(root.driftX, root.width - width))
        y: Math.max(0, Math.min(root.driftY, root.height - height))
        spacing: 10
        // 160/500, tabular numerals and −3 % tracking.
        Ui.UiText {
            objectName: "ambientClock"
            font.pixelSize: 160
            font.weight: 500
            font.letterSpacing: -0.03 * 160
            font.features: ({ "tnum": 1 })
            // The prototype's line-height 1: a 160 px line box with the
            // glyphs centred in it, as CSS does.
            height: 160
            verticalAlignment: Text.AlignVCenter
            wrapMode: Text.NoWrap
            color: root.clockColor
            text: root.clockText
        }
        // 32/500.
        Ui.UiText {
            objectName: "ambientDate"
            leftPadding: 6
            font.pixelSize: 32
            font.weight: 500
            wrapMode: Text.NoWrap
            color: root.dateColor
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
