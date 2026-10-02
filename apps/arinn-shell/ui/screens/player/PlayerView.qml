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
    property string episodeLine: ""
    property string clockText: ""
    property string liveClock: Qt.formatTime(new Date(), "hh:mm")
    readonly property string clockLabel: clockText !== "" ? clockText : liveClock
    property real position: 0
    property real duration: 0
    property real buffered: 0
    property var chapters: []
    property bool playing: false
    property bool seekSupported: true
    property var audioTracks: []
    property var subtitleTracks: []
    property string audioTrackId: ""
    property string subtitleTrackId: ""
    property string playbackState: "ready" // ready, loading, error, unavailable
    // Missing MediaTracks1 / MediaTimeline1 hides those controls (ADR-0054).
    // Defaults keep the fixture OSD, including an empty list labelled unavailable.
    property bool tracksSupported: true
    property bool timelineSupported: true
    // Fixture picture. False leaves this layer transparent for a video window.
    property bool pictureStandIn: true
    property string statusTitle: ""
    property string statusDescription: ""
    property bool osdVisible: true
    // Assets1 image URL for the focused timeline. Empty leaves the plate.
    property url thumbnail: ""
    // Local hide delay from specification §6.4. Not a provider result.
    property int osdIdleMs: 4000
    property string osdFocus: "timeline" // timeline, subtitles, audio, more
    property string sheetKind: "" // "", audio, subtitles
    property int audioIndex: 0
    property int subtitleIndex: 0
    readonly property int seekStepSeconds: 10
    readonly property bool transportReady: playbackState === "ready"
    readonly property bool timelineFocused: osdFocus === "timeline" && sheetKind === ""
    readonly property int audioCount: audioTracks && audioTracks.length !== undefined ? audioTracks.length : 0
    readonly property int subtitleCount: subtitleTracks && subtitleTracks.length !== undefined ? subtitleTracks.length : 0
    readonly property int chapterCount: timelineSupported && chapters && chapters.length !== undefined ? chapters.length : 0
    readonly property int openSheetCount: sheetKind === "audio" ? audioCount : (sheetKind === "subtitles" ? subtitleCount : 0)
    readonly property bool audioListAvailable: audioCount > 0
    readonly property bool subtitleListAvailable: subtitleCount > 0
    readonly property real progressFraction: duration > 0 ? Math.max(0, Math.min(1, position / duration)) : 0
    readonly property real bufferFraction: timelineSupported && duration > 0 && buffered > 0 ? Math.max(0, Math.min(1, buffered / duration)) : 0
    readonly property string audioCaption: selectedCaption(audioTracks, audioCount, audioTrackId, qsTr("Audio unavailable"))
    readonly property string subtitleCaption: selectedCaption(subtitleTracks, subtitleCount, subtitleTrackId, qsTr("Subtitles unavailable"))
    readonly property string audioPillText: audioListAvailable ? qsTr("Audio") + " · " + audioCaption : audioCaption
    readonly property string subtitlePillText: subtitleListAvailable ? qsTr("Subtitles") + " · " + subtitleCaption : subtitleCaption
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

    signal play()
    signal pause()
    signal seek(real targetSeconds)
    signal selectAudio(string trackId)
    signal selectSubtitle(string trackId)
    signal stop()
    signal leave()
    // Home shows the shell Home. It is not leave, and it does not stop playback.
    signal homeRequested()

    function selectedCaption(tracks, count, selectedId, unavailable) {
        if (count <= 0)
            return unavailable
        for (let i = 0; i < count; i++) {
            const track = tracks[i]
            if (track && String(track.id) === selectedId && track.label !== undefined && String(track.label) !== "")
                return String(track.label)
        }
        const first = tracks[0]
        if (first && first.label !== undefined && String(first.label) !== "")
            return String(first.label)
        return qsTr("Track")
    }

    function selectedIndex(tracks, count, selectedId) {
        for (let i = 0; i < count; i++) {
            if (tracks[i] && String(tracks[i].id) === selectedId)
                return i
        }
        return 0
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

    function chapterFraction(index) {
        if (!(duration > 0) || !chapters || chapters[index] === undefined)
            return 0
        const entry = chapters[index]
        let seconds = 0
        if (typeof entry === "number")
            seconds = entry
        else if (entry && entry.at !== undefined)
            seconds = Number(entry.at)
        else if (entry && entry.position !== undefined)
            seconds = Number(entry.position)
        if (!isFinite(seconds))
            return 0
        return Math.max(0, Math.min(1, seconds / duration))
    }

    function trackAt(tracks, index, count) {
        if (count <= 0)
            return null
        return tracks[Math.max(0, Math.min(index, count - 1))]
    }

    function sheetTrack(index) {
        const tracks = sheetKind === "audio" ? audioTracks : subtitleTracks
        return tracks && tracks[index] ? tracks[index] : null
    }

    function armIdle() {
        if (transportReady && osdVisible && playing)
            idle.restart()
        else
            idle.stop()
    }

    function showTimeline() {
        sheetKind = ""
        osdFocus = "timeline"
        osdVisible = true
        armIdle()
    }

    function visiblePills() {
        const order = []
        if (tracksSupported) {
            order.push("subtitles")
            order.push("audio")
        }
        order.push("more")
        return order
    }

    function movePill(delta) {
        const order = visiblePills()
        let index = order.indexOf(osdFocus)
        if (index < 0)
            index = 0
        osdFocus = order[Math.max(0, Math.min(order.length - 1, index + delta))]
    }

    function moveSheet(delta) {
        if (sheetKind === "audio" && audioCount > 0)
            audioIndex = Math.max(0, Math.min(audioIndex + delta, audioCount - 1))
        else if (sheetKind === "subtitles" && subtitleCount > 0)
            subtitleIndex = Math.max(0, Math.min(subtitleIndex + delta, subtitleCount - 1))
    }

    function confirmSheet() {
        if (sheetKind === "audio") {
            const track = trackAt(audioTracks, audioIndex, audioCount)
            if (!track || track.id === undefined || String(track.id) === "")
                return
            selectAudio(String(track.id))
        } else if (sheetKind === "subtitles") {
            const track = trackAt(subtitleTracks, subtitleIndex, subtitleCount)
            if (!track || track.id === undefined || String(track.id) === "")
                return
            selectSubtitle(String(track.id))
        } else {
            return
        }
        sheetKind = ""
    }

    function onHome() {
        // Close the sheet without selecting a track. Opening Home never
        // pauses or stops playback (specification §6.3).
        if (sheetKind !== "")
            sheetKind = ""
        homeRequested()
        armIdle()
    }

    function onBack() {
        if (sheetKind !== "") {
            sheetKind = ""
            armIdle()
            return
        }
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
        if (sheetKind !== "") {
            confirmSheet()
            armIdle()
            return
        }
        if (osdFocus === "timeline") {
            if (playing)
                pause()
            else
                play()
        } else if (tracksSupported && osdFocus === "audio" && audioListAvailable) {
            audioIndex = selectedIndex(audioTracks, audioCount, audioTrackId)
            sheetKind = "audio"
        } else if (tracksSupported && osdFocus === "subtitles" && subtitleListAvailable) {
            subtitleIndex = selectedIndex(subtitleTracks, subtitleCount, subtitleTrackId)
            sheetKind = "subtitles"
        }
        armIdle()
    }

    function onHorizontal(delta) {
        if (!transportReady)
            return
        if (sheetKind !== "") {
            moveSheet(delta)
            armIdle()
            return
        }
        if (!osdVisible || osdFocus === "timeline") {
            if (seekSupported)
                seek(clampedSeek(delta * seekStepSeconds))
            if (!osdVisible)
                showTimeline()
            else
                armIdle()
            return
        }
        movePill(delta)
        armIdle()
    }

    function onVertical(delta) {
        if (!transportReady)
            return
        if (!osdVisible) {
            showTimeline()
            return
        }
        if (sheetKind !== "") {
            moveSheet(delta)
            armIdle()
            return
        }
        if (osdFocus === "timeline") {
            if (delta > 0) {
                const order = visiblePills()
                if (order.length > 0)
                    osdFocus = order[0]
            }
        } else if (delta < 0) {
            osdFocus = "timeline"
        }
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
        const home = key === Qt.Key_Home
        const horizontal = key === Qt.Key_Left || key === Qt.Key_Right
        const vertical = key === Qt.Key_Up || key === Qt.Key_Down
        const media = key === Qt.Key_MediaPlay || key === Qt.Key_MediaPause
                || key === Qt.Key_MediaTogglePlayPause || key === Qt.Key_MediaStop
        if (!ok && !back && !home && !horizontal && !vertical && !media)
            return
        if (event.isAutoRepeat && !horizontal && !vertical) {
            event.accepted = true
            return
        }
        if (back)
            onBack()
        else if (home)
            onHome()
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
    onOsdVisibleChanged: {
        if (!osdVisible)
            sheetKind = ""
        armIdle()
    }
    onPlaybackStateChanged: armIdle()
    // The transport binding may update after onPlaybackStateChanged ran with
    // its old value; re-arm on the binding itself so the OSD hides after a
    // launch without input (#245).
    onTransportReadyChanged: armIdle()
    onOsdIdleMsChanged: armIdle()
    onSheetKindChanged: armIdle()
    onTracksSupportedChanged: {
        if (tracksSupported)
            return
        if (sheetKind === "audio" || sheetKind === "subtitles")
            sheetKind = ""
        if (osdFocus === "audio" || osdFocus === "subtitles")
            osdFocus = "more"
    }
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

    Timer {
        interval: 1000
        repeat: true
        running: root.clockText === ""
        onTriggered: root.liveClock = Qt.formatTime(new Date(), "hh:mm")
    }

    Ui.LogicalCanvas {
        anchors.fill: parent
        Item {
            id: surface
            objectName: "playbackSurface"
            width: Ui.Theme.canvasWidth
            height: Ui.Theme.canvasHeight

            Rectangle {
                objectName: "pictureStandIn"
                anchors.fill: parent
                color: root.pictureStandIn ? "#0b0c0f" : "transparent"
            }
            Rectangle {
                visible: root.pictureStandIn
                width: 980
                height: 720
                radius: width / 2
                anchors.centerIn: parent
                anchors.verticalCenterOffset: -40
                color: Qt.rgba(0.22, 0.18, 0.14, 0.28)
            }
            Ui.UiText {
                objectName: "videoLabel"
                visible: root.pictureStandIn
                anchors.centerIn: parent
                text: qsTr("video")
                font.pixelSize: 22
                font.weight: 500
                font.letterSpacing: 1.8
                color: Qt.rgba(1, 1, 1, 0.35)
            }

            Rectangle {
                objectName: "pausedMark"
                visible: root.transportReady && !root.playing
                width: 160
                height: 160
                radius: 80
                anchors.centerIn: parent
                color: Qt.rgba(0.12, 0.12, 0.13, 0.62)
                Rectangle {
                    width: 14
                    height: 56
                    radius: 4
                    color: Ui.Theme.textPrimary
                    anchors.verticalCenter: parent.verticalCenter
                    anchors.horizontalCenter: parent.horizontalCenter
                    anchors.horizontalCenterOffset: -16
                }
                Rectangle {
                    width: 14
                    height: 56
                    radius: 4
                    color: Ui.Theme.textPrimary
                    anchors.verticalCenter: parent.verticalCenter
                    anchors.horizontalCenter: parent.horizontalCenter
                    anchors.horizontalCenterOffset: 16
                }
            }

            Column {
                objectName: "statusPane"
                visible: !root.transportReady
                width: 820
                spacing: 24
                anchors.centerIn: parent
                z: 3
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
                z: 2

                Rectangle {
                    anchors.left: parent.left
                    anchors.right: parent.right
                    anchors.top: parent.top
                    height: 260
                    gradient: Gradient {
                        GradientStop { position: 0; color: Qt.rgba(0, 0, 0, 0.65) }
                        GradientStop { position: 1; color: Qt.rgba(0, 0, 0, 0) }
                    }
                }
                Rectangle {
                    anchors.left: parent.left
                    anchors.right: parent.right
                    anchors.bottom: parent.bottom
                    height: 420
                    gradient: Gradient {
                        GradientStop { position: 0; color: Qt.rgba(0, 0, 0, 0) }
                        GradientStop { position: 0.45; color: Qt.rgba(0, 0, 0, 0.45) }
                        GradientStop { position: 1; color: Qt.rgba(0, 0, 0, 0.82) }
                    }
                }

                Column {
                    x: 120
                    y: 72
                    spacing: 10
                    Ui.UiText {
                        objectName: "titleLabel"
                        text: root.title
                        font.pixelSize: 48
                        font.weight: 700
                        font.letterSpacing: -0.96
                        elide: Text.ElideRight
                        width: 1400
                    }
                    Ui.UiText {
                        objectName: "episodeLabel"
                        visible: root.episodeLine !== ""
                        text: root.episodeLine
                        font.pixelSize: 28
                        font.weight: 500
                        color: Ui.Theme.textSecondary
                    }
                }

                Ui.UiText {
                    objectName: "clockLabel"
                    anchors.right: parent.right
                    anchors.rightMargin: 120
                    y: 80
                    text: root.clockLabel
                    font.pixelSize: 30
                    font.weight: 500
                    color: Ui.Theme.textSecondary
                }

                Item {
                    id: timelineRow
                    objectName: "timelineRow"
                    anchors.left: parent.left
                    anchors.right: parent.right
                    anchors.bottom: parent.bottom
                    anchors.leftMargin: 120
                    anchors.rightMargin: 120
                    anchors.bottomMargin: 180
                    height: 60

                    Ui.UiText {
                        objectName: "positionLabel"
                        width: 130
                        height: parent.height
                        text: root.formatTime(root.position)
                        font.pixelSize: 28
                        font.weight: 600
                        verticalAlignment: Text.AlignVCenter
                    }

                    Item {
                        id: track
                        objectName: "timeline"
                        x: 162
                        width: parent.width - 324
                        height: parent.height
                        readonly property bool focused: root.timelineFocused
                        readonly property int trackHeight: focused ? 14 : 8

                        Rectangle {
                            id: trackBody
                            anchors.left: parent.left
                            anchors.right: parent.right
                            anchors.verticalCenter: parent.verticalCenter
                            height: track.trackHeight
                            radius: 8
                            color: Qt.rgba(1, 1, 1, 0.24)
                            Rectangle {
                                width: parent.width * root.bufferFraction
                                height: parent.height
                                radius: parent.radius
                                color: Qt.rgba(1, 1, 1, 0.18)
                            }
                            Rectangle {
                                width: parent.width * root.progressFraction
                                height: parent.height
                                radius: parent.radius
                                color: Ui.Theme.ember
                            }
                            Repeater {
                                model: root.chapterCount
                                delegate: Rectangle {
                                    required property int index
                                    width: 4
                                    height: parent.height
                                    x: parent.width * root.chapterFraction(index) - 2
                                    color: Qt.rgba(0, 0, 0, 0.6)
                                }
                            }
                        }
                        Rectangle {
                            anchors.fill: trackBody
                            anchors.margins: -6
                            radius: 12
                            color: "transparent"
                            border.width: track.focused ? 3 : 0
                            border.color: Ui.Theme.ring
                        }
                        Rectangle {
                            objectName: "timelineKnob"
                            visible: track.focused
                            width: 40
                            height: 40
                            radius: 20
                            color: Ui.Theme.textPrimary
                            y: (parent.height - height) / 2
                            x: Math.max(0, Math.min(track.width - width, root.progressFraction * track.width - width / 2))
                        }
                        Item {
                            objectName: "timelineThumb"
                            visible: track.focused && root.timelineSupported
                            width: 336
                            height: 249
                            y: -height - 20
                            x: Math.max(0, Math.min(track.width - width, root.progressFraction * track.width - width / 2))
                            Rectangle {
                                width: 336
                                height: 189
                                radius: 18
                                color: "#1a1c20"
                                border.width: 3
                                border.color: Ui.Theme.textPrimary
                                clip: true
                                Image {
                                    id: thumbImage
                                    objectName: "thumbnailImage"
                                    anchors.fill: parent
                                    anchors.margins: 3
                                    fillMode: Image.PreserveAspectCrop
                                    asynchronous: true
                                    cache: false
                                    source: root.thumbnail
                                    visible: status === Image.Ready
                                }
                                Text {
                                    objectName: "thumbnailLabel"
                                    visible: thumbImage.status !== Image.Ready
                                    x: 14
                                    y: 12
                                    text: qsTr("thumbnail")
                                    // Placeholder caption from the handoff. This face is not a UI text role.
                                    font.family: "JetBrains Mono"
                                    font.pixelSize: 20
                                    font.letterSpacing: 2
                                    color: Qt.rgba(1, 1, 1, 0.92)
                                }
                            }
                            Rectangle {
                                anchors.horizontalCenter: parent.horizontalCenter
                                anchors.bottom: parent.bottom
                                width: timeChip.implicitWidth + 36
                                height: 48
                                radius: 20
                                color: Ui.Theme.textPrimary
                                Ui.UiText {
                                    id: timeChip
                                    anchors.centerIn: parent
                                    text: root.formatTime(root.position)
                                    font.pixelSize: 28
                                    font.weight: 700
                                    color: Ui.Theme.ink
                                }
                            }
                        }
                    }

                    Ui.UiText {
                        objectName: "durationLabel"
                        anchors.right: parent.right
                        width: 130
                        height: parent.height
                        text: root.formatTime(root.duration)
                        font.pixelSize: 28
                        font.weight: 500
                        color: Ui.Theme.textSecondary
                        horizontalAlignment: Text.AlignRight
                        verticalAlignment: Text.AlignVCenter
                    }
                }

                Item {
                    id: pillRow
                    objectName: "pillRow"
                    anchors.left: parent.left
                    anchors.right: parent.right
                    anchors.bottom: parent.bottom
                    anchors.leftMargin: 120
                    anchors.rightMargin: 120
                    anchors.bottomMargin: 72
                    height: 76

                    Row {
                        height: parent.height
                        spacing: 16
                        Rectangle {
                            objectName: "subtitlePill"
                            visible: root.tracksSupported
                            height: 76
                            radius: 38
                            width: subtitlePillLabel.implicitWidth + 64
                            color: root.osdFocus === "subtitles" && root.sheetKind === "" ? Ui.Theme.plateFocus : Ui.Theme.plate
                            Ui.UiText {
                                id: subtitlePillLabel
                                objectName: "subtitlePillLabel"
                                anchors.centerIn: parent
                                text: root.subtitlePillText
                                font.pixelSize: 28
                                font.weight: 600
                                color: root.subtitleListAvailable ? Ui.Theme.textPrimary : Ui.Theme.error
                            }
                        }
                        Rectangle {
                            objectName: "audioPill"
                            visible: root.tracksSupported
                            height: 76
                            radius: 38
                            width: audioPillLabel.implicitWidth + 64
                            color: root.osdFocus === "audio" && root.sheetKind === "" ? Ui.Theme.plateFocus : Ui.Theme.plate
                            Ui.UiText {
                                id: audioPillLabel
                                objectName: "audioPillLabel"
                                anchors.centerIn: parent
                                text: root.audioPillText
                                font.pixelSize: 28
                                font.weight: 600
                                color: root.audioListAvailable ? Ui.Theme.textPrimary : Ui.Theme.error
                            }
                        }
                        Rectangle {
                            objectName: "morePill"
                            height: 76
                            radius: 38
                            width: morePillLabel.implicitWidth + 64
                            color: root.osdFocus === "more" && root.sheetKind === "" ? Ui.Theme.plateFocus : Ui.Theme.plate
                            Ui.UiText {
                                id: morePillLabel
                                objectName: "morePillLabel"
                                anchors.centerIn: parent
                                text: qsTr("More")
                                font.pixelSize: 28
                                font.weight: 600
                                color: Ui.Theme.textPrimary
                            }
                        }
                    }

                    Ui.UiText {
                        objectName: "osdHint"
                        anchors.right: parent.right
                        anchors.verticalCenter: parent.verticalCenter
                        text: qsTr("Left/Right — 10 seconds · OK — pause")
                        font.pixelSize: 24
                        font.weight: 500
                        color: Ui.Theme.textHint
                    }
                }
            }

            Item {
                objectName: "trackSheet"
                visible: root.sheetKind !== ""
                anchors.top: parent.top
                anchors.bottom: parent.bottom
                anchors.right: parent.right
                width: 720
                z: 4
                Rectangle {
                    anchors.fill: parent
                    color: Qt.rgba(0.08, 0.08, 0.09, 0.55)
                }
                Rectangle {
                    anchors.fill: parent
                    color: Qt.rgba(0.13, 0.14, 0.15, 0.88)
                    border.width: 1
                    border.color: Qt.rgba(1, 1, 1, 0.08)
                    Column {
                        anchors.fill: parent
                        anchors.leftMargin: 56
                        anchors.rightMargin: 56
                        anchors.topMargin: 112
                        anchors.bottomMargin: 64
                        spacing: 8
                        Ui.UiText {
                            text: qsTr("Playback")
                            font.pixelSize: 24
                            font.weight: 600
                            font.letterSpacing: 1.9
                            font.capitalization: Font.AllUppercase
                            color: Ui.Theme.textSecondary
                        }
                        Ui.UiText {
                            objectName: "sheetTitle"
                            text: root.sheetKind === "audio" ? qsTr("Audio") : qsTr("Subtitles")
                            font.pixelSize: 52
                            font.weight: 700
                            font.letterSpacing: -1
                            bottomPadding: 32
                        }
                        Repeater {
                            model: root.openSheetCount
                            delegate: Rectangle {
                                id: sheetRow
                                objectName: "sheetRow"
                                required property int index
                                width: 608
                                height: Math.max(104, sheetLabel.implicitHeight + (sheetDetail.visible ? sheetDetail.implicitHeight + 4 : 0) + 36)
                                radius: 28
                                color: sheetRowFocused ? Ui.Theme.plateFocus : Ui.Theme.plate
                                readonly property var entry: root.sheetTrack(index)
                                readonly property bool sheetRowFocused: (root.sheetKind === "audio" && root.audioIndex === index)
                                        || (root.sheetKind === "subtitles" && root.subtitleIndex === index)
                                readonly property bool current: entry && (
                                    (root.sheetKind === "audio" && String(entry.id) === root.audioTrackId)
                                    || (root.sheetKind === "subtitles" && String(entry.id) === root.subtitleTrackId))
                                Rectangle {
                                    width: 18
                                    height: 18
                                    radius: 9
                                    anchors.left: parent.left
                                    anchors.leftMargin: 28
                                    anchors.verticalCenter: parent.verticalCenter
                                    visible: sheetRow.current
                                    color: Ui.Theme.ember
                                }
                                Column {
                                    anchors.left: parent.left
                                    anchors.right: parent.right
                                    anchors.leftMargin: 68
                                    anchors.rightMargin: 28
                                    anchors.verticalCenter: parent.verticalCenter
                                    spacing: 4
                                    Ui.UiText {
                                        id: sheetLabel
                                        width: parent.width
                                        text: sheetRow.entry && sheetRow.entry.label !== undefined ? String(sheetRow.entry.label) : ""
                                        font.pixelSize: 32
                                        font.weight: 600
                                        color: Ui.Theme.textPrimary
                                    }
                                    Ui.UiText {
                                        id: sheetDetail
                                        width: parent.width
                                        visible: sheetRow.entry && sheetRow.entry.detail !== undefined && String(sheetRow.entry.detail) !== ""
                                        text: visible ? String(sheetRow.entry.detail) : ""
                                        font.pixelSize: 26
                                        font.weight: 500
                                        color: Ui.Theme.textSecondary
                                    }
                                }
                            }
                        }
                        Item { width: 1; height: 1 }
                        Ui.UiText {
                            text: qsTr("Back closes")
                            font.pixelSize: 26
                            font.weight: 500
                            color: Ui.Theme.textHint
                        }
                    }
                }
            }
        }
    }
}
