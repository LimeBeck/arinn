// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick

// The status area of the top bar, right to left: clock, Wi-Fi or cable,
// mute, downloads, browser state (handoff 01). Unknown platform state shows
// nothing (#387). Home draws it next to its lockup; the shell draws the
// same area over Library, Settings, Downloads and Open services
// (#756 C26).
Row {
    id: area
    objectName: "statusArea"
    // What the area shows; Home and the shell bind them to SystemStatus,
    // DownloadCatalog and the browser state.
    property bool browserUnavailable: false
    property bool downloadActive: false
    property real downloadProgress: -1
    property bool muted: false
    property bool wifi: false
    property int wifiLevel: 0
    property string network: ""
    property string clockText: ""
    height: 64; spacing: 24
    Row {
        objectName: "browserStatusChip"
        anchors.verticalCenter: parent.verticalCenter
        spacing: 12
        visible: area.browserUnavailable
        Rectangle { width: 28; height: 28; radius: 14; color: Theme.error; anchors.verticalCenter: parent.verticalCenter }
        UiText {
            objectName: "browserStatusLabel"
            text: qsTr("Browser unavailable")
            role: "caption"
            color: Theme.error
            anchors.verticalCenter: parent.verticalCenter
        }
    }
    Rectangle {
        objectName: "downloadChip"
        anchors.verticalCenter: parent.verticalCenter
        visible: area.downloadActive
        height: 48; radius: 24
        width: downloadLabel.x + downloadLabel.width + 22
        color: "transparent"
        Accessible.role: Accessible.StaticText
        Accessible.name: downloadLabel.text
        UiText {
            id: downloadArrow
            x: 20; anchors.verticalCenter: parent.verticalCenter
            text: "↓"; role: "caption"; font.weight: 700; color: Theme.ember
        }
        UiText {
            id: downloadLabel
            objectName: "downloadChipLabel"
            x: downloadArrow.x + downloadArrow.width + 8
            anchors.verticalCenter: parent.verticalCenter
            role: "caption"; font.weight: 600; wrapMode: Text.NoWrap
            text: area.downloadProgress >= 0
                ? qsTr("%1%").arg(Math.floor(Math.min(1, area.downloadProgress) * 100))
                : qsTr("Downloading")
        }
    }
    Canvas {
        objectName: "muteIcon"
        anchors.verticalCenter: parent.verticalCenter
        visible: area.muted
        width: 32; height: 32
        Accessible.role: Accessible.StaticText
        Accessible.name: qsTr("Sound muted")
        onPaint: {
            const ctx = getContext("2d");
            ctx.clearRect(0, 0, width, height);
            ctx.fillStyle = Theme.textSecondary;
            ctx.strokeStyle = Theme.textSecondary;
            ctx.lineWidth = 2.6;
            ctx.lineCap = "round";
            // A speaker and a cross: Material Symbols volume_off.
            ctx.beginPath();
            ctx.moveTo(4, 12); ctx.lineTo(9, 12); ctx.lineTo(16, 6);
            ctx.lineTo(16, 26); ctx.lineTo(9, 20); ctx.lineTo(4, 20);
            ctx.closePath();
            ctx.fill();
            ctx.beginPath();
            ctx.moveTo(20, 12); ctx.lineTo(28, 20);
            ctx.moveTo(28, 12); ctx.lineTo(20, 20);
            ctx.stroke();
        }
    }
    Canvas {
        objectName: "wifiIcon"
        anchors.verticalCenter: parent.verticalCenter
        visible: area.wifi
        width: 32; height: 32
        // Dot and three arcs light up to the level; all of them
        // when the level is unknown.
        readonly property int lit: area.wifiLevel > 0 ? area.wifiLevel : 4
        onLitChanged: requestPaint()
        Accessible.role: Accessible.StaticText
        Accessible.name: qsTr("Wi-Fi connected")
        onPaint: {
            const ctx = getContext("2d");
            ctx.clearRect(0, 0, width, height);
            ctx.lineWidth = 3;
            ctx.lineCap = "round";
            const tone = part => part <= lit ? Theme.textSecondary : Theme.plateFocus;
            ctx.fillStyle = tone(1);
            ctx.beginPath(); ctx.arc(16, 25, 2.6, 0, Math.PI * 2); ctx.fill();
            for (let part = 2; part <= 4; ++part) {
                ctx.strokeStyle = tone(part);
                ctx.beginPath();
                ctx.arc(16, 25, 6 + (part - 2) * 6, Math.PI * 1.22, Math.PI * 1.78);
                ctx.stroke();
            }
        }
    }
    Canvas {
        objectName: "wiredIcon"
        anchors.verticalCenter: parent.verticalCenter
        visible: area.network === "wired" && !area.wifi
        width: 32; height: 32
        Accessible.role: Accessible.StaticText
        Accessible.name: qsTr("Connected by cable")
        onPaint: {
            // Material Symbols lan: one node above two, joined.
            const ctx = getContext("2d");
            ctx.clearRect(0, 0, width, height);
            ctx.fillStyle = Theme.textSecondary;
            ctx.strokeStyle = Theme.textSecondary;
            ctx.lineWidth = 2.6;
            ctx.fillRect(12, 3, 8, 8);
            ctx.fillRect(3, 21, 8, 8);
            ctx.fillRect(21, 21, 8, 8);
            ctx.beginPath();
            ctx.moveTo(16, 11); ctx.lineTo(16, 16);
            ctx.moveTo(7, 21); ctx.lineTo(7, 16); ctx.lineTo(25, 16); ctx.lineTo(25, 21);
            ctx.stroke();
        }
    }
    Canvas {
        objectName: "noNetworkIcon"
        anchors.verticalCenter: parent.verticalCenter
        visible: area.network === "disconnected" && !area.wifi
        width: 32; height: 32
        Accessible.role: Accessible.StaticText
        Accessible.name: qsTr("No network")
        onPaint: {
            // Material Symbols wifi_off: the Wi-Fi glyph, crossed out.
            const ctx = getContext("2d");
            ctx.clearRect(0, 0, width, height);
            ctx.lineWidth = 3;
            ctx.lineCap = "round";
            ctx.fillStyle = Theme.textSecondary;
            ctx.strokeStyle = Theme.textSecondary;
            ctx.beginPath(); ctx.arc(16, 25, 2.6, 0, Math.PI * 2); ctx.fill();
            for (let part = 2; part <= 4; ++part) {
                ctx.beginPath();
                ctx.arc(16, 25, 6 + (part - 2) * 6, Math.PI * 1.22, Math.PI * 1.78);
                ctx.stroke();
            }
            // The slash, with a gap cut out of the arcs around it.
            ctx.globalCompositeOperation = "destination-out";
            ctx.lineWidth = 7;
            ctx.beginPath();
            ctx.moveTo(5, 4); ctx.lineTo(27, 28);
            ctx.stroke();
            ctx.globalCompositeOperation = "source-over";
            ctx.lineWidth = 3;
            ctx.beginPath();
            ctx.moveTo(5, 4); ctx.lineTo(27, 28);
            ctx.stroke();
        }
    }
    UiText {
        objectName: "clock"
        anchors.verticalCenter: parent.verticalCenter
        text: area.clockText; role: "meta"; font.pixelSize: 30; wrapMode: Text.NoWrap
    }
}
