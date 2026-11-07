// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import "../.." as Ui

// Library folders (handoff *Settings editors* LF-1…8): the folders of one
// folder-list setting, in the order added, with Add (through FolderPicker)
// and Remove. It edits the setting through `valuesRequested`; the daemon
// decides (ADR-0065, ADR-0071). Removing a folder never deletes anything:
// the verb is always Remove.
FocusScope {
    id: root
    width: 1920
    height: 1080

    // The settings row: {providerId, id, currentValue: [paths], pending,
    // error, unavailablePaths, constraints: {maxItems}}.
    property var row: ({})
    property var browser: null
    property bool pointerMode: Ui.InputMode.pointerActive
    property int selectedIndex: 0
    property string side: "row" // "row", "remove" or "add"
    property string toastMessage: ""
    // The folder added last, shown as "Adding to the library…" while the
    // setting is pending (LF-8).
    property string addedPath: ""
    property string removing: ""
    // The folder whose removal is being saved: "Folder removed" waits until
    // the row no longer lists it (review of #780).
    property string removedPath: ""
    // The added folder takes focus once the setting lists it (LF-8).
    property bool focusAdded: false
    readonly property bool picking: picker.visible
    // The setting's folders as saved.
    readonly property var savedFolders: pathsOf(row)
    // The folders shown: the saved ones, and the folder being added until
    // the row lists it or reports an error, so it shows "Adding…" at once
    // (LF-8). The catalog keeps the old value until the save settles and
    // its refresh arrives (review of #780).
    readonly property bool addOutstanding: addedPath.length > 0 && !row.error && indexOfPath(savedFolders, addedPath) < 0
    // The setting an add or a removal was made on: a result read from
    // another setting's row is not this operation's (review of #780).
    property string operationKey: ""
    function keyOf(value) { return value && value.id ? (value.providerId || "") + "/" + value.id : ""; }
    readonly property var folders: addOutstanding
                                   ? savedFolders.concat([addedPath]) : savedFolders
    // The library's own folder lists use the handoff's library wording; any
    // other provider's folder list gets plain folder wording (review of
    // #780). The catalog marks the lists of providers with the library
    // capability.
    readonly property bool library: row.libraryFolders === true
    // "/srv/films/" and "/srv/films" are one folder (review of #780).
    function normalPath(path) {
        const text = String(path);
        if (text.length <= 1) return text;
        return text.replace(/\/+$/, "") || "/";
    }
    function indexOfPath(list, path) {
        const wanted = normalPath(path);
        for (let i = 0; i < list.length; ++i)
            if (normalPath(list[i]) === wanted) return i;
        return -1;
    }
    // A list from C++ is not a JS Array; read it by length.
    function pathsOf(value) {
        const list = value ? value.currentValue : null;
        if (!list || list.length === undefined || typeof list === "string") return [];
        const out = [];
        for (let i = 0; i < list.length; ++i) out.push(String(list[i]));
        return out;
    }
    readonly property var unavailable: {
        const value = row ? row.unavailablePaths : null;
        const out = [];
        if (value && value.length !== undefined)
            for (let i = 0; i < value.length; ++i) out.push(String(value[i]));
        return out;
    }
    readonly property int maxItems: row.constraints && row.constraints.maxItems !== undefined
                                    ? Number(row.constraints.maxItems) : 64
    readonly property bool full: savedFolders.length >= maxItems
    readonly property int toastDurationMs: 2600
    // One change at a time: the catalog drops a second save while the first
    // is pending, so Add and Remove explain instead (review of #780).
    // An add or a removal still waiting for the saved value counts too: a
    // second change would be computed from the old value (review of #780).
    readonly property bool busy: !!row.pending || row.enabled === false || addOutstanding
                                 || removedPath.length > 0
    readonly property string busyText: row.enabled === false ? qsTr("These folders cannot be changed now")
                                                              : qsTr("Wait until the last change is saved")

    signal valuesRequested(var paths)
    signal closeRequested()
    signal homeRequested()

    // The daemon's error codes in words, as in SettingsView.daemonErrorText.
    function errorText(code) {
        if (!code) return "";
        if (code === "Timeout") return qsTr("Arinn did not reply. Try again.");
        if (code === "InvalidArgument") return qsTr("This value is not accepted.");
        if (code === "Unavailable") return qsTr("The service is not available. Try again later.");
        if (code === "Conflict") return qsTr("The settings changed. Try again.");
        if (code === "NotFound") return qsTr("This setting is no longer available.");
        if (code === "PermissionDenied") return qsTr("This setting cannot be changed.");
        if (code === "Cancelled" || code === "cancelled") return qsTr("Saving was cancelled.");
        return qsTr("Something went wrong. Try again.");
    }
    function nameOf(path) {
        if (path === "/") return "/";
        const trimmed = String(path).replace(/\/+$/, "");
        return trimmed.slice(trimmed.lastIndexOf("/") + 1) || trimmed;
    }
    function showToast(message) {
        toastMessage = message;
        toastTimer.restart();
    }
    function focusRow(index, part) {
        if (!folders.length) { side = "add"; Ui.Focus.take(emptyAdd); return; }
        const clamped = Math.max(0, Math.min(index, folders.length - 1));
        const item = rows.itemAt(clamped);
        if (!item) return;
        selectedIndex = clamped;
        side = part || "row";
        Ui.Focus.take(side === "remove" ? item.removeButton : item.rowButton);
        const top = item.y, bottom = item.y + item.height;
        if (top < list.contentY) list.contentY = top;
        else if (bottom > list.contentY + list.height) list.contentY = bottom - list.height;
    }
    function restore() {
        if (picking) { picker.settle(); return; }
        if (confirm.visible) { Ui.Focus.take(cancelButton); return; }
        if (focusAdded && addedPath.length && indexOfPath(folders, addedPath) >= 0) {
            focusAdded = false;
            focusRow(indexOfPath(folders, addedPath), "row");
            return;
        }
        if (side === "add" || !folders.length) { side = "add"; Ui.Focus.take(folders.length ? addButton : emptyAdd); return; }
        focusRow(selectedIndex, side);
    }
    function open() {
        // A fresh start for this setting: nothing from an earlier visit (a
        // picker, a confirmation, a pending add or removal) carries over
        // (review of #780).
        picker.visible = false;
        confirm.visible = false;
        removing = "";
        removedPath = "";
        addedPath = "";
        focusAdded = false;
        operationKey = "";
        clearToast();
        side = folders.length ? "row" : "add";
        selectedIndex = 0;
        Qt.callLater(restore);
    }
    function clearToast() {
        toastTimer.stop();
        toastMessage = "";
    }
    function add() {
        if (busy) { showToast(busyText); return; }
        if (full) {
            showToast(qsTr("This is the limit: %n folders. Remove one to add another.", "", maxItems));
            return;
        }
        picker.errorText = "";
        picker.visible = true;
        picker.start();
    }
    function chosen(path) {
        if (indexOfPath(folders, path) >= 0) {
            picker.errorText = Qt.binding(() => library ? qsTr("This folder is already in the library")
                                                       : qsTr("This folder is already in the list"));
            return;
        }
        if (busy) {
            picker.errorText = Qt.binding(() => root.busyText);
            return;
        }
        picker.visible = false;
        operationKey = keyOf(row);
        addedPath = path;
        focusAdded = true;
        valuesRequested(savedFolders.concat([path]));
        Qt.callLater(restore);
    }
    function askRemove(path) {
        if (busy) { showToast(busyText); return; }
        removing = path;
        confirm.visible = true;
        Qt.callLater(() => Ui.Focus.take(cancelButton));
    }
    function confirmRemove() {
        const index = folders.indexOf(removing);
        confirm.visible = false;
        if (index >= 0) {
            operationKey = keyOf(row);
            removedPath = removing;
            valuesRequested(savedFolders.filter(path => path !== removing));
        }
        removing = "";
        Qt.callLater(() => focusRow(Math.max(0, index - 1), "row"));
    }
    function cancelRemove() {
        confirm.visible = false;
        removing = "";
        Qt.callLater(restore);
    }
    onFoldersChanged: if (visible && !picking && !confirm.visible) Qt.callLater(restore)
    // The removal settles: the folder left the row, or the save failed and
    // the row shows the error.
    onRowChanged: {
        // The setting went away (a reload, a disconnect, a schema change):
        // the screen closes instead of editing an empty list (review of
        // #780).
        if (visible && !(row && row.id)) {
            closeRequested();
            return;
        }
        if (row.pending || keyOf(row) !== operationKey) return;
        // The add is saved: the tracker ends, so a later save does not read
        // "Adding…" on this row (review of #780).
        if (addedPath.length && !row.error && indexOfPath(pathsOf(row), addedPath) >= 0) {
            const index = indexOfPath(pathsOf(row), addedPath);
            addedPath = "";
            if (focusAdded) {
                focusAdded = false;
                Qt.callLater(() => focusRow(index, "row"));
            }
        }
        // An added folder the daemon refused leaves the list with the error.
        if (addedPath.length && row.error && indexOfPath(pathsOf(row), addedPath) < 0) {
            addedPath = "";
            focusAdded = false;
            showToast(errorText(row.error));
        }
        if (!removedPath.length) return;
        // Read the row itself: the folders binding may not have caught up.
        if (pathsOf(row).indexOf(removedPath) < 0) {
            removedPath = "";
            showToast(qsTr("Folder removed"));
        } else if (row.error) {
            removedPath = "";
            showToast(errorText(row.error));
        }
    }
    Keys.onPressed: event => {
        if ([Qt.Key_Up, Qt.Key_Down, Qt.Key_Left, Qt.Key_Right].includes(event.key)) Ui.InputMode.keyPressed();
        if (event.key === Qt.Key_Escape || event.key === Qt.Key_Backspace || event.key === Qt.Key_Back) {
            if (!event.isAutoRepeat) {
                if (confirm.visible) cancelRemove();
                else closeRequested();
            }
        } else if (event.key === Qt.Key_Home) {
            if (!event.isAutoRepeat) homeRequested();
        } else if (confirm.visible) {
            if (event.key === Qt.Key_Left || event.key === Qt.Key_Right)
                Ui.Focus.take(cancelButton.activeFocus ? confirmButton : cancelButton);
        } else if (event.key === Qt.Key_Up) {
            if (side === "add") {}
            else if (selectedIndex > 0) focusRow(selectedIndex - 1, side);
            else { side = "add"; Ui.Focus.take(addButton); }
        } else if (event.key === Qt.Key_Down) {
            if (side === "add") focusRow(0, "row");
            else focusRow(selectedIndex + 1, side);
        } else if (event.key === Qt.Key_Right) {
            if (side === "row") focusRow(selectedIndex, "remove");
        } else if (event.key === Qt.Key_Left) {
            if (side === "remove") focusRow(selectedIndex, "row");
        } else if (event.key === Qt.Key_Tab || event.key === Qt.Key_Backtab) {
        } else return;
        event.accepted = true;
    }
    Timer { id: toastTimer; interval: root.toastDurationMs; onTriggered: root.toastMessage = "" }

    Rectangle { anchors.fill: parent; color: Ui.Theme.background }
    // Pointer input stops here: Settings under this screen stays visible
    // and must not take a click or a hover (review of #780).
    MouseArea {
        objectName: "foldersPointerBlocker"
        anchors.fill: parent
        acceptedButtons: Qt.AllButtons
        hoverEnabled: true
        onWheel: wheel => wheel.accepted = true
    }
    Ui.LogicalCanvas {
        anchors.fill: parent
        visible: !root.picking
        Item {
            anchors.fill: parent
            Row {
                x: 120; y: 96
                spacing: 20
                Ui.UiText {
                    objectName: "foldersTitle"
                    text: root.library ? qsTr("Library folders") : (root.row.title || qsTr("Folders"))
                    role: "title"
                }
                Ui.UiText {
                    objectName: "foldersCount"
                    y: 12
                    visible: root.folders.length > 0
                    text: String(root.folders.length)
                    font.pixelSize: 40
                    font.weight: 600
                    color: Ui.Theme.textHint
                }
            }
            Ui.ActionButton {
                id: addButton
                objectName: "foldersAdd"
                anchors.right: parent.right
                anchors.rightMargin: 120
                y: 96
                height: 80
                visible: root.folders.length > 0
                text: qsTr("Add folder")
                // LF-5: at the limit it stays focusable and explains why.
                opacity: root.full ? 0.4 : 1
                pointerMode: root.pointerMode
                onActiveFocusChanged: if (activeFocus) root.side = "add"
                onActivated: root.add()
            }
            Ui.UiText {
                objectName: "foldersLimit"
                x: 120; y: 170
                visible: root.full
                text: qsTr("This is the limit: %n folders.", "", root.maxItems)
                color: Ui.Theme.textSecondary
            }
            Ui.UiText {
                objectName: "foldersError"
                x: 120; y: 170
                width: 1500
                visible: !root.full && !!root.row.error
                text: root.errorText(root.row.error)
                color: Ui.Theme.error
                elide: Text.ElideRight
            }
            Flickable {
                id: list
                objectName: "foldersList"
                x: 96; y: 210
                width: 1500 + 16 + 128 + 48
                height: 1080 - 210 - 120
                clip: true
                visible: root.folders.length > 0
                contentHeight: column.height + 24
                interactive: root.pointerMode
                boundsBehavior: Flickable.StopAtBounds
                Behavior on contentY { NumberAnimation { duration: 420; easing.type: Easing.OutCubic } }
                Column {
                    id: column
                    x: 24; y: 12
                    spacing: 16
                    Repeater {
                        id: rows
                        model: root.folders
                        delegate: Item {
                            id: slot
                            required property string modelData
                            required property int index
                            readonly property alias rowButton: rowButton
                            readonly property alias removeButton: removeButton
                            readonly property bool missing: root.unavailable.indexOf(modelData) >= 0
                            readonly property bool adding: modelData === root.addedPath && root.addOutstanding
                            width: 1500 + 16 + 128
                            height: 128
                            Ui.Surface {
                                id: rowButton
                                objectName: "folderRow-" + slot.index
                                width: 1500; height: 128
                                shapeRole: "container"
                                cornerRadius: 26
                                focusScale: 1.02; focusLift: 0; liftFromBottom: false
                                text: root.nameOf(slot.modelData)
                                pointerMode: root.pointerMode
                                onActiveFocusChanged: if (activeFocus) { root.selectedIndex = slot.index; root.side = "row"; }
                                contentItem: Item {
                                    Rectangle {
                                        x: 28; anchors.verticalCenter: parent.verticalCenter
                                        width: 76; height: 76; radius: 22
                                        color: slot.missing ? Qt.rgba(1, 1, 1, 0.06) : Ui.Theme.chip
                                        FolderGlyph {
                                            anchors.centerIn: parent
                                            width: 40; height: 40
                                            name: slot.missing ? "folder_off" : "folder"
                                            color: Ui.Theme.textSecondary
                                        }
                                    }
                                    Column {
                                        x: 28 + 76 + 28
                                        width: parent.width - x - 28
                                        anchors.verticalCenter: parent.verticalCenter
                                        spacing: 4
                                        Ui.UiText {
                                            objectName: "folderName-" + slot.index
                                            width: parent.width
                                            text: rowButton.text
                                            font.pixelSize: 30
                                            font.weight: 600
                                            color: slot.missing ? Ui.Theme.textSecondary : Ui.Theme.textPrimary
                                            elide: Text.ElideRight
                                        }
                                        Ui.UiText {
                                            objectName: "folderPath-" + slot.index
                                            width: parent.width
                                            text: slot.modelData
                                            font.family: "monospace"
                                            font.pixelSize: 24
                                            color: Ui.Theme.textSecondary
                                            elide: Text.ElideMiddle
                                        }
                                        Ui.UiText {
                                            objectName: "folderStatus-" + slot.index
                                            width: parent.width
                                            visible: text.length > 0
                                            text: slot.missing ? (root.library ? qsTr("Unavailable. Its films stay in the library as unavailable")
                                                                               : qsTr("Unavailable"))
                                                : slot.adding ? (root.library ? qsTr("Adding to the library…") : qsTr("Adding…")) : ""
                                            font.pixelSize: 24
                                            color: slot.missing ? Ui.Theme.error : Ui.Theme.ember
                                            elide: Text.ElideRight
                                        }
                                    }
                                }
                            }
                            Ui.Surface {
                                id: removeButton
                                objectName: "folderRemove-" + slot.index
                                x: 1500 + 16
                                width: 128; height: 128
                                shapeRole: "action"
                                cornerRadius: 26
                                focusScale: 1.06; focusLift: 0; liftFromBottom: false
                                restingColor: Qt.rgba(1, 1, 1, 0.06)
                                text: qsTr("Remove")
                                pointerMode: root.pointerMode
                                onActiveFocusChanged: if (activeFocus) { root.selectedIndex = slot.index; root.side = "remove"; }
                                onActivated: root.askRemove(slot.modelData)
                                contentItem: Ui.UiText {
                                    text: removeButton.text
                                    role: "control"
                                    font.pixelSize: 24
                                    horizontalAlignment: Text.AlignHCenter
                                    verticalAlignment: Text.AlignVCenter
                                    color: removeButton.activeFocus ? Ui.Theme.ink : Ui.Theme.textSecondary
                                }
                            }
                        }
                    }
                }
            }
            // LF-2: no folders yet.
            Column {
                objectName: "foldersEmpty"
                anchors.horizontalCenter: parent.horizontalCenter
                y: 280
                width: 820
                spacing: 24
                visible: root.folders.length === 0
                Rectangle {
                    anchors.horizontalCenter: parent.horizontalCenter
                    width: 160; height: 160; radius: 48
                    color: Ui.Theme.plate
                    FolderGlyph { anchors.centerIn: parent; width: 88; height: 88; name: "create_new_folder"; color: Ui.Theme.textSecondary }
                }
                Ui.UiText {
                    width: parent.width
                    horizontalAlignment: Text.AlignHCenter
                    text: qsTr("No folders yet")
                    role: "title"; font.pixelSize: 52
                }
                Ui.UiText {
                    objectName: "foldersEmptyBody"
                    width: parent.width
                    horizontalAlignment: Text.AlignHCenter
                    visible: text.length > 0
                    text: root.library ? qsTr("Add a folder with films and series. Arinn finds them and keeps the library up to date.")
                                       : (root.row.description || "")
                    color: Ui.Theme.textSecondary
                    wrapMode: Text.WordWrap
                }
                Ui.ActionButton {
                    id: emptyAdd
                    objectName: "foldersEmptyAdd"
                    anchors.horizontalCenter: parent.horizontalCenter
                    text: qsTr("Add folder")
                    pointerMode: root.pointerMode
                    onActivated: root.add()
                }
            }
            Rectangle {
                objectName: "foldersToast"
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
                    objectName: "foldersToastText"
                    anchors.centerIn: parent
                    text: root.toastMessage
                    role: "meta"
                }
            }
            // LF-6, LF-7: the power-confirm dialog.
            Rectangle {
                id: confirm
                objectName: "foldersConfirm"
                anchors.fill: parent
                visible: false
                color: Qt.rgba(0, 0, 0, 0.58)
                MouseArea { anchors.fill: parent; onClicked: root.cancelRemove() }
                Rectangle {
                    anchors.centerIn: parent
                    width: 900
                    height: confirmColumn.implicitHeight + 112
                    radius: 48
                    color: Qt.rgba(0.161, 0.165, 0.188, 0.94)
                    border.width: 1
                    border.color: Qt.rgba(1, 1, 1, 0.10)
                    MouseArea { anchors.fill: parent }
                    Column {
                        id: confirmColumn
                        x: 56; y: 56
                        width: parent.width - 112
                        spacing: 24
                        Ui.UiText {
                            objectName: "foldersConfirmTitle"
                            width: parent.width
                            text: (root.library ? qsTr("Remove “%1” from the library?")
                                                : qsTr("Remove “%1” from the list?")).arg(root.nameOf(root.removing))
                            role: "title"; font.pixelSize: 48
                            wrapMode: Text.WordWrap
                        }
                        Ui.UiText {
                            width: parent.width
                            text: root.library ? qsTr("Its films and series leave the library. Nothing is deleted from the disk.")
                                               : qsTr("Nothing is deleted from the disk.")
                            color: Ui.Theme.textSecondary
                            wrapMode: Text.WordWrap
                        }
                        Row {
                            spacing: 20
                            Ui.ActionButton {
                                id: cancelButton
                                objectName: "foldersConfirmCancel"
                                height: 96
                                text: qsTr("Cancel")
                                pointerMode: root.pointerMode
                                onActivated: root.cancelRemove()
                            }
                            Ui.ActionButton {
                                id: confirmButton
                                objectName: "foldersConfirmRemove"
                                height: 96
                                destructive: true
                                text: qsTr("Remove")
                                pointerMode: root.pointerMode
                                onActivated: root.confirmRemove()
                            }
                        }
                    }
                }
            }
        }
    }
    FolderPicker {
        id: picker
        objectName: "folderPicker"
        anchors.fill: parent
        visible: false
        browser: root.browser
        pointerMode: root.pointerMode
        onChosen: path => root.chosen(path)
        onCancelled: { visible = false; Qt.callLater(root.restore); }
    }
}
