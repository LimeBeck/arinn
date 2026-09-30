// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick

// Opening overlay for a web service. The tint is neutral: service hues wait
// for the icon contract. The overlay stays hidden when the operation finishes
// before revealDelay.
FocusScope {
    id: root
    property bool active: false
    property string serviceTitle: ""
    property string failure: ""
    property int revealDelay: 400
    property int slowDelay: 8000
    property int timeoutDelay: 30000
    property bool revealed: false
    property bool slow: false
    property bool timedOut: false
    readonly property bool showDialog: failure.length > 0 || timedOut
    readonly property bool showOverlay: showDialog || (revealed && active)
    readonly property string dialogTitle: qsTr("%1 did not open").arg(serviceTitle.length ? serviceTitle : qsTr("The service"))
    readonly property string dialogBody: timedOut && !failure.length
            ? qsTr("The browser did not respond within 30 seconds. Try again.")
            : qsTr("Try again. If this happens again, restart Arinn.")
    signal cancelRequested()
    signal retryRequested()
    signal dismissRequested()

    function arm() {
        revealed = false;
        slow = false;
        timedOut = false;
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
        timeoutTimer.interval = Math.max(0, timeoutDelay - slowDelay);
        timeoutTimer.restart();
    }
    function disarm() {
        revealTimer.stop();
        slowTimer.stop();
        timeoutTimer.stop();
        revealed = false;
        slow = false;
        timedOut = false;
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
    onFailureChanged: {
        if (failure.length) {
            revealTimer.stop();
            slowTimer.stop();
            timeoutTimer.stop();
            Qt.callLater(focusDialog);
        }
    }
    onShowDialogChanged: if (showDialog) Qt.callLater(focusDialog)
    onSlowChanged: if (slow && showOverlay && !showDialog) Qt.callLater(() => cancelButton.forceActiveFocus())
    function focusDialog() { if (retryButton.visible) retryButton.forceActiveFocus(); }
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
        visible: !root.showDialog
        UiText {
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            role: "section"
            text: qsTr("Opening %1…").arg(root.serviceTitle)
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
                    running: root.showOverlay && !root.showDialog
                    loops: Animation.Infinite
                    NumberAnimation { from: -120; to: 360; duration: 1400; easing.type: Easing.InOutCubic }
                }
            }
        }
        UiText {
            objectName: "launchSlow"
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            visible: root.slow
            text: qsTr("The browser is taking longer than usual")
            color: Theme.textSecondary
        }
        ActionButton {
            id: cancelButton
            objectName: "launchCancel"
            anchors.horizontalCenter: parent.horizontalCenter
            visible: root.slow
            text: qsTr("Cancel")
            onActivated: root.cancelRequested()
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
    Keys.onLeftPressed: if (showDialog) retryButton.forceActiveFocus()
    Keys.onRightPressed: if (showDialog) closeButton.forceActiveFocus()
    Keys.onPressed: event => {
        if (event.key !== Qt.Key_Escape && event.key !== Qt.Key_Backspace && event.key !== Qt.Key_Back)
            return;
        event.accepted = true;
        if (event.isAutoRepeat)
            return;
        if (root.showDialog)
            root.dismissRequested();
        else if (root.slow)
            root.cancelRequested();
    }
}
