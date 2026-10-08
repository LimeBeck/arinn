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
    property var suggestions: []
    property var recent: []
    property var ordinaryApplications: []
    readonly property bool busy: operationState === "running" || operationState === "queued"
    readonly property bool valid: kind === "address" || kind === "search"

    signal textEdited(string value)
    signal openRequested()
    signal shortcutRequested(string entryId)
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
        else if (canType && !busy)
            Ui.Focus.take(field);
        else if (suggestionRows.count)
            Ui.Focus.take(suggestionRows.itemAt(0));
        else
            Ui.Focus.take(backButton);
    }

    function controls() {
        let result = [];
        if (!browserUnavailable && canType)
            result.push(field);

        if (browserUnavailable) {
            result.push(retry);
        } else {
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
        if (list[index] !== field && list[index] !== retry && list[index] !== backButton)
            choices.contentY = Math.max(0, Math.min(choices.contentHeight - choices.height, pos.y - 180));

    }

    width: 1920
    height: 1080
    onVisibleChanged: {
        if (!visible) {
            keyboardSeen = false;
        }
    }
    Keys.onPressed: (event) => {
        if (event.key === Qt.Key_Escape || event.key === Qt.Key_Back || (event.key === Qt.Key_Backspace && !field.activeFocus)) {
            if (!event.isAutoRepeat)
                root.backRequested();

            event.accepted = true;
        } else if ([Qt.Key_Up, Qt.Key_Down, Qt.Key_Tab, Qt.Key_Backtab].includes(event.key)) {
            move(event.key === Qt.Key_Up || event.key === Qt.Key_Backtab ? -1 : 1);
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
        y: 96
        text: qsTr("Site or search")
        font.pixelSize: 56
        font.weight: 700
    }

    Rectangle {
        x: 120
        y: 210
        width: 1480
        height: 132
        radius: 34
        color: Qt.rgba(1, 1, 1, 0.12)
        opacity: root.browserUnavailable || !root.canType ? 0.6 : 1
        border.width: field.activeFocus ? 3 : 0
        border.color: Qt.rgba(1, 1, 1, 0.45)

        TextInput {
            id: field

            objectName: "browseInput"
            x: 32
            width: parent.width - 64
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
            enabled: !root.browserUnavailable && !root.busy && root.canType
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

    }

    Ui.UiText {
        objectName: "browseIntent"
        x: 152
        y: 370
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

        Column {
            id: choiceColumn

            width: 1480
            spacing: 14
            visible: !root.browserUnavailable

            Ui.UiText {
                visible: !root.canType
                text: qsTr("A keyboard is needed to type")
                font.pixelSize: 44
                font.weight: 700
            }

            Ui.UiText {
                visible: !root.canType
                text: qsTr("You can still open shortcuts with the remote.")
                role: "body"
            }

            Ui.UiText {
                visible: suggestionRows.count > 0
                text: qsTr("Shortcuts")
                role: "meta"
            }

            Repeater {
                id: suggestionRows

                model: root.suggestions

                Ui.ActionButton {
                    required property var modelData

                    objectName: "browseShortcut-" + modelData.id
                    width: 1040
                    height: 88
                    text: modelData.title + " · " + modelData.subtitle + (modelData.open ? " · " + qsTr("Open") : "")
                    onActivated: root.shortcutRequested(modelData.id)
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
                    onActivated: {
                        root.textEdited(modelData);
                        Ui.Focus.take(field);
                    }
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
        y: 918
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
        text: qsTr("Back")
        onActivated: root.backRequested()
    }

}
