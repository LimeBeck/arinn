// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import Arinn.Shell 1.0

// The on-screen keyboard panel (spec §6.2, ADR-0103, handoff KB-1…8).
//
// KeyboardModel owns the rows and the focus rules; this item draws them and
// applies a pressed key to the target TextInput. Rows from the top: up to
// four suggestions, the field's context row, three letter rows and the
// action row. One focus item takes all keys, so arrows never leave the
// panel by accident: Up from the suggestions asks the field to take focus
// back (leaveUp), Back asks it to hide the panel.
//
// A hardware keyboard keeps typing into the target while the panel is open
// and never closes it. Home and Menu are not handled here and reach the
// screen as on every other control.
FocusScope {
    id: root

    // The TextInput the panel types into.
    property Item target: null
    // Remembers the language and the last key per field.
    property alias memoryKey: keyboard.field
    property alias context: keyboard.context
    property alias submitLabel: keyboard.submitLabel
    // [{icon, title, subtitle, open}]; the field decides what OK on one does.
    property var suggestions: []
    // Shown in the suggestion row when there are none.
    property string emptySuggestions: ""
    // A limit in Unicode characters, as settings count them; 0 for none.
    property int maxCharacters: 0
    readonly property alias model: keyboard
    property string zone: "keys"
    property int suggestionIndex: 0
    readonly property int suggestionCount: Math.min(4, suggestions ? suggestions.length : 0)

    signal submitted()
    signal suggestionChosen(int index)
    signal hideRequested()
    signal leaveUp()

    // Called by the field when the panel is shown; focus goes to the keys.
    function start() {
        keyboard.open();
        zone = "keys";
        Focus.take(keys);
    }
    // Focus returns to the keys after the screen rebuilt its controls.
    function resume() {
        Focus.take(keys);
    }
    // Called by the field when the panel is hidden.
    function finish() {
        keyboard.close();
    }
    // Focus enters the panel from the field above it.
    function enterFromAbove() {
        if (suggestionCount > 0) {
            zone = "suggestions";
            suggestionIndex = 0;
        } else {
            zone = "keys";
            keyboard.enterTop(0);
        }
        Focus.take(keys);
    }

    function insertText(text) {
        if (!target || !text.length)
            return;
        // Input past the field's limit is ignored, not truncated: check the
        // result before touching a selection it would replace.
        const selected = target.selectedText;
        if (target.maximumLength > 0
            && target.length - selected.length + text.length > target.maximumLength)
            return;
        if (maxCharacters > 0
            && [...target.text].length - [...selected].length + [...text].length > maxCharacters)
            return;
        if (selected.length)
            target.remove(target.selectionStart, target.selectionEnd);
        // insert() and remove() report textEdited, as typing does.
        target.insert(target.cursorPosition, text);
    }
    function erase() {
        if (!target)
            return;
        if (target.selectedText.length) {
            target.remove(target.selectionStart, target.selectionEnd);
        } else if (target.cursorPosition > 0) {
            // A whole character: never half of a surrogate pair.
            const at = target.cursorPosition;
            const low = target.text.charCodeAt(at - 1);
            const high = at > 1 ? target.text.charCodeAt(at - 2) : 0;
            const pair = low >= 0xDC00 && low <= 0xDFFF && high >= 0xD800 && high <= 0xDBFF;
            target.remove(at - (pair ? 2 : 1), at);
        }
    }
    // One character left or right, stepping over a whole surrogate pair.
    function moveCursor(step) {
        if (!target)
            return;
        const text = target.text;
        const isHigh = code => code >= 0xD800 && code <= 0xDBFF;
        const isLow = code => code >= 0xDC00 && code <= 0xDFFF;
        let at = target.cursorPosition;
        if (step < 0 && at > 0)
            at -= at > 1 && isLow(text.charCodeAt(at - 1)) && isHigh(text.charCodeAt(at - 2)) ? 2 : 1;
        else if (step > 0 && at < text.length)
            at += isHigh(text.charCodeAt(at)) && isLow(text.charCodeAt(at + 1)) ? 2 : 1;
        target.cursorPosition = at;
    }
    function apply(result) {
        switch (result.action) {
        case "insert": insertText(result.text); break;
        case "backspace": erase(); break;
        case "left": moveCursor(-1); break;
        case "right": moveCursor(1); break;
        case "submit": submitted(); break;
        }
    }
    function navigate(direction) {
        if (zone === "suggestions") {
            if (direction === "left")
                suggestionIndex = Math.max(0, suggestionIndex - 1);
            else if (direction === "right")
                suggestionIndex = Math.min(suggestionCount - 1, suggestionIndex + 1);
            else if (direction === "up")
                leaveUp();
            else {
                keyboard.enterTop((suggestionIndex + 0.5) / Math.max(1, suggestionCount));
                zone = "keys";
            }
            return;
        }
        if (keyboard.move(direction) || direction !== "up")
            return;
        if (suggestionCount > 0) {
            zone = "suggestions";
            suggestionIndex = Math.min(suggestionCount - 1, Math.floor(keyboard.focusX() * suggestionCount));
        } else {
            leaveUp();
        }
    }

    onSuggestionCountChanged: {
        if (zone === "suggestions" && suggestionCount === 0) {
            zone = "keys";
            keyboard.enterTop(0);
        }
        suggestionIndex = Math.max(0, Math.min(suggestionIndex, suggestionCount - 1));
    }

    width: 1920
    implicitHeight: content.height + 22 + 30

    KeyboardModel {
        id: keyboard
    }

    Item {
        id: keys
        objectName: "keyboardKeys"
        focus: true
        Keys.onPressed: event => {
            const directions = {[Qt.Key_Left]: "left", [Qt.Key_Right]: "right",
                                [Qt.Key_Up]: "up", [Qt.Key_Down]: "down"};
            if (directions[event.key] !== undefined) {
                root.navigate(directions[event.key]);
                event.accepted = true;
            } else if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) {
                if (!event.isAutoRepeat) {
                    if (root.zone === "suggestions")
                        root.suggestionChosen(root.suggestionIndex);
                    else
                        root.apply(keyboard.activate());
                }
                event.accepted = true;
            } else if (event.key === Qt.Key_Escape || event.key === Qt.Key_Back) {
                if (!event.isAutoRepeat)
                    root.hideRequested();
                event.accepted = true;
            } else if (event.key === Qt.Key_Backspace) {
                // A hardware keyboard edits the field (§6.2 text exception).
                root.erase();
                event.accepted = true;
            } else if (event.text.length && event.text.charCodeAt(0) >= 32
                       && !(event.modifiers & (Qt.ControlModifier | Qt.AltModifier | Qt.MetaModifier))) {
                root.insertText(event.text);
                event.accepted = true;
            }
        }
    }

    Rectangle {
        id: panel
        objectName: "keyboardPanel"
        width: root.width
        // Only the top corners are rounded: the panel sits on the bottom
        // edge and its lower corners lie past it (Qt 6.6 has no per-corner
        // radius).
        height: content.height + 22 + 30 + radius
        radius: 44
        color: Qt.rgba(0.155, 0.158, 0.170, 0.96)

        Column {
            id: content
            x: 120
            y: 22
            width: parent.width - 240
            spacing: 14

            Item {
                width: parent.width
                height: 76
                Row {
                    spacing: 16
                    visible: root.suggestionCount > 0
                    Repeater {
                        model: root.suggestionCount
                        Rectangle {
                            id: pill
                            required property int index
                            readonly property var entry: root.suggestions[index]
                            readonly property bool focused: root.activeFocus && root.zone === "suggestions"
                                                            && root.suggestionIndex === index
                            objectName: "keyboardSuggestion-" + index
                            // Four pills always fit the row: each gets at most
                            // its share, and its texts elide inside it.
                            readonly property real share: Math.min(560,
                                (content.width - 16 * (root.suggestionCount - 1)) / root.suggestionCount)
                            readonly property real iconPart: pillIcon.visible ? pillIcon.width + 12 : 0
                            readonly property real dotPart: pillDot.visible ? pillDot.width + 12 : 0
                            readonly property real subtitlePart: pillSubtitle.visible ? pillSubtitle.width + 12 : 0
                            width: Math.min(share, pillRow.implicitWidth + 56)
                            height: 76
                            radius: 38
                            color: focused ? Theme.textPrimary : Qt.rgba(1, 1, 1, 0.10)
                            scale: focused ? 1.04 : 1
                            Behavior on scale { NumberAnimation { duration: Theme.pressDuration } }
                            Row {
                                id: pillRow
                                x: 28
                                width: pill.width - 56
                                anchors.verticalCenter: parent.verticalCenter
                                spacing: 12
                                KeyboardIcon {
                                    id: pillIcon
                                    width: 34
                                    height: 34
                                    anchors.verticalCenter: parent.verticalCenter
                                    name: pill.entry.icon || ""
                                    visible: name.length > 0
                                    color: pill.focused ? Theme.ink : Theme.textPrimary
                                }
                                UiText {
                                    anchors.verticalCenter: parent.verticalCenter
                                    width: Math.max(0, Math.min(implicitWidth,
                                        pill.share - 56 - pill.iconPart - pill.subtitlePart - pill.dotPart))
                                    text: pill.entry.title || ""
                                    font.pixelSize: 28
                                    font.weight: 600
                                    wrapMode: Text.NoWrap
                                    elide: Text.ElideRight
                                    color: pill.focused ? Theme.ink : Theme.textPrimary
                                }
                                UiText {
                                    id: pillSubtitle
                                    anchors.verticalCenter: parent.verticalCenter
                                    visible: text.length > 0
                                    width: Math.min(implicitWidth, 200, pill.share * 0.3)
                                    text: pill.entry.subtitle || ""
                                    font.pixelSize: 24
                                    font.weight: 500
                                    wrapMode: Text.NoWrap
                                    elide: Text.ElideRight
                                    color: pill.focused ? Theme.ink : Theme.textHint
                                }
                                Rectangle {
                                    id: pillDot
                                    objectName: "keyboardSuggestionOpen-" + pill.index
                                    anchors.verticalCenter: parent.verticalCenter
                                    visible: pill.entry.open === true
                                    width: 12
                                    height: 12
                                    radius: 6
                                    color: Theme.ember
                                }
                            }
                            TapHandler {
                                onTapped: root.suggestionChosen(pill.index)
                            }
                        }
                    }
                }
                UiText {
                    objectName: "keyboardNoSuggestions"
                    x: 8
                    anchors.verticalCenter: parent.verticalCenter
                    visible: root.suggestionCount === 0
                    text: root.emptySuggestions
                    font.pixelSize: 26
                    font.weight: 500
                    color: Theme.textHint
                }
            }

            Column {
                width: parent.width
                spacing: 10
                Repeater {
                    model: keyboard.rows
                    Row {
                        id: keyRow
                        required property var modelData
                        required property int index
                        readonly property real units: modelData.reduce((sum, key) => sum + key.width, 0)
                        width: parent.width
                        spacing: 10
                        Repeater {
                            model: keyRow.modelData
                            Rectangle {
                                id: keyItem
                                required property var modelData
                                required property int index
                                readonly property bool focused: root.activeFocus && root.zone === "keys"
                                    && keyboard.row === keyRow.index && keyboard.column === index
                                readonly property string kind: modelData.kind
                                objectName: "keyboardKey-" + keyRow.index + "-" + index
                                width: (keyRow.width - keyRow.spacing * (keyRow.modelData.length - 1))
                                       * modelData.width / keyRow.units
                                height: 86
                                radius: 22
                                z: focused ? 2 : 0
                                scale: focused ? 1.08 : 1
                                Behavior on scale { NumberAnimation { duration: Theme.pressDuration } }
                                color: focused ? Theme.textPrimary
                                       : modelData.on || kind === "primary" ? Qt.rgba(1, 0.541, 0.239, 0.28)
                                       : kind === "char" ? Qt.rgba(1, 1, 1, 0.10)
                                       : kind === "context" ? Qt.rgba(1, 1, 1, 0.06)
                                       : Qt.rgba(1, 1, 1, 0.05)
                                Rectangle {
                                    // Lifted focus: a soft shadow under the key.
                                    visible: keyItem.focused
                                    z: -1
                                    x: 6
                                    y: 14
                                    width: parent.width - 12
                                    height: parent.height
                                    radius: parent.radius
                                    color: Qt.rgba(0, 0, 0, 0.45)
                                }
                                Row {
                                    anchors.centerIn: parent
                                    spacing: 10
                                    KeyboardIcon {
                                        width: 38
                                        height: 38
                                        anchors.verticalCenter: parent.verticalCenter
                                        name: keyItem.modelData.icon
                                        visible: name.length > 0
                                        color: keyItem.focused ? Theme.ink
                                               : keyItem.kind === "action" ? Theme.textSecondary : Theme.textPrimary
                                    }
                                    UiText {
                                        anchors.verticalCenter: parent.verticalCenter
                                        visible: text.length > 0
                                        text: keyItem.modelData.label
                                        wrapMode: Text.NoWrap
                                        font.pixelSize: keyItem.kind === "char" ? 40 : 30
                                        font.weight: keyItem.kind === "char" ? 500 : 600
                                        color: keyItem.focused ? Theme.ink
                                               : keyItem.kind === "action" ? Theme.textSecondary : Theme.textPrimary
                                    }
                                }
                                TapHandler {
                                    onTapped: {
                                        keyboard.focusKey(keyRow.index, keyItem.index);
                                        root.zone = "keys";
                                        root.apply(keyboard.activate());
                                    }
                                }
                            }
                        }
                    }
                }
            }

            Row {
                objectName: "keyboardHints"
                height: 36
                spacing: 32
                Repeater {
                    model: [[qsTr("OK"), qsTr("Type")],
                            ["↑", root.suggestionCount > 0 ? qsTr("To suggestions") : qsTr("To the field")],
                            [qsTr("Back"), qsTr("Hide keyboard")]]
                    Row {
                        required property var modelData
                        spacing: 10
                        anchors.verticalCenter: parent.verticalCenter
                        Rectangle {
                            width: chip.implicitWidth + 20
                            height: 36
                            radius: 10
                            color: Qt.rgba(1, 1, 1, 0.12)
                            UiText {
                                id: chip
                                anchors.centerIn: parent
                                text: modelData[0]
                                font.pixelSize: 24
                                font.weight: 600
                                wrapMode: Text.NoWrap
                            }
                        }
                        UiText {
                            anchors.verticalCenter: parent.verticalCenter
                            text: modelData[1]
                            font.pixelSize: 24
                            font.weight: 500
                            wrapMode: Text.NoWrap
                            color: Theme.textSecondary
                        }
                    }
                }
            }
        }
    }
}
