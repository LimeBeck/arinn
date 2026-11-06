// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import "../.." as Ui

// Find over every connected source (brief mh-10, MH-10, #801): a line that
// says where it searches, then the result shelves Artists, Albums and
// Tracks with source marks, nothing merged. A source still answering adds a
// quiet line; late results never move focus.
FocusScope {
    id: root
    property var room: null
    property string playingKey: ""
    readonly property var shelves: (room && room.searchShelves) || []
    readonly property var pending: (room && room.searchPending) || []
    readonly property var places: room ? room.sources.filter(source => source.browsable !== false)
        .map(source => source.local ? qsTr("this computer") : source.name) : []
    signal entryActivated(var entry)
    signal upRequested()
    width: Ui.Theme.canvasWidth
    function focusInitial() {
        const shelf = shelfRepeater.count ? shelfRepeater.itemAt(0) : null;
        if (shelf) {
            Ui.Focus.take(shelf);
            return true;
        }
        return false;
    }
    function focusKey(key) {
        for (let i = 0; i < shelfRepeater.count; ++i)
            if (key && shelfRepeater.itemAt(i).focusKey(key))
                return true;
        return false;
    }
    Ui.UiText {
        objectName: "musicSearchWhere"
        x: Ui.Theme.safeLeft
        y: 0
        width: parent.width - Ui.Theme.safeLeft - Ui.Theme.safeRight
        role: "meta"
        color: Ui.Theme.textSecondary
        textFormat: Text.PlainText
        text: root.places.length > 1
            ? qsTr("Searching everywhere: %1 and %2").arg(root.places.slice(0, -1).join(", ")).arg(root.places[root.places.length - 1])
            : root.places.length ? qsTr("Searching %1").arg(root.places[0]) : ""
    }
    Ui.UiText {
        objectName: "musicSearchPending"
        x: Ui.Theme.safeLeft
        y: 40
        width: parent.width - Ui.Theme.safeLeft - Ui.Theme.safeRight
        visible: text.length > 0
        role: "caption"
        color: Ui.Theme.textHint
        textFormat: Text.PlainText
        text: root.pending.map(name => qsTr("%1 is still searching…").arg(name)).join("  ·  ")
    }
    Ui.UiText {
        objectName: "musicSearchEmpty"
        x: Ui.Theme.safeLeft
        y: 120
        visible: !!root.room && (root.room.searchQuery || "").length > 0 && !root.pending.length && !root.shelves.length
        color: Ui.Theme.textSecondary
        text: qsTr("Nothing found.")
    }
    // Shelves are keyed by id, so a late shelf never recreates the others.
    ListModel { id: shelfModel }
    // A shelf that leaves with focus hands it to the shelf now in its place,
    // or to the field when none is left (Codex review of #819).
    property int lostFocusAt: -1
    function syncShelves() {
        const ids = shelves.map(shelf => shelf.id);
        for (let i = shelfModel.count - 1; i >= 0; --i)
            if (!ids.includes(shelfModel.get(i).shelfKey)) {
                const item = shelfRepeater.itemAt(i);
                if (item && item.activeFocus)
                    lostFocusAt = i;
                shelfModel.remove(i);
            }
        for (let i = 0; i < ids.length; ++i) {
            let at = -1;
            for (let j = i; j < shelfModel.count && at < 0; ++j)
                if (shelfModel.get(j).shelfKey === ids[i])
                    at = j;
            if (at < 0)
                shelfModel.insert(i, {shelfKey: ids[i]});
            else if (at !== i)
                shelfModel.move(at, i, 1);
        }
        if (lostFocusAt >= 0)
            Qt.callLater(restoreFocus);
    }
    function restoreFocus() {
        const at = lostFocusAt;
        lostFocusAt = -1;
        if (at < 0 || !visible)
            return;
        const shelf = shelfRepeater.count ? shelfRepeater.itemAt(Math.min(at, shelfRepeater.count - 1)) : null;
        if (shelf)
            Ui.Focus.take(shelf);
        else
            upRequested();
    }
    onShelvesChanged: syncShelves()
    Component.onCompleted: syncShelves()
    Flickable {
        id: area
        objectName: "musicSearchShelves"
        x: Ui.Theme.safeLeft
        y: 96
        width: parent.width - Ui.Theme.safeLeft
        height: parent.height - y
        contentHeight: column.height + 96
        interactive: false
        clip: true
        Behavior on contentY { NumberAnimation { duration: Ui.Theme.focusDuration; easing.type: Easing.OutCubic } }
        function reveal(shelf) {
            if (shelf.y < contentY)
                contentY = shelf.y;
            else if (shelf.y + shelf.height > contentY + height)
                contentY = Math.max(0, shelf.y + shelf.height - height);
        }
        Column {
            id: column
            width: parent.width
            spacing: 24
            Repeater {
                id: shelfRepeater
                model: shelfModel
                MusicShelf {
                    id: shelfItem
                    required property string shelfKey
                    required property int index
                    objectName: "musicSearchShelf-" + shelfKey
                    width: column.width
                    room: root.room
                    showAll: false
                    shelf: root.shelves.find(item => item.id === shelfKey) || ({})
                    playingKey: root.playingKey
                    rowBrightness: activeFocus ? 0.82 : 0.62
                    onActiveFocusChanged: if (activeFocus)
                        area.reveal(shelfItem)
                    onYChanged: if (activeFocus)
                        area.reveal(shelfItem)
                    Keys.onUpPressed: event => {
                        const previous = shelfRepeater.itemAt(index - 1);
                        if (previous)
                            Ui.Focus.take(previous);
                        else
                            root.upRequested();
                    }
                    Keys.onDownPressed: event => {
                        const next = shelfRepeater.itemAt(index + 1);
                        if (next)
                            Ui.Focus.take(next);
                    }
                    onActivated: entry => root.entryActivated(entry)
                }
            }
        }
    }
}
