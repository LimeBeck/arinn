// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import QtTest
import "../../apps/arinn-shell/ui" as Ui
import "../../apps/arinn-shell/ui/screens/session" as Session

TestCase {
    id: suite
    name: "Session"
    width: 1920
    height: 1080
    visible: true
    when: windowShown

    property var view
    Component {
        id: factory
        Session.SessionView {
            title: "Fixture title"
            subtitle: "Fixture artist"
            playbackState: "playing"
            durationUs: 125000000
            positionUs: 65000000
            artworkHue: "#336699"
            availableActions: ["play", "pause", "playPause", "seek", "previous", "stop"]
        }
    }
    Component { id: plainFactory; Session.SessionView {} }
    SignalSpy { id: plays; signalName: "playRequested" }
    SignalSpy { id: pauses; signalName: "pauseRequested" }
    SignalSpy { id: toggles; signalName: "playPauseRequested" }
    SignalSpy { id: seeks; signalName: "seekRequested" }
    SignalSpy { id: nexts; signalName: "nextRequested" }
    SignalSpy { id: previouses; signalName: "previousRequested" }
    SignalSpy { id: stops; signalName: "stopRequested" }
    SignalSpy { id: backs; signalName: "backRequested" }

    function child(name) {
        const item = findChild(view, name);
        verify(item !== null, name);
        return item;
    }
    function focused(name) { tryCompare(child(name), "activeFocus", true, 5000, name); }
    function insideView(item) {
        let cursor = item;
        while (cursor) {
            if (cursor === view) return true;
            cursor = cursor.parent;
        }
        return false;
    }
    function arm(target) {
        plays.target = target; plays.clear();
        pauses.target = target; pauses.clear();
        toggles.target = target; toggles.clear();
        seeks.target = target; seeks.clear();
        nexts.target = target; nexts.clear();
        previouses.target = target; previouses.clear();
        stops.target = target; stops.clear();
        backs.target = target; backs.clear();
    }
    function createView(props) {
        mouseMove(suite, 10, 10);
        view = createTemporaryObject(factory, suite, props || {});
        verify(view);
        arm(view);
        // The view is created under the cursor. Deliver that hover before the
        // test, then leave D-pad mode so a repeat of the same point is not a move.
        mouseMove(view, 8, 8);
        view.pointerMode = false;
    }
    function init() { createView(); }
    function cleanup() { arm(null); }

    function test_play_and_pause() {
        focused("action-play");
        compare(child("sessionTitle").text, "Fixture title");
        compare(child("sessionSubtitle").text, "Fixture artist");
        compare(child("playbackLabel").text, "Playing");
        compare(child("positionLabel").text, "1:05");
        compare(child("durationLabel").text, "2:05");
        compare(child("progressTrack").fraction, 65000000 / 125000000);
        verify(Qt.colorEqual(child("artworkPlate").color, "#336699"));
        keyClick(Qt.Key_Return);
        compare(plays.count, 1);
        compare(pauses.count, 0);
        keyClick(Qt.Key_Right);
        focused("action-pause");
        keyClick(Qt.Key_Space);
        compare(pauses.count, 1);
        keyClick(Qt.Key_Right);
        focused("action-playPause");
        keyClick(Qt.Key_Enter);
        compare(toggles.count, 1);
        compare(plays.count, 1);
        view.availableActions = ["pause"];
        tryCompare(child("action-play"), "enabled", false);
        mouseClick(child("action-play"), child("action-play").width / 2, child("action-play").height / 2);
        compare(plays.count, 1);
        focused("action-pause");
        keyClick(Qt.Key_Return);
        compare(pauses.count, 2);
        view.durationUs = -1;
        compare(child("durationLabel").text, "—");
        compare(child("progressTrack").fraction, 0);
        view.playbackState = "buffering";
        compare(child("playbackLabel").text, "Unsupported");
        compare(view.positionUs, 65000000);
    }

    function test_disabled_next_does_not_emit() {
        const next = child("action-next");
        verify(next.visible);
        compare(next.enabled, false);
        compare(next.opacity, 0.4);
        focused("action-play");
        mouseClick(next, next.width / 2, next.height / 2);
        compare(nexts.count, 0);
        compare(plays.count, 0);
        focused("action-play");
        keyClick(Qt.Key_Right); focused("action-pause");
        keyClick(Qt.Key_Right); focused("action-playPause");
        keyClick(Qt.Key_Right); focused("action-seek");
        keyClick(Qt.Key_Right); focused("action-stop");
        keyClick(Qt.Key_Return);
        compare(stops.count, 1);
        compare(nexts.count, 0);
        compare(plays.count, 0);
    }

    function test_seek_intent_and_disabled_seek() {
        focused("action-play");
        keyClick(Qt.Key_Up);
        focused("seekTrack");
        keyClick(Qt.Key_Right);
        compare(seeks.count, 1);
        compare(seeks.signalArguments[0][0], 75000000);
        keyClick(Qt.Key_Left);
        compare(seeks.count, 2);
        compare(seeks.signalArguments[1][0], 55000000);
        compare(view.positionUs, 65000000);
        view.positionUs = 0;
        keyClick(Qt.Key_Left);
        compare(seeks.count, 2);
        view.availableActions = ["play", "pause"];
        tryCompare(child("action-seek"), "enabled", false);
        tryCompare(child("seekTrack"), "opacity", 0.4);
        keyClick(Qt.Key_Right);
        keyClick(Qt.Key_Return);
        compare(seeks.count, 2);
        compare(child("action-seek").visible, true);
    }

    function test_dpad_stays_inside() {
        focused("action-play");
        for (let i = 0; i < 8; ++i) keyClick(Qt.Key_Left);
        focused("action-previous");
        for (let i = 0; i < 8; ++i) keyClick(Qt.Key_Right);
        focused("action-stop");
        keyClick(Qt.Key_Up);
        focused("seekTrack");
        keyClick(Qt.Key_Up);
        focused("seekTrack");
        keyClick(Qt.Key_Down);
        focused("action-stop");
        compare(seeks.count, 0);
        compare(nexts.count, 0);
        verify(insideView(view.Window.window.activeFocusItem));
        compare(view.pointerMode, false);
    }

    function test_repeat_hover_keeps_dpad_mode() {
        const button = child("action-play");
        mouseMove(button, 30, 20);
        tryCompare(view, "pointerMode", true);
        // keyClick delivers the same hover point again. That must not undo D-pad.
        keyClick(Qt.Key_Left);
        compare(view.pointerMode, false);
    }

    function test_empty_shows_no_session() {
        view.sessionState = "empty";
        tryCompare(child("sessionBody"), "visible", false);
        tryCompare(child("transportRow"), "visible", false);
        tryCompare(child("sessionStatus"), "visible", true);
        compare(child("sessionStatus").kind, "empty");
        compare(child("sessionStatus").title, "Nothing playing");
        tryCompare(view, "activeFocus", true);
        keyClick(Qt.Key_Return);
        keyClick(Qt.Key_Right);
        keyClick(Qt.Key_Up);
        compare(plays.count, 0);
        compare(pauses.count, 0);
        compare(toggles.count, 0);
        compare(seeks.count, 0);
        compare(nexts.count, 0);
        compare(previouses.count, 0);
        compare(stops.count, 0);
        verify(insideView(view.Window.window.activeFocusItem));
    }

    function test_unavailable_shows_no_session() {
        view.sessionState = "unavailable";
        tryCompare(child("sessionBody"), "visible", false);
        tryCompare(child("sessionStatus"), "visible", true);
        compare(child("sessionStatus").kind, "unavailable");
        compare(child("sessionStatus").title, "Session unavailable");
        keyClick(Qt.Key_Return);
        keyClick(Qt.Key_Left);
        keyClick(Qt.Key_Down);
        compare(plays.count, 0);
        compare(nexts.count, 0);
        compare(stops.count, 0);
        verify(insideView(view.Window.window.activeFocusItem));
        view.sessionState = "ready";
        focused("action-play");
        keyClick(Qt.Key_Return);
        compare(plays.count, 1);
    }

    function test_back_matches_escape() {
        focused("action-play");
        keyClick(Qt.Key_Escape);
        compare(backs.count, 1);
        compare(stops.count, 0);
        compare(plays.count, 0);
        backs.clear();
        keyClick(Qt.Key_Back);
        compare(backs.count, 1);
        compare(stops.count, 0);
        compare(plays.count, 0);
        compare(view.positionUs, 65000000);
        compare(view.sessionState, "ready");
        backs.clear();
        keyClick(Qt.Key_Backspace);
        compare(backs.count, 1);
        compare(stops.count, 0);
        keyClick(Qt.Key_Up);
        focused("seekTrack");
        backs.clear();
        keyClick(Qt.Key_Back);
        compare(backs.count, 1);
        compare(seeks.count, 0);
    }

    function test_default_artwork_uses_theme_plate() {
        mouseMove(suite, 10, 10);
        const plain = createTemporaryObject(plainFactory, suite);
        verify(plain);
        const plate = findChild(plain, "artworkPlate");
        verify(plate !== null);
        verify(Qt.colorEqual(plate.color, Ui.Theme.plate));
    }
}
