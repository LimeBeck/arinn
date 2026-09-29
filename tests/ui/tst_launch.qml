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
    SignalSpy { id: cancels; signalName: "cancelRequested" }
    SignalSpy { id: retries; signalName: "retryRequested" }
    SignalSpy { id: dismisses; signalName: "dismissRequested" }

    function init() {
        mouseMove(suite, 10, 10);
        overlay = createTemporaryObject(factory, suite);
        verify(overlay);
        cancels.target = overlay; cancels.clear();
        retries.target = overlay; retries.clear();
        dismisses.target = overlay; dismisses.clear();
    }
    function cleanup() {
        cancels.target = null;
        retries.target = null;
        dismisses.target = null;
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

    function child(name) {
        const item = findChild(overlay, name);
        verify(item !== null, name);
        return item;
    }
    function focused(name) {
        tryCompare(child(name), "activeFocus", true, 5000, name);
    }
}
