// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import QtTest
import "../../apps/arinn-shell/ui/screens/player" as Player

TestCase {
    id: suite
    name: "PlayerOsd"
    width: 1920
    height: 1080
    visible: true
    when: windowShown

    property var player
    Component {
        id: factory
        Player.PlayerView {
            width: 1920
            height: 1080
            title: "North"
            position: 30
            duration: 120
            playing: false
            seekSupported: true
            audioTracks: [{id: "a1", label: "One"}, {id: "a2", label: "Two"}]
            subtitleTracks: [{id: "s1", label: "Notes"}]
            audioTrackId: "a1"
            subtitleTrackId: "s1"
        }
    }

    SignalSpy { id: plays; signalName: "play" }
    SignalSpy { id: pauses; signalName: "pause" }
    SignalSpy { id: seeks; signalName: "seek" }
    SignalSpy { id: audioPicks; signalName: "selectAudio" }
    SignalSpy { id: subtitlePicks; signalName: "selectSubtitle" }
    SignalSpy { id: stops; signalName: "stop" }
    SignalSpy { id: leaves; signalName: "leave" }

    function child(name) {
        const item = findChild(player, name)
        verify(item !== null, name)
        return item
    }
    function arm(spy) {
        spy.target = player
        spy.clear()
    }
    function assertNoCommands() {
        compare(plays.count, 0)
        compare(pauses.count, 0)
        compare(seeks.count, 0)
        compare(audioPicks.count, 0)
        compare(subtitlePicks.count, 0)
        compare(stops.count, 0)
        compare(leaves.count, 0)
    }
    function seekTarget(index) {
        return seeks.signalArguments[index][0]
    }

    function init() {
        mouseMove(suite, 10, 10)
        player = createTemporaryObject(factory, suite)
        verify(player)
        arm(plays); arm(pauses); arm(seeks)
        arm(audioPicks); arm(subtitlePicks); arm(stops); arm(leaves)
        player.forceActiveFocus()
        tryCompare(player, "activeFocus", true)
    }
    function cleanup() {
        plays.target = null
        pauses.target = null
        seeks.target = null
        audioPicks.target = null
        subtitlePicks.target = null
        stops.target = null
        leaves.target = null
    }

    function test_logical_surface_and_fixture_text() {
        compare(child("playbackSurface").width, 1920)
        compare(child("playbackSurface").height, 1080)
        compare(child("titleLabel").text, "North")
        compare(child("videoLabel").text, "video")
        compare(child("positionLabel").text, "0:30")
        compare(child("durationLabel").text, "2:00")
        compare(child("pausedMark").visible, true)
        compare(child("timelineKnob").visible, true)
        compare(child("timelineThumb").visible, true)
        compare(child("thumbnailLabel").text, "thumbnail")
        compare(child("thumbnailLabel").text.length, 9)
        compare(child("subtitlePillLabel").text, "Subtitles · Notes")
        compare(child("audioPillLabel").text, "Audio · One")
        compare(child("morePillLabel").text, "More")
        const surface = child("playbackSurface")
        const title = child("titleLabel").mapToItem(surface, 0, 0)
        verify(Math.abs(title.x - 120) < 2)
        verify(Math.abs(title.y - 72) < 2)
        const timeline = child("timelineRow")
        const timelineBottom = timeline.mapToItem(surface, 0, timeline.height)
        verify(Math.abs(timelineBottom.y - (1080 - 180)) < 2)
        const pills = child("pillRow")
        const pillBottom = pills.mapToItem(surface, 0, pills.height)
        verify(Math.abs(pillBottom.y - (1080 - 72)) < 2)
        verify(pillBottom.y > timelineBottom.y)
        compare(player.osdVisible, true)
        compare(player.osdFocus, "timeline")
        compare(player.sheetKind, "")
        compare(player.osdIdleMs, 4000)
        compare(player.seekStepSeconds, 10)
        compare(child("statusPane").visible, false)
        compare(child("osd").visible, true)
        compare(child("trackSheet").visible, false)
    }

    function test_startup_emits_nothing() {
        wait(50)
        assertNoCommands()
        compare(player.osdVisible, true)
        compare(player.playing, false)
    }

    function test_back_hides_then_leaves_data() {
        return [
            {tag: "escape", key: Qt.Key_Escape},
            {tag: "back", key: Qt.Key_Back},
            {tag: "backspace", key: Qt.Key_Backspace}
        ]
    }
    function test_back_hides_then_leaves(data) {
        keyClick(data.key)
        compare(player.osdVisible, false)
        compare(leaves.count, 0)
        compare(stops.count, 0)
        compare(player.playing, false)
        keyClick(data.key)
        compare(leaves.count, 1)
        compare(stops.count, 0)
        compare(plays.count, 0)
        compare(pauses.count, 0)
        compare(player.playing, false)
        compare(player.osdVisible, false)
    }

    function test_hidden_arrows_follow_section_64() {
        player.osdVisible = false
        keyClick(Qt.Key_Right)
        compare(seeks.count, 1)
        verify(Math.abs(seekTarget(0) - 40) < 0.001)
        compare(player.position, 30)
        compare(player.osdVisible, true)
        compare(player.osdFocus, "timeline")
        compare(plays.count, 0)
        compare(pauses.count, 0)

        player.osdVisible = false
        keyClick(Qt.Key_Left)
        compare(seeks.count, 2)
        verify(Math.abs(seekTarget(1) - 20) < 0.001)
        compare(player.osdFocus, "timeline")

        player.position = 5
        player.osdVisible = false
        keyClick(Qt.Key_Left)
        verify(Math.abs(seekTarget(2) - 0) < 0.001)
        player.position = 115
        player.osdVisible = false
        keyClick(Qt.Key_Right)
        verify(Math.abs(seekTarget(3) - 120) < 0.001)

        player.osdVisible = false
        player.osdFocus = "audio"
        keyClick(Qt.Key_Return)
        compare(player.osdVisible, true)
        compare(player.osdFocus, "timeline")
        compare(plays.count, 0)
        compare(pauses.count, 0)
        compare(seeks.count, 4)

        player.osdVisible = false
        keyClick(Qt.Key_Up)
        compare(seeks.count, 4)
        compare(plays.count, 0)
        compare(pauses.count, 0)
        compare(player.osdVisible, true)
        compare(player.osdFocus, "timeline")

        player.osdVisible = false
        keyClick(Qt.Key_Down)
        compare(seeks.count, 4)
        compare(plays.count, 0)
        compare(player.osdFocus, "timeline")
    }

    function test_visible_ok_toggles_without_changing_fixture() {
        keyClick(Qt.Key_Return)
        compare(plays.count, 1)
        compare(pauses.count, 0)
        compare(seeks.count, 0)
        compare(player.playing, false)
        compare(player.osdVisible, true)
        player.playing = true
        keyClick(Qt.Key_Enter)
        compare(pauses.count, 1)
        compare(plays.count, 1)
        compare(player.playing, true)
        compare(child("pausedMark").visible, false)
        compare(stops.count, 0)
        compare(leaves.count, 0)
    }

    function test_seek_hidden_when_unsupported() {
        player.seekSupported = false
        player.osdVisible = false
        player.osdFocus = "audio"
        keyClick(Qt.Key_Right)
        compare(seeks.count, 0)
        compare(player.position, 30)
        compare(player.osdVisible, true)
        compare(player.osdFocus, "timeline")
        keyClick(Qt.Key_Left)
        compare(seeks.count, 0)
        compare(player.osdVisible, true)
    }

    function test_track_intents_need_ok() {
        keyClick(Qt.Key_Down)
        compare(player.osdFocus, "subtitles")
        compare(child("timelineKnob").visible, false)
        keyClick(Qt.Key_Right)
        compare(player.osdFocus, "audio")
        compare(seeks.count, 0)
        keyClick(Qt.Key_Return)
        compare(player.sheetKind, "audio")
        compare(child("trackSheet").visible, true)
        compare(audioPicks.count, 0)
        compare(player.osdFocus, "audio")
        keyClick(Qt.Key_Down)
        compare(player.audioIndex, 1)
        compare(audioPicks.count, 0)
        compare(player.audioTrackId, "a1")
        keyClick(Qt.Key_Return)
        compare(audioPicks.count, 1)
        compare(audioPicks.signalArguments[0][0], "a2")
        compare(player.sheetKind, "")
        compare(player.osdFocus, "audio")
        compare(player.audioTrackId, "a1")
        keyClick(Qt.Key_Left)
        compare(player.osdFocus, "subtitles")
        keyClick(Qt.Key_Return)
        compare(player.sheetKind, "subtitles")
        compare(subtitlePicks.count, 0)
        keyClick(Qt.Key_Escape)
        compare(player.sheetKind, "")
        compare(player.osdVisible, true)
        compare(player.osdFocus, "subtitles")
        compare(leaves.count, 0)
        compare(stops.count, 0)
        keyClick(Qt.Key_Return)
        compare(player.sheetKind, "subtitles")
        keyClick(Qt.Key_Return)
        compare(subtitlePicks.count, 1)
        compare(subtitlePicks.signalArguments[0][0], "s1")
        compare(player.sheetKind, "")
        compare(player.subtitleTrackId, "s1")
        compare(seeks.count, 0)
    }

    function test_missing_tracks_are_unavailable() {
        player.audioTracks = []
        player.subtitleTracks = []
        compare(player.audioListAvailable, false)
        compare(player.subtitleListAvailable, false)
        compare(player.audioCaption, "Audio unavailable")
        compare(player.subtitleCaption, "Subtitles unavailable")
        compare(child("audioPillLabel").text, "Audio unavailable")
        compare(child("subtitlePillLabel").text, "Subtitles unavailable")
        keyClick(Qt.Key_Down)
        compare(player.osdFocus, "subtitles")
        keyClick(Qt.Key_Return)
        compare(player.sheetKind, "")
        compare(subtitlePicks.count, 0)
        keyClick(Qt.Key_Right)
        compare(player.osdFocus, "audio")
        keyClick(Qt.Key_Return)
        compare(player.sheetKind, "")
        compare(audioPicks.count, 0)
        compare(plays.count, 0)
        compare(pauses.count, 0)
    }

    function test_idle_hide_is_local_while_playing() {
        player.osdIdleMs = 80
        player.playing = true
        tryCompare(player, "osdVisible", false, 2000)
        assertNoCommands()
        compare(player.playing, true)
        compare(player.position, 30)
    }

    function test_paused_osd_does_not_hide() {
        player.osdIdleMs = 80
        player.playing = false
        player.osdVisible = true
        wait(200)
        compare(player.osdVisible, true)
        assertNoCommands()
    }

    function test_states_block_transport_intents_data() {
        return [
            {tag: "loading", state: "loading", heading: "Loading"},
            {tag: "error", state: "error", heading: "Playback failed"},
            {tag: "unavailable", state: "unavailable", heading: "Playback unavailable"}
        ]
    }
    function test_states_block_transport_intents(data) {
        player.playbackState = data.state
        compare(child("statusPane").visible, true)
        compare(child("osd").visible, false)
        compare(child("statusHeading").text, data.heading)
        keyClick(Qt.Key_Return)
        keyClick(Qt.Key_Right)
        keyClick(Qt.Key_MediaPlay)
        compare(plays.count, 0)
        compare(pauses.count, 0)
        compare(seeks.count, 0)
        compare(stops.count, 0)
        keyClick(Qt.Key_Escape)
        compare(leaves.count, 1)
        compare(stops.count, 0)
        compare(player.playing, false)
    }

    // Qt Test keyClick aborts on media keys (no ASCII mapping). The same
    // handler serves real key events; this drives it with those key codes.
    function pressMedia(key) {
        const event = {key: key, isAutoRepeat: false, accepted: false}
        player.handleKey(event)
        verify(event.accepted)
    }
    function test_media_keys_emit_intents_while_osd_hidden() {
        player.osdVisible = false
        pressMedia(Qt.Key_MediaPlay)
        compare(plays.count, 1)
        compare(player.playing, false)
        compare(player.osdVisible, true)
        compare(player.osdFocus, "timeline")
        player.osdVisible = false
        player.playing = true
        pressMedia(Qt.Key_MediaPause)
        compare(pauses.count, 1)
        compare(player.playing, true)
        player.osdVisible = false
        pressMedia(Qt.Key_MediaStop)
        compare(stops.count, 1)
        compare(leaves.count, 0)
        compare(player.playing, true)
        player.osdVisible = false
        pressMedia(Qt.Key_MediaTogglePlayPause)
        compare(pauses.count, 2)
        compare(plays.count, 1)
    }
}
