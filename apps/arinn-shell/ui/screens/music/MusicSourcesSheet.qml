// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtQuick.Window
import "../.." as Ui

// The Sources sheet (brief mh-01, MH-5): every music source with its mark,
// its state and its housekeeping as pills, by capability. It never filters
// the room. Disconnect asks first, with Cancel as the initial focus.
FocusScope {
    id: root
    property var room: null
    readonly property var sources: room ? room.sources : []
    // Rows are keyed by source id and read their source by id, so a state
    // change (a command in flight, a new state) never recreates the focused
    // pill (review of #812).
    ListModel { id: rowModel }
    function syncRows() {
        const ids = sources.map(source => source.id);
        for (let i = rowModel.count - 1; i >= 0; --i)
            if (!ids.includes(rowModel.get(i).sourceKey))
                rowModel.remove(i);
        for (let i = 0; i < ids.length; ++i) {
            let at = -1;
            for (let j = i; j < rowModel.count && at < 0; ++j)
                if (rowModel.get(j).sourceKey === ids[i])
                    at = j;
            if (at < 0)
                rowModel.insert(i, {sourceKey: ids[i]});
            else if (at !== i)
                rowModel.move(at, i, 1);
        }
        // A source that left with focus on its pill hands focus to the
        // first row that has an action.
        Qt.callLater(() => {
            const window = Window.window;
            if (visible && window && window.activeFocusItem === root)
                focusFirst();
        });
    }
    onSourcesChanged: syncRows()
    Component.onCompleted: syncRows()
    // The source whose Disconnect waits for confirmation.
    property string disconnecting: ""
    signal closed()
    anchors.fill: parent
    visible: false
    function open() {
        disconnecting = "";
        visible = true;
        Qt.callLater(focusFirst);
    }
    function close() {
        disconnecting = "";
        visible = false;
        closed();
    }
    function focusFirst() {
        for (let i = 0; i < rows.count; ++i) {
            const row = rows.itemAt(i);
            if (row && row.firstAction()) {
                Ui.Focus.take(row.firstAction());
                return;
            }
        }
        Ui.Focus.take(root);
    }
    // Keep the focused row on screen, with the next one partly visible.
    function reveal(row) {
        const pad = rowArea.pad, view = rowArea.height - 2 * pad;
        const top = row.y, bottom = row.y + row.height;
        if (top < rowArea.contentY + pad)
            rowArea.contentY = Math.max(-pad, top - 40 - pad);
        else if (bottom > rowArea.contentY + pad + view)
            rowArea.contentY = Math.min(Math.max(-pad, rowArea.contentHeight - view - pad),
                                        bottom - view + 80 - pad);
    }
    function label(action) {
        return action === "connect" ? qsTr("Connect")
            : action === "authorize" ? qsTr("Sign in")
            : action === "cancel" ? qsTr("Cancel sign-in")
            : action === "scan" ? qsTr("Refresh")
            : qsTr("Disconnect");
    }
    function statusText(source) {
        const status = source.status;
        return status === "connected" ? qsTr("Connected")
            : status === "scanning" ? qsTr("Scanning the library…")
            : status === "signin" ? qsTr("Needs sign-in")
            : status === "connecting" ? qsTr("Connecting…")
            : source.reason ? qsTr("Unavailable · %1").arg(source.reason)
            : qsTr("Unavailable");
    }
    function act(source, action) {
        if (action === "disconnect") {
            disconnecting = source.id;
            Qt.callLater(() => Ui.Focus.take(keepSource));
        } else if (room) {
            room.sourceCommand(source.id, action);
        }
    }
    Keys.onPressed: event => {
        if ([Qt.Key_Escape, Qt.Key_Back, Qt.Key_Backspace].includes(event.key)) {
            event.accepted = true;
            if (disconnecting.length) {
                disconnecting = "";
                focusFirst();
            } else {
                close();
            }
        }
    }
    Rectangle {
        anchors.fill: parent
        color: Ui.Theme.scrim
        MouseArea {
            anchors.fill: parent
            onClicked: root.close()
        }
    }
    Rectangle {
        objectName: "musicSourcesSheet"
        x: parent.width - width
        width: 720
        height: parent.height
        color: Ui.Theme.dialogSurface
        border.color: Ui.Theme.dialogLine
        MouseArea { anchors.fill: parent }
        Ui.UiText {
            x: 64
            y: 72
            role: "title"
            text: qsTr("Sources")
        }
        // The rows scroll with focus: many sources, or a sign-in code, make
        // them taller than the sheet (Codex review of #812).
        Flickable {
            id: rowArea
            objectName: "musicSourceRows"
            // A focused pill scales up: the margins keep it unclipped.
            readonly property int pad: 16
            x: 64 - pad
            y: 180 - pad
            width: parent.width - 128 + 2 * pad
            height: parent.height - y - 64
            leftMargin: pad
            topMargin: pad
            bottomMargin: pad
            contentHeight: rowColumn.height
            interactive: false
            clip: true
            Behavior on contentY { NumberAnimation { duration: Ui.Theme.focusDuration; easing.type: Easing.OutCubic } }
        Column {
            id: rowColumn
            width: parent.width
            spacing: 40
            Repeater {
                id: rows
                model: rowModel
                Column {
                    id: sourceRow
                    required property string sourceKey
                    required property int index
                    readonly property var modelData: root.sources.find(source => source.id === sourceKey) || ({})
                    // The pills change only when the actions do; focus then
                    // stays on the same action, or the row's first one.
                    property var actionList: []
                    onModelDataChanged: {
                        const next = modelData.actions || [];
                        if (JSON.stringify(next) === JSON.stringify(actionList))
                            return;
                        let focused = "";
                        for (let i = 0; i < pills.count; ++i)
                            if (pills.itemAt(i) && pills.itemAt(i).activeFocus)
                                focused = pills.itemAt(i).pillAction;
                        actionList = next;
                        if (focused.length)
                            Qt.callLater(() => {
                                for (let i = 0; i < pills.count; ++i)
                                    if (pills.itemAt(i).pillAction === focused) {
                                        Ui.Focus.take(pills.itemAt(i));
                                        return;
                                    }
                                root.focusFirst();
                            });
                    }
                    Component.onCompleted: actionList = modelData.actions || []
                    objectName: "musicSourceRow"
                    width: parent.width
                    spacing: 16
                    function firstAction() {
                        return pills.count ? pills.itemAt(0) : null;
                    }
                    Row {
                        spacing: 20
                        MusicSourceMark {
                            anchors.verticalCenter: parent.verticalCenter
                            size: 56
                            local: sourceRow.modelData.local === true
                            name: sourceRow.modelData.name || ""
                        }
                        Column {
                            anchors.verticalCenter: parent.verticalCenter
                            Ui.UiText {
                                objectName: "musicSourceName"
                                role: "control"
                                text: sourceRow.modelData.local ? qsTr("This computer") : (sourceRow.modelData.name || "")
                                textFormat: Text.PlainText
                            }
                            Ui.UiText {
                                objectName: "musicSourceStatus"
                                width: 520
                                role: "meta"
                                color: Ui.Theme.textSecondary
                                text: root.statusText(sourceRow.modelData)
                                textFormat: Text.PlainText
                                wrapMode: Text.WordWrap
                            }
                        }
                    }
                    // A pending sign-in shows where to confirm and its code.
                    Ui.UiText {
                        objectName: "musicSourcePrompt"
                        width: parent.width
                        visible: !!sourceRow.modelData.prompt && !!sourceRow.modelData.prompt.verification_url
                        role: "meta"
                        color: Ui.Theme.textSecondary
                        wrapMode: Text.WordWrap
                        textFormat: Text.PlainText
                        text: visible ? qsTr("Open %1 on your phone and enter %2")
                            .arg(sourceRow.modelData.prompt.verification_url)
                            .arg(sourceRow.modelData.prompt.user_code || "") : ""
                    }
                    Row {
                        spacing: 16
                        Repeater {
                            id: pills
                            model: sourceRow.actionList
                            Ui.ActionButton {
                                required property var modelData
                                required property int index
                                readonly property string pillAction: modelData
                                objectName: "musicSourceAction-" + modelData
                                text: root.label(modelData)
                                busy: sourceRow.modelData.busy === true
                                KeyNavigation.left: index > 0 ? pills.itemAt(index - 1) : null
                                KeyNavigation.right: index + 1 < pills.count ? pills.itemAt(index + 1) : null
                                // Rows without an action are skipped both ways
                                // (Codex review of #812).
                                Keys.onUpPressed: event => {
                                    for (let i = sourceRow.index - 1; i >= 0; --i)
                                        if (rows.itemAt(i).firstAction()) {
                                            Ui.Focus.take(rows.itemAt(i).firstAction());
                                            return;
                                        }
                                }
                                Keys.onDownPressed: event => {
                                    for (let i = sourceRow.index + 1; i < rows.count; ++i)
                                        if (rows.itemAt(i).firstAction()) {
                                            Ui.Focus.take(rows.itemAt(i).firstAction());
                                            return;
                                        }
                                }
                                onActivated: root.act(sourceRow.modelData, modelData)
                                onActiveFocusChanged: if (activeFocus)
                                    root.reveal(sourceRow)
                            }
                        }
                    }
                }
            }
        }
        }
    }
    MusicDialog {
        objectName: "musicDisconnectDialog"
        visible: root.disconnecting.length > 0
        z: 2
        title: qsTr("Disconnect this source?")
        body: qsTr("Its music leaves the shelves until you connect it again.")
        onDismissed: { root.disconnecting = ""; root.focusFirst(); }
        Ui.ActionButton {
            id: keepSource
            objectName: "musicKeepSource"
            text: qsTr("Cancel")
            KeyNavigation.right: dropSource
            onActivated: { root.disconnecting = ""; root.focusFirst(); }
        }
        Ui.ActionButton {
            id: dropSource
            objectName: "musicDisconnectSource"
            text: qsTr("Disconnect")
            destructive: true
            KeyNavigation.left: keepSource
            onActivated: {
                const id = root.disconnecting;
                root.disconnecting = "";
                if (root.room)
                    root.room.sourceCommand(id, "disconnect");
                root.focusFirst();
            }
        }
    }
}
