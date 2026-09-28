// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import "../../" as Ui

// Fixture playback surface. The picture belongs to a media provider; this
// screen only draws the OSD and emits intents. It does not start a player,
// open a socket or talk to the daemon.
FocusScope {
    id: root
    implicitWidth: Ui.Theme.canvasWidth
    implicitHeight: Ui.Theme.canvasHeight
    focus: true

    property string title: ""
    property real position: 0
    property real duration: 0
    property bool playing: false
    property bool seekSupported: true
    property var audioTracks: []
    property var subtitleTracks: []
    property string audioTrackId: ""
    property string subtitleTrackId: ""
    property string playbackState: "ready" // ready, loading, error, unavailable
    property string statusTitle: ""
    property string statusDescription: ""
    property bool osdVisible: true
    // Local hide delay from specification §6.4. Not a provider result.
    property int osdIdleMs: 4000
    property string osdFocus: "timeline" // timeline, audio, subtitles
    property int audioIndex: 0
    property int subtitleIndex: 0
    readonly property int seekStepSeconds: 10
    readonly property bool transportReady: playbackState === "ready"
    readonly property int audioCount: audioTracks && audioTracks.length !== undefined ? audioTracks.length : 0
    readonly property int subtitleCount: subtitleTracks && subtitleTracks.length !== undefined ? subtitleTracks.length : 0
    readonly property bool audioListAvailable: audioCount > 0
    readonly property bool subtitleListAvailable: subtitleCount > 0
    readonly property string audioCaption: trackCaption(audioTracks, audioIndex, audioCount, qsTr("Audio unavailable"))
    readonly property string subtitleCaption: trackCaption(subtitleTracks, subtitleIndex, subtitleCount, qsTr("Subtitles unavailable"))
    readonly property string statusHeading: {
        if (statusTitle !== "")
            return statusTitle
        if (playbackState === "loading")
            return qsTr("Loading")
        if (playbackState === "error")
            return qsTr("Playback failed")
        return qsTr("Playback unavailable")
    }
    readonly property string statusBody: {
        if (statusDescription !== "")
            return statusDescription
        if (playbackState === "loading")
            return qsTr("Waiting for playback.")
        if (playbackState === "error")
            return qsTr("Playback could not be shown.")
        return qsTr("Playback is not available.")
    }
    readonly property string playPauseText: playing ? qsTr("Pause") : qsTr("Play")

    signal play()
    signal pause()
    signal seek(real targetSeconds)
    signal selectAudio(string trackId)
    signal selectSubtitle(string trackId)
    signal stop()
    signal leave()

    function trackCaption(tracks, index, count, unavailable) {
        if (count <= 0)
            return unavailable
        const track = tracks[Math.max(0, Math.min(index, count - 1))]
        if (track && track.label !== undefined && String(track.label) !== "")
            return String(track.label)
        return qsTr("Track")
    }

    function clampedSeek(deltaSeconds) {
        let next = Number(position) + deltaSeconds
        if (!isFinite(next))
            next = 0
        if (next < 0)
            next = 0
        const span = Number(duration)
        if (isFinite(span) && span > 0 && next > span)
            next = span
        return next
    }

    function formatTime(seconds) {
        if (!isFinite(seconds) || seconds < 0)
            seconds = 0
        const total = Math.floor(seconds)
        const hours = Math.floor(total / 3600)
        const minutes = Math.floor((total % 3600) / 60)
        const secs = total % 60
        const padded = (secs < 10 ? "0" : "") + secs
        if (hours > 0)
            return hours + ":" + (minutes < 10 ? "0" : "") + minutes + ":" + padded
        return minutes + ":" + padded
    }

    function trackAt(tracks, index, count) {
        if (count <= 0)
            return null
        return tracks[Math.max(0, Math.min(index, count - 1))]
    }

    function armIdle() {
        if (transportReady && osdVisible && playing)
            idle.restart()
        else
            idle.stop()
    }

    function showTimeline() {
        osdFocus = "timeline"
        osdVisible = true
        armIdle()
    }

    function onBack() {
        if (transportReady && osdVisible) {
            osdVisible = false
            return
        }
        leave()
    }

    function onOk() {
        if (!transportReady)
            return
        if (!osdVisible) {
            showTimeline()
            return
        }
        if (osdFocus === "timeline") {
            if (playing)
                pause()
            else
                play()
        } else if (osdFocus === "audio") {
            const track = trackAt(audioTracks, audioIndex, audioCount)
            if (track && track.id !== undefined && String(track.id) !== "")
                selectAudio(String(track.id))
        } else if (osdFocus === "subtitles") {
            const track = trackAt(subtitleTracks, subtitleIndex, subtitleCount)
            if (track && track.id !== undefined && String(track.id) !== "")
                selectSubtitle(String(track.id))
        }
        armIdle()
    }

    function onHorizontal(delta) {
        if (!transportReady)
            return
        if (!osdVisible || osdFocus === "timeline") {
            if (seekSupported)
                seek(clampedSeek(delta * seekStepSeconds))
            if (!osdVisible)
                showTimeline()
            else
                armIdle()
            return
        }
        if (osdFocus === "audio" && audioCount > 0)
            audioIndex = Math.max(0, Math.min(audioIndex + delta, audioCount - 1))
        else if (osdFocus === "subtitles" && subtitleCount > 0)
            subtitleIndex = Math.max(0, Math.min(subtitleIndex + delta, subtitleCount - 1))
        armIdle()
    }

    function onVertical(delta) {
        if (!transportReady)
            return
        if (!osdVisible) {
            showTimeline()
            return
        }
        const order = ["timeline", "audio", "subtitles"]
        let index = order.indexOf(osdFocus)
        if (index < 0)
            index = 0
        osdFocus = order[Math.max(0, Math.min(order.length - 1, index + delta))]
        armIdle()
    }

    function onMedia(kind) {
        if (!transportReady)
            return
        if (kind === "play")
            play()
        else if (kind === "pause")
            pause()
        else if (kind === "toggle") {
            if (playing)
                pause()
            else
                play()
        } else if (kind === "stop")
            stop()
        if (!osdVisible)
            showTimeline()
        else
            armIdle()
    }

    function handleKey(event) {
        const key = event.key
        const ok = key === Qt.Key_Return || key === Qt.Key_Enter || key === Qt.Key_Select
        const back = key === Qt.Key_Escape || key === Qt.Key_Back || key === Qt.Key_Backspace
        const horizontal = key === Qt.Key_Left || key === Qt.Key_Right
        const vertical = key === Qt.Key_Up || key === Qt.Key_Down
        const media = key === Qt.Key_MediaPlay || key === Qt.Key_MediaPause
                || key === Qt.Key_MediaTogglePlayPause || key === Qt.Key_MediaStop
        if (!ok && !back && !horizontal && !vertical && !media)
            return
        if (event.isAutoRepeat && !horizontal && !vertical) {
            event.accepted = true
            return
        }
        if (back)
            onBack()
        else if (ok)
            onOk()
        else if (key === Qt.Key_Left)
            onHorizontal(-1)
        else if (key === Qt.Key_Right)
            onHorizontal(1)
        else if (key === Qt.Key_Up)
            onVertical(-1)
        else if (key === Qt.Key_Down)
            onVertical(1)
        else if (key === Qt.Key_MediaPlay)
            onMedia("play")
        else if (key === Qt.Key_MediaPause)
            onMedia("pause")
        else if (key === Qt.Key_MediaTogglePlayPause)
            onMedia("toggle")
        else if (key === Qt.Key_MediaStop)
            onMedia("stop")
        event.accepted = true
    }

    Keys.onPressed: event => handleKey(event)
    onPlayingChanged: armIdle()
    onOsdVisibleChanged: armIdle()
    onPlaybackStateChanged: armIdle()
    onOsdIdleMsChanged: armIdle()
    Component.onCompleted: armIdle()

    Timer {
        id: idle
        interval: Math.max(0, root.osdIdleMs)
        repeat: false
        onTriggered: {
            // Local chrome only. This must not emit a playback intent.
            if (root.transportReady && root.osdVisible && root.playing)
                root.osdVisible = false
        }
    }

    Ui.LogicalCanvas {
        anchors.fill: parent
        Item {
            id: surface
            objectName: "playbackSurface"
            width: Ui.Theme.canvasWidth
            height: Ui.Theme.canvasHeight

            Rectangle {
                anchors.fill: parent
                color: Ui.Theme.background
            }

            Column {
                objectName: "statusPane"
                visible: !root.transportReady
                width: 820
                spacing: 24
                anchors.centerIn: parent
                Rectangle {
                    anchors.horizontalCenter: parent.horizontalCenter
                    width: 160
                    height: 160
                    radius: 48
                    color: Ui.Theme.plate
                    Ui.UiText {
                        anchors.centerIn: parent
                        text: root.playbackState === "loading" ? "…" : "!"
                        font.pixelSize: 88
                        color: root.playbackState === "loading" ? Ui.Theme.textSecondary : Ui.Theme.error
                    }
                }
                Ui.UiText {
                    objectName: "statusHeading"
                    width: parent.width
                    role: "title"
                    font.pixelSize: 52
                    text: root.statusHeading
                    horizontalAlignment: Text.AlignHCenter
                }
                Ui.UiText {
                    width: parent.width
                    text: root.statusBody
                    color: Ui.Theme.textSecondary
                    horizontalAlignment: Text.AlignHCenter
                }
            }

            Item {
                objectName: "osd"
                anchors.fill: parent
                visible: root.transportReady && root.osdVisible

                Rectangle {
                    anchors.left: parent.left
                    anchors.right: parent.right
                    anchors.top: parent.top
                    height: 260
                    gradient: Gradient {
                        GradientStop { position: 0; color: Qt.rgba(0.075, 0.078, 0.086, 0.92) }
                        GradientStop { position: 1; color: Qt.rgba(0.075, 0.078, 0.086, 0) }
                    }
                }
                Rectangle {
                    anchors.left: parent.left
                    anchors.right: parent.right
                    anchors.bottom: parent.bottom
                    height: 420
                    gradient: Gradient {
                        GradientStop { position: 0; color: Qt.rgba(0.075, 0.078, 0.086, 0) }
                        GradientStop { position: 1; color: Qt.rgba(0.075, 0.078, 0.086, 0.94) }
                    }
                }

                Ui.UiText {
                    objectName: "titleLabel"
                    x: Ui.Theme.safeLeft
                    y: 72
                    width: parent.width - Ui.Theme.safeLeft - Ui.Theme.safeRight
                    text: root.title
                    font.pixelSize: 48
                    font.weight: Font.Bold
                    elide: Text.ElideRight
                    maximumLineCount: 1
                }

                Rectangle {
                    objectName: "pausedMark"
                    visible: !root.playing
                    width: 160
                    height: 160
                    radius: 80
                    anchors.centerIn: parent
                    color: Ui.Theme.plate
                    Ui.UiText {
                        anchors.centerIn: parent
                        text: root.playPauseText
                        role: "control"
                    }
                }

                Column {
                    anchors.left: parent.left
                    anchors.right: parent.right
                    anchors.bottom: parent.bottom
                    anchors.leftMargin: Ui.Theme.safeLeft
                    anchors.rightMargin: Ui.Theme.safeRight
                    anchors.bottomMargin: 64
                    spacing: 20

                    Row {
                        width: parent.width
                        height: 40
                        spacing: 24
                        Ui.UiText {
                            objectName: "positionLabel"
                            width: 140
                            height: parent.height
                            text: root.formatTime(root.position)
                            role: "meta"
                            verticalAlignment: Text.AlignVCenter
                        }
                        Item {
                            objectName: "timeline"
                            width: parent.width - 328
                            height: parent.height
                            Rectangle {
                                anchors.fill: parent
                                anchors.margins: -8
                                radius: 16
                                color: "transparent"
                                border.width: root.osdFocus === "timeline" ? 3 : 0
                                border.color: Ui.Theme.ring
                            }
                            Ui.ProgressTrack {
                                anchors.left: parent.left
                                anchors.right: parent.right
                                anchors.verticalCenter: parent.verticalCenter
                                height: root.osdFocus === "timeline" ? 14 : 8
                                value: root.duration > 0 ? root.position / root.duration : 0
                            }
                        }
                        Ui.UiText {
                            objectName: "durationLabel"
                            width: 140
                            height: parent.height
                            text: root.formatTime(root.duration)
                            role: "meta"
                            horizontalAlignment: Text.AlignRight
                            verticalAlignment: Text.AlignVCenter
                        }
                    }

                    Ui.UiText {
                        objectName: "playPause"
                        text: root.playPauseText
                        role: "control"
                        color: root.osdFocus === "timeline" ? Ui.Theme.textPrimary : Ui.Theme.textSecondary
                    }

                    Column {
                        id: audioColumn
                        objectName: "audioList"
                        width: parent.width
                        spacing: 8
                        Ui.UiText {
                            text: qsTr("Audio")
                            role: "caption"
                            color: Ui.Theme.textHint
                        }
                        Repeater {
                            model: root.audioCount
                            delegate: Rectangle {
                                required property int index
                                objectName: "audio-" + (root.audioTracks[index] ? String(root.audioTracks[index].id) : index)
                                width: audioColumn.width
                                height: 76
                                radius: 28
                                color: root.osdFocus === "audio" && root.audioIndex === index ? Ui.Theme.textPrimary : Ui.Theme.plate
                                Ui.UiText {
                                    anchors.fill: parent
                                    anchors.leftMargin: 28
                                    anchors.rightMargin: 28
                                    text: root.trackCaption(root.audioTracks, index, root.audioCount, qsTr("Audio unavailable"))
                                    role: "control"
                                    verticalAlignment: Text.AlignVCenter
                                    color: root.osdFocus === "audio" && root.audioIndex === index ? Ui.Theme.ink : Ui.Theme.textPrimary
                                }
                            }
                        }
                        Rectangle {
                            objectName: "audioUnavailable"
                            visible: !root.audioListAvailable
                            width: audioColumn.width
                            height: 76
                            radius: 28
                            color: root.osdFocus === "audio" ? Ui.Theme.textPrimary : Ui.Theme.plate
                            Ui.UiText {
                                anchors.fill: parent
                                anchors.leftMargin: 28
                                anchors.rightMargin: 28
                                text: root.audioCaption
                                role: "control"
                                verticalAlignment: Text.AlignVCenter
                                color: root.osdFocus === "audio" ? Ui.Theme.ink : Ui.Theme.error
                            }
                        }
                    }

                    Column {
                        id: subtitleColumn
                        objectName: "subtitleList"
                        width: parent.width
                        spacing: 8
                        Ui.UiText {
                            text: qsTr("Subtitles")
                            role: "caption"
                            color: Ui.Theme.textHint
                        }
                        Repeater {
                            model: root.subtitleCount
                            delegate: Rectangle {
                                required property int index
                                objectName: "subtitle-" + (root.subtitleTracks[index] ? String(root.subtitleTracks[index].id) : index)
                                width: subtitleColumn.width
                                height: 76
                                radius: 28
                                color: root.osdFocus === "subtitles" && root.subtitleIndex === index ? Ui.Theme.textPrimary : Ui.Theme.plate
                                Ui.UiText {
                                    anchors.fill: parent
                                    anchors.leftMargin: 28
                                    anchors.rightMargin: 28
                                    text: root.trackCaption(root.subtitleTracks, index, root.subtitleCount, qsTr("Subtitles unavailable"))
                                    role: "control"
                                    verticalAlignment: Text.AlignVCenter
                                    color: root.osdFocus === "subtitles" && root.subtitleIndex === index ? Ui.Theme.ink : Ui.Theme.textPrimary
                                }
                            }
                        }
                        Rectangle {
                            objectName: "subtitleUnavailable"
                            visible: !root.subtitleListAvailable
                            width: subtitleColumn.width
                            height: 76
                            radius: 28
                            color: root.osdFocus === "subtitles" ? Ui.Theme.textPrimary : Ui.Theme.plate
                            Ui.UiText {
                                anchors.fill: parent
                                anchors.leftMargin: 28
                                anchors.rightMargin: 28
                                text: root.subtitleCaption
                                role: "control"
                                verticalAlignment: Text.AlignVCenter
                                color: root.osdFocus === "subtitles" ? Ui.Theme.ink : Ui.Theme.error
                            }
                        }
                    }

                    Ui.UiText {
                        text: qsTr("Left/Right — 10 seconds · OK — play/pause")
                        role: "caption"
                        color: Ui.Theme.textHint
                    }
                }
            }
        }
    }
}
