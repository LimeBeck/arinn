// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import QtTest
import "../../apps/arinn-shell/ui/screens/power" as Power

TestCase {
    id: suite
    name: "PowerMenu"
    width: 1920
    height: 1080
    visible: true
    when: windowShown

    property var menu
    Component {
        id: factory
        Power.PowerMenu {}
    }
    SignalSpy { id: sleepSpy; signalName: "sleepRequested" }
    SignalSpy { id: restartSpy; signalName: "restartRequested" }
    SignalSpy { id: shutdownSpy; signalName: "shutdownRequested" }
    SignalSpy { id: ambientSpy; signalName: "ambientRequested" }
    SignalSpy { id: homeSpy; signalName: "homeRequested" }
    SignalSpy { id: switchSpy; signalName: "desktopSwitchRequested" }

    function child(name) {
        const item = findChild(menu, name);
        verify(item !== null, name);
        return item;
    }
    function focused(name) {
        tryCompare(child(name), "activeFocus", true, 5000, name);
    }
    function init() {
        // Qt Quick Test keeps the pointer between fixtures. Park it before the
        // overlay exists so a queued hover cannot steal the first focus.
        mouseMove(suite, 10, 10);
        menu = createTemporaryObject(factory, suite, {closingApplications: ["One", "Two"]});
        verify(menu);
        sleepSpy.target = menu; sleepSpy.clear();
        restartSpy.target = menu; restartSpy.clear();
        shutdownSpy.target = menu; shutdownSpy.clear();
        ambientSpy.target = menu; ambientSpy.clear();
        homeSpy.target = menu; homeSpy.clear();
        switchSpy.target = menu; switchSpy.clear();
        focused("sleepButton");
    }
    function cleanup() {
        sleepSpy.target = null;
        restartSpy.target = null;
        shutdownSpy.target = null;
        ambientSpy.target = null;
        homeSpy.target = null;
        switchSpy.target = null;
        if (menu)
            menu.destroy();
        menu = null;
    }

    function test_cancel_is_default_confirm_focus_data() {
        return [
            {tag: "restart", downs: 1, title: "Restart?"},
            {tag: "shutdown", downs: 2, title: "Shut down?"}
        ];
    }
    function test_cancel_is_default_confirm_focus(data) {
        const opener = data.tag === "restart" ? "restartButton" : "shutdownButton";
        for (let i = 0; i < data.downs; ++i)
            keyClick(Qt.Key_Down);
        focused(opener);
        keyClick(Qt.Key_Return);
        compare(menu.step, "confirm");
        focused("cancelButton");
        verify(!child("confirmButton").activeFocus);
        compare(child("confirmButton").destructive, true);
        compare(child("powerTitle").text, data.title);
        verify(child("closingList").visible);
        verify(child("closingList").text.indexOf("One") >= 0);
        verify(child("closingList").text.indexOf("Two") >= 0);
        keyClick(Qt.Key_Right);
        focused("confirmButton");
        keyClick(Qt.Key_Left);
        focused("cancelButton");
        keyClick(Qt.Key_Return);
        compare(menu.step, "menu");
        compare(restartSpy.count, 0);
        compare(shutdownSpy.count, 0);
        focused(opener);
        keyClick(Qt.Key_Return);
        focused("cancelButton");
        keyClick(Qt.Key_Right);
        focused("confirmButton");
        keyClick(Qt.Key_Return);
        compare(data.tag === "restart" ? restartSpy.count : shutdownSpy.count, 1);
        compare(data.tag === "restart" ? shutdownSpy.count : restartSpy.count, 0);
        compare(sleepSpy.count, 0);
        compare(menu.step, "confirm");
        verify(findChild(menu, "powerSuccess") === null);
        keyClick(Qt.Key_Escape);
        compare(menu.step, "menu");
        focused(opener);
    }

    function test_home_closes_confirm_without_action() {
        keyClick(Qt.Key_Down);
        keyClick(Qt.Key_Down);
        focused("shutdownButton");
        keyClick(Qt.Key_Return);
        focused("cancelButton");
        keyClick(Qt.Key_Home);
        compare(menu.step, "menu");
        compare(shutdownSpy.count, 0);
        compare(restartSpy.count, 0);
        compare(homeSpy.count, 1);
        focused("shutdownButton");
    }

    function test_closing_list_is_explicit_fixture() {
        menu.closingApplications = [{title: "Playback"}];
        keyClick(Qt.Key_Down);
        keyClick(Qt.Key_Return);
        focused("cancelButton");
        verify(child("closingList").text.indexOf("Playback") >= 0);
        menu.closingApplications = [];
        verify(child("closingList").text.indexOf("No running applications") >= 0);
        compare(restartSpy.count, 0);
    }

    function test_sleep_intent_has_no_success() {
        focused("sleepButton");
        keyClick(Qt.Key_Return);
        compare(sleepSpy.count, 1);
        compare(ambientSpy.count, 0);
        compare(menu.step, "menu");
        compare(menu.refused, false);
        verify(!child("powerError").visible);
        verify(!child("powerConfirm").visible);
        verify(findChild(menu, "powerSuccess") === null);
        wait(300);
        compare(sleepSpy.count, 1);
        compare(menu.step, "menu");
        compare(menu.refused, false);
        verify(findChild(menu, "powerSuccess") === null);
    }

    function test_refusal_is_explicit_error() {
        menu.refusalMessage = "Suspend was denied";
        tryCompare(child("powerError"), "visible", true);
        compare(child("powerError").kind, "error");
        verify(child("powerError").description.indexOf("Suspend was denied") >= 0);
        compare(sleepSpy.count, 0);
        compare(menu.step, "menu");
        wait(300);
        verify(child("powerError").visible);
        compare(child("powerError").kind, "error");
        verify(findChild(menu, "powerSuccess") === null);
        menu.refusalMessage = "";
        tryCompare(child("powerError"), "visible", false);
        focused("sleepButton");
        keyClick(Qt.Key_Return);
        compare(sleepSpy.count, 1);
        compare(menu.step, "menu");
        menu.refusalMessage = "Permission denied";
        tryCompare(child("powerError"), "visible", true);
        compare(menu.refused, true);
        verify(findChild(menu, "powerSuccess") === null);
    }

    // The Ambient row asks the shell for its ambient screen (#307); the menu
    // itself shows nothing else and requests no power action.
    function test_ambient_row_requests_ambient() {
        keyClick(Qt.Key_Down);
        keyClick(Qt.Key_Down);
        keyClick(Qt.Key_Down);
        focused("ambientButton");
        keyClick(Qt.Key_Return);
        compare(ambientSpy.count, 1);
        compare(sleepSpy.count, 0);
        compare(restartSpy.count, 0);
        compare(shutdownSpy.count, 0);
        compare(menu.step, "menu");
        verify(findChild(menu, "ambientScreen") === null);
        focused("ambientButton");
    }

    function test_focus_stays_in_power_scope() {
        for (let i = 0; i < 5; ++i)
            keyClick(Qt.Key_Up);
        focused("sleepButton");
        keyClick(Qt.Key_Left);
        focused("sleepButton");
        keyClick(Qt.Key_Down);
        focused("restartButton");
        keyClick(Qt.Key_Down);
        focused("shutdownButton");
        keyClick(Qt.Key_Down);
        focused("ambientButton");
        keyClick(Qt.Key_Down);
        focused("ambientButton");
        keyClick(Qt.Key_Tab);
        focused("sleepButton");
        keyClick(Qt.Key_Down);
        keyClick(Qt.Key_Return);
        focused("cancelButton");
        for (let i = 0; i < 4; ++i)
            keyClick(Qt.Key_Up);
        focused("cancelButton");
        keyClick(Qt.Key_Right);
        focused("confirmButton");
        keyClick(Qt.Key_Right);
        focused("confirmButton");
        keyClick(Qt.Key_Backspace);
        compare(restartSpy.count, 0);
        compare(menu.step, "menu");
        focused("restartButton");
    }

    function test_panel_matches_handoff() {
        const scrim = child("powerScrim");
        compare(scrim.width, 1920);
        compare(scrim.height, 1080);
        verify(scrim.visible);
        verify(Math.abs(scrim.color.a - 0.55) < 0.01);
        const panel = child("powerPanel");
        compare(panel.width, 800);
        compare(panel.radius, 48);
        verify(Math.abs(panel.x - 560) < 1);
        verify(Math.abs(panel.y - (1080 - panel.height) / 2) < 1);
        verify(Math.abs(panel.color.a - 0.82) < 0.02);
        compare(panel.border.width, 1);
        const title = child("powerTitle");
        compare(title.text, "Power");
        compare(title.font.pixelSize, 52);
        compare(title.font.weight, 700);
        const sleep = child("sleepButton");
        compare(sleep.height, 112);
        compare(sleep.cornerRadius, 30);
        verify(sleep.focusedColor.r > 0.9);
        compare(child("sleepIcon").width, 64);
        compare(child("sleepIcon").height, 64);
        compare(child("sleepLabel").font.pixelSize, 32);
        compare(child("sleepDescription").font.pixelSize, 26);
        verify(!child("powerConfirm").visible);
        keyClick(Qt.Key_Down);
        keyClick(Qt.Key_Return);
        focused("cancelButton");
        compare(child("cancelButton").height, 96);
        compare(child("confirmButton").height, 96);
        compare(child("cancelButton").cornerRadius, 48);
        compare(child("confirmButton").cornerRadius, 48);
        compare(child("powerConfirm").spacing, 20);
        verify(child("cancelButton").activeFocus);
        verify(!child("confirmButton").activeFocus);
    }

    function test_escape_closes_confirm_without_shutdown_data() {
        return [
            {tag: "escape", key: Qt.Key_Escape},
            {tag: "backspace", key: Qt.Key_Backspace},
            {tag: "back", key: Qt.Key_Back}
        ];
    }
    function test_escape_closes_confirm_without_shutdown(data) {
        keyClick(Qt.Key_Down);
        keyClick(Qt.Key_Down);
        focused("shutdownButton");
        keyClick(Qt.Key_Return);
        compare(menu.step, "confirm");
        focused("cancelButton");
        keyClick(Qt.Key_Right);
        focused("confirmButton");
        keyClick(data.key);
        compare(menu.step, "menu");
        compare(shutdownSpy.count, 0);
        compare(restartSpy.count, 0);
        compare(sleepSpy.count, 0);
        focused("shutdownButton");
        verify(findChild(menu, "powerSuccess") === null);
    }

    // #438, spec §5.7: Switch to desktop exists only in the dedicated Arinn
    // session (GetSessionSwitch "available"). Otherwise it is not shown and
    // the arrows and Tab never reach it.
    function test_switch_to_desktop_is_offered_only_when_available_data() {
        return [
            {tag: "unknown", value: "unknown"},
            {tag: "unsupported", value: "unsupported"},
            {tag: "empty", value: ""}
        ];
    }
    function test_switch_to_desktop_is_offered_only_when_available(data) {
        menu.desktopSwitchAvailability = data.value;
        verify(!child("desktopButton").visible);
        keyClick(Qt.Key_Down);
        keyClick(Qt.Key_Down);
        focused("shutdownButton");
        keyClick(Qt.Key_Down);
        focused("ambientButton");
        keyClick(Qt.Key_Tab);
        focused("sleepButton");
        keyClick(Qt.Key_Backtab);
        focused("ambientButton");
        keyClick(Qt.Key_Up);
        focused("shutdownButton");
        compare(switchSpy.count, 0);
    }

    // Offered, it sits after Shut down, and its confirmation is the one of
    // Restart and Shut down: the live list, Cancel as the default focus.
    function test_switch_to_desktop_confirmation_defaults_to_cancel() {
        menu.desktopSwitchAvailability = "available";
        verify(child("desktopButton").visible);
        verify(child("desktopButton").enabled);
        compare(child("desktopLabel").text, "Switch to desktop");
        compare(child("desktopDescription").text, "Log out; the next login starts the desktop");
        keyClick(Qt.Key_Down);
        keyClick(Qt.Key_Down);
        keyClick(Qt.Key_Down);
        focused("desktopButton");
        keyClick(Qt.Key_Down);
        focused("ambientButton");
        keyClick(Qt.Key_Up);
        focused("desktopButton");
        keyClick(Qt.Key_Return);
        compare(menu.step, "confirm");
        compare(menu.pendingAction, "desktop");
        focused("cancelButton");
        verify(!child("confirmButton").activeFocus);
        compare(child("confirmButton").destructive, true);
        compare(child("confirmButton").text, "Switch");
        compare(child("powerTitle").text, "Switch to desktop?");
        verify(child("confirmDetail").visible);
        verify(child("confirmDetail").text.indexOf("Return to Arinn") >= 0, child("confirmDetail").text);
        compare(child("closingList").text, "These applications will close: One, Two.");
        // The live list follows the registry while the confirmation is open.
        menu.closingApplications = ["One"];
        compare(child("closingList").text, "These applications will close: One.");
        verify(!child("confirmNote").visible);
        // Cancel, Back: nothing is requested.
        keyClick(Qt.Key_Return);
        compare(menu.step, "menu");
        focused("desktopButton");
        keyClick(Qt.Key_Return);
        focused("cancelButton");
        keyClick(Qt.Key_Escape);
        compare(menu.step, "menu");
        focused("desktopButton");
        compare(switchSpy.count, 0);
        // Home leaves without an action.
        keyClick(Qt.Key_Return);
        focused("cancelButton");
        keyClick(Qt.Key_Home);
        compare(homeSpy.count, 1);
        compare(switchSpy.count, 0);
        compare(menu.step, "menu");
        // Confirm sends the intent once; the menu reports nothing by itself.
        focused("desktopButton");
        keyClick(Qt.Key_Return);
        focused("cancelButton");
        keyClick(Qt.Key_Right);
        focused("confirmButton");
        keyClick(Qt.Key_Return);
        compare(switchSpy.count, 1);
        compare(restartSpy.count, 0);
        compare(shutdownSpy.count, 0);
        compare(menu.step, "confirm");
        verify(!child("powerError").visible);
        verify(!child("powerAccepted").visible);
        // The other confirmations carry no switch text.
        keyClick(Qt.Key_Escape);
        keyClick(Qt.Key_Up);
        focused("shutdownButton");
        keyClick(Qt.Key_Return);
        focused("cancelButton");
        verify(!child("confirmDetail").visible);
    }

    // With five rows the panel still fits the canvas.
    function test_switch_to_desktop_panel_fits() {
        menu.desktopSwitchAvailability = "available";
        const panel = child("powerPanel");
        verify(panel.y >= 0 && panel.y + panel.height <= 1080, panel.y + " " + panel.height);
        compare(child("desktopButton").height, 112);
        compare(child("desktopIcon").width, 64);
        keyClick(Qt.Key_Down);
        keyClick(Qt.Key_Down);
        keyClick(Qt.Key_Down);
        keyClick(Qt.Key_Return);
        focused("cancelButton");
        verify(panel.y >= 0 && panel.y + panel.height <= 1080, panel.y + " " + panel.height);
    }
}
