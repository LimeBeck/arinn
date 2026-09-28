// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import "../.." as Ui

component MenuRow: Ui.Surface {
    id: row
    required property var owner
    property string description: ""
    width: 704
    height: 112
    cornerRadius: 30
    focusScale: 1.02
    pointerMode: owner.pointerMode
        hoverFocusEnabled: owner.hoverFocusEnabled
    onDpadRequested: owner.noteDpad()
    contentItem: Item {
        Column {
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            spacing: 4
            Ui.UiText {
                x: 28
                width: parent.width - 56
                text: row.text
                role: "control"
                color: row.activeFocus ? Ui.Theme.textPrimary : Ui.Theme.textSecondary
                elide: Text.ElideRight
                maximumLineCount: 1
            }
            Ui.UiText {
                x: 28
                width: parent.width - 56
                text: row.description
                role: "caption"
                color: Ui.Theme.textHint
                elide: Text.ElideRight
                maximumLineCount: 1
            }
        }
    }
    Keys.onUpPressed: event => { event.accepted = true; row.owner.noteDpad(); row.owner.moveMenu(-1, false); }
    Keys.onDownPressed: event => { event.accepted = true; row.owner.noteDpad(); row.owner.moveMenu(1, false); }
    Keys.onLeftPressed: event => { event.accepted = true; row.owner.noteDpad(); }
    Keys.onRightPressed: event => { event.accepted = true; row.owner.noteDpad(); }
    Keys.onTabPressed: event => { event.accepted = true; row.owner.moveMenu(1, true); }
    Keys.onBacktabPressed: event => { event.accepted = true; row.owner.moveMenu(-1, true); }
    Keys.onEscapePressed: event => { event.accepted = true; row.owner.goBack(); }
    Keys.onPressed: event => {
        if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter)
            return;
        if (event.key === Qt.Key_Backspace || event.key === Qt.Key_Back) {
            event.accepted = true;
            row.owner.goBack();
        } else if (event.key === Qt.Key_Home) {
            event.accepted = true;
            row.owner.goHome();
        }
    }
}

component ConfirmButton: Ui.ActionButton {
    id: confirm
    required property var owner
    width: 344
    pointerMode: owner.pointerMode
        hoverFocusEnabled: owner.hoverFocusEnabled
    onDpadRequested: owner.noteDpad()
    Keys.onLeftPressed: event => { event.accepted = true; confirm.owner.noteDpad(); confirm.owner.moveConfirm(-1, false); }
    Keys.onRightPressed: event => { event.accepted = true; confirm.owner.noteDpad(); confirm.owner.moveConfirm(1, false); }
    Keys.onUpPressed: event => { event.accepted = true; confirm.owner.noteDpad(); }
    Keys.onDownPressed: event => { event.accepted = true; confirm.owner.noteDpad(); }
    Keys.onTabPressed: event => { event.accepted = true; confirm.owner.moveConfirm(1, true); }
    Keys.onBacktabPressed: event => { event.accepted = true; confirm.owner.moveConfirm(-1, true); }
    Keys.onEscapePressed: event => { event.accepted = true; confirm.owner.goBack(); }
    Keys.onPressed: event => {
        if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter)
            return;
        if (event.key === Qt.Key_Backspace || event.key === Qt.Key_Back) {
            event.accepted = true;
            confirm.owner.goBack();
        } else if (event.key === Qt.Key_Home) {
            event.accepted = true;
            confirm.owner.goHome();
        }
    }
}


