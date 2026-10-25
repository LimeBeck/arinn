// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick

// Opening overlay for a web service or an application. The tint is neutral:
// service hues wait for the icon contract. The overlay stays hidden when the
// operation finishes before revealDelay.
//
// An application launch (kind "application", spec §6.5) has no timeout of
// its own: it ends when the application shows a window or fails. After
// slowDelay it reads "Starting <app>…", and Cancel asks for confirmation
// with Keep starting focused first.
FocusScope {
    id: root
    property string kind: "service"
    property bool active: false
    property string serviceTitle: ""
    property string failure: ""
    // Whether Cancel can stop the operation now.
    property bool cancellable: true
    // A confirmed stop is still under way (application launches).
    property bool stopping: false
    property int revealDelay: 400
    property int slowDelay: 8000
    // 0 disables the timeout stage.
    property int timeoutDelay: 30000
    property bool revealed: false
    property bool slow: false
    property bool timedOut: false
    // The stop confirmation of an application launch is open.
    property bool confirming: false
    readonly property bool application: kind === "application"
    // Home hides the progress overlay while the open keeps running.
    property bool homeHidden: false
    readonly property bool showDialog: failure.length > 0 || timedOut
    readonly property bool showOverlay: showDialog || (!homeHidden && revealed && active)
    readonly property string dialogTitle: qsTr("%1 did not open").arg(serviceTitle.length ? serviceTitle : qsTr("The service"))
    readonly property string dialogBody: timedOut && !failure.length
            ? qsTr("The browser did not respond within 30 seconds. Try again.")
            : application && failure === "Unavailable"
            ? qsTr("The application could not start or closed before it showed a window.")
            : qsTr("Try again. If this happens again, restart Arinn.")
    readonly property string progressTitle: application && stopping
            ? qsTr("Stopping %1…").arg(serviceTitle)
            : application && slow
            ? qsTr("Starting %1…").arg(serviceTitle)
            : qsTr("Opening %1…").arg(serviceTitle)
    signal cancelRequested()
    signal retryRequested()
    signal dismissRequested()
    signal homeRequested()

    function arm() {
        homeHidden = false;
        revealed = false;
        slow = false;
        timedOut = false;
        confirming = false;
        slowTimer.stop();
        timeoutTimer.stop();
        revealTimer.interval = Math.max(0, revealDelay);
        revealTimer.restart();
    }
    // One stage at a time. QML timers share the animation clock, so a stalled
    // process would otherwise finish every deadline in the same turn and the
    // timeout dialog would hide the slow message before it could appear.
    function scheduleSlow() {
        slowTimer.interval = Math.max(0, slowDelay - revealDelay);
        slowTimer.restart();
    }
    function scheduleTimeout() {
        if (timeoutDelay <= 0)
            return;
        timeoutTimer.interval = Math.max(0, timeoutDelay - slowDelay);
        timeoutTimer.restart();
    }
    function disarm() {
        revealTimer.stop();
        slowTimer.stop();
        timeoutTimer.stop();
        homeHidden = false;
        revealed = false;
        slow = false;
        timedOut = false;
        confirming = false;
    }
    // The operation is still running and the user chose its tile again,
    // `elapsed` ms after it started: it shows at once, and "Starting…" when
    // it has run for slowDelay already. It may be another launch than the
    // one shown before (one launch per application, §6.5).
    function showAgain(elapsed) {
        if (!active || showDialog)
            return;
        revealTimer.stop();
        timeoutTimer.stop();
        confirming = false;
        homeHidden = false;
        revealed = true;
        slow = (elapsed || 0) >= slowDelay;
        if (slow) {
            slowTimer.stop();
            Qt.callLater(() => Focus.take(cancelButton));
        } else {
            slowTimer.interval = Math.max(0, slowDelay - (elapsed || 0));
            slowTimer.restart();
        }
    }
    // Cancel: an application launch asks first; Keep starting is focused.
    function requestCancel() {
        if (!cancellable)
            return;
        if (!application) {
            cancelRequested();
            return;
        }
        confirming = true;
        Qt.callLater(() => Focus.take(keepButton));
    }
    function keepStarting() {
        confirming = false;
        Qt.callLater(() => Focus.take(cancelButton));
    }
    // The stop is sent before the confirmation closes: closing it may bring
    // a failure that waited behind it on screen, and the stop must still
    // reach the launch the user confirmed it for.
    function confirmStop() {
        cancelRequested();
        confirming = false;
    }
    // Specification §6.3: opening Home never pauses or stops the operation.
    // Cancel and Back remain the ways to cancel.
    function hideForHome() {
        revealTimer.stop();
        slowTimer.stop();
        timeoutTimer.stop();
        confirming = false;
        homeHidden = true;
    }
    onActiveChanged: {
        if (active)
            arm();
        else if (!failure.length)
            disarm();
        else {
            revealTimer.stop();
            slowTimer.stop();
            timeoutTimer.stop();
        }
    }
    // A later open keeps `active` true, so the title change is what shows it again.
    onServiceTitleChanged: if (active && homeHidden) arm()
    onCancellableChanged: {
        if (!cancellable && confirming)
            confirming = false;
        // Cancel appears on the starting screen: it takes the focus there,
        // as it does when the screen turns slow (§6.5).
        else if (cancellable && slow && showOverlay && !showDialog && !confirming)
            Qt.callLater(() => Focus.take(cancelButton));
    }
    onFailureChanged: {
        if (failure.length) {
            confirming = false;
            revealTimer.stop();
            slowTimer.stop();
            timeoutTimer.stop();
            Qt.callLater(focusDialog);
        }
    }
    onShowDialogChanged: if (showDialog) Qt.callLater(focusDialog)
    onSlowChanged: if (slow && showOverlay && !showDialog) Qt.callLater(() => Focus.take(cancelButton))
    function focusDialog() { if (retryButton.visible) Focus.take(retryButton); }
    visible: showOverlay
    Timer {
        id: revealTimer
        onTriggered: {
            if (!root.active || root.showDialog)
                return;
            root.revealed = true;
            root.scheduleSlow();
        }
    }
    Timer {
        id: slowTimer
        onTriggered: {
            if (!root.active || root.showDialog)
                return;
            root.slow = true;
            root.scheduleTimeout();
        }
    }
    Timer { id: timeoutTimer; onTriggered: if (root.active && !root.failure.length) root.timedOut = true }

    Rectangle {
        anchors.fill: parent
        color: "#12141a"
        opacity: 0.92
    }
    Column {
        anchors.centerIn: parent
        width: 720
        spacing: 28
        visible: !root.showDialog && !root.confirming
        UiText {
            objectName: "launchTitle"
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            role: "section"
            text: root.progressTitle
            maximumLineCount: 2
            elide: Text.ElideRight
        }
        Item {
            anchors.horizontalCenter: parent.horizontalCenter
            width: 360
            height: 6
            Rectangle { anchors.fill: parent; radius: 3; color: Qt.rgba(1, 1, 1, 0.12) }
            Rectangle {
                id: segment
                width: 120
                height: 6
                radius: 3
                color: Theme.ember
                SequentialAnimation on x {
                    running: root.showOverlay && !root.showDialog && !root.confirming
                    loops: Animation.Infinite
                    NumberAnimation { from: -120; to: 360; duration: 1400; easing.type: Easing.InOutCubic }
                }
            }
        }
        UiText {
            objectName: "launchSlow"
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            visible: root.slow && !root.application
            text: qsTr("The browser is taking longer than usual")
            color: Theme.textSecondary
        }
        ActionButton {
            id: cancelButton
            objectName: "launchCancel"
            anchors.horizontalCenter: parent.horizontalCenter
            visible: root.slow && root.cancellable
            text: qsTr("Cancel")
            onActivated: root.requestCancel()
        }
    }
    Column {
        objectName: "launchConfirm"
        anchors.centerIn: parent
        width: 800
        spacing: 24
        visible: root.confirming && !root.showDialog
        UiText {
            objectName: "launchConfirmTitle"
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            font.pixelSize: 48
            font.weight: 700
            text: qsTr("Stop starting %1?").arg(root.serviceTitle)
            wrapMode: Text.WordWrap
        }
        UiText {
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            text: qsTr("The application and everything it has started so far will be closed.")
            color: Theme.textSecondary
            wrapMode: Text.WordWrap
        }
        Row {
            anchors.horizontalCenter: parent.horizontalCenter
            spacing: 24
            ActionButton {
                id: keepButton
                objectName: "launchKeepStarting"
                text: qsTr("Keep starting")
                onActivated: root.keepStarting()
            }
            ActionButton {
                id: stopButton
                objectName: "launchConfirmStop"
                destructive: true
                text: qsTr("Stop")
                onActivated: root.confirmStop()
            }
        }
    }
    Column {
        objectName: "launchDialog"
        anchors.centerIn: parent
        width: 800
        spacing: 24
        visible: root.showDialog
        Rectangle {
            anchors.horizontalCenter: parent.horizontalCenter
            width: 88
            height: 88
            radius: 44
            color: "transparent"
            border.width: 6
            border.color: Theme.error
        }
        UiText {
            objectName: "launchDialogTitle"
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            font.pixelSize: 48
            font.weight: 700
            text: root.dialogTitle
            wrapMode: Text.WordWrap
        }
        UiText {
            objectName: "launchDialogBody"
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            text: root.dialogBody
            color: Theme.textSecondary
            wrapMode: Text.WordWrap
        }
        Row {
            anchors.horizontalCenter: parent.horizontalCenter
            spacing: 24
            ActionButton {
                id: retryButton
                objectName: "launchRetry"
                text: qsTr("Retry")
                onActivated: root.retryRequested()
            }
            ActionButton {
                id: closeButton
                objectName: "launchClose"
                text: qsTr("Close")
                onActivated: root.dismissRequested()
            }
        }
    }
    Keys.onLeftPressed: {
        if (showDialog)
            Focus.take(retryButton);
        else if (confirming)
            Focus.take(keepButton);
    }
    Keys.onRightPressed: {
        if (showDialog)
            Focus.take(closeButton);
        else if (confirming)
            Focus.take(stopButton);
    }
    Keys.onPressed: event => {
        if (event.key === Qt.Key_Home) {
            if (!root.showOverlay) {
                event.accepted = false;
                return;
            }
            event.accepted = true;
            if (event.isAutoRepeat)
                return;
            // Close the dialog, or hide a slow open, without retrying or
            // cancelling. A launch the dialog covered stays hidden too: Home
            // is on screen next (§6.5).
            if (root.showDialog)
                root.dismissRequested();
            root.hideForHome();
            root.homeRequested();
            return;
        }
        if (event.key !== Qt.Key_Escape && event.key !== Qt.Key_Backspace && event.key !== Qt.Key_Back)
            return;
        event.accepted = true;
        if (event.isAutoRepeat)
            return;
        if (root.showDialog)
            root.dismissRequested();
        else if (root.confirming)
            root.keepStarting();
        else if (root.slow)
            root.requestCancel();
    }
}
