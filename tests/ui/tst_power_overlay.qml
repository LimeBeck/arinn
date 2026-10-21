// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import QtTest
import "../../apps/arinn-shell/ui/screens/power" as Power

// #148: the power scope and volume HUD over a SystemControl double. The menu
// shows the platform's availability, refusals and accepted requests; nothing
// turns into success without the platform's answer (spec §5.7, ADR-0108).
TestCase {
    id: suite
    name: "PowerOverlay"
    width: 1920
    height: 1080
    visible: true
    when: windowShown

    Component {
        id: controlFactory
        QtObject {
            property string powerState: ""
            property string suspend: "available"
            property string reboot: "available"
            property string powerOff: "available"
            property var inhibitors: []
            property string pendingAction: ""
            property string acceptedAction: ""
            property string refusal: ""
            property string refusalAction: ""
            property bool volumeKnown: false
            property bool volumeMuted: false
            property int volumeLevel: 0
            property var calls: []
            signal volumeShown()
            signal volumeFailed(string refusal)
            function readPower() { calls.push("readPower"); powerState = "ready"; }
            function request(action) { calls.push("request:" + action); pendingAction = action; }
            function reset() { calls.push("reset"); acceptedAction = ""; refusal = ""; refusalAction = ""; }
            function stepVolume(steps) { calls.push("step:" + steps); }
            function toggleMuted() { calls.push("toggle"); }
            // What the model does with the daemon's answer.
            function answer(error) {
                const action = pendingAction;
                pendingAction = "";
                if (error) { refusal = error; refusalAction = action; }
                else acceptedAction = action;
            }
            function confirmVolume(level, muted) {
                volumeKnown = true; volumeLevel = level; volumeMuted = muted;
                volumeShown();
            }
        }
    }
    Component {
        id: overlayFactory
        Power.PowerOverlay {
            keysEnabled: true
            runningApplications: [{title: "Player"}, {title: "Player"}, {title: "Browser"}]
        }
    }
    SignalSpy { id: closedSpy; signalName: "closed" }
    SignalSpy { id: homeSpy; signalName: "homeRequested" }
    SignalSpy { id: volumeSpy; signalName: "volumeUnavailable" }

    property var control
    property var overlay
    function child(name) {
        const item = findChild(overlay, name);
        verify(item !== null, name);
        return item;
    }
    function focused(name) {
        tryCompare(child(name), "activeFocus", true, 5000, name);
    }
    function init() {
        mouseMove(suite, 10, 10);
        control = createTemporaryObject(controlFactory, suite);
        overlay = createTemporaryObject(overlayFactory, suite, {control: control});
        verify(overlay);
        closedSpy.target = overlay; closedSpy.clear();
        homeSpy.target = overlay; homeSpy.clear();
        volumeSpy.target = overlay; volumeSpy.clear();
    }
    function cleanup() {
        closedSpy.target = null;
        homeSpy.target = null;
        volumeSpy.target = null;
        overlay = null;
        control = null;
    }
    function openMenu() {
        overlay.openMenu();
        verify(overlay.open);
        focused("sleepButton");
    }

    function test_opening_reads_power_and_back_closes() {
        verify(!child("powerMenu").visible);
        openMenu();
        compare(control.calls.slice(0, 2), ["reset", "readPower"]);
        verify(child("powerMenu").visible);
        keyClick(Qt.Key_Escape);
        verify(!overlay.open);
        compare(closedSpy.count, 1);
    }

    function test_sleep_waits_for_the_platform() {
        openMenu();
        keyClick(Qt.Key_Return);
        verify(control.calls.indexOf("request:suspend") >= 0);
        compare(overlay.menu.busyAction, "sleep");
        compare(child("sleepDescription").text, "Waiting for the system…");
        // No timer turns the request into success.
        wait(400);
        verify(!child("powerAccepted").visible);
        verify(overlay.open);
        // Accepted: an honest waiting state, not success.
        control.answer("");
        tryCompare(child("powerAccepted"), "visible", true);
        compare(child("powerAccepted").title, "Going to sleep…");
        compare(child("powerAccepted").kind, "loading");
        // The system sleeps: the lifecycle guard hides the shell; on wake the
        // screen underneath is back.
        overlay.sessionBlocked();
        verify(!overlay.open);
        compare(closedSpy.count, 1);
    }

    function test_accepted_back_returns_to_arinn() {
        openMenu();
        keyClick(Qt.Key_Return);
        control.answer("");
        tryCompare(child("powerAccepted"), "visible", true);
        tryCompare(child("powerAccepted"), "activeFocus", true);
        keyClick(Qt.Key_Escape);
        verify(!overlay.open);
        verify(control.calls.lastIndexOf("reset") > control.calls.indexOf("request:suspend"));
    }

    function test_refusal_names_the_inhibitor() {
        control.suspend = "inhibited";
        control.inhibitors = [{what: "sleep", who: "Fixture player", why: "Playing"},
                              {what: "shutdown", who: "Updater", why: "Installing"}];
        openMenu();
        // The row says who blocks it before anything is requested.
        compare(child("sleepDescription").text, "Blocked by Fixture player (Playing)");
        keyClick(Qt.Key_Return);
        control.answer("inhibited");
        const pane = child("powerError");
        tryCompare(pane, "visible", true);
        compare(pane.kind, "error");
        compare(pane.title, "Sleep was refused");
        verify(pane.description.indexOf("Fixture player (Playing)") >= 0, pane.description);
        verify(pane.description.indexOf("Updater") < 0, pane.description);
        verify(!child("powerAccepted").visible);
        // Back clears the refusal and returns to the menu, not success.
        keyClick(Qt.Key_Escape);
        verify(overlay.open);
        tryCompare(pane, "visible", false);
        focused("sleepButton");
    }

    function test_refusal_codes_have_their_own_text_data() {
        return [
            {tag: "denied", text: "The system does not allow it for this user."},
            {tag: "unsupported", text: "This computer does not support it."},
            {tag: "unavailable", text: "Power control is not available right now. Try again later."},
            {tag: "timeout", text: "The system did not answer in time. Try again."},
            {tag: "failed", text: "The system could not do it. Try again."},
            {tag: "inhibited", text: "An application or another power operation is preventing it right now."}
        ];
    }
    function test_refusal_codes_have_their_own_text(data) {
        openMenu();
        keyClick(Qt.Key_Down);
        focused("restartButton");
        keyClick(Qt.Key_Return);
        focused("cancelButton");
        keyClick(Qt.Key_Right);
        keyClick(Qt.Key_Return);
        verify(control.calls.indexOf("request:reboot") >= 0);
        verify(child("confirmButton").busy);
        control.answer(data.tag);
        tryCompare(child("powerError"), "visible", true);
        compare(child("powerError").title, "Restart was refused");
        compare(child("powerError").description, data.text);
    }

    function test_confirmation_shows_blockers_and_live_list() {
        control.powerOff = "denied";
        openMenu();
        keyClick(Qt.Key_Down);
        keyClick(Qt.Key_Down);
        focused("shutdownButton");
        compare(child("shutdownDescription").text, "Not allowed for this user");
        keyClick(Qt.Key_Return);
        focused("cancelButton");
        // Each running application once, from the registry.
        compare(child("closingList").text, "These applications will close: Player, Browser.");
        verify(child("confirmNote").visible);
        compare(child("confirmNote").text, "Not allowed for this user");
        overlay.registryKnown = false;
        compare(child("closingList").text, "The list of running applications is not available.");
        keyClick(Qt.Key_Escape);
        focused("shutdownButton");
        verify(control.calls.indexOf("request:power-off") < 0);
    }

    function test_unsupported_rows_are_disabled_and_skipped() {
        control.suspend = "unsupported";
        overlay.openMenu();
        focused("restartButton");
        verify(!child("sleepButton").enabled);
        compare(child("sleepDescription").text, "Not supported on this computer");
        keyClick(Qt.Key_Up);
        focused("restartButton");
        keyClick(Qt.Key_Down);
        keyClick(Qt.Key_Down);
        focused("ambientButton");
        keyClick(Qt.Key_Tab);
        focused("restartButton");
    }

    function test_home_closes_the_menu() {
        openMenu();
        keyClick(Qt.Key_Home);
        verify(!overlay.open);
        compare(homeSpy.count, 1);
        verify(control.calls.indexOf("request:suspend") < 0);
    }

    function test_volume_keys_show_the_platform_level() {
        const hud = child("volumeHud");
        verify(!hud.shown);
        keyClick(Qt.Key_VolumeUp);
        compare(control.calls, ["step:1"]);
        // Nothing is shown before the platform answers.
        verify(!hud.shown);
        control.confirmVolume(55, false);
        verify(hud.shown);
        compare(child("volumeHudValue").text, "55");
        compare(child("volumeHudBar").width, child("volumeHudTrack").width * 0.55);
        keyClick(Qt.Key_VolumeDown);
        keyClick(Qt.Key_VolumeMute);
        compare(control.calls, ["step:1", "step:-1", "toggle"]);
        control.confirmVolume(50, true);
        verify(hud.muted);
        // Non-modal: no focus moves to the HUD.
        verify(!hud.activeFocus);
        // It hides 1.8 s after the last change.
        tryCompare(hud, "shown", false, 3000);
        // A failure shows no level.
        control.volumeFailed("unavailable");
        compare(volumeSpy.count, 1);
        verify(!hud.shown);
        // Outside the shell the keys are not Arinn's.
        overlay.keysEnabled = false;
        keyClick(Qt.Key_VolumeUp);
        compare(control.calls.length, 3);
    }

    function test_hud_matches_handoff() {
        const hud = child("volumeHud");
        compare(hud.width, 560);
        compare(hud.height, 88);
        compare(hud.x, (1920 - 560) / 2);
        compare(child("volumeHudIcon").width, 40);
        compare(child("volumeHudTrack").height, 8);
        compare(child("volumeHudValue").font.pixelSize, 30);
        compare(child("volumeHudValue").font.weight, 600);
        compare(hud.hideAfter, 1800);
    }
}
