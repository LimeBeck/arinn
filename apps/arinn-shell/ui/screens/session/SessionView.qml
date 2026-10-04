// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import "../.." as Ui

// One media session. Properties are a presentation snapshot. Signals are intents
// only: this screen does not talk to D-Bus and it does not log titles.
FocusScope {
    id: root
    width: 1920
    height: 1080
    // A closed session stays in the shell tree. It must not become the window's
    // focus item, or Home never sees Backspace.
    focus: visible && enabled

    // ready shows the session. empty and unavailable (and anything else) do not.
    property string sessionState: "ready"
    property string title: ""
    property string subtitle: ""
    property string playbackState: "stopped"
    // Microseconds. Unknown duration is -1. Position is non-negative.
    property real durationUs: -1
    property real positionUs: 0
    // Neutral plate from the shared theme. Callers may override with a fixture hue.
    property color artworkHue: Ui.Theme.plate
    // Assets1 image URL. Empty, or a fetch that fails, leaves the plate.
    property url artwork: ""
    property var availableActions: ["play", "pause", "playPause", "seek", "next", "previous", "stop"]
    // Empty until a command is sent. pending waits; succeeded is quiet;
    // any other value is an error the banner shows.
    property string operationState: ""
    property bool pointerMode: false

    readonly property bool hasSession: sessionState === "ready"
    readonly property bool operationPending: operationState === "pending" || operationState === "queued" || operationState === "running"
    readonly property bool operationFailed: operationState !== "" && operationState !== "succeeded" && !operationPending
    readonly property string operationMessage: {
        if (operationState === "Unsupported") return qsTr("That control is not available.");
        if (operationState === "Timeout") return qsTr("The player did not answer in time.");
        if (operationState === "InvalidArgument") return qsTr("That position is not valid.");
        if (operationState === "Unavailable") return qsTr("The session is not available.");
        return operationState;
    }
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
    // Leave this screen. Esc, Backspace and the remote Back key share it.
    // Playback properties stay as the fixture supplied them.
    signal backRequested()

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
        if (!visible || !enabled)
            return;
        if (!hasSession) {
            Ui.Focus.take(root);
            return;
        }
        const button = preferredButton();
        if (button) Ui.Focus.take(button);
        else Ui.Focus.take(seekTrack);
    }
    function keepFocus() {
        if (!hasSession) return;
        if (seekTrack.activeFocus) return;
        const row = visualButtons();
        for (let i = 0; i < row.length; ++i)
            if (row[i].activeFocus && row[i].enabled) return;
        restore();
    }
    function isBackKey(key) {
        return key === Qt.Key_Escape || key === Qt.Key_Backspace || key === Qt.Key_Back;
    }
    function acceptBack(event) {
        if (!isBackKey(event.key))
            return false;
        if (!event.isAutoRepeat)
            backRequested();
        event.accepted = true;
        return true;
    }
    function moveFocus(key) {
        // A hover delivery for the cursor's current point can land after this
        // key. Remember that point so it does not turn pointer mode back on.
        if (hoverPoint.hovered)
            hoverPoint.previousPosition = hoverPoint.point.scenePosition;
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
                Ui.Focus.take(seekTrack);
            }
            return;
        }
        if (key === Qt.Key_Down) {
            if (!seekTrack.activeFocus) return;
            const target = actionAnchor && actionAnchor.enabled ? actionAnchor : preferredButton();
            if (target) Ui.Focus.take(target);
            return;
        }
        const row = enabledButtons();
        if (!row.length) return;
        let index = -1;
        for (let i = 0; i < row.length; ++i)
            if (row[i].activeFocus) index = i;
        if (index < 0) return;
        if (key === Qt.Key_Left && index > 0) Ui.Focus.take(row[index - 1]);
        if (key === Qt.Key_Right && index + 1 < row.length) Ui.Focus.take(row[index + 1]);
    }

    Component.onCompleted: if (visible && enabled) Qt.callLater(restore)
    onVisibleChanged: if (visible && enabled) Qt.callLater(restore)
    onEnabledChanged: if (visible && enabled) Qt.callLater(restore)
    onSessionStateChanged: if (visible && enabled) Qt.callLater(restore)
    onAvailableActionsChanged: if (visible && enabled) Qt.callLater(keepFocus)
    Timer { id: hoverDelay; interval: 500 }
    HoverHandler {
        id: hoverPoint
        property point previousPosition: Qt.point(-1, -1)
        onPointChanged: {
            const next = point.scenePosition;
            if (next.x === previousPosition.x && next.y === previousPosition.y)
                return;
            previousPosition = next;
            root.pointerMode = true;
        }
    }
    Keys.onPressed: event => {
        if (acceptBack(event))
            return;
        const key = event.key;
        // Media keys, including a configured play_pause binding (ADR-0079).
        if (key === Qt.Key_MediaTogglePlayPause || key === Qt.Key_MediaPlay
                || key === Qt.Key_MediaPause || key === Qt.Key_MediaStop) {
            if (!event.isAutoRepeat) {
                if (key === Qt.Key_MediaPlay) request("play");
                else if (key === Qt.Key_MediaPause) request("pause");
                else if (key === Qt.Key_MediaStop) request("stop");
                else if (allows("playPause")) request("playPause");
                else request(playbackState === "playing" ? "pause" : "play");
            }
            event.accepted = true;
            return;
        }
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
                        clip: true
                        Image {
                            id: artworkImage
                            objectName: "artworkImage"
                            anchors.fill: parent
                            fillMode: Image.PreserveAspectCrop
                            asynchronous: true
                            cache: false
                            source: root.artwork
                            visible: status === Image.Ready
                        }
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
                    Keys.onPressed: event => root.acceptBack(event)
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
                        visible: root.allows("previous")
                        enabled: root.allows("previous")
                        busy: root.operationPending
                        pointerMode: root.pointerMode
                        hoverFocusEnabled: root.hoverFocusEnabled
                        Keys.onPressed: event => root.acceptBack(event)
                        onActivated: root.request("previous")
                    }
                    Ui.ActionButton {
                        id: playButton
                        objectName: "action-play"
                        text: qsTr("Play")
                        visible: root.allows("play")
                        enabled: root.allows("play")
                        busy: root.operationPending
                        pointerMode: root.pointerMode
                        hoverFocusEnabled: root.hoverFocusEnabled
                        Keys.onPressed: event => root.acceptBack(event)
                        onActivated: root.request("play")
                    }
                    Ui.ActionButton {
                        id: pauseButton
                        objectName: "action-pause"
                        text: qsTr("Pause")
                        visible: root.allows("pause")
                        enabled: root.allows("pause")
                        busy: root.operationPending
                        pointerMode: root.pointerMode
                        hoverFocusEnabled: root.hoverFocusEnabled
                        Keys.onPressed: event => root.acceptBack(event)
                        onActivated: root.request("pause")
                    }
                    Ui.ActionButton {
                        id: playPauseButton
                        objectName: "action-playPause"
                        text: qsTr("Play/Pause")
                        visible: root.allows("playPause")
                        enabled: root.allows("playPause")
                        busy: root.operationPending
                        pointerMode: root.pointerMode
                        hoverFocusEnabled: root.hoverFocusEnabled
                        Keys.onPressed: event => root.acceptBack(event)
                        onActivated: root.request("playPause")
                    }
                    Ui.ActionButton {
                        id: seekButton
                        objectName: "action-seek"
                        text: qsTr("Seek")
                        visible: root.allows("seek")
                        enabled: root.allows("seek")
                        busy: root.operationPending
                        pointerMode: root.pointerMode
                        hoverFocusEnabled: root.hoverFocusEnabled
                        Keys.onPressed: event => root.acceptBack(event)
                        onActivated: if (root.allows("seek")) Ui.Focus.take(seekTrack)
                    }
                    Ui.ActionButton {
                        id: nextButton
                        objectName: "action-next"
                        text: qsTr("Next")
                        visible: root.allows("next")
                        enabled: root.allows("next")
                        busy: root.operationPending
                        pointerMode: root.pointerMode
                        hoverFocusEnabled: root.hoverFocusEnabled
                        Keys.onPressed: event => root.acceptBack(event)
                        onActivated: root.request("next")
                    }
                    Ui.ActionButton {
                        id: stopButton
                        objectName: "action-stop"
                        text: qsTr("Stop")
                        visible: root.allows("stop")
                        enabled: root.allows("stop")
                        busy: root.operationPending
                        pointerMode: root.pointerMode
                        hoverFocusEnabled: root.hoverFocusEnabled
                        Keys.onPressed: event => root.acceptBack(event)
                        onActivated: root.request("stop")
                    }
                }

                Ui.UiText {
                    objectName: "operationPending"
                    visible: root.hasSession && root.operationPending
                    text: qsTr("Waiting…")
                    role: "meta"
                    color: Ui.Theme.textSecondary
                }
                Rectangle {
                    id: operationBanner
                    objectName: "operationBanner"
                    visible: root.hasSession && root.operationFailed
                    width: parent.width
                    implicitHeight: operationBannerText.implicitHeight + 28
                    height: implicitHeight
                    radius: 18
                    color: Qt.rgba(0.91, 0.28, 0.31, 0.16)
                    Ui.UiText {
                        id: operationBannerText
                        objectName: "operationBannerText"
                        width: parent.width - 28
                        anchors.left: parent.left
                        anchors.verticalCenter: parent.verticalCenter
                        anchors.leftMargin: 14
                        text: root.operationMessage
                        role: "body"
                        color: Ui.Theme.error
                    }
                }
            }

            Ui.StatusPane {
                id: statusPane
                objectName: "sessionStatus"
                anchors.centerIn: parent
                width: 820
                visible: !root.hasSession
                kind: root.sessionState === "empty" ? "empty"
                      : root.sessionState === "loading" ? "loading" : "unavailable"
                title: root.sessionState === "empty" ? qsTr("Nothing playing")
                       : root.sessionState === "loading" ? qsTr("Loading the session…")
                       : qsTr("Session unavailable")
                description: root.sessionState === "empty"
                             ? qsTr("No media session is active.")
                             : root.sessionState === "loading"
                               ? qsTr("The media session is being read.")
                               : qsTr("The media session cannot be shown.")
                actionText: ""
            }
        }
    }
}
