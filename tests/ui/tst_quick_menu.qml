// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import QtTest
import "../../apps/arinn-shell/ui" as Ui

// The quick menu sheet (spec §5.7, §6.3, ADR-0106).
TestCase {
    id: suite
    name: "QuickMenu"
    width: 1920
    height: 1080
    visible: true
    when: windowShown

    property var menu
    Component {
        id: factory
        Ui.QuickMenu {
            pointerMode: false
            clockText: "21:47"
            sessions: [({id: "song", kind: "music", title: "Song", subtitle: "Singer", state: "playing",
                         playPause: true, previous: true, next: true, open: true, eyebrow: "Night Wave · Fixture music",
                         positionUs: 30e6, durationUs: 120e6})]
            audio: "muted"
            network: "wifi"
            wifiLevel: 3
        }
    }
    SignalSpy { id: playPauseSpy; signalName: "playPauseRequested" }
    SignalSpy { id: previousSpy; signalName: "previousRequested" }
    SignalSpy { id: nextSpy; signalName: "nextRequested" }
    SignalSpy { id: openSpy; signalName: "openPlayerRequested" }
    SignalSpy { id: backSpy; signalName: "backRequested" }
    SignalSpy { id: homeSpy; signalName: "homeRequested" }
    SignalSpy { id: closeSpy; signalName: "closeShellRequested" }
    SignalSpy { id: sleepSpy; signalName: "sleepRequested" }
    SignalSpy { id: powerSpy; signalName: "powerMenuRequested" }
    SignalSpy { id: muteSpy; signalName: "muteToggleRequested" }
    SignalSpy { id: volumeSpy; signalName: "volumeStepRequested" }

    function child(name) {
        const item = findChild(menu, name);
        verify(item !== null, name);
        return item;
    }
    function focused(name) {
        tryCompare(child(name), "activeFocus", true, 5000, name);
    }
    function init() {
        mouseMove(suite, 10, 10);
        menu = createTemporaryObject(factory, suite);
        verify(menu);
        for (const spy of [playPauseSpy, previousSpy, nextSpy, openSpy, backSpy, homeSpy, closeSpy, sleepSpy, powerSpy, muteSpy, volumeSpy]) {
            spy.target = menu;
            spy.clear();
        }
        menu.forceActiveFocus();
        menu.start();
    }

    // #805 (mh-06): while a session plays or is paused, Now playing is the
    // media card; focus starts on play/pause.
    function test_opens_on_now_playing_and_toggles_it() {
        focused("quickMenuCardPlayPause");
        verify(child("quickMenuMediaCard").visible);
        verify(!child("quickMenuMedia").visible);
        compare(child("quickMenuCardTitle").text, "Song");
        compare(child("quickMenuCardTitle").font.pixelSize, 32);
        compare(child("quickMenuCardTitle").font.weight, 600);
        compare(child("quickMenuCardSubtitle").text, "Singer");
        compare(child("quickMenuCardSubtitle").font.pixelSize, 24);
        compare(child("quickMenuCardEyebrow").text, "Night Wave · Fixture music");
        compare(child("quickMenuMediaCard").width, 624);
        compare(child("quickMenuMediaCard").height, 284);
        compare(child("quickMenuCardCover").width, 128);
        compare(child("quickMenuCardPlayPause").width, 64);
        verify(child("quickMenuCardProgress").visible);
        keyClick(Qt.Key_Return);
        compare(playPauseSpy.count, 1);
        compare(playPauseSpy.signalArguments[0][0], "song");
    }
    // Left and Right along the card's row, stopping at its ends; previous and
    // next only with the capability; Open player for the shown session.
    function test_card_row_and_capabilities() {
        focused("quickMenuCardPlayPause");
        keyClick(Qt.Key_Left); focused("quickMenuCardPrevious");
        keyClick(Qt.Key_Left); focused("quickMenuCardPrevious");
        keyClick(Qt.Key_Return); compare(previousSpy.count, 1); compare(previousSpy.signalArguments[0][0], "song");
        keyClick(Qt.Key_Right); keyClick(Qt.Key_Right); focused("quickMenuCardNext");
        keyClick(Qt.Key_Return); compare(nextSpy.count, 1);
        keyClick(Qt.Key_Right); focused("quickMenuCardOpen");
        keyClick(Qt.Key_Right); focused("quickMenuCardOpen");
        keyClick(Qt.Key_Return); compare(openSpy.count, 1); compare(openSpy.signalArguments[0][0], "song");
        // A video session: no previous or next, the poster and the episode.
        menu.sessions = [{id: "film", kind: "media", title: "Film", subtitle: "S1 E2", state: "paused",
                          playPause: true, open: true}];
        menu.start();
        focused("quickMenuCardPlayPause");
        verify(!child("quickMenuCardPrevious").enabled);
        verify(!child("quickMenuCardNext").enabled);
        compare(child("quickMenuCardSubtitle").text, "S1 E2");
        keyClick(Qt.Key_Left); focused("quickMenuCardPlayPause");
        keyClick(Qt.Key_Right); focused("quickMenuCardOpen");
    }

    function test_rows_in_order_and_network_is_not_focusable() {
        focused("quickMenuCardPlayPause");
        // Without platform volume control the row says so and is skipped.
        verify(!child("quickMenuVolume").enabled);
        verify(child("quickMenuVolumeDescription").text.indexOf("Muted") === 0);
        compare(child("quickMenuNetworkState").text, "Wi-Fi · signal 3 of 4");
        const order = ["quickMenuSleep", "quickMenuPower", "quickMenuHome", "quickMenuClose"];
        for (const name of order) {
            keyClick(Qt.Key_Down);
            focused(name);
        }
        // No wrap at the end, and Left/Right stay inside the sheet.
        keyClick(Qt.Key_Down);
        keyClick(Qt.Key_Left);
        keyClick(Qt.Key_Right);
        focused("quickMenuClose");
        for (let i = 0; i < 6; ++i) keyClick(Qt.Key_Up);
        focused("quickMenuCardPlayPause");
    }

    function test_actions_leave_as_intents() {
        keyClick(Qt.Key_Down);
        focused("quickMenuSleep");
        keyClick(Qt.Key_Return);
        compare(sleepSpy.count, 1);
        keyClick(Qt.Key_Down);
        keyClick(Qt.Key_Return);
        compare(powerSpy.count, 1);
        keyClick(Qt.Key_Down);
        focused("quickMenuHome");
        keyClick(Qt.Key_Return);
        compare(homeSpy.count, 1);
        keyClick(Qt.Key_Down);
        keyClick(Qt.Key_Return);
        compare(closeSpy.count, 1);
    }

    function test_back_and_home_keys() {
        for (const key of [Qt.Key_Escape, Qt.Key_Backspace, Qt.Key_Back])
            keyClick(key);
        compare(backSpy.count, 3);
        keyClick(Qt.Key_Home);
        compare(homeSpy.count, 1);
        // Menu does not leave the sheet scope.
        keyClick(Qt.Key_Menu);
        focused("quickMenuCardPlayPause");
        compare(backSpy.count, 3);
    }

    function test_outside_click_is_back() {
        mouseClick(menu, 300, 500);
        compare(backSpy.count, 1);
        mouseClick(child("quickMenuSheet"), 600, 950);
        compare(backSpy.count, 1);
    }

    function test_volume_row_when_the_platform_controls_it() {
        menu.volumeControl = true;
        menu.audio = "unmuted";
        keyClick(Qt.Key_Down);
        focused("quickMenuVolume");
        keyClick(Qt.Key_Right);
        keyClick(Qt.Key_Left);
        compare(volumeSpy.count, 2);
        compare(volumeSpy.signalArguments[0][0], 1);
        compare(volumeSpy.signalArguments[1][0], -1);
        keyClick(Qt.Key_Return);
        compare(muteSpy.count, 1);
        focused("quickMenuVolume");
    }

    // The platform's reply (SystemControl1) wins over SystemStatus1's mute
    // state and is the only source of a level; none is ever guessed.
    function test_volume_row_shows_the_platform_level() {
        menu.volumeControl = true;
        menu.audio = "unmuted";
        compare(child("quickMenuVolumeDescription").text, "Sound on · Left/Right: volume, OK: mute");
        menu.volumeKnown = true;
        menu.volumeLevel = 40;
        compare(child("quickMenuVolumeDescription").text, "Sound on · 40% · Left/Right: volume, OK: mute");
        menu.volumeMuted = true;
        compare(child("quickMenuVolumeDescription").text, "Muted · Left/Right: volume, OK: sound on");
        verify(child("quickMenuVolumeDescription").text.indexOf("40") < 0);
        menu.volumeKnown = false;
        compare(child("quickMenuVolumeDescription").text, "Sound on · Left/Right: volume, OK: mute");
    }

    function test_media_without_play_pause_or_session() {
        menu.sessions = [{id: "radio", kind: "media", title: "Radio", state: "playing", playPause: false}];
        verify(!child("quickMenuCardPlayPause").enabled);
        compare(child("quickMenuCardEyebrow").text, "Cannot be paused from Arinn");
        menu.sessions = [{id: "radio", kind: "media", title: "Radio", state: "paused", playPause: false}];
        compare(child("quickMenuCardEyebrow").text, "Cannot be resumed from Arinn");
        // Nothing on the card works: focus starts on the next row.
        menu.start();
        focused("quickMenuSleep");
        // A stopped session is not shown; without one the plain row stays.
        menu.sessions = [{id: "radio", kind: "media", title: "Radio", state: "stopped", playPause: true}];
        verify(!child("quickMenuMediaCard").visible);
        menu.sessions = [];
        verify(child("quickMenuMedia").visible);
        compare(child("quickMenuMediaLabel").text, "Nothing is playing");
        menu.start();
        focused("quickMenuSleep");
    }
    // #805: one precedence order. Playing ranks above paused; then the change
    // observed last; with none observed, the native music session; no
    // product is preferred. The other session is a compact row under the
    // card, and the two change places only when the order changes.
    function music(state) {
        return {id: "music", kind: "music", title: "Track", subtitle: "Artist", state: state, playPause: true, open: true};
    }
    function video(state) {
        return {id: "video", kind: "media", title: "Film", subtitle: "S1 E2", state: state, playPause: true, open: true};
    }
    function test_precedence_and_the_second_session() {
        // Both paused when first seen: the native music session first.
        menu.sessions = [video("paused"), music("paused")];
        compare(menu.nowPlaying.id, "music");
        compare(menu.otherSession.id, "video");
        verify(child("quickMenuOtherSession").visible);
        compare(child("quickMenuOtherSession").height, 88);
        compare(child("quickMenuOtherTitle").text, "Film");
        compare(child("quickMenuOtherState").text, "Paused");
        // The video starts playing: playing ranks above paused, they swap.
        menu.sessions = [video("playing"), music("paused")];
        compare(menu.nowPlaying.id, "video");
        compare(child("quickMenuCardTitle").text, "Film");
        compare(child("quickMenuOtherTitle").text, "Track");
        // Both play: the one whose change was observed last (the music).
        menu.sessions = [video("playing"), music("playing")];
        compare(menu.nowPlaying.id, "music");
        // Pausing the one in the row moves nothing: playing still ranks first.
        menu.sessions = [video("paused"), music("playing")];
        compare(menu.nowPlaying.id, "music");
        // Both paused: the change observed last (the video's pause).
        menu.sessions = [video("paused"), music("paused")];
        compare(menu.nowPlaying.id, "music");
        menu.sessions = [video("playing"), music("paused")];
        menu.sessions = [video("paused"), music("paused")];
        compare(menu.nowPlaying.id, "video");
        // A stopped session takes no part.
        menu.sessions = [video("stopped"), music("paused")];
        verify(!child("quickMenuOtherSession").visible);
        compare(menu.nowPlaying.id, "music");
    }
    // The card's controls act only on its session; OK on the row toggles the
    // row's. Down: card, row, then Sound; Up from the row: play/pause.
    function test_controls_follow_the_shown_session_and_down_order() {
        menu.volumeControl = true;
        menu.sessions = [video("paused"), music("playing")];
        menu.start();
        focused("quickMenuCardPlayPause");
        keyClick(Qt.Key_Return);
        compare(playPauseSpy.signalArguments[0][0], "music");
        keyClick(Qt.Key_Down); focused("quickMenuOtherSession");
        keyClick(Qt.Key_Return);
        compare(playPauseSpy.signalArguments[1][0], "video");
        keyClick(Qt.Key_Up); focused("quickMenuCardPlayPause");
        keyClick(Qt.Key_Down); keyClick(Qt.Key_Down); focused("quickMenuVolume");
        // Without a second session, Down from the card goes to Sound.
        menu.sessions = [music("playing")];
        menu.start();
        keyClick(Qt.Key_Down); focused("quickMenuVolume");
    }
    // Brief *Fitting the sheet*: the rows scroll with focus, the focused row
    // never at an edge; fades show that more rows follow; opening starts at
    // the top on play/pause.
    function test_rows_scroll_with_focus() {
        menu.volumeControl = true;
        menu.sessions = [video("paused"), music("playing")];
        menu.start();
        const column = child("quickMenuColumn");
        compare(column.contentY, 0);
        verify(child("quickMenuBottomFade").visible);
        verify(!child("quickMenuTopFade").visible);
        for (const name of ["quickMenuOtherSession", "quickMenuVolume", "quickMenuSleep", "quickMenuPower",
                            "quickMenuHome", "quickMenuClose"]) {
            keyClick(Qt.Key_Down);
            focused(name);
            const row = child(name);
            tryVerify(() => {
                const top = row.mapToItem(column, 0, 0).y;
                return top >= 0 && top + row.height <= column.height;
            }, 2000, name);
        }
        tryVerify(() => column.contentY > 0);
        verify(child("quickMenuTopFade").visible);
        menu.start();
        tryCompare(column, "contentY", 0);
        focused("quickMenuCardPlayPause");
    }

    // Degraded without the daemon: Open Home and Close Arinn stay available,
    // and focus leaves a row that becomes unavailable (§6.1).
    function test_degraded_without_daemon() {
        keyClick(Qt.Key_Down);
        focused("quickMenuSleep");
        menu.connected = false;
        focused("quickMenuHome");
        for (const name of ["quickMenuMedia", "quickMenuVolume", "quickMenuSleep", "quickMenuPower"])
            verify(!child(name).enabled, name);
        compare(child("quickMenuNetworkState").text, "Network state unknown");
        keyClick(Qt.Key_Up);
        focused("quickMenuHome");
        keyClick(Qt.Key_Down);
        focused("quickMenuClose");
        keyClick(Qt.Key_Return);
        compare(closeSpy.count, 1);
        menu.start();
        focused("quickMenuHome");
        menu.connected = true;
        menu.start();
        focused("quickMenuCardPlayPause");
    }

    function test_notice_and_overlay_dim() {
        verify(!child("quickMenuNotice").visible);
        menu.showNotice("Not yet");
        verify(child("quickMenuNotice").visible);
        compare(child("quickMenuNotice").text, "Not yet");
        // The sheet sits at the right edge, 720 wide.
        const sheet = child("quickMenuSheet");
        compare(sheet.x, 1200);
        compare(sheet.width, 720);
        const scrim = child("quickMenuScrim");
        const shellDim = scrim.color.a;
        menu.overActivity = true;
        verify(scrim.color.a < shellDim);
    }
}
