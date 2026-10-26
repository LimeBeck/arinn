// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import QtTest
import "../../apps/arinn-shell/ui" as Ui
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
            // GetSessionSwitch; unknown until a test sets it.
            property string sessionSwitch: "unknown"
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
            // Like SystemControl: one request at a time, a second is ignored.
            function request(action) { calls.push("request:" + action); if (!pendingAction) pendingAction = action; }
            function reset() { calls.push("reset"); acceptedAction = ""; refusal = ""; refusalAction = ""; }
            function stepVolume(steps) { calls.push("step:" + steps); }
            function raiseVolume() { calls.push("raise"); }
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
    Component {
        id: launchFactory
        Ui.LaunchOverlay {
            width: 1920
            height: 1080
            z: 20
            serviceTitle: "Service"
        }
    }
    Component {
        id: beneathFactory
        Rectangle {
            objectName: "beneath"
            width: 200
            height: 100
            activeFocusOnTab: true
        }
    }
    SignalSpy { id: closedSpy; signalName: "closed" }
    SignalSpy { id: ambientSpy; signalName: "ambientRequested" }
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

    // A lock or sleep that arrives before the daemon's reply (another bus)
    // also closes the menu: no stale waiting pane after wake.
    function test_session_block_closes_a_pending_request() {
        openMenu();
        keyClick(Qt.Key_Return);
        compare(control.pendingAction, "suspend");
        overlay.sessionBlocked();
        verify(!overlay.open);
        compare(closedSpy.count, 1);
    }

    // The menu follows the shell's one input mode (ADR-0095).
    function test_pointer_mode_is_the_shell_input_mode() {
        openMenu();
        Ui.InputMode.pointerActive = true;
        verify(overlay.menu.pointerMode);
        verify(child("sleepButton").pointerMode);
        keyClick(Qt.Key_Down);
        verify(!Ui.InputMode.pointerActive);
        verify(!overlay.menu.pointerMode);
        focused("restartButton");
    }

    // A Restart or Shut down still in flight survives an unrelated lock:
    // its refusal is shown after unlock instead of being lost.
    function test_lock_keeps_a_pending_restart() {
        openMenu();
        keyClick(Qt.Key_Down);
        focused("restartButton");
        keyClick(Qt.Key_Return);
        focused("cancelButton");
        keyClick(Qt.Key_Right);
        keyClick(Qt.Key_Return);
        compare(control.pendingAction, "reboot");
        overlay.sessionBlocked();
        verify(overlay.open);
        control.answer("denied");
        tryCompare(child("powerError"), "visible", true);
    }

    // Once a request is sent it cannot be cancelled: Cancel and Back do
    // nothing until the system answers (spec §6.3, owner decision 2026-10-23).
    function test_a_dispatched_request_cannot_look_cancelled() {
        openMenu();
        keyClick(Qt.Key_Down);
        keyClick(Qt.Key_Return);
        focused("cancelButton");
        keyClick(Qt.Key_Right);
        keyClick(Qt.Key_Return);
        compare(control.pendingAction, "reboot");
        verify(!child("cancelButton").enabled);
        keyClick(Qt.Key_Escape);
        keyClick(Qt.Key_Backspace);
        mouseClick(child("cancelButton"));
        verify(overlay.open);
        compare(overlay.menu.step, "confirm");
        compare(closedSpy.count, 0);
        compare(control.pendingAction, "reboot");
        control.answer("");
        tryCompare(child("powerAccepted"), "visible", true);
    }

    function restartInFlight() {
        openMenu();
        keyClick(Qt.Key_Down);
        keyClick(Qt.Key_Return);
        focused("cancelButton");
        keyClick(Qt.Key_Right);
        keyClick(Qt.Key_Return);
        compare(control.pendingAction, "reboot");
    }

    // Home leaves while a request is in flight and does not cancel it; the
    // system's success needs nothing more (spec §6.3, §6.5 pattern).
    function test_home_leaves_without_cancelling_the_request() {
        restartInFlight();
        keyClick(Qt.Key_Home);
        verify(!overlay.open);
        compare(homeSpy.count, 1);
        compare(control.pendingAction, "reboot");
        verify(overlay.awaitingAnswer);
        control.answer("");
        wait(100);
        verify(!overlay.open);
        verify(!overlay.awaitingAnswer);
        compare(control.calls.filter(call => call.indexOf("request:") === 0), ["request:reboot"]);
    }

    // A refusal or timeout that arrives after Home reopens the power scope
    // on the refusal; Back closes it again.
    function test_late_refusal_reaches_the_user_data() {
        return [
            {tag: "restart timeout", rows: 1, code: "timeout", title: "Restart was refused",
             text: "The system did not answer in time. Try again."},
            {tag: "restart inhibited", rows: 1, code: "inhibited", title: "Restart was refused",
             text: "An application or another power operation is preventing it right now."},
            {tag: "sleep denied", rows: 0, code: "denied", title: "Sleep was refused",
             text: "The system does not allow it for this user."}
        ];
    }
    function test_late_refusal_reaches_the_user(data) {
        if (data.rows === 0) {
            openMenu();
            keyClick(Qt.Key_Return);
            compare(control.pendingAction, "suspend");
        } else {
            restartInFlight();
        }
        keyClick(Qt.Key_Home);
        verify(!overlay.open);
        compare(homeSpy.count, 1);
        control.answer(data.code);
        tryCompare(overlay, "open", true);
        tryCompare(child("powerError"), "visible", true);
        focused("powerError");
        compare(child("powerError").title, data.title);
        compare(child("powerError").description, data.text);
        keyClick(Qt.Key_Escape);
        verify(!overlay.open);
        compare(control.refusal, "");
        compare(homeSpy.count, 1);
    }

    function requestCalls() {
        return control.calls.filter(call => call.indexOf("request:") === 0);
    }
    // Reopened after Home while Restart is in flight, the menu shows that
    // confirmation as it was left: busy, Cancel disabled, Back ignored,
    // nothing more is sent. Home still leaves.
    function test_reopened_menu_shows_the_pending_restart() {
        restartInFlight();
        keyClick(Qt.Key_Home);
        overlay.openMenu();
        tryCompare(overlay.menu, "step", "confirm");
        compare(overlay.menu.pendingAction, "restart");
        focused("confirmButton");
        verify(child("confirmButton").busy);
        verify(!child("cancelButton").enabled);
        keyClick(Qt.Key_Escape);
        keyClick(Qt.Key_Return);
        mouseClick(child("confirmButton"));
        compare(overlay.menu.step, "confirm");
        compare(requestCalls(), ["request:reboot"]);
        keyClick(Qt.Key_Home);
        verify(!overlay.open);
        compare(homeSpy.count, 2);
        control.answer("denied");
        tryCompare(overlay, "open", true);
        tryCompare(child("powerError"), "visible", true);
    }

    // While Sleep is in flight, the other power rows are held: Restart and
    // Shut down open no confirmation and send nothing, on a reopened menu
    // as on the first one.
    function test_other_power_rows_are_held_while_a_request_is_in_flight_data() {
        return [{tag: "reopened", reopen: true}, {tag: "same opening", reopen: false}];
    }
    function test_other_power_rows_are_held_while_a_request_is_in_flight(data) {
        openMenu();
        keyClick(Qt.Key_Return);
        compare(control.pendingAction, "suspend");
        if (data.reopen) {
            keyClick(Qt.Key_Home);
            verify(!overlay.open);
            overlay.openMenu();
            // The opening settles its focus on the next event loop turn.
            wait(50);
            focused("sleepButton");
        }
        compare(child("restartDescription").text, "Available after the system answers");
        compare(child("restartButton").opacity, 0.4);
        keyClick(Qt.Key_Down);
        focused("restartButton");
        keyClick(Qt.Key_Return);
        compare(overlay.menu.step, "menu");
        keyClick(Qt.Key_Down);
        focused("shutdownButton");
        keyClick(Qt.Key_Return);
        mouseClick(child("shutdownButton"));
        compare(overlay.menu.step, "menu");
        verify(!child("powerConfirm").visible);
        keyClick(Qt.Key_Up);
        keyClick(Qt.Key_Up);
        focused("sleepButton");
        keyClick(Qt.Key_Return);
        compare(requestCalls(), ["request:suspend"]);
        // The answer frees the rows again.
        control.answer("inhibited");
        tryCompare(child("powerError"), "visible", true);
        keyClick(Qt.Key_Escape);
        tryCompare(child("restartButton"), "opacity", 1);
        keyClick(Qt.Key_Down);
        keyClick(Qt.Key_Return);
        compare(overlay.menu.step, "confirm");
        compare(overlay.menu.pendingAction, "restart");
    }

    // Ambient waits for the answer too: the refusal is never shown behind
    // the ambient screen (#307).
    function test_ambient_row_is_held_while_a_request_is_in_flight() {
        ambientSpy.target = overlay;
        ambientSpy.clear();
        openMenu();
        keyClick(Qt.Key_Return);
        compare(control.pendingAction, "suspend");
        keyClick(Qt.Key_Down);
        keyClick(Qt.Key_Down);
        keyClick(Qt.Key_Down);
        focused("ambientButton");
        compare(child("ambientDescription").text, "Available after the system answers");
        keyClick(Qt.Key_Return);
        mouseClick(child("ambientButton"));
        compare(ambientSpy.count, 0);
        control.answer("denied");
        tryCompare(child("powerError"), "visible", true);
        keyClick(Qt.Key_Escape);
        tryCompare(child("ambientButton"), "opacity", 1);
        focused("ambientButton");
        keyClick(Qt.Key_Return);
        compare(ambientSpy.count, 1);
        ambientSpy.target = null;
    }

    // A late refusal never takes focus behind a dialog drawn above the power
    // overlay: it waits until the dialog closes, then appears.
    function test_late_refusal_waits_for_the_launch_dialog() {
        const launch = createTemporaryObject(launchFactory, suite);
        overlay.holdLateAnswer = Qt.binding(() => launch.visible);
        restartInFlight();
        keyClick(Qt.Key_Home);
        launch.failure = "failed";
        verify(launch.visible);
        tryCompare(findChild(launch, "launchRetry"), "activeFocus", true);
        control.answer("timeout");
        wait(100);
        verify(!overlay.open);
        verify(findChild(launch, "launchRetry").activeFocus);
        verify(overlay.awaitingAnswer);
        launch.failure = "";
        verify(!launch.visible);
        tryCompare(overlay, "open", true);
        focused("powerError");
        compare(child("powerError").description, "The system did not answer in time. Try again.");
    }

    // Back on a late refusal returns focus to the item the user was on.
    function test_late_refusal_returns_focus_to_the_user() {
        const beneath = createTemporaryObject(beneathFactory, suite);
        restartInFlight();
        keyClick(Qt.Key_Home);
        beneath.forceActiveFocus();
        verify(beneath.activeFocus);
        control.answer("failed");
        tryCompare(overlay, "open", true);
        focused("powerError");
        keyClick(Qt.Key_Escape);
        verify(!overlay.open);
        // Main calls this when the overlay closes.
        verify(overlay.restoreLateFocus());
        verify(beneath.activeFocus);
        verify(!overlay.restoreLateFocus());
    }

    // Home on the late refusal goes Home; reopening the menu before the
    // answer shows it there as usual.
    function test_late_refusal_home_and_reopened_menu() {
        restartInFlight();
        keyClick(Qt.Key_Home);
        control.answer("failed");
        tryCompare(overlay, "open", true);
        focused("powerError");
        keyClick(Qt.Key_Home);
        verify(!overlay.open);
        compare(homeSpy.count, 2);

        restartInFlight();
        keyClick(Qt.Key_Home);
        overlay.openMenu();
        verify(!overlay.awaitingAnswer);
        control.answer("denied");
        tryCompare(child("powerError"), "visible", true);
        keyClick(Qt.Key_Escape);
        verify(overlay.open);
        compare(control.refusal, "");
    }

    // A row reported unsupported after the menu opened gives up focus.
    function test_focus_leaves_a_row_that_becomes_unsupported() {
        openMenu();
        focused("sleepButton");
        control.suspend = "unsupported";
        focused("restartButton");
        keyClick(Qt.Key_Down);
        focused("shutdownButton");
    }

    // #497: the menu opened before GetPower answered, and the confirmation
    // was opened while its action was still unknown. When the read reports
    // it unsupported, the action pill is disabled, focus is on Cancel, and
    // the confirmation says why in place, as the row does. Nothing is sent.
    function test_confirmation_that_becomes_unsupported_data() {
        return [
            {tag: "restart", row: "restartButton", availability: "reboot", downs: 1},
            {tag: "shutdown", row: "shutdownButton", availability: "powerOff", downs: 2}
        ];
    }
    function test_confirmation_that_becomes_unsupported(data) {
        control.reboot = "unknown";
        control.powerOff = "unknown";
        openMenu();
        for (let i = 0; i < data.downs; ++i)
            keyClick(Qt.Key_Down);
        focused(data.row);
        keyClick(Qt.Key_Return);
        focused("cancelButton");
        verify(child("confirmButton").enabled);
        verify(!child("confirmNote").visible);
        // The user is already on the action pill when the answer arrives.
        keyClick(Qt.Key_Right);
        focused("confirmButton");
        control[data.availability] = "unsupported";
        verify(!child("confirmButton").enabled);
        focused("cancelButton");
        compare(overlay.menu.step, "confirm");
        verify(child("confirmNote").visible);
        compare(child("confirmNote").text, "Not supported on this computer");
        // The arrows and Tab no longer reach the pill.
        keyClick(Qt.Key_Right);
        verify(child("cancelButton").activeFocus);
        keyClick(Qt.Key_Tab);
        verify(child("cancelButton").activeFocus);
        // Return is Cancel: the confirmation closes and nothing is sent.
        keyClick(Qt.Key_Return);
        tryCompare(overlay.menu, "step", "menu");
        focused("sleepButton");
        verify(!child(data.row).enabled);
        compare(requestCalls(), []);
    }

    // Long lists never push the buttons off the canvas.
    function test_long_lists_are_bounded() {
        const many = [];
        for (let i = 0; i < 12; ++i)
            many.push({title: "Application " + i});
        overlay.runningApplications = many;
        const holders = [];
        for (let i = 0; i < 16; ++i)
            holders.push({what: "shutdown", who: "Holder " + i + " " + "x".repeat(120), why: "y".repeat(250)});
        control.reboot = "inhibited";
        control.inhibitors = holders;
        openMenu();
        keyClick(Qt.Key_Down);
        keyClick(Qt.Key_Return);
        focused("cancelButton");
        const list = child("closingList");
        verify(list.text.indexOf("and 8 more") >= 0, list.text);
        compare(list.maximumLineCount, 3);
        verify(child("confirmNote").text.length < 300, child("confirmNote").text.length);
        verify(child("confirmNote").text.indexOf("and 13 more") >= 0);
        const panel = child("powerPanel");
        verify(panel.y >= 0 && panel.y + panel.height <= 1080, panel.y + " " + panel.height);
        keyClick(Qt.Key_Right);
        keyClick(Qt.Key_Return);
        control.answer("inhibited");
        tryCompare(child("powerError"), "visible", true);
        verify(child("powerError").description.length < 300);
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
        // Volume Up also turns the sound back on (owner, 2026-10-21).
        compare(control.calls, ["raise"]);
        // Nothing is shown before the platform answers.
        verify(!hud.shown);
        control.confirmVolume(55, false);
        verify(hud.shown);
        compare(child("volumeHudValue").text, "55");
        compare(child("volumeHudBar").width, child("volumeHudTrack").width * 0.55);
        keyClick(Qt.Key_VolumeDown);
        keyClick(Qt.Key_VolumeMute);
        compare(control.calls, ["raise", "step:-1", "toggle"]);
        control.confirmVolume(50, true);
        verify(hud.muted);
        // Non-modal: no focus moves to the HUD.
        verify(!hud.activeFocus);
        // It hides 1.8 s after the last change.
        tryCompare(hud, "shown", false, 3000);
        // A failure shows no level, but the HUD says it failed: the
        // feedback strip is hidden over managed playback.
        control.volumeFailed("unavailable");
        compare(volumeSpy.count, 1);
        verify(hud.shown);
        verify(hud.unavailable);
        verify(child("volumeHudUnavailable").visible);
        verify(!child("volumeHudValue").visible);
        verify(!child("volumeHudTrack").visible);
        // The next confirmed change shows its level again.
        control.confirmVolume(40, false);
        verify(!hud.unavailable);
        compare(child("volumeHudValue").text, "40");
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

    // #438: Switch to desktop through the overlay (spec §5.7, ADR-0107).
    function switchInFlight() {
        control.sessionSwitch = "available";
        openMenu();
        for (let i = 0; i < 3; ++i)
            keyClick(Qt.Key_Down);
        focused("desktopButton");
        keyClick(Qt.Key_Return);
        focused("cancelButton");
        keyClick(Qt.Key_Right);
        keyClick(Qt.Key_Return);
        compare(control.pendingAction, "switch-to-desktop");
    }

    // The entry follows the model's GetSessionSwitch: only "available" shows
    // it, and an older control without the property offers none.
    function test_switch_entry_follows_the_session_switch() {
        openMenu();
        verify(!child("desktopButton").visible);
        control.sessionSwitch = "available";
        verify(child("desktopButton").visible);
        // Focus on the row when it goes away moves to a row that exists.
        keyClick(Qt.Key_Down);
        keyClick(Qt.Key_Down);
        keyClick(Qt.Key_Down);
        focused("desktopButton");
        control.sessionSwitch = "unsupported";
        verify(!child("desktopButton").visible);
        focused("sleepButton");
        // An open confirmation for it closes too, before anything is sent.
        control.sessionSwitch = "available";
        keyClick(Qt.Key_Down);
        keyClick(Qt.Key_Down);
        keyClick(Qt.Key_Down);
        focused("desktopButton");
        keyClick(Qt.Key_Return);
        focused("cancelButton");
        control.sessionSwitch = "unknown";
        tryCompare(overlay.menu, "step", "menu");
        verify(!child("powerConfirm").visible);
        focused("sleepButton");
        compare(requestCalls(), []);
        keyClick(Qt.Key_Escape);
        verify(!overlay.open);
    }

    // Confirmed, the switch is a request like Restart: busy, Cancel disabled
    // and Back ignored until the daemon answers; acceptance is a waiting
    // pane, never success.
    function test_switch_request_waits_for_the_answer() {
        switchInFlight();
        compare(overlay.menu.busyAction, "desktop");
        verify(child("confirmButton").busy);
        verify(!child("cancelButton").enabled);
        keyClick(Qt.Key_Escape);
        keyClick(Qt.Key_Backspace);
        mouseClick(child("cancelButton"));
        verify(overlay.open);
        compare(overlay.menu.step, "confirm");
        compare(closedSpy.count, 0);
        compare(requestCalls(), ["request:switch-to-desktop"]);
        wait(300);
        verify(!child("powerAccepted").visible);
        control.answer("");
        tryCompare(child("powerAccepted"), "visible", true);
        compare(child("powerAccepted").title, "Switching to the desktop…");
        compare(child("powerAccepted").kind, "loading");
        keyClick(Qt.Key_Escape);
        verify(!overlay.open);
    }

    // The power rows hold each other in both directions: one request at a
    // time, as the daemon's shared power lock.
    function test_switch_and_power_rows_hold_each_other() {
        switchInFlight();
        keyClick(Qt.Key_Home);
        overlay.openMenu();
        // Reopened: the pending switch as it was left.
        tryCompare(overlay.menu, "step", "confirm");
        compare(overlay.menu.pendingAction, "desktop");
        focused("confirmButton");
        verify(!child("cancelButton").enabled);
        keyClick(Qt.Key_Return);
        compare(requestCalls(), ["request:switch-to-desktop"]);
        control.answer("unavailable");
        tryCompare(child("powerError"), "visible", true);
        keyClick(Qt.Key_Escape);
        tryCompare(child("powerError"), "visible", false);
        focused("cancelButton");
        keyClick(Qt.Key_Return);
        compare(overlay.menu.step, "menu");
        // Sleep in flight holds the switch row.
        keyClick(Qt.Key_Up);
        keyClick(Qt.Key_Up);
        keyClick(Qt.Key_Up);
        focused("sleepButton");
        keyClick(Qt.Key_Return);
        compare(control.pendingAction, "suspend");
        keyClick(Qt.Key_Down);
        keyClick(Qt.Key_Down);
        keyClick(Qt.Key_Down);
        focused("desktopButton");
        compare(child("desktopDescription").text, "Available after the system answers");
        compare(child("desktopButton").opacity, 0.4);
        keyClick(Qt.Key_Return);
        compare(overlay.menu.step, "menu");
        compare(requestCalls(), ["request:switch-to-desktop", "request:suspend"]);
    }

    // Home leaves while the switch is in flight and does not cancel it; a
    // late refusal comes back, titled for the switch (owner decision
    // 2026-10-23). Success needs nothing more: the session ends.
    function test_switch_late_refusal_after_home() {
        switchInFlight();
        keyClick(Qt.Key_Home);
        verify(!overlay.open);
        compare(homeSpy.count, 1);
        verify(overlay.awaitingAnswer);
        control.answer("timeout");
        tryCompare(overlay, "open", true);
        focused("powerError");
        compare(child("powerError").title, "Could not switch to the desktop");
        compare(child("powerError").description, "The desktop did not answer in time. Arinn keeps running; try again.");
        keyClick(Qt.Key_Escape);
        verify(!overlay.open);
        compare(control.refusal, "");

        switchInFlight();
        keyClick(Qt.Key_Home);
        control.answer("");
        wait(100);
        verify(!overlay.open);
        verify(!overlay.awaitingAnswer);
    }

    // A failed switch is an error and the session continues (spec §5.7).
    function test_switch_refusal_texts_data() {
        return [
            {tag: "unavailable", text: "The desktop did not accept the logout. Arinn keeps running; try again later."},
            {tag: "timeout", text: "The desktop did not answer in time. Arinn keeps running; try again."},
            {tag: "failed", text: "Arinn could not prepare the switch. Arinn keeps running; try again."},
            {tag: "unsupported", text: "This session cannot switch to the desktop. Arinn keeps running."},
            {tag: "inhibited", text: "Another power request is still running. Arinn keeps running; try again when it is done."},
            {tag: "denied", text: "The system does not allow it for this user. Arinn keeps running."}
        ];
    }
    function test_switch_refusal_texts(data) {
        // A shutdown holder is not a reason for the switch.
        control.inhibitors = [{what: "shutdown", who: "Updater", why: "Installing"}];
        switchInFlight();
        control.answer(data.tag);
        tryCompare(child("powerError"), "visible", true);
        focused("powerError");
        compare(child("powerError").title, "Could not switch to the desktop");
        compare(child("powerError").description, data.text);
        verify(!child("powerAccepted").visible);
        if (data.tag === "unsupported") {
            // The model reads GetSessionSwitch again: the switch goes, and
            // Back leaves its confirmation for the first row.
            control.sessionSwitch = "unsupported";
            keyClick(Qt.Key_Escape);
            tryCompare(child("powerError"), "visible", false);
            tryCompare(overlay.menu, "step", "menu");
            focused("sleepButton");
            verify(!child("desktopButton").visible);
            return;
        }
        // Back returns to the confirmation, Cancel focused.
        keyClick(Qt.Key_Escape);
        tryCompare(child("powerError"), "visible", false);
        focused("cancelButton");
        compare(overlay.menu.pendingAction, "desktop");
    }
}
