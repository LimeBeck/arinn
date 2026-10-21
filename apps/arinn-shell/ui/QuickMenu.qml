// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick

// The quick menu (spec §5.7, ADR-0106): a side sheet over the current
// Activity or over the shell. It is the sheet focus scope (§6.1). Intents
// leave this file; nothing here talks to the daemon, a provider or the
// platform. There is no design handoff for it yet: the layout follows the
// tile action sheet (handoff cl-1) and the power menu rows.
FocusScope {
    id: root
    objectName: "quickMenu"
    width: 1920; height: 1080

    // The live Activity is behind the sheet: dim it less than the shell.
    property bool overActivity: false
    // The daemon is connected. Without it only Open Home and Close Arinn
    // stay available (degraded form).
    property bool connected: true
    // The media session for Now playing ({id, title, subtitle, state,
    // playPause}), or null.
    property var nowPlaying: null
    // "muted", "unmuted", or empty when unknown (SystemStatus1).
    property string audio: ""
    // Volume and mute are executed by the platform layer. Until it offers
    // them the row shows the state only.
    property bool volumeControl: false
    // "wifi", "wired", "other", "disconnected", or empty when unknown.
    property string network: ""
    property int wifiLevel: 0
    property string clockText: ""
    property bool pointerMode: InputMode.pointerActive
    // A short result line, such as an action that is not available yet.
    property string notice: ""
    function showNotice(text) {
        notice = text;
        noticeTimer.restart();
    }
    Timer { id: noticeTimer; interval: 3000; onTriggered: root.notice = "" }

    signal playPauseRequested(string sessionId)
    signal muteToggleRequested()
    signal volumeStepRequested(int step)
    signal sleepRequested()
    signal powerMenuRequested()
    signal homeRequested()
    signal closeShellRequested()
    signal backRequested()

    readonly property bool mediaAvailable: connected && !!nowPlaying && !!nowPlaying.id
    readonly property bool playing: mediaAvailable && nowPlaying.state === "playing"
    readonly property string audioText: audio === "muted" ? qsTr("Muted")
        : audio === "unmuted" ? qsTr("Sound on") : qsTr("Sound state unknown")
    readonly property string networkText: {
        if (!connected || network === "") return qsTr("Network state unknown");
        if (network === "disconnected") return qsTr("Not connected");
        if (network === "wired") return qsTr("Connected by cable");
        if (network === "wifi")
            return wifiLevel > 0 ? qsTr("Wi-Fi · signal %1 of 4").arg(wifiLevel) : qsTr("Wi-Fi");
        return qsTr("Connected");
    }
    // Focusable rows in display order; disabled ones are skipped.
    readonly property var rows: [mediaRow, volumeRow, sleepRow, powerRow, homeRow, closeRow]
    property var lastFocused: null

    // Focus the first available row, as when the sheet opens.
    function start() {
        lastFocused = null;
        restoreFocus();
    }
    function restoreFocus() {
        if (lastFocused && Focus.take(lastFocused)) return;
        for (const row of rows) if (Focus.take(row)) return;
    }
    function move(step) {
        const index = rows.findIndex(row => row.activeFocus);
        for (let i = index + step; i >= 0 && i < rows.length; i += step)
            if (Focus.take(rows[i])) return;
    }
    // A row that becomes unavailable hands focus to the next available one,
    // or the previous one (§6.1).
    function settle(row) {
        if (row.enabled || lastFocused !== row || !root.activeFocus) return;
        const index = rows.indexOf(row);
        for (let i = index + 1; i < rows.length; ++i) if (Focus.take(rows[i])) return;
        for (let i = index - 1; i >= 0; --i) if (Focus.take(rows[i])) return;
    }

    Keys.onPressed: event => {
        if (event.key === Qt.Key_Up) move(-1);
        else if (event.key === Qt.Key_Down) move(1);
        else if (event.key === Qt.Key_Escape || event.key === Qt.Key_Backspace || event.key === Qt.Key_Back) {
            if (!event.isAutoRepeat) backRequested();
        } else if (event.key === Qt.Key_Home) {
            if (!event.isAutoRepeat) homeRequested();
        } else if (event.key === Qt.Key_Left || event.key === Qt.Key_Right || event.key === Qt.Key_Menu
                   || event.key === Qt.Key_Tab || event.key === Qt.Key_Backtab) {
            // Focus stays inside the sheet scope.
        } else return;
        event.accepted = true;
    }

    // The Activity, or the shell screen, stays visible behind the sheet.
    Rectangle {
        objectName: "quickMenuScrim"
        anchors.fill: parent
        color: Qt.rgba(0, 0, 0, root.overActivity ? 0.35 : 0.55)
    }
    MouseArea { anchors.fill: parent; onClicked: root.backRequested() }

    component Glyph: Canvas {
        id: glyph
        property string kind: ""
        property color ink: Theme.textPrimary
        width: 38; height: 38
        onInkChanged: requestPaint()
        onKindChanged: requestPaint()
        Component.onCompleted: requestPaint()
        onPaint: {
            const ctx = getContext("2d");
            ctx.clearRect(0, 0, width, height);
            ctx.strokeStyle = ink;
            ctx.fillStyle = ink;
            ctx.lineWidth = 2.6;
            ctx.lineCap = "round";
            ctx.lineJoin = "round";
            const cx = width / 2, cy = height / 2;
            ctx.beginPath();
            if (kind === "pause") {
                ctx.fillRect(cx - 9, cy - 11, 6, 22);
                ctx.fillRect(cx + 3, cy - 11, 6, 22);
            } else if (kind === "play") {
                ctx.moveTo(cx - 7, cy - 12); ctx.lineTo(cx + 11, cy); ctx.lineTo(cx - 7, cy + 12);
                ctx.closePath(); ctx.fill();
            } else if (kind === "sound" || kind === "muted") {
                ctx.moveTo(cx - 14, cy - 5); ctx.lineTo(cx - 8, cy - 5); ctx.lineTo(cx, cy - 12);
                ctx.lineTo(cx, cy + 12); ctx.lineTo(cx - 8, cy + 5); ctx.lineTo(cx - 14, cy + 5);
                ctx.closePath(); ctx.fill();
                ctx.beginPath();
                if (kind === "muted") {
                    ctx.moveTo(cx + 5, cy - 5); ctx.lineTo(cx + 14, cy + 5);
                    ctx.moveTo(cx + 14, cy - 5); ctx.lineTo(cx + 5, cy + 5);
                } else {
                    ctx.arc(cx + 2, cy, 8, -0.9, 0.9);
                    ctx.moveTo(cx + 2 + 14 * Math.cos(-0.9), cy + 14 * Math.sin(-0.9));
                    ctx.arc(cx + 2, cy, 14, -0.9, 0.9);
                }
                ctx.stroke();
            } else if (kind === "network") {
                ctx.arc(cx, cy + 9, 2.6, 0, Math.PI * 2); ctx.fill();
                for (let part = 0; part < 3; ++part) {
                    ctx.beginPath();
                    ctx.arc(cx, cy + 9, 7 + part * 6, Math.PI * 1.22, Math.PI * 1.78);
                    ctx.stroke();
                }
            } else if (kind === "sleep") {
                ctx.arc(cx - 1, cy, 12, 0.7, 5.7);
                ctx.arc(cx + 7, cy - 3, 10, 4.6, 1.5, true);
                ctx.fill();
            } else if (kind === "power") {
                ctx.arc(cx, cy + 2, 11, 0.85, Math.PI - 0.85, true);
                ctx.stroke();
                ctx.beginPath();
                ctx.moveTo(cx, cy - 13); ctx.lineTo(cx, cy + 2);
                ctx.stroke();
            } else if (kind === "home") {
                ctx.moveTo(cx - 12, cy - 1); ctx.lineTo(cx, cy - 12); ctx.lineTo(cx + 12, cy - 1);
                ctx.moveTo(cx - 8, cy - 4); ctx.lineTo(cx - 8, cy + 11); ctx.lineTo(cx + 8, cy + 11);
                ctx.lineTo(cx + 8, cy - 4);
                ctx.stroke();
            } else if (kind === "close") {
                ctx.moveTo(cx - 9, cy - 9); ctx.lineTo(cx + 9, cy + 9);
                ctx.moveTo(cx + 9, cy - 9); ctx.lineTo(cx - 9, cy + 9);
                ctx.stroke();
            }
        }
    }

    component SheetRow: Surface {
        id: row
        property string glyph: ""
        property string description: ""
        width: 624; height: 96
        cornerRadius: 30
        focusScale: 1.02
        restingColor: Qt.rgba(1, 1, 1, 0.05)
        focusedColor: Theme.textPrimary
        pointerMode: root.pointerMode
        onActiveFocusChanged: if (activeFocus) root.lastFocused = row
        onEnabledChanged: Qt.callLater(() => root.settle(row))
        contentItem: Item {
            Rectangle {
                id: plate
                x: 28; anchors.verticalCenter: parent.verticalCenter
                width: 64; height: 64; radius: 32
                color: row.activeFocus ? Qt.rgba(0, 0, 0, 0.08) : Qt.rgba(1, 1, 1, 0.08)
                Glyph {
                    anchors.centerIn: parent
                    kind: row.glyph
                    ink: row.activeFocus ? Theme.ink : Theme.textPrimary
                }
            }
            Column {
                x: plate.x + plate.width + 24
                width: parent.width - x - 28
                anchors.verticalCenter: parent.verticalCenter
                spacing: 4
                UiText {
                    objectName: row.objectName + "Label"
                    width: parent.width
                    text: row.text
                    role: "control"; font.pixelSize: 32
                    color: row.activeFocus ? Theme.ink : Theme.textPrimary
                    elide: Text.ElideRight; maximumLineCount: 1; wrapMode: Text.NoWrap
                }
                UiText {
                    objectName: row.objectName + "Description"
                    width: parent.width
                    visible: text.length > 0
                    text: row.description
                    role: "meta"
                    color: row.activeFocus ? Qt.rgba(0.07, 0.08, 0.09, 0.72) : Theme.textSecondary
                    elide: Text.ElideRight; maximumLineCount: 1; wrapMode: Text.NoWrap
                }
            }
        }
    }

    Rectangle {
        id: sheet
        objectName: "quickMenuSheet"
        x: parent.width - width; y: 0
        width: 720; height: parent.height
        color: "#1d1e21"
        // The sheet takes clicks itself; only the area outside it is Back.
        MouseArea { anchors.fill: parent }
        UiText {
            objectName: "quickMenuClock"
            x: 48; y: 64
            text: root.clockText; role: "meta"; color: Theme.textSecondary; wrapMode: Text.NoWrap
        }
        UiText {
            objectName: "quickMenuTitle"
            x: 48; y: 104; width: 624
            text: qsTr("Quick menu"); role: "title"; font.pixelSize: 52
            elide: Text.ElideRight; maximumLineCount: 1; wrapMode: Text.NoWrap
        }
        Column {
            x: 48; y: 200
            spacing: 6
            SheetRow {
                id: mediaRow
                objectName: "quickMenuMedia"
                glyph: root.playing ? "pause" : "play"
                text: root.mediaAvailable ? (root.nowPlaying.title || qsTr("Untitled")) : qsTr("Nothing is playing")
                description: !root.mediaAvailable ? (root.connected ? qsTr("Now playing") : qsTr("Not available"))
                    : !root.nowPlaying.playPause ? qsTr("Cannot be paused from Arinn")
                    : root.playing ? qsTr("Playing · OK pauses") : qsTr("Paused · OK plays")
                enabled: root.mediaAvailable && !!root.nowPlaying.playPause
                onActivated: root.playPauseRequested(root.nowPlaying.id)
            }
            SheetRow {
                id: volumeRow
                objectName: "quickMenuVolume"
                glyph: root.audio === "muted" ? "muted" : "sound"
                text: qsTr("Sound")
                description: !root.connected ? qsTr("Not available")
                    : root.volumeControl ? qsTr("%1 · OK mutes, Left and Right change the volume").arg(root.audioText)
                    : qsTr("%1 · volume control is not available yet").arg(root.audioText)
                enabled: root.connected && root.volumeControl
                onActivated: root.muteToggleRequested()
                Keys.onLeftPressed: root.volumeStepRequested(-1)
                Keys.onRightPressed: root.volumeStepRequested(1)
            }
            // Information only: network status is not an action.
            Item {
                objectName: "quickMenuNetwork"
                width: 624; height: 96
                Rectangle {
                    id: networkPlate
                    x: 28; anchors.verticalCenter: parent.verticalCenter
                    width: 64; height: 64; radius: 32
                    color: Qt.rgba(1, 1, 1, 0.08)
                    Glyph { anchors.centerIn: parent; kind: "network"; ink: Theme.textSecondary }
                }
                Column {
                    x: networkPlate.x + networkPlate.width + 24
                    width: parent.width - x - 28
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: 4
                    UiText {
                        width: parent.width; text: qsTr("Network")
                        role: "control"; font.pixelSize: 32; wrapMode: Text.NoWrap
                    }
                    UiText {
                        objectName: "quickMenuNetworkState"
                        width: parent.width; text: root.networkText
                        role: "meta"; color: Theme.textSecondary
                        elide: Text.ElideRight; maximumLineCount: 1; wrapMode: Text.NoWrap
                    }
                }
            }
            SheetRow {
                id: sleepRow
                objectName: "quickMenuSleep"
                glyph: "sleep"
                text: qsTr("Sleep")
                description: root.connected ? qsTr("Request system suspend") : qsTr("Not available")
                enabled: root.connected
                onActivated: root.sleepRequested()
            }
            SheetRow {
                id: powerRow
                objectName: "quickMenuPower"
                glyph: "power"
                text: qsTr("Power")
                description: root.connected ? qsTr("Restart, shut down, ambient") : qsTr("Not available")
                enabled: root.connected
                onActivated: root.powerMenuRequested()
            }
            SheetRow {
                id: homeRow
                objectName: "quickMenuHome"
                glyph: "home"
                text: qsTr("Open Home")
                onActivated: root.homeRequested()
            }
            SheetRow {
                id: closeRow
                objectName: "quickMenuClose"
                glyph: "close"
                text: qsTr("Close Arinn")
                onActivated: root.closeShellRequested()
            }
        }
        UiText {
            objectName: "quickMenuNotice"
            x: 48; y: parent.height - 146; width: 624
            visible: text.length > 0
            text: root.notice
            role: "caption"
            elide: Text.ElideRight; maximumLineCount: 1; wrapMode: Text.NoWrap
        }
        Row {
            x: 48; y: parent.height - 84
            spacing: 16
            Rectangle {
                width: okHint.implicitWidth + 24; height: 44; radius: 10; color: Theme.plate
                UiText { id: okHint; anchors.centerIn: parent; text: qsTr("OK"); role: "caption"; font.weight: 700; wrapMode: Text.NoWrap }
            }
            UiText { anchors.verticalCenter: parent.verticalCenter; text: qsTr("Select"); role: "caption"; color: Theme.textSecondary; wrapMode: Text.NoWrap }
            Item { width: 16; height: 1 }
            Rectangle {
                width: backHint.implicitWidth + 24; height: 44; radius: 10; color: Theme.plate
                UiText { id: backHint; anchors.centerIn: parent; text: qsTr("Back"); role: "caption"; font.weight: 700; wrapMode: Text.NoWrap }
            }
            UiText {
                anchors.verticalCenter: parent.verticalCenter
                text: root.overActivity ? qsTr("Return") : qsTr("Close")
                role: "caption"; color: Theme.textSecondary; wrapMode: Text.NoWrap
            }
        }
    }
}
