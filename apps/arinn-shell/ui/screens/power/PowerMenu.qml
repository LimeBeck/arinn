// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import "../.." as Ui

// Power-scope overlay. Presentation only: intents leave this file, and nothing
// here talks to the session, a registry, or a suspend API. Main supplies the
// platform's availability, refusals and accepted requests from
// SystemControl1 (ADR-0108) and the running applications from the registry.
FocusScope {
    id: root
    objectName: "powerMenu"
    width: 1920
    height: 1080
    focus: true

    // Caller-supplied titles. This overlay does not read a live registry.
    property var closingApplications: []
    // False when the caller cannot know the running applications (no
    // registry): the confirmation says so instead of "none".
    property bool closingKnown: true
    // Platform availability per row: available, inhibited, denied,
    // unsupported or unknown. Unsupported rows are disabled; the others stay
    // usable, and the platform's answer decides.
    property string sleepAvailability: "unknown"
    property string restartAvailability: "unknown"
    property string shutdownAvailability: "unknown"
    // Block inhibitors {what: sleep|shutdown, who, why} that explain a refusal.
    property var inhibitors: []
    // The row whose request is in flight, and the row the platform accepted:
    // sleep, restart or shutdown. Only the caller sets them.
    property string busyAction: ""
    property string acceptedAction: ""
    // Why the platform refused the request of refusalAction: inhibited,
    // denied, unsupported, unavailable, timeout or failed.
    property string refusalCode: ""
    property string refusalAction: ""
    // Non-empty text is the refusal. Nothing in this overlay sets it, and no
    // timer turns a sleep intent into success.
    property string refusalMessage: describeRefusal(refusalCode, refusalAction, inhibitors)
    readonly property bool accepted: acceptedAction.length > 0 && !refused
    property bool pointerMode: false
    property bool hoverFocusEnabled: true
    readonly property bool refused: refusalMessage.length > 0
    property string step: "menu" // menu, confirm
    property string pendingAction: "" // restart, shutdown
    property string menuFocus: "sleep"
    readonly property var menuEntries: [
        {id: "sleep", glyph: "sleep", title: qsTr("Sleep"), description: qsTr("Request system suspend")},
        {id: "restart", glyph: "restart", title: qsTr("Restart"), description: qsTr("Takes about a minute")},
        {id: "shutdown", glyph: "shutdown", title: qsTr("Shut down"), description: qsTr("Turn the computer off completely")},
        {id: "ambient", glyph: "ambient", title: qsTr("Ambient"), description: qsTr("Show the ambient screen")}
    ]
    readonly property string confirmTitle: pendingAction === "shutdown" ? qsTr("Shut down?") : qsTr("Restart?")
    readonly property string confirmActionText: pendingAction === "shutdown" ? qsTr("Shut down") : qsTr("Restart")
    readonly property string closingSummary: {
        if (!closingKnown)
            return qsTr("The list of running applications is not available.");
        const names = [];
        const entries = closingApplications || [];
        for (let i = 0; i < entries.length; ++i) {
            const entry = entries[i];
            const title = typeof entry === "string" ? entry : (entry && entry.title ? String(entry.title) : "");
            if (title.length > 0 && names.indexOf(title) < 0)
                names.push(title);
        }
        if (names.length === 0)
            return qsTr("No running applications.");
        return qsTr("These applications will close: %1.").arg(names.join(", "));
    }

    signal sleepRequested()
    signal restartRequested()
    signal shutdownRequested()
    // The Ambient row: the shell shows its ambient screen over this menu.
    signal ambientRequested()
    signal dismissRequested()
    signal homeRequested()
    // Back on a refusal or on an accepted request: the caller clears it.
    signal refusalDismissed()
    signal acceptedDismissed()

    function availabilityOf(id) {
        return id === "sleep" ? sleepAvailability : id === "restart" ? restartAvailability
             : id === "shutdown" ? shutdownAvailability : "available";
    }
    // "who (why)" of the inhibitors that hold a row's action.
    function holders(id) {
        const lock = id === "sleep" ? "sleep" : "shutdown";
        const names = [];
        const list = inhibitors || [];
        for (let i = 0; i < list.length; ++i) {
            const entry = list[i];
            if (!entry || entry.what !== lock || !entry.who)
                continue;
            const text = entry.why ? qsTr("%1 (%2)").arg(entry.who).arg(entry.why) : String(entry.who);
            if (names.indexOf(text) < 0)
                names.push(text);
        }
        return names;
    }
    // Why a row's action may be refused, or "" when nothing is known.
    function availabilityNote(id) {
        const availability = availabilityOf(id);
        if (availability === "unsupported")
            return qsTr("Not supported on this computer");
        if (availability === "denied")
            return qsTr("Not allowed for this user");
        if (availability === "inhibited") {
            const names = holders(id);
            return names.length ? qsTr("Blocked by %1").arg(names.join(", ")) : qsTr("Blocked by an application");
        }
        return "";
    }
    function rowDescription(entry) {
        if (busyAction === entry.id)
            return qsTr("Waiting for the system…");
        const note = availabilityNote(entry.id);
        return note.length ? note : entry.description;
    }
    function describeRefusal(code, action, list) {
        if (!code)
            return "";
        const id = action === "suspend" ? "sleep" : action === "reboot" ? "restart"
                 : action === "power-off" ? "shutdown" : action;
        if (code === "inhibited") {
            const names = holders(id);
            return names.length ? qsTr("An application is preventing it: %1.").arg(names.join(", "))
                                : qsTr("An application or another power operation is preventing it right now.");
        }
        if (code === "denied")
            return qsTr("The system does not allow it for this user.");
        if (code === "unsupported")
            return qsTr("This computer does not support it.");
        if (code === "unavailable")
            return qsTr("Power control is not available right now. Try again later.");
        if (code === "timeout")
            return qsTr("The system did not answer in time. Try again.");
        return qsTr("The system could not do it. Try again.");
    }
    readonly property string refusalTitle: {
        const id = refusalAction === "suspend" ? "sleep" : refusalAction === "reboot" ? "restart"
                 : refusalAction === "power-off" ? "shutdown" : refusalAction;
        return id === "sleep" ? qsTr("Sleep was refused")
             : id === "restart" ? qsTr("Restart was refused")
             : id === "shutdown" ? qsTr("Shut down was refused") : qsTr("Request refused");
    }

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
        // Disabled rows (unsupported actions) are skipped.
        let next = index;
        for (let tries = 0; tries < items.length; ++tries) {
            let candidate = next + delta;
            if (wrap)
                candidate = (candidate + items.length) % items.length;
            else if (candidate < 0 || candidate >= items.length)
                return;
            next = candidate;
            if (items[next].enabled)
                break;
        }
        if (!items[next].enabled)
            return;
        menuFocus = menuNames()[next];
        Ui.Focus.take(items[next]);
    }
    function moveConfirm(delta, wrap) {
        const items = [cancelButton, confirmButton];
        let index = confirmButton.activeFocus ? 1 : 0;
        let next = index + delta;
        if (wrap)
            next = (next + items.length) % items.length;
        else
            next = Math.max(0, Math.min(items.length - 1, next));
        Ui.Focus.take(items[next]);
    }
    function restoreMenuFocus() {
        const items = menuItems();
        const names = menuNames();
        for (let i = 0; i < names.length; ++i) {
            if (names[i] === menuFocus && Ui.Focus.take(items[i]))
                return;
        }
        for (let i = 0; i < items.length; ++i)
            if (Ui.Focus.take(items[i]))
                return;
    }
    function openConfirm(action) {
        pendingAction = action;
        menuFocus = action;
        step = "confirm";
        Qt.callLater(() => {
            if (step === "confirm" && !refused)
                Ui.Focus.take(cancelButton);
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
        // A refusal stays until the caller clears it. Back does not report success.
        if (refused) {
            refusalDismissed();
            return;
        }
        if (accepted) {
            acceptedDismissed();
            return;
        }
        if (step === "confirm") {
            closeConfirm();
            return;
        }
        dismissRequested();
    }
    // Esc, Backspace and Back are Back in every power scope (§6.2, AC-18).
    // Return and Enter stay with the control so a confirmation can still activate.
    function handleScopeKey(event, onBack) {
        if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter)
            return;
        if (event.key === Qt.Key_Escape || event.key === Qt.Key_Backspace || event.key === Qt.Key_Back) {
            event.accepted = true;
            onBack();
        } else if (event.key === Qt.Key_Home) {
            event.accepted = true;
            goHome();
        }
    }
    function goHome() {
        if (step === "confirm" && !refused && !accepted)
            closeConfirm();
        homeRequested();
    }
    function startAmbient() {
        ambientRequested();
    }

    // Fallback when focus is on this scope rather than a row or pill.
    Keys.onPressed: event => handleScopeKey(event, () => goBack())

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
        }
        MouseArea {
            anchors.fill: parent
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
            visible: !root.refused && !root.accepted
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
                // Confirmation never hides an inhibitor or permission problem.
                Ui.UiText {
                    objectName: "confirmNote"
                    width: parent.width
                    leftPadding: 8
                    rightPadding: 8
                    topPadding: 8
                    visible: root.step === "confirm" && text.length > 0
                    text: root.availabilityNote(root.pendingAction)
                    role: "body"
                    color: Ui.Theme.error
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
                            enabled: root.availabilityOf(modelData.id) !== "unsupported"
                            busy: root.busyAction === modelData.id
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
                                            text: root.rowDescription(row.modelData)
                                            role: "meta"
                                            font.pixelSize: 26
                                            color: row.activeFocus ? Ui.Theme.ink : Ui.Theme.textSecondary
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
                    Keys.onPressed: event => root.handleScopeKey(event, () => root.goBack())
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
                    Keys.onPressed: event => root.handleScopeKey(event, () => root.goBack())
                }
                Ui.ActionButton {
                    id: confirmButton
                    objectName: "confirmButton"
                    width: 342
                    height: 96
                    text: root.confirmActionText
                    destructive: true
                    busy: root.busyAction.length > 0 && root.busyAction === root.pendingAction
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
                    Keys.onPressed: event => root.handleScopeKey(event, () => root.goBack())
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
        visible: root.refused
        focus: visible
        kind: "error"
        title: root.refusalTitle
        description: root.refusalMessage
        actionText: qsTr("Back")
        pointerMode: root.pointerMode
        hoverFocusEnabled: root.hoverFocusEnabled
        onActionRequested: root.goBack()
        onDpadRequested: root.noteDpad()
        Keys.onPressed: event => root.handleScopeKey(event, () => root.goBack())
        onVisibleChanged: {
            if (visible) {
                Qt.callLater(() => { if (visible) Ui.Focus.take(errorPane); });
                return;
            }
            // Clearing an explicit refusal returns to the power step. It does not
            // report success.
            Qt.callLater(() => {
                if (root.refused)
                    return;
                if (root.step === "confirm")
                    Ui.Focus.take(cancelButton);
                else
                    root.restoreMenuFocus();
            });
        }
    }

    // The platform accepted the request. This is not success: the session
    // lifecycle guard hides the shell when the system really sleeps, and
    // nothing here turns into success by itself.
    Ui.StatusPane {
        id: acceptedPane
        objectName: "powerAccepted"
        z: 4
        anchors.centerIn: parent
        width: 820
        visible: root.accepted
        focus: visible
        kind: "loading"
        title: root.acceptedAction === "sleep" ? qsTr("Going to sleep…")
             : root.acceptedAction === "restart" ? qsTr("Restarting…") : qsTr("Shutting down…")
        description: qsTr("Waiting for the system. Press Back to return to Arinn.")
        pointerMode: root.pointerMode
        hoverFocusEnabled: root.hoverFocusEnabled
        onDpadRequested: root.noteDpad()
        Keys.onPressed: event => root.handleScopeKey(event, () => root.goBack())
        onVisibleChanged: {
            if (visible) {
                Qt.callLater(() => { if (visible) Ui.Focus.take(acceptedPane); });
                return;
            }
            Qt.callLater(() => {
                if (root.refused || root.accepted)
                    return;
                if (root.step === "confirm")
                    Ui.Focus.take(cancelButton);
                else
                    root.restoreMenuFocus();
            });
        }
    }

    Component.onCompleted: Qt.callLater(restoreMenuFocus)
    }
}
