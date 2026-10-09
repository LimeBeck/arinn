import "../../" as Ui
// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick

FocusScope {
    id: root

    property string text: ""
    property string kind: ""
    property string resolvedUrl: ""
    property string engine: ""
    property string openTitle: ""
    property string operationState: ""
    property bool keyboardAvailable: true
    property bool browserUnavailable: false
    property bool keyboardSeen: false
    readonly property bool canType: keyboardAvailable || keyboardSeen
    // The on-screen keyboard panel (spec §6.2, ADR-0103).
    property bool keyboardOpen: false
    // Recent entries and open services that match the text, up to four:
    // [{icon, title, subtitle, value, open}] (spec §7.1).
    property var suggestions: []
    property var recent: []
    property var ordinaryApplications: []
    readonly property bool busy: operationState === "running" || operationState === "queued"
    readonly property bool valid: kind === "address" || kind === "search"
    // While the panel is hidden, the list under the field: recent entries
    // with Clear for an empty field (V-2b), suggestions while typing.
    readonly property var listedSuggestions: keyboardOpen || !text.length ? [] : suggestions

    signal textEdited(string value)
    signal openRequested()
    signal applicationRequested(string entryId)
    signal clearRequested()
    signal retryRequested()
    signal backRequested()
    // Home, also from the address field (§6.2: only Backspace and Esc keep
    // a text meaning there).
    signal homeRequested()

    function focusInitial() {
        if (browserUnavailable)
            Ui.Focus.take(retry);
        else if (keyboardOpen)
            keyboard.start();
        else if (canType && !busy)
            Ui.Focus.take(field);
        else
            // No hardware keyboard: the remote types through the panel.
            Ui.Focus.take(keyboardButton);
    }

    function showKeyboard() {
        if (keyboardOpen || browserUnavailable)
            return;
        keyboardOpen = true;
        keyboard.start();
    }

    function hideKeyboard() {
        if (!keyboardOpen)
            return;
        keyboard.finish();
        keyboardOpen = false;
        Ui.Focus.take(keyboardButton);
    }

    function chooseSuggestion(entry) {
        if (!entry || busy)
            return;
        root.textEdited(entry.value);
        if (!keyboardOpen)
            Ui.Focus.take(canType ? field : keyboardButton);
    }

    function controls() {
        let result = [];
        if (!browserUnavailable && canType)
            result.push(field);

        if (browserUnavailable) {
            result.push(retry);
        } else {
            result.push(keyboardButton);
            if (keyboardOpen)
                return result;
            for (let i = 0; i < suggestionRows.count; ++i) result.push(suggestionRows.itemAt(i))
            for (let i = 0; i < recentRows.count; ++i) result.push(recentRows.itemAt(i))
            if (clear.visible)
                result.push(clear);

            for (let i = 0; i < ordinaryRows.count; ++i) result.push(ordinaryRows.itemAt(i))
        }
        result.push(backButton);
        return result;
    }

    function move(delta) {
        // With the panel open, Down from the field or its button enters it.
        if (keyboardOpen && delta > 0) {
            keyboard.enterFromAbove();
            return;
        }
        const list = controls();
        let index = list.findIndex((item) => {
            return item.activeFocus;
        });
        index = Math.max(0, Math.min(list.length - 1, index + delta));
        Ui.Focus.take(list[index]);
        // Columns position new rows on the next polish pass. A move in the
        // same turn as a model change would read stale positions and leave
        // the focused row outside the viewport (#275).
        actionColumn.forceLayout();
        choiceColumn.forceLayout();
        const pos = list[index].mapToItem(choiceColumn, 0, 0);
        if (list[index] !== field && list[index] !== retry && list[index] !== backButton && list[index] !== keyboardButton)
            choices.contentY = Math.max(0, Math.min(choices.contentHeight - choices.height, pos.y - 180));

    }

    width: 1920
    height: 1080
    onVisibleChanged: {
        if (!visible) {
            keyboardSeen = false;
            if (keyboardOpen) {
                keyboard.finish();
                keyboardOpen = false;
            }
        }
    }
    onBrowserUnavailableChanged: if (browserUnavailable && keyboardOpen) {
        keyboard.finish();
        keyboardOpen = false;
        Qt.callLater(() => Ui.Focus.take(retry));
    }
    Keys.onPressed: (event) => {
        if (event.key === Qt.Key_Escape || event.key === Qt.Key_Back || (event.key === Qt.Key_Backspace && !field.activeFocus)) {
            if (!event.isAutoRepeat) {
                // The open panel is one more level for Back (DR-46).
                if (keyboardOpen)
                    root.hideKeyboard();
                else
                    root.backRequested();
            }
            event.accepted = true;
        } else if ([Qt.Key_Up, Qt.Key_Down, Qt.Key_Tab, Qt.Key_Backtab].includes(event.key)) {
            move(event.key === Qt.Key_Up || event.key === Qt.Key_Backtab ? -1 : 1);
            event.accepted = true;
        } else if (event.key === Qt.Key_Right && field.activeFocus) {
            // The field gives Right back at the end of the text.
            Ui.Focus.take(keyboardButton);
            event.accepted = true;
        } else if (event.key === Qt.Key_Left && keyboardButton.activeFocus && canType) {
            Ui.Focus.take(field);
            event.accepted = true;
        } else if (!field.activeFocus && event.text.length && !(event.modifiers & (Qt.ControlModifier | Qt.AltModifier | Qt.MetaModifier)) && event.key !== Qt.Key_Return && event.key !== Qt.Key_Enter && !browserUnavailable && !busy) {
            root.keyboardSeen = true;
            Ui.Focus.take(field);
            field.insert(field.cursorPosition, event.text);
            event.accepted = true;
        }
    }

    Rectangle {
        anchors.fill: parent
        color: Ui.Theme.background
    }

    Ui.UiText {
        x: 120
        // The open panel moves the title and the field up (KB-1).
        y: root.keyboardOpen ? 64 : 96
        text: qsTr("Site or search")
        font.pixelSize: 56
        font.weight: 700
    }

    Rectangle {
        id: fieldPlate

        x: 120
        y: root.keyboardOpen ? 160 : 210
        width: 1480
        height: 132
        radius: 34
        color: Qt.rgba(1, 1, 1, 0.12)
        border.width: field.activeFocus || root.keyboardOpen ? 3 : 0
        border.color: Qt.rgba(1, 1, 1, 0.45)

        TextInput {
            id: field

            objectName: "browseInput"
            x: 32
            // Dimmed without a way to type; the keyboard button stays bright.
            opacity: root.browserUnavailable || !(root.canType || root.keyboardOpen) ? 0.6 : 1
            width: keyboardButton.x - x - 24
            height: 80
            anchors.verticalCenter: parent.verticalCenter
            text: root.text
            color: Ui.Theme.textPrimary
            font.family: Ui.Theme.fontFamily
            font.pixelSize: 48
            font.weight: 500
            verticalAlignment: TextInput.AlignVCenter
            clip: true
            selectByMouse: true
            // The panel types here without focus; the caret stays visible.
            cursorVisible: activeFocus || root.keyboardOpen
            cursorDelegate: Rectangle {
                width: 4
                color: Ui.Theme.ember
                visible: field.cursorVisible
            }
            enabled: !root.browserUnavailable && !root.busy && (root.canType || root.keyboardOpen)
            onTextEdited: root.textEdited(text)
            Keys.onPressed: (event) => {
                if (event.key === Qt.Key_Home && event.modifiers === Qt.NoModifier) {
                    // TextInput would take Home as cursor movement (#329).
                    if (!event.isAutoRepeat) root.homeRequested();
                    event.accepted = true;
                } else if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) {
                    if (!event.isAutoRepeat && root.valid && !root.busy)
                        root.openRequested();

                    event.accepted = true;
                }
            }

            Ui.UiText {
                anchors.verticalCenter: parent.verticalCenter
                text: qsTr("Website address or what to find")
                font.pixelSize: 48
                color: Ui.Theme.textHint
                visible: !field.text.length
            }

        }

        Ui.KeyboardButton {
            id: keyboardButton

            objectName: "browseKeyboard"
            x: parent.width - width - 24
            anchors.verticalCenter: parent.verticalCenter
            panelOpen: root.keyboardOpen
            enabled: !root.browserUnavailable
            onActivated: root.keyboardOpen ? root.hideKeyboard() : root.showKeyboard()
        }

    }

    Ui.UiText {
        objectName: "browseIntent"
        x: 152
        y: fieldPlate.y + 160
        width: 1448
        font.pixelSize: 30
        elide: Text.ElideRight
        color: root.kind === "invalid" ? Ui.Theme.error : Ui.Theme.textSecondary
        text: root.openTitle.length ? qsTr("Enter · Go to open %1").arg(root.openTitle) : root.kind === "address" ? qsTr("Enter · Open website %1").arg(root.resolvedUrl) : root.kind === "search" ? qsTr("Enter · Search “%1” in %2").arg(root.text).arg(root.engine) : root.kind === "invalid" ? qsTr("Arinn opens only http and https addresses") : ""
    }

    Flickable {
        id: choices

        x: 120
        y: 438
        width: 1480
        height: 450
        clip: true
        contentHeight: choiceColumn.height
        visible: !root.keyboardOpen

        Column {
            id: choiceColumn

            width: 1480
            spacing: 14
            visible: !root.browserUnavailable

            Ui.UiText {
                visible: suggestionRows.count > 0
                text: qsTr("Suggestions")
                role: "meta"
            }

            Repeater {
                id: suggestionRows

                model: root.listedSuggestions

                Ui.ActionButton {
                    required property var modelData
                    required property int index

                    objectName: "browseSuggestion-" + index
                    width: 1040
                    height: 88
                    text: modelData.title + (modelData.subtitle ? " · " + modelData.subtitle : "") + (modelData.open ? " · " + qsTr("Open") : "")
                    onActivated: root.chooseSuggestion(modelData)
                }

            }

            Ui.UiText {
                visible: recentRows.count > 0
                text: qsTr("Recent")
                role: "meta"
            }

            Repeater {
                id: recentRows

                model: root.text.length ? [] : root.recent

                Ui.ActionButton {
                    required property string modelData
                    required property int index

                    objectName: "browseRecent-" + index
                    width: 1040
                    height: 64
                    text: modelData
                    onActivated: root.chooseSuggestion({value: modelData})
                }

            }

            Column {
                id: actionColumn

                spacing: 14

                Ui.ActionButton {
                    id: clear

                    objectName: "browseClear"
                    visible: recentRows.count > 0
                    height: 72
                    text: qsTr("Clear")
                    onActivated: root.clearRequested()
                }

                Repeater {
                    id: ordinaryRows

                    model: root.ordinaryApplications

                    Ui.ActionButton {
                        required property var modelData
                        required property int index

                        objectName: "browseOrdinary-" + index
                        width: 1040
                        height: 72
                        text: qsTr("Open %1 with tabs").arg(modelData.title)
                        onActivated: root.applicationRequested(modelData.id)
                    }

                }

            }

        }

    }

    Column {
        x: 120
        y: 440
        spacing: 24
        visible: root.browserUnavailable

        Ui.UiText {
            text: qsTr("Browser unavailable")
            font.pixelSize: 44
            font.weight: 700
        }

        Ui.UiText {
            text: qsTr("Online services cannot be opened right now.")
            role: "body"
        }

        Ui.ActionButton {
            id: retry

            objectName: "browseRetry"
            text: qsTr("Retry")
            onActivated: root.retryRequested()
        }

    }

    Ui.UiText {
        objectName: "browseFeedback"
        x: 120
        // Above the open panel, under the intent line.
        y: root.keyboardOpen ? fieldPlate.y + 204 : 918
        width: 1400
        role: "body"
        color: root.busy || root.operationState === "succeeded" ? Ui.Theme.textSecondary : Ui.Theme.error
        text: root.busy ? qsTr("Opening…") : root.operationState === "succeeded" ? qsTr("Opened") : root.operationState === "Timeout" ? qsTr("Opening timed out. Try again.") : root.operationState.length ? qsTr("Could not open. Try again.") : ""
    }

    Ui.ActionButton {
        id: backButton

        objectName: "browseBack"
        x: 1580
        y: 922
        visible: !root.keyboardOpen
        text: qsTr("Back")
        onActivated: root.backRequested()
    }

    Ui.OnScreenKeyboard {
        id: keyboard

        objectName: "browseOnScreenKeyboard"
        y: root.height - implicitHeight
        z: 3
        visible: root.keyboardOpen
        target: root.busy ? null : field
        memoryKey: "browse"
        context: "address"
        submitLabel: qsTr("Open")
        suggestions: root.suggestions
        emptySuggestions: qsTr("No suggestions")
        onSubmitted: if (root.valid && !root.busy) root.openRequested()
        onSuggestionChosen: index => root.chooseSuggestion(root.suggestions[index])
        onHideRequested: root.hideKeyboard()
        onLeaveUp: Ui.Focus.take(keyboardButton)
    }

}
