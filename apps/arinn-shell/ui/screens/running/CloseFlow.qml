// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import QtQuick.Window
import "../.." as Ui

// One close flow for the tile sheet, the Continue card and the Running
// screen (handoff *Closing applications*): the confirmation (CL-8), the
// closing state (CL-9), the closed toast (CL-10, CL-16) and the
// informational "did not close" dialog (CL-11). Arinn only asks an
// application to close; there is no retry and no force close.
FocusScope {
    id: root
    anchors.fill: parent
    // A system scope above it (power, the quick menu, ambient) hides the
    // dialog while it is open; the dialog comes back with focus after it.
    property bool suppressed: false
    visible: !suppressed && (phase.length > 0 || toastMessage.length > 0)
    enabled: !suppressed && phase.length > 0

    // Running windows as the shell lists them: {id, title, terminable}.
    property var windows: []
    // Whether `windows` is the daemon's current list. While it is not (the
    // daemon went away, the catalog is reloading), a window missing from it
    // has not closed (review of #777).
    property bool windowsKnown: true
    property bool pointerMode: Ui.InputMode.pointerActive
    // How long a window may stay after a successful request before it
    // counts as "did not close" (an open question of the handoff; 10 s).
    property int closeWaitMs: 10000
    readonly property int toastDurationMs: 2600
    // "", "confirm" or "failed".
    property string phase: ""
    property var target: ({})
    // Windows asked to close and still listed (CL-9).
    property var closingIds: []
    // Windows whose request failed or that stayed (CL-11).
    property var failedIds: []
    property var startedAt: ({})
    property var opener: null
    property string toastMessage: ""
    readonly property bool open: phase.length > 0

    signal terminateRequested(string windowId)
    signal switchRequested(string windowId)
    // A window that was asked to close has gone.
    signal closed(string windowId)

    function windowById(id) {
        return (windows || []).find(entry => entry && entry.id === id) || null;
    }
    function remember() {
        const window = root.Window.window;
        opener = window ? window.activeFocusItem : null;
    }
    function restoreOpener() {
        const item = opener;
        opener = null;
        if (item && item.visible && item.enabled) Ui.Focus.take(item);
    }
    // CL-8: ask before closing. A window Arinn cannot close is not asked.
    function ask(windowId) {
        const entry = windowById(windowId);
        if (!entry || entry.terminable === false || closingIds.indexOf(windowId) >= 0) return false;
        remember();
        target = {id: entry.id, title: entry.title || ""};
        phase = "confirm";
        Qt.callLater(() => { if (phase === "confirm") Ui.Focus.take(cancelButton); });
        return true;
    }
    function cancel() {
        if (!open) return;
        phase = "";
        Qt.callLater(restoreOpener);
        Qt.callLater(showQueued);
    }
    // A failure that arrives while another dialog is open waits for it
    // (review of #777).
    property var queuedFailures: []
    function showQueued() {
        if (open || !queuedFailures.length) return;
        const id = queuedFailures[0];
        queuedFailures = queuedFailures.slice(1);
        showFailure(id);
    }
    function showFailure(id) {
        const entry = windowById(id);
        const title = entry ? (entry.title || "") : (titles[id] || "");
        remember();
        target = {id: id, title: title};
        phase = "failed";
        Qt.callLater(() => { if (phase === "failed") Ui.Focus.take(switchButton); });
    }
    function confirm() {
        if (phase !== "confirm") return;
        const id = target.id;
        phase = "";
        failedIds = failedIds.filter(other => other !== id);
        closingIds = closingIds.concat([id]);
        const started = Object.assign({}, startedAt);
        started[id] = Date.now();
        startedAt = started;
        waitTimer.start();
        terminateRequested(id);
        Qt.callLater(restoreOpener);
        Qt.callLater(showQueued);
    }
    function forget(id) {
        closingIds = closingIds.filter(other => other !== id);
        const started = Object.assign({}, startedAt);
        delete started[id];
        startedAt = started;
    }
    // CL-11: the request failed or the window stayed.
    function didNotClose(id) {
        if (closingIds.indexOf(id) < 0) return;
        forget(id);
        failedIds = failedIds.concat([id]);
        if (open) {
            if (queuedFailures.indexOf(id) < 0) queuedFailures = queuedFailures.concat([id]);
            return;
        }
        showFailure(id);
    }
    // The shell's operation result for a close request.
    function operationFinished(id, state) {
        if (state !== "succeeded" && state !== "cancelled") didNotClose(id);
    }
    function restoreFocus() {
        if (phase === "confirm") Ui.Focus.take(cancelButton);
        else if (phase === "failed") Ui.Focus.take(switchButton);
    }
    function check() {
        const listed = (windows || []).map(entry => entry.id);
        // No evidence either way while the list is not the daemon's own:
        // a request stays pending until a trusted snapshot settles it
        // (review of #777).
        if (!windowsKnown) return;
        for (const id of closingIds.slice()) {
            if (listed.indexOf(id) < 0) {
                const title = (target.id === id && target.title) || titles[id] || "";
                forget(id);
                closed(id);
                showToast(title.length ? qsTr("%1 closed").arg(title) : qsTr("Closed"));
            } else if (Date.now() - (startedAt[id] || 0) >= closeWaitMs) {
                didNotClose(id);
            }
        }
        failedIds = failedIds.filter(id => listed.indexOf(id) >= 0);
        // The window closed itself while the confirmation was open.
        if (open && listed.indexOf(target.id) < 0) cancel();
        if (!closingIds.length) waitTimer.stop();
    }
    // Titles of windows asked to close, kept for the toast once they are gone.
    property var titles: ({})
    onClosingIdsChanged: {
        const next = Object.assign({}, titles);
        for (const id of closingIds) {
            const entry = windowById(id);
            if (entry && entry.title) next[id] = entry.title;
        }
        titles = next;
    }
    onWindowsChanged: check()
    onWindowsKnownChanged: check()
    function showToast(message) {
        toastMessage = message;
        toastTimer.restart();
    }
    Timer { id: waitTimer; interval: 500; repeat: true; onTriggered: root.check() }
    Timer { id: toastTimer; interval: root.toastDurationMs; onTriggered: root.toastMessage = "" }

    Keys.onPressed: event => {
        if (event.key === Qt.Key_Escape || event.key === Qt.Key_Backspace || event.key === Qt.Key_Back
                || event.key === Qt.Key_Home) {
            // Back is Cancel; Home closes the dialog without closing the app.
            if (!event.isAutoRepeat) cancel();
        } else if (event.key === Qt.Key_Left || event.key === Qt.Key_Right
                   || event.key === Qt.Key_Tab || event.key === Qt.Key_Backtab) {
            const pair = phase === "confirm" ? [cancelButton, confirmButton] : [switchButton, stayButton];
            Ui.InputMode.keyPressed();
            const next = pair[0].activeFocus ? pair[1] : pair[0];
            // After this key's delivery: a button still handling it would
            // take focus back.
            Qt.callLater(() => Ui.Focus.take(next));
        }
        event.accepted = true;
    }

    Ui.LogicalCanvas {
        anchors.fill: parent
        Item {
            anchors.fill: parent
            Rectangle {
                objectName: "closeFlowScrim"
                anchors.fill: parent
                visible: root.open
                color: Qt.rgba(0, 0, 0, 0.58)
                MouseArea { anchors.fill: parent; onClicked: root.cancel() }
            }
            Rectangle {
                id: dialog
                objectName: root.phase === "failed" ? "closeFailedDialog" : "closeConfirmDialog"
                visible: root.open
                anchors.centerIn: parent
                width: 900
                height: content.implicitHeight + 112
                radius: 48
                color: Qt.rgba(0.161, 0.165, 0.188, 0.94)
                border.width: 1
                border.color: Qt.rgba(1, 1, 1, 0.10)
                MouseArea { anchors.fill: parent }
                Column {
                    id: content
                    x: 56; y: 56
                    width: parent.width - 112
                    spacing: 24
                    Rectangle {
                        objectName: "closeFailedIcon"
                        visible: root.phase === "failed"
                        width: 64; height: 64; radius: 32
                        color: Qt.rgba(1, 1, 1, 0.08)
                        Ui.UiText { anchors.centerIn: parent; text: "i"; font.pixelSize: 36; font.weight: 700; color: Ui.Theme.textSecondary }
                    }
                    Ui.UiText {
                        objectName: "closeFlowTitle"
                        width: parent.width
                        text: root.phase === "failed" ? qsTr("%1 did not close").arg(root.target.title || "")
                                                      : qsTr("Close %1?").arg(root.target.title || "")
                        role: "title"
                        font.pixelSize: 48
                        wrapMode: Text.WordWrap
                    }
                    Ui.UiText {
                        objectName: "closeFlowBody"
                        width: parent.width
                        text: root.phase === "failed"
                              ? qsTr("It may be waiting for an answer on its own screen, for example to save changes. Go to %1 and answer there.").arg(root.target.title || "")
                              : qsTr("Arinn will ask %1 to close. Unsaved changes and game progress may be lost.").arg(root.target.title || "")
                        color: Ui.Theme.textSecondary
                        wrapMode: Text.WordWrap
                    }
                    Row {
                        spacing: 20
                        visible: root.phase === "confirm"
                        Ui.ActionButton {
                            id: cancelButton
                            objectName: "closeFlowCancel"
                            height: 96
                            text: qsTr("Cancel")
                            pointerMode: root.pointerMode
                            onActivated: root.cancel()
                        }
                        Ui.ActionButton {
                            id: confirmButton
                            objectName: "closeFlowConfirm"
                            height: 96
                            text: qsTr("Close")
                            destructive: true
                            pointerMode: root.pointerMode
                            onActivated: root.confirm()
                        }
                    }
                    Row {
                        spacing: 20
                        visible: root.phase === "failed"
                        Ui.ActionButton {
                            id: switchButton
                            objectName: "closeFlowSwitch"
                            height: 96
                            text: qsTr("Go to %1").arg(root.target.title || "")
                            pointerMode: root.pointerMode
                            onActivated: {
                                const id = root.target.id;
                                root.phase = "";
                                root.opener = null;
                                root.switchRequested(id);
                                Qt.callLater(root.showQueued);
                            }
                        }
                        Ui.ActionButton {
                            id: stayButton
                            objectName: "closeFlowStay"
                            height: 96
                            text: qsTr("Stay")
                            pointerMode: root.pointerMode
                            onActivated: root.cancel()
                        }
                    }
                }
            }
            Rectangle {
                objectName: "closeFlowToast"
                visible: root.toastMessage.length > 0
                anchors.horizontalCenter: parent.horizontalCenter
                y: 1080 - 48 - height
                height: 84
                width: Math.min(1600, toastText.implicitWidth + 96)
                radius: height / 2
                color: Ui.Theme.plate
                border.width: 1
                border.color: Qt.rgba(1, 1, 1, 0.08)
                Ui.UiText {
                    id: toastText
                    objectName: "closeFlowToastText"
                    anchors.centerIn: parent
                    text: root.toastMessage
                    role: "meta"
                }
            }
        }
    }
}
