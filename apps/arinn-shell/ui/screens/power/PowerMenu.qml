// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import "../.." as Ui

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
    readonly property var menuEntries: [
        {id: "sleep", glyph: "sleep", title: qsTr("Sleep"), description: qsTr("Request system suspend")},
        {id: "restart", glyph: "restart", title: qsTr("Restart"), description: qsTr("Takes about a minute")},
        {id: "shutdown", glyph: "shutdown", title: qsTr("Shut down"), description: qsTr("Turn the computer off completely")},
        {id: "ambient", glyph: "ambient", title: qsTr("Ambient"), description: qsTr("Show the ambient screen")}
    ]
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
        const items = [];
        for (let i = 0; i < menuRows.count; ++i)
            items.push(menuRows.itemAt(i));
        return items;
    }
    function menuNames() {
        const names = [];
        for (let i = 0; i < menuEntries.length; ++i)
            names.push(menuEntries[i].id);
        return names;
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
        if (items.length > 0)
            items[0].forceActiveFocus();
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

    Ui.LogicalCanvas {
        id: canvas
        anchors.fill: parent
        Rectangle {
            anchors.fill: parent
            color: Ui.Theme.background
        }
        Rectangle {
            objectName: "powerScrim"
            anchors.fill: parent
            color: Qt.rgba(0, 0, 0, 0.55)
            visible: root.step !== "ambient"
        }
        MouseArea {
            anchors.fill: parent
            enabled: root.step !== "ambient"
            z: 0
            onClicked: root.goBack()
        }
        Rectangle {
            z: 1
            width: panel.width + 48
            height: panel.height * 0.92
            radius: 70
            color: Qt.rgba(0, 0, 0, 0.45)
            anchors.horizontalCenter: panel.horizontalCenter
            anchors.top: panel.top
            anchors.topMargin: 36
            visible: panel.visible
        }
        Rectangle {
            id: panel
            objectName: "powerPanel"
            z: 2
            width: 800
            radius: 48
            // Translucent dark glass over the dimmed field, with a light edge.
            color: Qt.rgba(0.161, 0.165, 0.188, 0.82)
            border.width: 1
            border.color: Qt.rgba(1, 1, 1, 0.10)
            anchors.centerIn: parent
            visible: !root.refused && root.step !== "ambient"
            height: column.implicitHeight + 96
            Rectangle {
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.top: parent.top
                anchors.leftMargin: 48
                anchors.rightMargin: 48
                height: 1
                color: Qt.rgba(1, 1, 1, 0.10)
            }
            Column {
                id: column
                x: 48
                y: 52
                width: 704
                spacing: 0
                Ui.UiText {
                    objectName: "powerTitle"
                    width: parent.width
                    leftPadding: 8
                    rightPadding: 8
                    role: "title"
                    font.pixelSize: 52
                    font.weight: 700
                    font.letterSpacing: -1.04
                    text: root.step === "confirm" ? root.confirmTitle : qsTr("Power")
                }
                Item {
                    width: 1
                    height: 10
                    visible: root.step === "confirm"
                }
                Ui.UiText {
                    objectName: "closingList"
                    width: parent.width
                    leftPadding: 8
                    rightPadding: 8
                    visible: root.step === "confirm"
                    text: root.closingSummary
                    role: "body"
                    color: Ui.Theme.textSecondary
                }
                Item {
                    width: 1
                    height: root.step === "menu" ? 22 : 16
                    visible: root.step === "menu" || root.step === "confirm"
                }
                Column {
                    width: parent.width
                    spacing: 8
                    visible: root.step === "menu"
                    Repeater {
                        id: menuRows
                        model: root.menuEntries
                        delegate: Ui.Surface {
                            id: row
                            required property var modelData
                            required property int index
                            objectName: modelData.id + "Button"
                            width: 704
                            height: 112
                            text: modelData.title
                            cornerRadius: 30
                            focusScale: 1.02
                            restingColor: Qt.rgba(1, 1, 1, 0.05)
                            focusedColor: Ui.Theme.textPrimary
                            pointerMode: root.pointerMode
                            hoverFocusEnabled: root.hoverFocusEnabled
                            onDpadRequested: root.noteDpad()
                            onActiveFocusChanged: if (activeFocus) root.menuFocus = modelData.id
                            onActivated: {
                                if (modelData.id === "sleep")
                                    root.sleepRequested();
                                else if (modelData.id === "restart")
                                    root.openConfirm("restart");
                                else if (modelData.id === "shutdown")
                                    root.openConfirm("shutdown");
                                else
                                    root.startAmbient();
                            }
                            contentItem: Item {
                                Row {
                                    anchors.left: parent.left
                                    anchors.leftMargin: 28
                                    anchors.right: parent.right
                                    anchors.rightMargin: 28
                                    anchors.verticalCenter: parent.verticalCenter
                                    spacing: 24
                                    Rectangle {
                                        objectName: row.modelData.id + "Icon"
                                        width: 64
                                        height: 64
                                        radius: 32
                                        color: row.activeFocus ? Qt.rgba(0, 0, 0, 0.08) : Qt.rgba(1, 1, 1, 0.08)
                                        Canvas {
                                            id: glyph
                                            anchors.centerIn: parent
                                            width: 38
                                            height: 38
                                            property color ink: row.activeFocus ? Ui.Theme.ink : Ui.Theme.textPrimary
                                            onInkChanged: requestPaint()
                                            onPaint: {
                                                const ctx = getContext("2d");
                                                ctx.clearRect(0, 0, width, height);
                                                ctx.strokeStyle = ink;
                                                ctx.fillStyle = ink;
                                                ctx.lineWidth = 2.6;
                                                ctx.lineCap = "round";
                                                ctx.lineJoin = "round";
                                                const cx = width / 2;
                                                const cy = height / 2;
                                                const kind = row.modelData.glyph;
                                                if (kind === "sleep") {
                                                    ctx.beginPath();
                                                    ctx.arc(cx - 1, cy, 12, 0.7, 5.7);
                                                    ctx.arc(cx + 7, cy - 3, 10, 4.6, 1.5, true);
                                                    ctx.fill();
                                                } else if (kind === "restart") {
                                                    ctx.beginPath();
                                                    ctx.arc(cx, cy, 11, 0.6, 5.3);
                                                    ctx.stroke();
                                                    ctx.beginPath();
                                                    ctx.moveTo(cx + 12, cy - 8);
                                                    ctx.lineTo(cx + 8, cy + 2);
                                                    ctx.lineTo(cx + 2, cy - 6);
                                                    ctx.closePath();
                                                    ctx.fill();
                                                } else if (kind === "shutdown") {
                                                    ctx.beginPath();
                                                    ctx.arc(cx, cy + 2, 11, 0.85, Math.PI - 0.85, true);
                                                    ctx.stroke();
                                                    ctx.beginPath();
                                                    ctx.moveTo(cx, cy - 13);
                                                    ctx.lineTo(cx, cy + 2);
                                                    ctx.stroke();
                                                } else {
                                                    ctx.beginPath();
                                                    ctx.arc(cx, cy, 6.5, 0, Math.PI * 2);
                                                    ctx.stroke();
                                                    for (let ray = 0; ray < 8; ++ray) {
                                                        const angle = ray * Math.PI / 4;
                                                        ctx.beginPath();
                                                        ctx.moveTo(cx + Math.cos(angle) * 10, cy + Math.sin(angle) * 10);
                                                        ctx.lineTo(cx + Math.cos(angle) * 15, cy + Math.sin(angle) * 15);
                                                        ctx.stroke();
                                                    }
                                                }
                                            }
                                            Component.onCompleted: requestPaint()
                                        }
                                    }
                                    Column {
                                        width: parent.width - 88
                                        anchors.verticalCenter: parent.verticalCenter
                                        spacing: 4
                                        Ui.UiText {
                                            objectName: row.modelData.id + "Label"
                                            width: parent.width
                                            text: row.text
                                            role: "control"
                                            font.pixelSize: 32
                                            font.weight: 600
                                            color: row.activeFocus ? Ui.Theme.ink : Ui.Theme.textPrimary
                                            elide: Text.ElideRight
                                            maximumLineCount: 1
                                        }
                                        Ui.UiText {
                                            objectName: row.modelData.id + "Description"
                                            width: parent.width
                                            text: row.modelData.description
                                            role: "meta"
                                            font.pixelSize: 26
                                            color: row.activeFocus ? "#5E6068" : Ui.Theme.textSecondary
                                            elide: Text.ElideRight
                                            maximumLineCount: 1
                                        }
                                    }
                                }
                            }
                    Keys.onUpPressed: event => { event.accepted = true; root.noteDpad(); root.moveMenu(-1, false); }
                    Keys.onDownPressed: event => { event.accepted = true; root.noteDpad(); root.moveMenu(1, false); }
                    Keys.onLeftPressed: event => { event.accepted = true; root.noteDpad(); }
                    Keys.onRightPressed: event => { event.accepted = true; root.noteDpad(); }
                    Keys.onTabPressed: event => { event.accepted = true; root.moveMenu(1, true); }
                    Keys.onBacktabPressed: event => { event.accepted = true; root.moveMenu(-1, true); }
                    Keys.onEscapePressed: event => { event.accepted = true; root.goBack(); }
                    Keys.onPressed: event => {
                        if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter)
                            return;
                        if (event.key === Qt.Key_Backspace || event.key === Qt.Key_Back) {
                            event.accepted = true;
                            root.goBack();
                        } else if (event.key === Qt.Key_Home) {
                            event.accepted = true;
                            root.goHome();
                        }
                    }
                }
            }
        }
                Row {
                    objectName: "powerConfirm"
                    visible: root.step === "confirm"
                    spacing: 20
                    Ui.ActionButton {
                        id: cancelButton
                        objectName: "cancelButton"
                        width: 342
                        height: 96
                        restingColor: Qt.rgba(1, 1, 1, 0.13)
                        text: qsTr("Cancel")
                    pointerMode: root.pointerMode
                    hoverFocusEnabled: root.hoverFocusEnabled
                    onDpadRequested: root.noteDpad()
                    onActivated: root.closeConfirm()
                    Keys.onLeftPressed: event => { event.accepted = true; root.noteDpad(); root.moveConfirm(-1, false); }
                    Keys.onRightPressed: event => { event.accepted = true; root.noteDpad(); root.moveConfirm(1, false); }
                    Keys.onUpPressed: event => { event.accepted = true; root.noteDpad(); }
                    Keys.onDownPressed: event => { event.accepted = true; root.noteDpad(); }
                    Keys.onTabPressed: event => { event.accepted = true; root.moveConfirm(1, true); }
                    Keys.onBacktabPressed: event => { event.accepted = true; root.moveConfirm(-1, true); }
                    Keys.onEscapePressed: event => { event.accepted = true; root.goBack(); }
                    Keys.onPressed: event => {
                        if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter)
                            return;
                        if (event.key === Qt.Key_Backspace || event.key === Qt.Key_Back) {
                            event.accepted = true;
                            root.goBack();
                        } else if (event.key === Qt.Key_Home) {
                            event.accepted = true;
                            root.goHome();
                        }
                    }
                }
                Ui.ActionButton {
                    id: confirmButton
                    objectName: "confirmButton"
                    width: 342
                    height: 96
                    text: root.confirmActionText
                    destructive: true
                    pointerMode: root.pointerMode
                    hoverFocusEnabled: root.hoverFocusEnabled
                    onDpadRequested: root.noteDpad()
                    onActivated: root.confirmAction()
                    Keys.onLeftPressed: event => { event.accepted = true; root.noteDpad(); root.moveConfirm(-1, false); }
                    Keys.onRightPressed: event => { event.accepted = true; root.noteDpad(); root.moveConfirm(1, false); }
                    Keys.onUpPressed: event => { event.accepted = true; root.noteDpad(); }
                    Keys.onDownPressed: event => { event.accepted = true; root.noteDpad(); }
                    Keys.onTabPressed: event => { event.accepted = true; root.moveConfirm(1, true); }
                    Keys.onBacktabPressed: event => { event.accepted = true; root.moveConfirm(-1, true); }
                    Keys.onEscapePressed: event => { event.accepted = true; root.goBack(); }
                    Keys.onPressed: event => {
                        if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter)
                            return;
                        if (event.key === Qt.Key_Backspace || event.key === Qt.Key_Back) {
                            event.accepted = true;
                            root.goBack();
                        } else if (event.key === Qt.Key_Home) {
                            event.accepted = true;
                            root.goHome();
                        }
                    }
                }
            }
        }
    }

    Ui.StatusPane {
        id: errorPane
        objectName: "powerError"
        z: 4
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
        onVisibleChanged: {
            if (visible) {
                Qt.callLater(() => { if (visible) forceActiveFocus(); });
                return;
            }
            // Clearing an explicit refusal returns to the power step. It does not
            // report success.
            Qt.callLater(() => {
                if (root.refused || root.step === "ambient")
                    return;
                if (root.step === "confirm")
                    cancelButton.forceActiveFocus();
                else
                    root.restoreMenuFocus();
            });
        }
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

    Component.onCompleted: Qt.callLater(restoreMenuFocus)
    }
}
