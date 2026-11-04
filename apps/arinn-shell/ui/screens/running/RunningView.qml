// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import QtQuick.Window
import "../.." as Ui

// Running applications (handoff *Closing applications*, CL-4…7, CL-13…15):
// one row per running window of an ordinary application, the one on screen
// first, with a Close button to the right of each row, in the Open
// services geometry. The caller supplies the windows and receives intents;
// closing itself is the shell's close flow (CloseFlow.qml).
FocusScope {
    id: root
    width: 1920
    height: 1080
    focus: true

    // {id, title, applicationId, providerId, iconSource, visualIdentity,
    // monogram, terminable, enabled}; browser windows are not listed.
    property var windows: []
    // The id of the window on screen, or "".
    property string currentId: ""
    // Windows whose close request is in flight (CL-9), and those that did
    // not close (CL-11).
    property var closingIds: []
    property var failedIds: []
    property bool ownsFocus: true
    property bool pointerMode: Ui.InputMode.pointerActive
    property string toastMessage: ""
    property string selectedId: ""
    property int selectedIndex: 0
    property string focusPart: "row" // "row" or "close"
    readonly property int toastDurationMs: 2600
    readonly property int rowHeight: 128
    readonly property int rowGap: 16
    // The on-screen window first, then the caller's order (by last use).
    readonly property var orderedWindows: {
        const list = (windows || []).filter(entry => entry && entry.id);
        const current = list.filter(entry => entry.id === currentId);
        return current.concat(list.filter(entry => entry.id !== currentId));
    }
    readonly property int count: orderedWindows.length

    signal activateRequested(string windowId)
    signal closeRequested(string windowId)
    signal backRequested()
    signal homeRequested()
    signal navigationRequested()

    // "One of 2 windows" when an application has several (CL-14).
    function siblings(entry) {
        if (!entry) return 1;
        return orderedWindows.filter(other => other.providerId === entry.providerId
                                     && other.applicationId === entry.applicationId).length;
    }
    function statusText(entry) {
        if (!entry) return "";
        if (closingIds.indexOf(entry.id) >= 0) return qsTr("Closing…");
        const parts = [];
        const total = siblings(entry);
        if (total > 1) parts.push(qsTr("One of %n windows", "", total));
        if (entry.terminable === false) parts.push(qsTr("Arinn cannot close this"));
        else if (failedIds.indexOf(entry.id) >= 0) parts.push(qsTr("Did not close"));
        else parts.push(entry.id === currentId ? qsTr("On screen") : qsTr("In the background"));
        return parts.join(" · ");
    }
    function showToast(message) {
        toastMessage = message;
        toastTimer.restart();
    }
    function rowItem(index) { return rows.itemAt(index); }
    function focusRow(index, part) {
        if (!count) {
            Ui.Focus.take(emptyAction);
            return false;
        }
        const clamped = Math.max(0, Math.min(index, count - 1));
        const item = rowItem(clamped);
        if (!item) return false;
        selectedIndex = clamped;
        selectedId = orderedWindows[clamped].id;
        focusPart = part || "row";
        Ui.Focus.take(focusPart === "close" ? item.closeButton : item.rowButton);
        reveal(clamped);
        return true;
    }
    // Keep focus two rows from the bottom while there is more below (CL-13).
    function reveal(index) {
        const pitch = rowHeight + rowGap;
        const top = index * pitch;
        const margin = 2 * pitch;
        let next = list.contentY;
        if (top + rowHeight + margin > next + list.height) next = top + rowHeight + margin - list.height;
        if (top - pitch < next) next = top - pitch;
        list.contentY = Math.max(0, Math.min(next, Math.max(0, list.contentHeight - list.height)));
    }
    function restore() {
        if (!count) {
            Ui.Focus.take(emptyAction);
            return;
        }
        let index = orderedWindows.findIndex(entry => entry.id === selectedId);
        if (index < 0) index = Math.min(selectedIndex, count - 1);
        focusRow(index, focusPart);
    }
    function restoreOwned() { if (ownsFocus && visible) restore(); }
    onOrderedWindowsChanged: Qt.callLater(restoreOwned)
    // CL-9: when the focused window starts closing, focus moves on to the
    // next row at once (the previous one at the end).
    onClosingIdsChanged: {
        if (!ownsFocus || !visible || closingIds.indexOf(selectedId) < 0 || count < 2) return;
        const index = orderedWindows.findIndex(entry => entry.id === selectedId);
        const next = index + 1 < count ? index + 1 : index - 1;
        Qt.callLater(() => focusRow(next, "row"));
    }
    onOwnsFocusChanged: if (ownsFocus) Qt.callLater(restoreOwned)
    onVisibleChanged: if (visible) Qt.callLater(restoreOwned)
    Component.onCompleted: Qt.callLater(restoreOwned)
    function move(step) {
        const next = selectedIndex + step;
        if (next < 0 || next >= count) return;
        focusRow(next, focusPart);
    }
    function acceptKey(event) {
        if ([Qt.Key_Left, Qt.Key_Right, Qt.Key_Up, Qt.Key_Down].includes(event.key))
            Ui.InputMode.keyPressed();
        if (event.key === Qt.Key_Escape || event.key === Qt.Key_Backspace || event.key === Qt.Key_Back) {
            if (!event.isAutoRepeat) backRequested();
        } else if (event.key === Qt.Key_Home) {
            if (!event.isAutoRepeat) homeRequested();
        } else if (event.key === Qt.Key_Up) move(-1);
        else if (event.key === Qt.Key_Down) move(1);
        else if (event.key === Qt.Key_Right) {
            if (count && focusPart === "row") focusRow(selectedIndex, "close");
        } else if (event.key === Qt.Key_Left) {
            if (count && focusPart === "close") focusRow(selectedIndex, "row");
            else if (!event.isAutoRepeat) navigationRequested();
        } else if (event.key === Qt.Key_Tab || event.key === Qt.Key_Backtab) {
            // Focus stays inside the screen.
        } else return;
        event.accepted = true;
    }
    function closeWindow(entry) {
        if (!entry || closingIds.indexOf(entry.id) >= 0) return;
        // CL-15: no dialog for a window Arinn cannot close, only a note.
        if (entry.terminable === false) {
            showToast(qsTr("Arinn cannot close “%1”. Close it in the application itself.").arg(entry.title || ""));
            return;
        }
        closeRequested(entry.id);
    }
    Keys.onPressed: event => acceptKey(event)
    Timer { id: toastTimer; interval: root.toastDurationMs; onTriggered: root.toastMessage = "" }

    // Material `block`: a circle with a diagonal bar (CL-15).
    component BlockGlyph: Canvas {
        id: glyph
        property color color: Ui.Theme.textSecondary
        width: 44; height: 44
        onColorChanged: requestPaint()
        onPaint: {
            const ctx = getContext("2d");
            ctx.reset();
            ctx.scale(width / 24, height / 24);
            ctx.strokeStyle = glyph.color;
            ctx.lineWidth = 2;
            ctx.lineCap = "round";
            ctx.beginPath();
            ctx.arc(12, 12, 9, 0, Math.PI * 2);
            ctx.moveTo(5.6, 5.6); ctx.lineTo(18.4, 18.4);
            ctx.stroke();
        }
    }
    // A chip with the application's icon or its letter on the neutral chip.
    component Chip: Rectangle {
        id: chip
        property string iconSource: ""
        property string monogram: ""
        width: 76; height: 76; radius: 22
        color: Ui.Theme.chip
        border.width: 1
        border.color: Ui.Theme.chipLine
        Image {
            id: icon
            anchors.centerIn: parent
            width: 48; height: 48
            sourceSize: Qt.size(96, 96)
            source: chip.iconSource
            asynchronous: true
            fillMode: Image.PreserveAspectFit
            visible: status === Image.Ready
        }
        Ui.UiText {
            anchors.centerIn: parent
            visible: !icon.visible
            text: chip.monogram
            font.pixelSize: 34
            font.weight: 700
            color: Ui.Theme.textSecondary
        }
    }

    Rectangle { anchors.fill: parent; color: Ui.Theme.background }
    Ui.LogicalCanvas {
        anchors.fill: parent
        Item {
            anchors.fill: parent
            Row {
                x: Ui.Theme.safeLeft
                y: 96
                spacing: 20
                Ui.UiText {
                    objectName: "runningTitle"
                    text: qsTr("Running applications")
                    role: "title"
                }
                Ui.UiText {
                    objectName: "runningCount"
                    y: 12
                    visible: root.count > 0
                    text: String(root.count)
                    font.pixelSize: 40
                    font.weight: 600
                    color: Ui.Theme.textHint
                }
            }
            Ui.UiText {
                objectName: "runningPosition"
                anchors.right: parent.right
                anchors.rightMargin: Ui.Theme.safeRight
                y: 150
                visible: list.contentHeight > list.height && root.count > 0
                text: qsTr("%1 of %2").arg(root.selectedIndex + 1).arg(root.count)
                font.pixelSize: 26
                font.weight: 500
                color: Ui.Theme.textSecondary
            }
            Flickable {
                id: list
                objectName: "runningList"
                x: Ui.Theme.safeLeft - 24
                y: 186
                width: 1500 + 16 + 128 + 48
                height: 1080 - y - 150
                visible: root.count > 0
                clip: true
                contentWidth: width
                contentHeight: column.height + 48
                interactive: root.pointerMode
                boundsBehavior: Flickable.StopAtBounds
                Behavior on contentY { NumberAnimation { duration: 420; easing.type: Easing.OutCubic } }
                Column {
                    id: column
                    x: 24
                    y: 24
                    spacing: root.rowGap
                    Repeater {
                        id: rows
                        model: root.orderedWindows
                        delegate: Item {
                            id: rowSlot
                            required property var modelData
                            required property int index
                            readonly property bool closing: root.closingIds.indexOf(modelData.id) >= 0
                            readonly property alias rowButton: rowButton
                            readonly property alias closeButton: closeButton
                            width: 1500 + root.rowGap + 128
                            height: root.rowHeight
                            opacity: closing ? 0.5 : 1
                            Ui.Surface {
                                id: rowButton
                                objectName: "runningRow-" + rowSlot.modelData.id
                                width: 1500
                                height: root.rowHeight
                                text: rowSlot.modelData.title || ""
                                visualIdentity: rowSlot.modelData.visualIdentity || rowSlot.modelData.id
                                shapeRole: "container"
                                cornerRadius: 26
                                focusScale: 1.02
                                focusLift: 0
                                liftFromBottom: false
                                // A window that cannot be activated still takes
                                // focus, so its Close stays reachable; OK does
                                // nothing on it (review of #777).
                                readonly property bool activatable: rowSlot.modelData.enabled !== false
                                opacity: activatable ? 1 : 0.6
                                pointerMode: root.pointerMode
                                onActiveFocusChanged: if (activeFocus) {
                                    root.selectedIndex = rowSlot.index;
                                    root.selectedId = rowSlot.modelData.id;
                                    root.focusPart = "row";
                                }
                                onActivated: if (activatable) root.activateRequested(rowSlot.modelData.id)
                                Keys.onPressed: event => root.acceptKey(event)
                                contentItem: Item {
                                    Chip {
                                        x: 28
                                        anchors.verticalCenter: parent.verticalCenter
                                        iconSource: rowSlot.modelData.iconSource || ""
                                        monogram: rowSlot.modelData.monogram
                                                  || String(rowSlot.modelData.title || "").trim().slice(0, 1).toUpperCase()
                                    }
                                    Column {
                                        x: 28 + 76 + 28
                                        width: parent.width - x - 28
                                        anchors.verticalCenter: parent.verticalCenter
                                        spacing: 4
                                        Ui.UiText {
                                            objectName: "runningName-" + rowSlot.modelData.id
                                            width: parent.width
                                            text: rowSlot.modelData.title || ""
                                            font.pixelSize: 30
                                            font.weight: 600
                                            maximumLineCount: 1
                                            wrapMode: Text.NoWrap
                                            elide: Text.ElideRight
                                        }
                                        Row {
                                            spacing: 10
                                            Rectangle {
                                                objectName: "runningOnScreen-" + rowSlot.modelData.id
                                                anchors.verticalCenter: parent.verticalCenter
                                                width: 10; height: 10; radius: 5
                                                color: Ui.Theme.ember
                                                visible: rowSlot.modelData.id === root.currentId && !rowSlot.closing
                                            }
                                            Ui.UiText {
                                                objectName: "runningStatus-" + rowSlot.modelData.id
                                                text: root.statusText(rowSlot.modelData)
                                                font.pixelSize: 24
                                                font.weight: 500
                                                color: rowSlot.modelData.terminable === false ? Ui.Theme.textHint : Ui.Theme.textSecondary
                                            }
                                        }
                                    }
                                }
                            }
                            Ui.Surface {
                                id: closeButton
                                objectName: "runningClose-" + rowSlot.modelData.id
                                x: 1500 + root.rowGap
                                width: 128
                                height: 128
                                text: qsTr("Close")
                                shapeRole: "action"
                                cornerRadius: 26
                                focusScale: 1.06
                                focusLift: 0
                                liftFromBottom: false
                                busy: rowSlot.closing
                                // CL-15: focusable, drawn at 40 % with a block icon.
                                readonly property bool blocked: rowSlot.modelData.terminable === false
                                opacity: blocked ? 0.4 : 1
                                restingColor: Qt.rgba(1, 1, 1, 0.06)
                                pointerMode: root.pointerMode
                                Accessible.name: qsTr("Close %1").arg(rowSlot.modelData.title || "")
                                onActiveFocusChanged: if (activeFocus) {
                                    root.selectedIndex = rowSlot.index;
                                    root.selectedId = rowSlot.modelData.id;
                                    root.focusPart = "close";
                                }
                                onActivated: root.closeWindow(rowSlot.modelData)
                                Keys.onPressed: event => root.acceptKey(event)
                                contentItem: Item {
                                    Ui.LineIcon {
                                        objectName: "runningCloseIcon-" + rowSlot.modelData.id
                                        anchors.centerIn: parent
                                        width: 44; height: 44
                                        visible: !rowSlot.closing && !closeButton.blocked
                                        name: "close"
                                        color: closeButton.activeFocus ? Ui.Theme.ink : Ui.Theme.textSecondary
                                    }
                                    BlockGlyph {
                                        objectName: "runningBlockIcon-" + rowSlot.modelData.id
                                        anchors.centerIn: parent
                                        visible: !rowSlot.closing && closeButton.blocked
                                        color: closeButton.activeFocus ? Ui.Theme.ink : Ui.Theme.textSecondary
                                    }
                                    Ui.UiText {
                                        objectName: "runningClosing-" + rowSlot.modelData.id
                                        anchors.centerIn: parent
                                        visible: rowSlot.closing
                                        text: "…"
                                        font.pixelSize: 44
                                        color: closeButton.activeFocus ? Ui.Theme.ink : Ui.Theme.textSecondary
                                    }
                                }
                            }
                        }
                    }
                }
            }
            // Fade masks over the scrolled list (CL-13, OS-4).
            Rectangle {
                objectName: "runningTopFade"
                x: list.x; y: list.y; width: list.width; height: 72
                visible: list.visible && list.contentY > 0
                gradient: Gradient {
                    GradientStop { position: 0; color: Ui.Theme.background }
                    GradientStop { position: 1; color: Qt.rgba(Ui.Theme.background.r, Ui.Theme.background.g, Ui.Theme.background.b, 0) }
                }
            }
            Rectangle {
                objectName: "runningBottomFade"
                x: list.x; y: list.y + list.height - height; width: list.width; height: 72
                visible: list.visible && list.contentY + list.height < list.contentHeight - 1
                gradient: Gradient {
                    GradientStop { position: 0; color: Qt.rgba(Ui.Theme.background.r, Ui.Theme.background.g, Ui.Theme.background.b, 0) }
                    GradientStop { position: 1; color: Ui.Theme.background }
                }
            }
            // Nothing running (CL-6).
            Column {
                objectName: "runningEmpty"
                anchors.horizontalCenter: parent.horizontalCenter
                y: 300
                width: 900
                spacing: 24
                visible: root.count === 0
                Ui.LineIcon {
                    anchors.horizontalCenter: parent.horizontalCenter
                    width: 88; height: 88
                    name: "screen"
                    color: Ui.Theme.textHint
                }
                Ui.UiText {
                    width: parent.width
                    horizontalAlignment: Text.AlignHCenter
                    text: qsTr("Nothing is running")
                    role: "title"
                    font.pixelSize: 48
                }
                Ui.UiText {
                    width: parent.width
                    horizontalAlignment: Text.AlignHCenter
                    text: qsTr("Applications and games you open appear here. Online services are in Open services.")
                    color: Ui.Theme.textSecondary
                    wrapMode: Text.WordWrap
                }
                Ui.ActionButton {
                    id: emptyAction
                    objectName: "runningHome"
                    anchors.horizontalCenter: parent.horizontalCenter
                    text: qsTr("Back to Home")
                    pointerMode: root.pointerMode
                    onActivated: root.homeRequested()
                    Keys.onPressed: event => root.acceptKey(event)
                }
            }
            Ui.UiText {
                objectName: "runningHint"
                x: Ui.Theme.safeLeft
                y: 1080 - 96
                visible: root.count > 0
                text: qsTr("OK Go to · → Close · Back Exit")
                font.pixelSize: 24
                color: Ui.Theme.textHint
            }
            Rectangle {
                id: toast
                objectName: "runningToast"
                z: 10
                visible: root.toastMessage.length > 0
                anchors.horizontalCenter: parent.horizontalCenter
                y: 1080 - 48 - height
                height: 84
                width: Math.min(1600, toastLabel.implicitWidth + 96)
                radius: height / 2
                color: Ui.Theme.plate
                border.width: 1
                border.color: Qt.rgba(1, 1, 1, 0.08)
                Ui.UiText {
                    id: toastLabel
                    objectName: "runningToastText"
                    anchors.centerIn: parent
                    width: Math.min(1504, implicitWidth)
                    text: root.toastMessage
                    role: "meta"
                    elide: Text.ElideRight
                }
            }
        }
    }
}
