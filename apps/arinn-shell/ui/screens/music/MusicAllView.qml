// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtQuick.Window
import "../.." as Ui

// The full list behind a shelf's All tile (brief mh-11, MH-10): tabs
// "All · This computer · <service>" that filter without making the source a
// mode, starting at All every time, and the shelf order or A–Z. The shelf
// order is labelled as a date order only once entries carry dates (#806).
FocusScope {
    id: root
    property var room: null
    property string shelfId: ""
    property string title: ""
    property string playingKey: ""
    property string source: ""
    property bool alphabetical: false
    // Bumped when a page arrives, so the list is read again.
    property int revision: 0
    readonly property var sources: room ? room.sources : []
    readonly property bool round: shelfId === "artists"
    readonly property bool wide: shelfId === "stations"
    readonly property real cardWidth: wide ? 448 : 240
    readonly property real cardHeight: wide ? 252 : 240
    readonly property var entries: {
        revision;
        return room && shelfId.length ? room.list(shelfId, source, alphabetical) : [];
    }
    readonly property var focusedEntry: grid.currentIndex >= 0 && grid.currentIndex < entries.length
        ? entries[grid.currentIndex] : null
    signal entryActivated(var entry)
    width: Ui.Theme.canvasWidth
    height: Ui.Theme.canvasHeight
    function open(shelf, shelfTitle) {
        shelfId = shelf;
        title = shelfTitle;
        source = "";
        alphabetical = false;
        grid.currentIndex = 0;
        Qt.callLater(focusInitial);
    }
    function focusInitial() {
        if (grid.count)
            Ui.Focus.take(grid);
        else
            Ui.Focus.take(tabs.itemAt(0));
    }
    // Focus routed here by scope (the shell opening the section) lands on
    // this scope itself, with no card to hold it: hand it on (review of #812).
    onActiveFocusChanged: if (activeFocus)
        Qt.callLater(claimFocus)
    function claimFocus() {
        const window = Window.window;
        if (visible && activeFocus && window && window.activeFocusItem === root)
            focusInitial();
    }
    function focusKey(key) {
        const at = entries.findIndex(entry => entry.key === key);
        if (at < 0)
            return false;
        grid.currentIndex = at;
        grid.positionViewAtIndex(at, GridView.Contain);
        Ui.Focus.take(grid);
        return true;
    }
    Connections {
        target: root.room
        ignoreUnknownSignals: true
        function onListChanged() {
            const key = root.focusedEntry ? root.focusedEntry.key : "";
            root.revision++;
            // A late page keeps focus on its entry.
            if (key.length) {
                const at = root.entries.findIndex(entry => entry.key === key);
                if (at >= 0)
                    grid.currentIndex = at;
            }
        }
    }
    Ui.AmbientGlow {
        anchors.fill: parent
        veilEnabled: false
        strength: 0.6
        lit: !!root.focusedEntry && grid.activeFocus
        source: root.focusedEntry ? (root.focusedEntry.image || "") : ""
        anchor: grid.currentItem
    }
    Ui.UiText {
        objectName: "musicAllTitle"
        x: Ui.Theme.safeLeft
        y: 96
        role: "title"
        text: root.title
    }
    Row {
        id: tabRow
        objectName: "musicAllTabs"
        x: Ui.Theme.safeLeft
        y: 196
        spacing: 16
        Repeater {
            id: tabs
            model: [{id: "", name: qsTr("All")}].concat(root.sources.map(source => ({
                id: source.id, name: source.local ? qsTr("This computer") : source.name})))
            Ui.ActionButton {
                required property var modelData
                required property int index
                objectName: "musicAllTab-" + index
                text: (root.source === modelData.id ? "● " : "") + modelData.name
                KeyNavigation.left: index > 0 ? tabs.itemAt(index - 1) : null
                KeyNavigation.right: index + 1 < tabs.count ? tabs.itemAt(index + 1) : byOrder
                KeyNavigation.down: grid
                onActivated: {
                    root.source = modelData.id;
                    grid.currentIndex = 0;
                }
            }
        }
        Item { width: 32; height: 1 }
        Ui.ActionButton {
            id: byOrder
            objectName: "musicAllDefaultOrder"
            text: (root.alphabetical ? "" : "● ") + (root.room && root.room.dated ? qsTr("By date") : qsTr("Suggested"))
            KeyNavigation.left: tabs.count ? tabs.itemAt(tabs.count - 1) : null
            KeyNavigation.right: byTitle
            KeyNavigation.down: grid
            onActivated: { root.alphabetical = false; grid.currentIndex = 0; }
        }
        Ui.ActionButton {
            id: byTitle
            objectName: "musicAllAlphabetical"
            text: (root.alphabetical ? "● " : "") + qsTr("A–Z")
            KeyNavigation.left: byOrder
            KeyNavigation.down: grid
            onActivated: { root.alphabetical = true; grid.currentIndex = 0; }
        }
    }
    GridView {
        id: grid
        objectName: "musicAllGrid"
        x: Ui.Theme.safeLeft
        y: 320
        width: parent.width - Ui.Theme.safeLeft - Ui.Theme.safeRight + 32
        height: parent.height - y
        cellWidth: root.cardWidth + 32
        cellHeight: root.cardHeight + Ui.Theme.contentCaptionHeight + 40
        clip: true
        keyNavigationEnabled: true
        model: root.entries.length
        highlightRangeMode: GridView.ApplyRange
        preferredHighlightBegin: 0
        preferredHighlightEnd: height - cellHeight
        Keys.onUpPressed: event => {
            if (currentIndex < Math.floor(width / cellWidth))
                Ui.Focus.take(tabs.itemAt(0));
            else
                event.accepted = false;
        }
        onCurrentIndexChanged: if (root.room && currentIndex > count - 16 && root.room.more(root.shelfId, root.source))
            root.room.loadMore(root.shelfId, root.source)
        delegate: MusicCard {
            required property int index
            objectName: "musicAllCard"
            entry: root.entries[index] || ({})
            focus: GridView.isCurrentItem
            artworkWidth: root.cardWidth
            artworkHeight: root.cardHeight
            round: root.round
            playing: entry.key === root.playingKey
            onActivated: root.entryActivated(entry)
        }
    }
    Ui.UiText {
        objectName: "musicAllEmpty"
        x: Ui.Theme.safeLeft
        y: 360
        visible: !root.entries.length
        color: Ui.Theme.textSecondary
        text: qsTr("Nothing here from this source.")
    }
}
