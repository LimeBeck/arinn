// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick

// Volume HUD from the design handoff (Toast and volume HUD): a top-center
// glass pill 560×88 with a 40 px icon, an 8 px white bar and the value at
// 30/600. It hides 1.8 s after the last change and never takes focus.
// It only shows a level the platform reported after a change (§5.7).
Item {
    id: root
    objectName: "volumeHud"
    width: 560
    height: 88
    property int level: 0
    property bool muted: false
    property int hideAfter: 1800
    readonly property bool shown: hideTimer.running
    // The last change failed: no level is shown, only that it failed.
    property bool unavailable: false

    // A change the platform confirmed: show it and restart the hide timer.
    function show(newLevel, newMuted) {
        level = Math.max(0, newLevel);
        muted = newMuted;
        unavailable = false;
        hideTimer.restart();
    }
    // A change failed. Never a guessed level.
    function showUnavailable() {
        unavailable = true;
        hideTimer.restart();
    }
    function hide() {
        hideTimer.stop();
    }

    visible: opacity > 0
    opacity: shown ? 1 : 0
    Behavior on opacity { NumberAnimation { duration: 180 } }
    enabled: false

    Timer { id: hideTimer; interval: root.hideAfter }

    Rectangle {
        anchors.fill: parent
        radius: height / 2
        color: Qt.rgba(0.161, 0.165, 0.188, 0.82)
        border.width: 1
        border.color: Qt.rgba(1, 1, 1, 0.10)
    }
    Row {
        anchors.verticalCenter: parent.verticalCenter
        x: 32
        spacing: 24
        Canvas {
            id: icon
            objectName: "volumeHudIcon"
            width: 40
            height: 40
            anchors.verticalCenter: parent.verticalCenter
            property bool muted: root.muted || root.unavailable
            onMutedChanged: requestPaint()
            Accessible.role: Accessible.StaticText
            Accessible.name: root.muted ? qsTr("Sound muted") : qsTr("Volume")
            onPaint: {
                const ctx = getContext("2d");
                ctx.clearRect(0, 0, width, height);
                ctx.fillStyle = Theme.textPrimary;
                ctx.strokeStyle = Theme.textPrimary;
                ctx.lineWidth = 3;
                ctx.lineCap = "round";
                // Material Symbols volume_up / volume_off, scaled to 40.
                ctx.beginPath();
                ctx.moveTo(5, 15); ctx.lineTo(11, 15); ctx.lineTo(20, 7);
                ctx.lineTo(20, 33); ctx.lineTo(11, 25); ctx.lineTo(5, 25);
                ctx.closePath();
                ctx.fill();
                ctx.beginPath();
                if (muted) {
                    ctx.moveTo(25, 15); ctx.lineTo(35, 25);
                    ctx.moveTo(35, 15); ctx.lineTo(25, 25);
                } else {
                    ctx.arc(20, 20, 8, -0.8, 0.8);
                    ctx.moveTo(20 + 14 * Math.cos(-0.85), 20 + 14 * Math.sin(-0.85));
                    ctx.arc(20, 20, 14, -0.85, 0.85);
                }
                ctx.stroke();
            }
        }
        UiText {
            objectName: "volumeHudUnavailable"
            visible: root.unavailable
            anchors.verticalCenter: parent.verticalCenter
            width: 424
            text: qsTr("Volume control is not available")
            font.pixelSize: 30
            font.weight: 600
            elide: Text.ElideRight
            maximumLineCount: 1
            color: Theme.textSecondary
        }
        Rectangle {
            objectName: "volumeHudTrack"
            visible: !root.unavailable
            width: 320
            height: 8
            radius: 4
            anchors.verticalCenter: parent.verticalCenter
            color: Qt.rgba(1, 1, 1, 0.18)
            Rectangle {
                objectName: "volumeHudBar"
                height: parent.height
                radius: parent.radius
                width: parent.width * Math.min(root.level, 100) / 100
                color: Theme.textPrimary
                opacity: root.muted ? 0.35 : 1
            }
        }
        UiText {
            objectName: "volumeHudValue"
            visible: !root.unavailable
            anchors.verticalCenter: parent.verticalCenter
            width: 80
            text: String(root.level)
            font.pixelSize: 30
            font.weight: 600
            horizontalAlignment: Text.AlignRight
            wrapMode: Text.NoWrap
            color: root.muted ? Theme.textSecondary : Theme.textPrimary
        }
    }
}
