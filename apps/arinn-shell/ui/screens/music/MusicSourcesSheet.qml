// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtQuick.Window
import "../.." as Ui

// The Sources sheet (brief mh-01, MH-5): every music source with its mark,
// its state and its housekeeping as pills, by capability. It never filters
// the room. Disconnect asks first, with Cancel as the initial focus.
// Turn off (#842) takes a source out of the room and moves it to the Off
// group at the bottom, where Turn on brings it back; the source that plays
// asks first, then its session stops.
FocusScope {
    id: root
    property var room: null
    // The source whose session plays or is paused, if any.
    property string playingSource: ""
    // The music catalogue, to stop that session.
    property var catalog: null
    // The playing source being stopped before it turns off: it turns off
    // only once its session no longer plays (Codex review of #851). A
    // failed stop keeps it on and says so on its row.
    property string stopping: ""
    property string stopFailed: ""
    property bool stopSent: false
    property string stopAction: ""
    function stopThenTurnOff(id) {
        stopping = id;
        stopFailed = "";
        stopSent = false;
        stopWatch.elapsed = 0;
        stopWatch.restart();
    }
    function failStop() {
        stopWatch.stop();
        stopFailed = stopping;
        stopping = "";
    }
    Timer {
        id: stopWatch
        interval: 100
        repeat: true
        property int elapsed: 0
        onTriggered: {
            elapsed += interval;
            const music = root.catalog;
            if (!music || elapsed > 10000) {
                root.failStop();
                return;
            }
            if (music.transportBusy)
                return;
            const state = music.activeState || ({});
            const mine = music.activeSourceId === root.stopping;
            const sounding = mine && ["playing", "loading"].includes(state.playback);
            const active = mine && (sounding || state.playback === "paused");
            if (!root.stopSent) {
                // Stop ends a playing or a paused session; without it, Pause
                // silences a playing one, and a paused one is silent already
                // (Codex review of #851).
                const actions = state.actions || [];
                root.stopAction = !active ? "" : actions.includes("stop") ? "stop"
                    : sounding && actions.includes("pause") ? "pause" : sounding ? "none" : "";
                if (root.stopAction === "none") {
                    root.failStop();
                    return;
                }
                if (root.stopAction.length) {
                    music.transport(root.stopAction);
                    root.stopSent = true;
                    return;
                }
                stop();
                root.setEnabled(root.stopping, false);
                root.stopping = "";
                return;
            }
            if ((music.errorText || "").length) {
                root.failStop();
                return;
            }
            if (root.stopAction === "stop" ? !active : !sounding) {
                stop();
                root.setEnabled(root.stopping, false);
                root.stopping = "";
            }
        }
    }
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
    // The playing source whose Turn off waits for confirmation (#842).
    property string turningOff: ""
    function setEnabled(id, enabled) {
        if (room && room.setSourceEnabled)
            room.setSourceEnabled(id, enabled);
    }
    signal closed()
    anchors.fill: parent
    visible: false
    function open() {
        disconnecting = "";
        turningOff = "";
        stopFailed = "";
        visible = true;
        Qt.callLater(focusFirst);
    }
    function close() {
        disconnecting = "";
        turningOff = "";
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
            : action === "off" ? qsTr("Turn off")
            : action === "on" ? qsTr("Turn on")
            : qsTr("Disconnect");
    }
    function statusText(source) {
        const status = source.status;
        return status === "off" ? qsTr("Off")
            : status === "connected" ? qsTr("Connected")
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
        } else if (action === "off" && source.id === playingSource) {
            turningOff = source.id;
            Qt.callLater(() => Ui.Focus.take(keepPlaying));
        } else if (action === "off" || action === "on") {
            stopFailed = "";
            setEnabled(source.id, action === "on");
        } else if (room) {
            room.sourceCommand(source.id, action);
        }
    }
    Keys.onPressed: event => {
        if ([Qt.Key_Escape, Qt.Key_Back, Qt.Key_Backspace].includes(event.key)) {
            event.accepted = true;
            if (disconnecting.length || turningOff.length) {
                disconnecting = "";
                turningOff = "";
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
                    // Every source that is on can be turned off; one that is
                    // off can only be turned on (#842).
                    readonly property bool off: modelData.status === "off"
                    function pillsFor(source) {
                        return source.status === "off" ? ["on"] : (source.actions || []).concat(["off"]);
                    }
                    property var actionList: []
                    onModelDataChanged: {
                        const next = pillsFor(modelData);
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
                                // Turned off or on: focus stays on this
                                // source, on its new first pill.
                                if (pills.count)
                                    Ui.Focus.take(pills.itemAt(0));
                                else
                                    root.focusFirst();
                            });
                    }
                    Component.onCompleted: actionList = pillsFor(modelData)
                    objectName: "musicSourceRow"
                    width: parent.width
                    spacing: 16
                    function firstAction() {
                        return pills.count ? pills.itemAt(0) : null;
                    }
                    // The Off group's heading, over its first source (#842).
                    Ui.UiText {
                        objectName: "musicSourcesOffGroup"
                        visible: sourceRow.off && (sourceRow.index === 0
                            || !rows.itemAt(sourceRow.index - 1) || !rows.itemAt(sourceRow.index - 1).off)
                        role: "section"
                        color: Ui.Theme.textSecondary
                        text: qsTr("Turned off")
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
                    // A command the source refused says so (Codex review).
                    Ui.UiText {
                        objectName: "musicSourceError"
                        width: parent.width
                        visible: text.length > 0
                        role: "meta"
                        color: Ui.Theme.textSecondary
                        wrapMode: Text.WordWrap
                        textFormat: Text.PlainText
                        text: sourceRow.modelData.id === root.stopFailed
                            ? qsTr("The music could not be stopped, so the source stays on.")
                            : (sourceRow.modelData.error || "")
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
                                    || (pillAction === "off" && root.stopping === sourceRow.modelData.id)
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
    // Turning off the source that plays stops its music (#842).
    MusicDialog {
        objectName: "musicTurnOffDialog"
        visible: root.turningOff.length > 0
        z: 2
        title: qsTr("Turn off this source?")
        body: qsTr("The music from this source stops.")
        onDismissed: { root.turningOff = ""; root.focusFirst(); }
        Ui.ActionButton {
            id: keepPlaying
            objectName: "musicKeepPlaying"
            text: qsTr("Cancel")
            KeyNavigation.right: stopSource
            onActivated: { root.turningOff = ""; root.focusFirst(); }
        }
        Ui.ActionButton {
            id: stopSource
            objectName: "musicTurnOffSource"
            text: qsTr("Turn off")
            destructive: true
            KeyNavigation.left: keepPlaying
            onActivated: {
                const id = root.turningOff;
                root.turningOff = "";
                root.stopThenTurnOff(id);
                Qt.callLater(root.focusFirst);
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
