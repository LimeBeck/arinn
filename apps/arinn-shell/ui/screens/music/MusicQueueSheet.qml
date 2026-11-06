// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import "../.." as Ui

// The Queue sheet (brief *Queue*, #802): a right-hand sheet 720 wide. The
// header names the station of a continuous queue; container rows carry the
// cover, title, artist and duration; the current entry carries the playing
// mark and is the initial focus. A continuous queue shows only its loaded
// window and a closing line, never a total.
FocusScope {
    id: root
    // [{occurrence, track, current}], the current entry first.
    property var entries: []
    property string title: ""
    property bool continuous: false
    property bool canSave: false
    property bool saveEnabled: true
    readonly property alias list: queueList
    readonly property Item saveButton: saveQueue
    signal entryActivated(var entry)
    signal saveRequested()
    signal dismissed()
    anchors.fill: parent
    // Rows are keyed by occurrence: the state poll republishes the queue
    // every second, and a rebuilt list would drop the focused row.
    ListModel { id: rowModel }
    function syncRows() {
        const keys = entries.map(entry => entry.occurrence || "");
        for (let i = rowModel.count - 1; i >= 0; --i)
            if (!keys.includes(rowModel.get(i).rowKey))
                rowModel.remove(i);
        for (let i = 0; i < keys.length; ++i) {
            let at = -1;
            for (let j = i; j < rowModel.count && at < 0; ++j)
                if (rowModel.get(j).rowKey === keys[i])
                    at = j;
            if (at < 0)
                rowModel.insert(i, {rowKey: keys[i]});
            else if (at !== i)
                rowModel.move(at, i, 1);
        }
    }
    onEntriesChanged: syncRows()
    Component.onCompleted: syncRows()
    function focusInitial() {
        const at = Math.max(0, entries.findIndex(entry => entry.current === true));
        if (queueList.count) {
            queueList.currentIndex = at;
            queueList.positionViewAtIndex(at, ListView.Contain);
            Ui.Focus.take(queueList);
        } else if (!(saveQueue.visible && saveQueue.enabled && Ui.Focus.take(saveQueue))) {
            // A disabled Save (a command in flight) cannot hold focus: the
            // sheet does (Codex review of #821).
            Ui.Focus.take(root);
        }
    }
    function timeLabel(microseconds) {
        const seconds = Math.max(0, Math.floor((microseconds || 0) / 1000000));
        const hours = Math.floor(seconds / 3600);
        const minutes = Math.floor(seconds % 3600 / 60);
        const rest = String(seconds % 60).padStart(2, "0");
        return hours > 0 ? hours + ":" + String(minutes).padStart(2, "0") + ":" + rest : minutes + ":" + rest;
    }
    Keys.onLeftPressed: {}
    Keys.onRightPressed: {}
    Rectangle {
        anchors.fill: parent
        color: Ui.Theme.scrim
        MouseArea {
            anchors.fill: parent
            onClicked: root.dismissed()
        }
    }
    Rectangle {
        objectName: "musicQueueSheet"
        x: parent.width - width
        width: 720
        height: parent.height
        color: Ui.Theme.dialogSurface
        border.color: Ui.Theme.dialogLine
        MouseArea { anchors.fill: parent }
        Ui.UiText {
            objectName: "musicQueueEyebrow"
            x: 64
            y: 64
            role: "eyebrow"
            color: Ui.Theme.textSecondary
            text: qsTr("Queue")
        }
        Ui.UiText {
            objectName: "musicQueueTitle"
            x: 64
            y: 100
            width: parent.width - 128
            role: "title"
            textFormat: Text.PlainText
            maximumLineCount: 1
            elide: Text.ElideRight
            text: root.title.length ? root.title : qsTr("Up next")
        }
        Ui.ActionButton {
            id: saveQueue
            objectName: "musicSaveQueue"
            x: 64
            y: 188
            visible: root.canSave
            enabled: root.saveEnabled
            text: qsTr("Save queue")
            // An empty queue offers nothing below (review of #821).
            KeyNavigation.down: queueList.count ? queueList : null
            onActivated: root.saveRequested()
        }
        Ui.UiText {
            objectName: "musicQueueEmpty"
            x: 64
            y: saveQueue.visible ? 320 : 220
            width: parent.width - 128
            visible: !queueList.count
            role: "body"
            color: Ui.Theme.textSecondary
            text: qsTr("The queue is empty")
        }
        ListView {
            id: queueList
            objectName: "musicQueueEntries"
            x: 48
            y: saveQueue.visible ? 300 : 200
            width: parent.width - 96
            height: parent.height - y - 48
            spacing: 12
            clip: true
            keyNavigationEnabled: true
            model: rowModel
            Keys.onUpPressed: event => {
                if (currentIndex <= 0 && saveQueue.visible)
                    Ui.Focus.take(saveQueue);
                else
                    event.accepted = false;
            }
            footer: Ui.UiText {
                objectName: "musicQueueFooter"
                width: queueList.width
                height: visible ? 88 : 0
                visible: root.continuous && queueList.count > 0
                leftPadding: 24
                verticalAlignment: Text.AlignVCenter
                role: "meta"
                color: Ui.Theme.textSecondary
                text: qsTr("More tracks will follow")
            }
            delegate: Ui.Surface {
                id: row
                required property string rowKey
                required property int index
                readonly property var entry: root.entries.find(item => (item.occurrence || "") === rowKey) || ({})
                readonly property var track: entry.track || ({})
                objectName: entry.current ? "musicQueueCurrent" : "musicQueueRow"
                shapeRole: "container"
                cornerRadius: Ui.Theme.containerRadius
                focusScale: 1
                focusLift: 0
                width: queueList.width
                height: 104
                focus: ListView.isCurrentItem
                text: track.title || qsTr("Untitled")
                contentItem: Item {
                    MusicArtwork {
                        x: 16
                        anchors.verticalCenter: parent.verticalCenter
                        width: 72
                        height: 72
                        radius: Ui.Theme.stillRadius
                        source: row.track.image || ""
                    }
                    Column {
                        x: 112
                        anchors.verticalCenter: parent.verticalCenter
                        width: parent.width - x - 140
                        Ui.UiText {
                            width: parent.width
                            text: row.text
                            role: "body"
                            textFormat: Text.PlainText
                            maximumLineCount: 1
                            elide: Text.ElideRight
                            color: Ui.Theme.textPrimary
                        }
                        Ui.UiText {
                            width: parent.width
                            visible: text.length > 0
                            text: row.track.artist || ""
                            role: "meta"
                            textFormat: Text.PlainText
                            maximumLineCount: 1
                            elide: Text.ElideRight
                            color: Ui.Theme.textSecondary
                        }
                    }
                    // The current entry carries the playing mark, before its
                    // duration (Codex review of #821).
                    Row {
                        objectName: "musicQueuePlaying"
                        anchors.right: rowDuration.visible ? rowDuration.left : parent.right
                        anchors.rightMargin: rowDuration.visible ? 20 : 28
                        anchors.verticalCenter: parent.verticalCenter
                        visible: row.entry.current === true
                        spacing: 4
                        Repeater {
                            model: [14, 22, 10]
                            Rectangle {
                                required property var modelData
                                width: 5
                                height: modelData
                                radius: 2
                                anchors.bottom: parent.bottom
                                color: Ui.Theme.ember
                            }
                        }
                    }
                    Ui.UiText {
                        id: rowDuration
                        objectName: "musicQueueDuration"
                        anchors.right: parent.right
                        anchors.rightMargin: 28
                        anchors.verticalCenter: parent.verticalCenter
                        visible: (row.track.duration_us || 0) > 0
                        role: "meta"
                        color: Ui.Theme.textSecondary
                        font.features: ({ "tnum": 1 })
                        text: root.timeLabel(row.track.duration_us)
                    }
                }
                onActivated: root.entryActivated(entry)
            }
        }
    }
}
