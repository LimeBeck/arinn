// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import QtTest
import "../../apps/arinn-shell/ui" as Ui

TestCase {
    id: suite
    name: "LaunchOverlay"
    width: 1920
    height: 1080
    visible: true
    when: windowShown

    property var overlay
    Component {
        id: factory
        Ui.LaunchOverlay {
            anchors.fill: parent
            revealDelay: 40
            slowDelay: 80
            timeoutDelay: 140
            serviceTitle: "Example"
        }
    }
    Component {
        id: applicationFactory
        Ui.LaunchOverlay {
            anchors.fill: parent
            kind: "application"
            revealDelay: 40
            slowDelay: 80
            timeoutDelay: 0
            serviceTitle: "Game"
        }
    }
    SignalSpy { id: cancels; signalName: "cancelRequested" }
    SignalSpy { id: retries; signalName: "retryRequested" }
    SignalSpy { id: dismisses; signalName: "dismissRequested" }
    SignalSpy { id: homes; signalName: "homeRequested" }

    function init() {
        mouseMove(suite, 10, 10);
        overlay = createTemporaryObject(factory, suite);
        verify(overlay);
        cancels.target = overlay; cancels.clear();
        retries.target = overlay; retries.clear();
        dismisses.target = overlay; dismisses.clear();
        homes.target = overlay; homes.clear();
    }
    function cleanup() {
        cancels.target = null;
        retries.target = null;
        dismisses.target = null;
        homes.target = null;
    }

    function test_skip_when_fast() {
        overlay.active = true;
        compare(overlay.visible, false);
        overlay.active = false;
        wait(80);
        compare(overlay.visible, false);
        compare(overlay.revealed, false);
    }

    function test_slow_message_and_cancel() {
        overlay.active = true;
        tryCompare(overlay, "revealed", true);
        compare(overlay.visible, true);
        tryCompare(overlay, "slow", true);
        compare(child("launchSlow").visible, true);
        focused("launchCancel");
        keyClick(Qt.Key_Return);
        compare(cancels.count, 1);
        overlay.active = true;
        tryCompare(overlay, "slow", true);
        focused("launchCancel");
        keyClick(Qt.Key_Escape);
        compare(cancels.count, 2);
    }

    function test_home_hides_slow_open_without_cancel() {
        // Main.qml maps cancelRequested to browserCatalog.cancelOpen().
        overlay.active = true;
        tryCompare(overlay, "slow", true);
        compare(overlay.visible, true);
        focused("launchCancel");
        keyClick(Qt.Key_Home);
        compare(cancels.count, 0);
        compare(retries.count, 0);
        compare(dismisses.count, 0);
        compare(homes.count, 1);
        compare(overlay.visible, false);
        compare(overlay.active, true);
        compare(overlay.showDialog, false);
    }

    function test_stall_keeps_slow_message_visible() {
        overlay.active = true;
        wait(0);
        // Block the UI thread after the timers are armed, for longer than
        // every deadline. Parallel timers finish together and the timeout
        // hides the slow message's parent, so visible becomes false.
        const started = Date.now();
        while (Date.now() - started < overlay.timeoutDelay + 50) {}
        tryCompare(overlay, "slow", true);
        compare(overlay.timedOut, false);
        compare(child("launchSlow").visible, true);
    }

    function test_timeout_dialog() {
        overlay.active = true;
        tryCompare(overlay, "timedOut", true);
        compare(child("launchDialog").visible, true);
        compare(child("launchDialogBody").text, "The browser did not respond within 30 seconds. Try again.");
        focused("launchRetry");
        keyClick(Qt.Key_Return);
        compare(retries.count, 1);
        keyClick(Qt.Key_Right);
        focused("launchClose");
        keyClick(Qt.Key_Return);
        compare(dismisses.count, 1);
    }

    function test_error_dialog() {
        overlay.failure = "failed";
        compare(overlay.showDialog, true);
        compare(child("launchDialogTitle").text, "Example did not open");
        compare(child("launchDialogBody").text, "Try again. If this happens again, restart Arinn.");
        focused("launchRetry");
    }

    function application() {
        overlay = createTemporaryObject(applicationFactory, suite);
        verify(overlay);
        cancels.target = overlay; cancels.clear();
        retries.target = overlay; retries.clear();
        dismisses.target = overlay; dismisses.clear();
        homes.target = overlay; homes.clear();
    }

    // Spec §6.5: the launch animation runs, then "Starting <app>…" with
    // Cancel. Cancel asks first and Keep starting has the focus.
    function test_application_starting_and_confirmed_cancel() {
        application();
        overlay.active = true;
        tryCompare(overlay, "revealed", true);
        compare(child("launchTitle").text, "Opening Game…");
        tryCompare(overlay, "slow", true);
        compare(child("launchTitle").text, "Starting Game…");
        compare(child("launchSlow").visible, false);
        focused("launchCancel");
        keyClick(Qt.Key_Return);
        compare(cancels.count, 0);
        compare(overlay.confirming, true);
        compare(child("launchConfirm").visible, true);
        compare(child("launchConfirmTitle").text, "Stop starting Game?");
        focused("launchKeepStarting");
        keyClick(Qt.Key_Return);
        compare(overlay.confirming, false);
        compare(cancels.count, 0);
        focused("launchCancel");
        keyClick(Qt.Key_Return);
        focused("launchKeepStarting");
        keyClick(Qt.Key_Right);
        focused("launchConfirmStop");
        keyClick(Qt.Key_Left);
        focused("launchKeepStarting");
        keyClick(Qt.Key_Right);
        keyClick(Qt.Key_Return);
        compare(cancels.count, 1);
        compare(overlay.confirming, false);
    }

    function test_application_back_asks_and_back_again_keeps_starting() {
        application();
        overlay.active = true;
        tryCompare(overlay, "slow", true);
        focused("launchCancel");
        keyClick(Qt.Key_Escape);
        compare(overlay.confirming, true);
        focused("launchKeepStarting");
        keyClick(Qt.Key_Escape);
        compare(overlay.confirming, false);
        compare(cancels.count, 0);
        compare(overlay.visible, true);
        // Home hides the confirmation and the launch, cancelling nothing.
        keyClick(Qt.Key_Escape);
        focused("launchKeepStarting");
        keyClick(Qt.Key_Home);
        compare(homes.count, 1);
        compare(cancels.count, 0);
        compare(overlay.confirming, false);
        compare(overlay.visible, false);
        // Choosing the tile again shows the starting state again.
        overlay.showAgain(overlay.slowDelay);
        compare(overlay.visible, true);
        compare(overlay.slow, true);
        focused("launchCancel");
    }

    // Another application's launch, chosen again behind a newer one, shows
    // at once: "Starting…" when it has run that long, else when it has.
    function test_application_show_again_follows_the_launch_shown() {
        application();
        overlay.active = true;
        tryCompare(overlay, "slow", true);
        keyClick(Qt.Key_Home);
        compare(overlay.visible, false);
        overlay.serviceTitle = "Other";
        overlay.showAgain(0);
        compare(overlay.visible, true);
        compare(overlay.slow, false);
        compare(child("launchTitle").text, "Opening Other…");
        tryCompare(overlay, "slow", true);
        compare(child("launchTitle").text, "Starting Other…");
        focused("launchCancel");
        // Shown while visible, for a launch that has run long enough.
        overlay.serviceTitle = "Game";
        overlay.showAgain(overlay.slowDelay + 1);
        compare(overlay.slow, true);
        compare(child("launchTitle").text, "Starting Game…");
    }

    // A confirmed stop that the provider has not finished yet: "Stopping…",
    // without Cancel, until the launch ends.
    function test_application_stopping() {
        application();
        overlay.active = true;
        tryCompare(overlay, "slow", true);
        overlay.cancellable = false;
        overlay.stopping = true;
        compare(child("launchTitle").text, "Stopping Game…");
        compare(child("launchCancel").visible, false);
        keyClick(Qt.Key_Escape);
        compare(overlay.confirming, false);
        compare(cancels.count, 0);
    }

    function test_application_has_no_timeout() {
        application();
        overlay.active = true;
        tryCompare(overlay, "slow", true);
        wait(300);
        compare(overlay.timedOut, false);
        compare(overlay.showDialog, false);
        compare(child("launchTitle").text, "Starting Game…");
    }

    function test_application_without_cancel() {
        application();
        overlay.cancellable = false;
        overlay.active = true;
        tryCompare(overlay, "slow", true);
        compare(child("launchCancel").visible, false);
        keyClick(Qt.Key_Escape);
        compare(overlay.confirming, false);
        compare(cancels.count, 0);
        // The launch became cancellable: Cancel appears.
        overlay.cancellable = true;
        compare(child("launchCancel").visible, true);
    }

    function test_application_ends_while_confirming() {
        application();
        overlay.active = true;
        tryCompare(overlay, "slow", true);
        focused("launchCancel");
        keyClick(Qt.Key_Return);
        compare(overlay.confirming, true);
        // The window appeared: the launch is over, nothing is stopped.
        overlay.active = false;
        compare(overlay.visible, false);
        compare(overlay.confirming, false);
        compare(cancels.count, 0);
    }

    function test_application_failure_dialog() {
        application();
        overlay.active = true;
        tryCompare(overlay, "slow", true);
        // Cancel takes focus when it appears; wait for it, as the other
        // cases do, or Return can arrive first and open nothing.
        focused("launchCancel");
        keyClick(Qt.Key_Return);
        compare(overlay.confirming, true);
        overlay.failure = "Unavailable";
        overlay.active = false;
        compare(overlay.confirming, false);
        compare(overlay.showDialog, true);
        compare(child("launchDialogTitle").text, "Game did not open");
        compare(child("launchDialogBody").text,
                "The application could not start or closed before it showed a window.");
        focused("launchRetry");
        keyClick(Qt.Key_Return);
        compare(retries.count, 1);
        compare(cancels.count, 0);
    }

    function child(name) {
        const item = findChild(overlay, name);
        verify(item !== null, name);
        return item;
    }
    function focused(name) {
        tryCompare(child(name), "activeFocus", true, 5000, name);
    }
}
