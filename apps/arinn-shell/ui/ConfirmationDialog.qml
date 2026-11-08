// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtQml.Models

// The owning screen opens the dialog, chooses its initial action and restores
// focus. This local scope owns only its plate, slots and modal key navigation.
FocusScope {
    id: root
    anchors.fill: parent
    visible: false

    property string title: ""
    property string body: ""
    property bool dismissible: true
    property Item initialFocusItem: null
    property alias leading: leadingSlot.data
    property alias content: contentSlot.data
    default property alias actions: actionRow.data
    readonly property alias plate: plate
    readonly property alias scrim: scrim
    readonly property alias titleItem: titleText
    readonly property alias bodyItem: bodyText
    signal dismissed()

    Accessible.role: Accessible.Dialog
    Accessible.name: title

    function availableActions() {
        const result = [];
        for (const item of actionRow.children)
            if (item.visible && item.enabled && item.activeFocusOnTab)
                result.push(item);
        return result;
    }
    function focusInitial() {
        if (!visible || !enabled)
            return false;
        if (initialFocusItem && initialFocusItem.visible && initialFocusItem.enabled
                && Focus.take(initialFocusItem))
            return true;
        const items = availableActions();
        return Focus.take(items.length ? items[0] : root);
    }
    function adjacentAction(item, step) {
        const items = availableActions();
        const at = items.indexOf(item);
        return items.length ? items[(at + step + items.length) % items.length] : root;
    }
    // Qt's built-in Tab handling runs before a parent scope receives the key.
    // Keep each action's tab chain local, also when an action is disabled or
    // disappears. Restore attached bindings when the dialog releases a child.
    Instantiator {
        model: actionRow.children
        delegate: QtObject {
            id: link
            required property var modelData
            property Binding next: Binding {
                target: link.modelData.KeyNavigation
                property: "tab"
                value: root.adjacentAction(link.modelData, 1)
            }
            property Binding previous: Binding {
                target: link.modelData.KeyNavigation
                property: "backtab"
                value: root.adjacentAction(link.modelData, -1)
            }
            function noteKeyboardFocus() {
                if (modelData.activeFocus
                        && [Qt.TabFocusReason, Qt.BacktabFocusReason].includes(modelData.focusReason))
                    InputMode.keyPressed();
            }
            property Connections focusWatch: Connections {
                target: link.modelData
                ignoreUnknownSignals: true
                function onActiveFocusChanged() { link.noteKeyboardFocus(); }
                function onFocusReasonChanged() { link.noteKeyboardFocus(); }
            }
        }
    }
    Keys.onPressed: event => {
        if (!root.visible || !root.enabled) {
            event.accepted = false;
            return;
        }
        // Home belongs to the screen's routing contract, including its policy
        // for an operation in flight. Every other unhandled key stays local.
        if (event.key === Qt.Key_Home) {
            event.accepted = false;
            return;
        }
        if ([Qt.Key_Escape, Qt.Key_Back, Qt.Key_Backspace].includes(event.key)) {
            if (!event.isAutoRepeat && dismissible)
                dismissed();
        } else if ([Qt.Key_Left, Qt.Key_Right, Qt.Key_Tab, Qt.Key_Backtab].includes(event.key)) {
            InputMode.keyPressed();
            const step = event.key === Qt.Key_Left || event.key === Qt.Key_Backtab ? -1 : 1;
            // A button may still process this key. Never reclaim focus if the
            // screen lost ownership, or retain an action that may disappear.
            Qt.callLater(() => {
                if (root.visible && root.enabled && root.activeFocus) {
                    const current = availableActions().find(item => item.activeFocus);
                    Focus.take(adjacentAction(current, step));
                }
            });
        }
        event.accepted = true;
    }

    Rectangle {
        id: scrim
        objectName: "confirmationScrim"
        anchors.fill: parent
        color: Theme.scrim
        MouseArea {
            anchors.fill: parent
            acceptedButtons: Qt.AllButtons
            onClicked: if (root.dismissible) root.dismissed()
            onWheel: wheel => wheel.accepted = true
        }
    }
    Rectangle {
        id: plate
        objectName: "confirmationPlate"
        anchors.centerIn: parent
        width: 900
        height: column.implicitHeight + 112
        radius: Theme.dialogRadius
        color: Theme.dialogSurface
        border.width: 1
        border.color: Theme.dialogLine
        MouseArea {
            anchors.fill: parent
            acceptedButtons: Qt.AllButtons
            onWheel: wheel => wheel.accepted = true
        }
        Column {
            id: column
            x: 56
            y: 56
            width: parent.width - 112
            spacing: 24
            Column {
                id: leadingSlot
                width: parent.width
                spacing: 24
                visible: implicitHeight > 0
            }
            UiText {
                id: titleText
                objectName: "confirmationTitle"
                width: parent.width
                text: root.title
                role: "title"
                font.pixelSize: 48
                wrapMode: Text.WordWrap
            }
            UiText {
                id: bodyText
                objectName: "confirmationBody"
                width: parent.width
                visible: text.length > 0
                text: root.body
                color: Theme.textSecondary
                wrapMode: Text.WordWrap
            }
            Column {
                id: contentSlot
                width: parent.width
                spacing: 24
                visible: implicitHeight > 0
            }
            Flow {
                id: actionRow
                width: parent.width
                spacing: 20
            }
        }
    }
}
