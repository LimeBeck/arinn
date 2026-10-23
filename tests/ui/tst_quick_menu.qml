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
            nowPlaying: ({id: "song", title: "Song", state: "playing", playPause: true})
            audio: "muted"
            network: "wifi"
            wifiLevel: 3
        }
    }
    SignalSpy { id: playPauseSpy; signalName: "playPauseRequested" }
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
        for (const spy of [playPauseSpy, backSpy, homeSpy, closeSpy, sleepSpy, powerSpy, muteSpy, volumeSpy]) {
            spy.target = menu;
            spy.clear();
        }
        menu.forceActiveFocus();
        menu.start();
    }

    function test_opens_on_now_playing_and_toggles_it() {
        focused("quickMenuMedia");
        compare(child("quickMenuMediaLabel").text, "Song");
        keyClick(Qt.Key_Return);
        compare(playPauseSpy.count, 1);
        compare(playPauseSpy.signalArguments[0][0], "song");
    }

    function test_rows_in_order_and_network_is_not_focusable() {
        focused("quickMenuMedia");
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
        focused("quickMenuMedia");
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
        focused("quickMenuMedia");
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
        menu.nowPlaying = {id: "radio", title: "Radio", state: "playing", playPause: false};
        verify(!child("quickMenuMedia").enabled);
        menu.nowPlaying = null;
        compare(child("quickMenuMediaLabel").text, "Nothing is playing");
        menu.start();
        focused("quickMenuSleep");
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
        focused("quickMenuMedia");
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