// Power-scope overlay. Presentation only: intents leave this file, and nothing
// here talks to the session, a registry, or a suspend API.
FocusScope {
    id: root
    objectName: "powerMenu"
    width: 1920
    height: 1080
    focus: true

    // Caller-supplied titles. This overlay does not read a live registry.
    property var closingApplications: []
    // Non-empty text is the refusal. Nothing in this overlay sets it, and no
    // timer turns a sleep intent into success.
    property string refusalMessage: ""
    property string clockText: ""
    property string dateText: ""
    property bool pointerMode: false
    property bool hoverFocusEnabled: true
    readonly property bool refused: refusalMessage.length > 0
    property string step: "menu" // menu, confirm, ambient
    property string pendingAction: "" // restart, shutdown
    property string menuFocus: "sleep"
    property string returnStep: "menu"
    property bool ambientArmed: false
    // Set on the ambient key press so the matching release performs the wake.
    // Restoring focus before that release would activate the control underneath.
    property bool waking: false
    readonly property string confirmTitle: pendingAction === "shutdown" ? qsTr("Shut down?") : qsTr("Restart?")
    readonly property string confirmActionText: pendingAction === "shutdown" ? qsTr("Shut down") : qsTr("Restart")
    readonly property string closingSummary: {
        const names = [];
        const entries = closingApplications || [];
        for (let i = 0; i < entries.length; ++i) {
            const entry = entries[i];
            const title = typeof entry === "string" ? entry : (entry && entry.title ? String(entry.title) : "");
            if (title.length > 0)
                names.push(title);
        }
        if (names.length === 0)
            return qsTr("No running applications.");
        return qsTr("These applications will close: %1.").arg(names.join(", "));
    }

    signal sleepRequested()
    signal restartRequested()
    signal shutdownRequested()
    signal ambientRequested()
    signal wakeRequested()
    signal dismissRequested()
    signal homeRequested()

    function noteDpad() {
        pointerMode = false;
        hoverFocusEnabled = true;
    }
    function menuItems() {
        return [sleepButton, restartButton, shutdownButton, ambientButton];
    }
    function menuNames() {
        return ["sleep", "restart", "shutdown", "ambient"];
    }
    function moveMenu(delta, wrap) {
        const items = menuItems();
        let index = 0;
        for (let i = 0; i < items.length; ++i)
            if (items[i].activeFocus)
                index = i;
        let next = index + delta;
        if (wrap)
            next = (next + items.length) % items.length;
        else
            next = Math.max(0, Math.min(items.length - 1, next));
        menuFocus = menuNames()[next];
        items[next].forceActiveFocus();
    }
    function moveConfirm(delta, wrap) {
        const items = [cancelButton, confirmButton];
        let index = confirmButton.activeFocus ? 1 : 0;
        let next = index + delta;
        if (wrap)
            next = (next + items.length) % items.length;
        else
            next = Math.max(0, Math.min(items.length - 1, next));
        items[next].forceActiveFocus();
    }
    function restoreMenuFocus() {
        const items = menuItems();
        const names = menuNames();
        for (let i = 0; i < names.length; ++i) {
            if (names[i] === menuFocus) {
                items[i].forceActiveFocus();
                return;
            }
        }
        sleepButton.forceActiveFocus();
    }
    function openConfirm(action) {
        pendingAction = action;
        menuFocus = action;
        step = "confirm";
        Qt.callLater(() => {
            if (step === "confirm" && !refused)
                cancelButton.forceActiveFocus();
        });
    }
    function closeConfirm() {
        step = "menu";
        pendingAction = "";
        Qt.callLater(restoreMenuFocus);
    }
    function confirmAction() {
        if (pendingAction === "restart")
            restartRequested();
        else if (pendingAction === "shutdown")
            shutdownRequested();
    }
    function goBack() {
        if (step === "confirm" && !refused) {
            closeConfirm();
            return;
        }
        // A refusal stays until the caller clears it. Back does not report success.
        dismissRequested();
    }
    function goHome() {
        if (step === "confirm" && !refused)
            closeConfirm();
        homeRequested();
    }
    function startAmbient() {
        if (step === "ambient")
            return;
        returnStep = step === "confirm" ? "confirm" : "menu";
        ambientRequested();
        step = "ambient";
    }
    function wakeFromAmbient() {
        if (step !== "ambient")
            return;
        const back = returnStep === "confirm" ? "confirm" : "menu";
        step = back;
        ambientArmed = false;
        waking = false;
        // The wake event itself must not move focus onto a control.
        hoverFocusEnabled = false;
        wakeRequested();
        Qt.callLater(() => {
            if (step === "confirm")
                cancelButton.forceActiveFocus();
            else
                restoreMenuFocus();
            Qt.callLater(() => { hoverFocusEnabled = true; });
        });
    }

    Rectangle {
        anchors.fill: parent
        color: Qt.rgba(0, 0, 0, 0.58)
        visible: root.step !== "ambient"
    }
    MouseArea {
        anchors.fill: parent
        enabled: root.step !== "ambient"
        z: 0
        onClicked: root.goBack()
    }
    Rectangle {
        id: panel
        z: 1
        width: 800
        radius: 48
        color: "#292b30"
        anchors.centerIn: parent
        visible: !root.refused && root.step !== "ambient"
        height: column.implicitHeight + 96
        Column {
            id: column
            x: 48
            y: 48
            width: 704
            spacing: 16
            Ui.UiText {
                objectName: "powerTitle"
                width: parent.width
                role: "title"
                text: root.step === "confirm" ? root.confirmTitle : qsTr("Power")
            }
            Ui.UiText {
                objectName: "closingList"
                width: parent.width
                visible: root.step === "confirm"
                text: root.closingSummary
                role: "body"
                color: Ui.Theme.textSecondary
            }
            MenuRow {
                id: sleepButton
                objectName: "sleepButton"
                owner: root
                visible: root.step === "menu"
                text: qsTr("Sleep")
                description: qsTr("Request system suspend")
                onActiveFocusChanged: if (activeFocus) root.menuFocus = "sleep"
                onActivated: root.sleepRequested()
            }
            MenuRow {
                id: restartButton
                objectName: "restartButton"
                owner: root
                visible: root.step === "menu"
                text: qsTr("Restart")
                description: qsTr("Confirmation is required")
                onActiveFocusChanged: if (activeFocus) root.menuFocus = "restart"
                onActivated: root.openConfirm("restart")
            }
            MenuRow {
                id: shutdownButton
                objectName: "shutdownButton"
                owner: root
                visible: root.step === "menu"
                text: qsTr("Shut down")
                description: qsTr("Confirmation is required")
                onActiveFocusChanged: if (activeFocus) root.menuFocus = "shutdown"
                onActivated: root.openConfirm("shutdown")
            }
            MenuRow {
                id: ambientButton
                objectName: "ambientButton"
                owner: root
                visible: root.step === "menu"
                text: qsTr("Ambient")
                description: qsTr("Show the ambient screen")
                onActiveFocusChanged: if (activeFocus) root.menuFocus = "ambient"
                onActivated: root.startAmbient()
            }
            Row {
                objectName: "powerConfirm"
                visible: root.step === "confirm"
                spacing: 16
                ConfirmButton {
                    id: cancelButton
                    objectName: "cancelButton"
                    owner: root
                    text: qsTr("Cancel")
                    onActivated: root.closeConfirm()
                }
                ConfirmButton {
                    id: confirmButton
                    objectName: "confirmButton"
                    owner: root
                    text: root.confirmActionText
                    destructive: true
                    onActivated: root.confirmAction()
                }
            }
        }
    }

    Ui.StatusPane {
        id: errorPane
        objectName: "powerError"
        z: 2
        anchors.centerIn: parent
        width: 820
        visible: root.refused && root.step !== "ambient"
        focus: visible
        kind: "error"
        title: qsTr("Request refused")
        description: root.refusalMessage
        actionText: qsTr("Back")
        pointerMode: root.pointerMode
        hoverFocusEnabled: root.hoverFocusEnabled
        onActionRequested: root.goBack()
        onDpadRequested: root.noteDpad()
        onVisibleChanged: if (visible) Qt.callLater(() => { if (visible) forceActiveFocus(); })
    }

    Rectangle {
        id: ambientLayer
        objectName: "ambientScreen"
        anchors.fill: parent
        z: 3
        visible: root.step === "ambient"
        color: Ui.Theme.background
        focus: visible
        onVisibleChanged: {
            root.ambientArmed = false;
            if (!visible)
                return;
            forceActiveFocus();
            Qt.callLater(() => {
                if (visible)
                    root.ambientArmed = true;
            });
        }
        Column {
            anchors.centerIn: parent
            spacing: 16
            width: 1200
            Ui.UiText {
                objectName: "ambientClock"
                width: parent.width
                horizontalAlignment: Text.AlignHCenter
                role: "display"
                color: Ui.Theme.textSecondary
                text: root.clockText
            }
            Ui.UiText {
                objectName: "ambientDate"
                width: parent.width
                horizontalAlignment: Text.AlignHCenter
                role: "section"
                color: Ui.Theme.textHint
                text: root.dateText
            }
        }
        Keys.onPressed: event => {
            event.accepted = true;
            root.waking = true;
        }
        Keys.onReleased: event => {
            event.accepted = true;
            if (root.waking && root.step === "ambient")
                root.wakeFromAmbient();
        }
        MouseArea {
            anchors.fill: parent
            hoverEnabled: true
            onPressed: mouse => {
                mouse.accepted = true;
                if (root.ambientArmed)
                    root.wakeFromAmbient();
            }
            onPositionChanged: {
                if (root.ambientArmed)
                    root.wakeFromAmbient();
            }
        }
    }

    Component.onCompleted: sleepButton.forceActiveFocus()
}
