// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import "../.." as Ui

// One media session, drawn from fixture properties. Signals are intents only:
// this screen does not talk to MPRIS, D-Bus or a daemon, and it does not log titles.
FocusScope {
    id: root
    width: 1920
    height: 1080
    focus: true

    // ready shows the session. empty and unavailable (and anything else) do not.
    property string sessionState: "ready"
    property string title: ""
    property string subtitle: ""
    property string playbackState: "stopped"
    // Microseconds. Unknown duration is -1. Position is non-negative.
    property real durationUs: -1
    property real positionUs: 0
    property color artworkHue: "#2a2d33"
    property var availableActions: ["play", "pause", "playPause", "seek", "next", "previous", "stop"]
    property bool pointerMode: false

    readonly property bool hasSession: sessionState === "ready"
    readonly property bool hoverFocusEnabled: !hoverDelay.running
    readonly property real progressFraction: {
        if (!(durationUs > 0) || !isFinite(positionUs)) return 0;
        return Math.max(0, Math.min(1, positionUs / durationUs));
    }
    readonly property string playbackLabelText: {
        if (playbackState === "playing") return qsTr("Playing");
        if (playbackState === "paused") return qsTr("Paused");
        if (playbackState === "stopped") return qsTr("Stopped");
        return qsTr("Unsupported");
    }
    readonly property string positionLabelText: formatUs(positionUs)
    readonly property string durationLabelText: formatUs(durationUs)
    property Item actionAnchor: null

    signal playRequested()
    signal pauseRequested()
    signal playPauseRequested()
    signal seekRequested(real targetUs)
    signal nextRequested()
    signal previousRequested()
    signal stopRequested()

    function allows(name) {
        if (!hasSession || !availableActions) return false;
        for (let i = 0; i < availableActions.length; ++i)
            if (availableActions[i] === name) return true;
        return false;
    }
    function formatUs(microseconds) {
        if (!isFinite(microseconds) || microseconds < 0) return "—";
        const total = Math.floor(microseconds / 1000000);
        const seconds = total % 60;
        const minutes = Math.floor(total / 60) % 60;
        const hours = Math.floor(total / 3600);
        const pad = value => (value < 10 ? "0" : "") + value;
        if (hours > 0) return hours + ":" + pad(minutes) + ":" + pad(seconds);
        return minutes + ":" + pad(seconds);
    }
    function visualButtons() {
        return [previousButton, playButton, pauseButton, playPauseButton, seekButton, nextButton, stopButton];
    }
    function enabledButtons() {
        return visualButtons().filter(button => button.enabled);
    }
    function preferredButton() {
        const preferred = [playButton, pauseButton, playPauseButton, previousButton, seekButton, nextButton, stopButton];
        for (let i = 0; i < preferred.length; ++i)
            if (preferred[i].enabled) return preferred[i];
        return null;
    }
    function request(name) {
        if (!allows(name)) return;
        if (name === "play") playRequested();
        else if (name === "pause") pauseRequested();
        else if (name === "playPause") playPauseRequested();
        else if (name === "next") nextRequested();
        else if (name === "previous") previousRequested();
        else if (name === "stop") stopRequested();
    }
    function nudgeSeek(direction) {
        if (!allows("seek")) return;
        const current = isFinite(positionUs) && positionUs > 0 ? positionUs : 0;
        let target = current + direction * 10000000;
        if (target < 0) target = 0;
        if (isFinite(durationUs) && durationUs > 0 && target > durationUs) target = durationUs;
        if (target === current) return;
        seekRequested(target);
    }
    function restore() {
        if (!hasSession) {
            forceActiveFocus();
            return;
        }
        const button = preferredButton();
        if (button) button.forceActiveFocus();
        else seekTrack.forceActiveFocus();
    }
    function keepFocus() {
        if (!hasSession) return;
        if (seekTrack.activeFocus) return;
        const row = visualButtons();
        for (let i = 0; i < row.length; ++i)
            if (row[i].activeFocus && row[i].enabled) return;
        restore();
    }
    function moveFocus(key) {
        pointerMode = false;
        if (!hasSession) return;
        if (seekTrack.activeFocus && (key === Qt.Key_Left || key === Qt.Key_Right)) {
            nudgeSeek(key === Qt.Key_Left ? -1 : 1);
            return;
        }
        if (key === Qt.Key_Up) {
            if (!seekTrack.activeFocus) {
                const row = visualButtons();
                for (let i = 0; i < row.length; ++i)
                    if (row[i].activeFocus) actionAnchor = row[i];
                seekTrack.forceActiveFocus();
            }
            return;
        }
        if (key === Qt.Key_Down) {
            if (!seekTrack.activeFocus) return;
            const target = actionAnchor && actionAnchor.enabled ? actionAnchor : preferredButton();
            if (target) target.forceActiveFocus();
            return;
        }
        const row = enabledButtons();
        if (!row.length) return;
        let index = -1;
        for (let i = 0; i < row.length; ++i)
            if (row[i].activeFocus) index = i;
        if (index < 0) return;
        if (key === Qt.Key_Left && index > 0) row[index - 1].forceActiveFocus();
        if (key === Qt.Key_Right && index + 1 < row.length) row[index + 1].forceActiveFocus();
    }

    Component.onCompleted: Qt.callLater(restore)
    onSessionStateChanged: Qt.callLater(restore)
    onAvailableActionsChanged: Qt.callLater(keepFocus)
    Timer { id: hoverDelay; interval: 500 }
    HoverHandler { onPointChanged: root.pointerMode = true }
    Keys.onPressed: event => {
        const key = event.key;
        if (key === Qt.Key_Left || key === Qt.Key_Right || key === Qt.Key_Up || key === Qt.Key_Down
                || key === Qt.Key_Tab || key === Qt.Key_Backtab) {
            const direction = key === Qt.Key_Backtab ? Qt.Key_Left : key === Qt.Key_Tab ? Qt.Key_Right : key;
            moveFocus(direction);
            event.accepted = true;
        } else if (!hasSession && (key === Qt.Key_Return || key === Qt.Key_Enter || key === Qt.Key_Space)) {
            event.accepted = true;
        }
    }

    Rectangle { anchors.fill: parent; color: Ui.Theme.background }
    Ui.LogicalCanvas {
        anchors.fill: parent
        Item {
            id: page
            anchors.fill: parent
            anchors.leftMargin: Ui.Theme.safeLeft
            anchors.rightMargin: Ui.Theme.safeRight
            anchors.topMargin: 72
            anchors.bottomMargin: 64

            Column {
                id: sessionBody
                objectName: "sessionBody"
                width: parent.width
                spacing: 36
                visible: root.hasSession

                Row {
                    spacing: 48
                    Rectangle {
                        id: artworkPlate
                        objectName: "artworkPlate"
                        width: 640
                        height: 360
                        radius: Ui.Theme.cardRadius
                        color: root.artworkHue
                    }
                    Column {
                        width: sessionBody.width - artworkPlate.width - 48
                        spacing: 12
                        anchors.verticalCenter: artworkPlate.verticalCenter
                        Ui.UiText {
                            objectName: "sessionHeading"
                            text: qsTr("Now playing")
                            role: "eyebrow"
                            color: Ui.Theme.textHint
                        }
                        Ui.UiText {
                            objectName: "sessionTitle"
                            width: parent.width
                            text: root.title
                            role: "title"
                            maximumLineCount: 2
                            elide: Text.ElideRight
                        }
                        Ui.UiText {
                            objectName: "sessionSubtitle"
                            width: parent.width
                            text: root.subtitle
                            role: "body"
                            color: Ui.Theme.textSecondary
                            maximumLineCount: 2
                            elide: Text.ElideRight
                        }
                        Ui.UiText {
                            objectName: "playbackLabel"
                            text: root.playbackLabelText
                            role: "meta"
                            color: Ui.Theme.textSecondary
                        }
                    }
                }

                Item {
                    id: seekTrack
                    objectName: "seekTrack"
                    width: parent.width
                    height: 88
                    enabled: root.hasSession
                    opacity: root.allows("seek") ? 1 : 0.4
                    Accessible.role: Accessible.Slider
                    Accessible.name: qsTr("Seek")
                    Ui.UiText {
                        text: qsTr("Seek")
                        role: "meta"
                        color: Ui.Theme.textSecondary
                    }
                    Ui.UiText {
                        objectName: "positionLabel"
                        anchors.right: durationLabel.left
                        anchors.rightMargin: 24
                        text: root.positionLabelText
                        role: "meta"
                    }
                    Ui.UiText {
                        id: durationLabel
                        objectName: "durationLabel"
                        anchors.right: parent.right
                        text: root.durationLabelText
                        role: "meta"
                        color: Ui.Theme.textSecondary
                    }
                    Ui.ProgressTrack {
                        objectName: "progressTrack"
                        anchors.left: parent.left
                        anchors.right: parent.right
                        anchors.bottom: parent.bottom
                        value: root.progressFraction
                    }
                }

                Row {
                    id: transportRow
                    objectName: "transportRow"
                    spacing: 16
                    visible: root.hasSession
                    Ui.ActionButton {
                        id: previousButton
                        objectName: "action-previous"
                        text: qsTr("Previous")
                        enabled: root.allows("previous")
                        pointerMode: root.pointerMode
                        hoverFocusEnabled: root.hoverFocusEnabled
                        onActivated: root.request("previous")
                    }
                    Ui.ActionButton {
                        id: playButton
                        objectName: "action-play"
                        text: qsTr("Play")
                        enabled: root.allows("play")
                        pointerMode: root.pointerMode
                        hoverFocusEnabled: root.hoverFocusEnabled
                        onActivated: root.request("play")
                    }
                    Ui.ActionButton {
                        id: pauseButton
                        objectName: "action-pause"
                        text: qsTr("Pause")
                        enabled: root.allows("pause")
                        pointerMode: root.pointerMode
                        hoverFocusEnabled: root.hoverFocusEnabled
                        onActivated: root.request("pause")
                    }
                    Ui.ActionButton {
                        id: playPauseButton
                        objectName: "action-playPause"
                        text: qsTr("Play/Pause")
                        enabled: root.allows("playPause")
                        pointerMode: root.pointerMode
                        hoverFocusEnabled: root.hoverFocusEnabled
                        onActivated: root.request("playPause")
                    }
                    Ui.ActionButton {
                        id: seekButton
                        objectName: "action-seek"
                        text: qsTr("Seek")
                        enabled: root.allows("seek")
                        pointerMode: root.pointerMode
                        hoverFocusEnabled: root.hoverFocusEnabled
                        onActivated: if (root.allows("seek")) seekTrack.forceActiveFocus()
                    }
                    Ui.ActionButton {
                        id: nextButton
                        objectName: "action-next"
                        text: qsTr("Next")
                        enabled: root.allows("next")
                        pointerMode: root.pointerMode
                        hoverFocusEnabled: root.hoverFocusEnabled
                        onActivated: root.request("next")
                    }
                    Ui.ActionButton {
                        id: stopButton
                        objectName: "action-stop"
                        text: qsTr("Stop")
                        enabled: root.allows("stop")
                        pointerMode: root.pointerMode
                        hoverFocusEnabled: root.hoverFocusEnabled
                        onActivated: root.request("stop")
                    }
                }
            }

            Ui.StatusPane {
                id: statusPane
                objectName: "sessionStatus"
                anchors.centerIn: parent
                width: 820
                visible: !root.hasSession
                kind: root.sessionState === "empty" ? "empty" : "unavailable"
                title: root.sessionState === "empty" ? qsTr("Nothing playing") : qsTr("Session unavailable")
                description: root.sessionState === "empty"
                             ? qsTr("No media session is active.")
                             : qsTr("The media session cannot be shown.")
                actionText: ""
            }
        }
    }
}
