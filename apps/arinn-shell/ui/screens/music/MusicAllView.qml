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
    property string openedTitle: ""
    property int presentationRevision: 0
    readonly property string title: {
        presentationRevision;
        return room && typeof room.shelfTitle === "function" ? room.shelfTitle(shelfId) : openedTitle;
    }
    property bool presenting: false
    property string playingKey: ""
    // The cover of the track that plays, for a playing station without its
    // own (#843, Codex review of #848).
    property string playingImage: ""
    property string source: ""
    property bool alphabetical: false
    // Bumped when a page arrives, so the list is read again.
    property int revision: 0
    readonly property var sources: room ? room.sources : []
    // A filtered source that can no longer browse loses its tab: the list
    // returns to All.
    // The tabs change only when the browsable sources or their names do, so
    // a busy flag or a status line never recreates the focused tab; when the
    // tabs do change, focus stays on the same source, or moves to All
    // (Codex review of #812).
    property var tabList: [{id: "", name: qsTr("All")}]
    function syncTabs() {
        const next = [{id: "", name: qsTr("All")}].concat(sources.filter(row => row.browsable !== false)
            .map(row => ({id: row.id, name: row.local ? qsTr("This computer") : row.name})));
        if (JSON.stringify(next) === JSON.stringify(tabList))
            return;
        let focused = null;
        for (let i = 0; i < tabs.count; ++i)
            if (tabs.itemAt(i) && tabs.itemAt(i).activeFocus)
                focused = tabList[i].id;
        tabList = next;
        if (focused !== null) {
            focusTab = focused;
            Qt.callLater(restoreTabFocus);
        }
    }
    property var focusTab: null
    function restoreTabFocus() {
        if (focusTab === null)
            return;
        const at = Math.max(0, tabList.findIndex(tab => tab.id === focusTab));
        focusTab = null;
        if (visible && enabled && activeFocus)
            Ui.Focus.take(tabs.itemAt(at));
    }
    onSourcesChanged: {
        if (source.length && !sources.some(row => row.id === source && row.browsable !== false))
            source = "";
        syncTabs();
    }
    Component.onCompleted: syncTabs()
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
        openedTitle = shelfTitle;
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
    function pageIfNeeded() {
        if (!presenting && visible && room && shelfId.length && grid.currentIndex > grid.count - 16
                && room.more(shelfId, source))
            room.loadMore(shelfId, source);
    }
    onSourceChanged: Qt.callLater(pageIfNeeded)
    onVisibleChanged: if (visible) Qt.callLater(pageIfNeeded)
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
            root.updateEntries();
            // A page may add no card of this kind (selections mix kinds):
            // page on while the end is near (Codex review of #812).
            Qt.callLater(root.pageIfNeeded);
        }
        function onPresentationChanged() {
            root.presenting = true;
            root.presentationRevision++;
            root.syncTabs();
            root.updateEntries();
            Qt.callLater(root.finishPresentation);
        }
    }
    function finishPresentation() { presenting = false; }
    function updateEntries() {
        const key = focusedEntry ? focusedEntry.key : "";
        revision++;
        // Preserve the same card when locale-aware ordering changes.
        if (key.length) {
            const at = entries.findIndex(entry => entry.key === key);
            if (at >= 0)
                grid.currentIndex = at;
        }
    }
    Ui.AmbientGlow {
        objectName: "musicAllGlow"
        anchors.fill: parent
        veilEnabled: false
        strength: 0.6
        lit: !!root.focusedEntry && grid.activeFocus
        // The artwork the card shows: a playing station without its own
        // cover shows the playing track's (#843, Codex review of #848).
        source: root.focusedEntry ? (root.focusedEntry.image
            || (root.focusedEntry.kind === "station" && root.focusedEntry.key === root.playingKey
                ? root.playingImage : "")) : ""
        anchor: grid.currentItem
    }
    Ui.UiText {
        objectName: "musicAllTitle"
        x: Ui.Theme.safeLeft
        y: 96
        role: "title"
        text: root.title
    }
    // The tabs scroll with focus: many sources or long names do not fit
    // the canvas (Codex review of #812).
    Flickable {
        id: tabArea
        objectName: "musicAllTabArea"
        // A focused pill scales up: the margins keep it unclipped.
        readonly property int pad: 16
        x: Ui.Theme.safeLeft - pad
        y: 196 - pad
        width: parent.width - Ui.Theme.safeLeft - Ui.Theme.safeRight + 2 * pad
        height: tabRow.height + 2 * pad
        leftMargin: pad
        rightMargin: pad
        topMargin: pad
        contentWidth: tabRow.width
        interactive: false
        clip: true
        Behavior on contentX { NumberAnimation { duration: Ui.Theme.focusDuration; easing.type: Easing.OutCubic } }
        function reveal(item) {
            const left = item.x, right = item.x + item.width;
            const view = width - leftMargin - rightMargin;
            if (left < contentX + leftMargin)
                contentX = Math.max(-leftMargin, left - 96 - leftMargin);
            else if (right > contentX + leftMargin + view)
                contentX = Math.min(Math.max(-leftMargin, contentWidth - view - leftMargin),
                                    right - view + 96 - leftMargin);
        }
    Row {
        id: tabRow
        objectName: "musicAllTabs"
        spacing: 16
        Repeater {
            id: tabs
            // Only sources that can browse get a tab (MH-10: connected ones).
            model: root.tabList
            Ui.ActionButton {
                required property var modelData
                required property int index
                objectName: "musicAllTab-" + index
                text: (root.source === modelData.id ? "● " : "") + modelData.name
                KeyNavigation.left: index > 0 ? tabs.itemAt(index - 1) : null
                KeyNavigation.right: index + 1 < tabs.count ? tabs.itemAt(index + 1) : byOrder
                KeyNavigation.down: grid
                onActiveFocusChanged: if (activeFocus)
                    tabArea.reveal(this)
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
            onActiveFocusChanged: if (activeFocus)
                tabArea.reveal(this)
            onActivated: { root.alphabetical = false; grid.currentIndex = 0; }
        }
        Ui.ActionButton {
            id: byTitle
            objectName: "musicAllAlphabetical"
            text: (root.alphabetical ? "● " : "") + qsTr("A–Z")
            KeyNavigation.left: byOrder
            KeyNavigation.down: grid
            onActiveFocusChanged: if (activeFocus)
                tabArea.reveal(this)
            onActivated: { root.alphabetical = true; grid.currentIndex = 0; }
        }
    }
    }
    // The grid's clip, larger than the grid by a focused card's growth, its
    // mat and rim: a focused card on the first column or row stays whole
    // (#838).
    Item {
        id: allClip
        objectName: "musicAllClip"
        // A focused card grows from its bottom edge by the content focus
        // scale, lifts, and has the mat and rim around it: the padding is
        // that, from the card's own size, so a wide station card fits too
        // (review of #848).
        readonly property real bleed: Ui.Theme.matThickness + Ui.Theme.rimThickness + 2
        readonly property real padX: Math.ceil(root.cardWidth * (Ui.Theme.contentFocusScale - 1) / 2 + bleed)
        readonly property real padTop: Math.ceil((root.cardHeight + Ui.Theme.contentCaptionHeight)
            * (Ui.Theme.contentFocusScale - 1) - Ui.Theme.contentFocusLift + bleed)
        x: Ui.Theme.safeLeft - padX
        y: 320 - padTop
        width: grid.width + 2 * padX
        height: parent.height - y
        clip: true
        GridView {
            id: grid
            objectName: "musicAllGrid"
            x: allClip.padX
            y: allClip.padTop
            width: root.width - Ui.Theme.safeLeft - Ui.Theme.safeRight + 32
            height: allClip.height - y
            cellWidth: root.cardWidth + 32
            cellHeight: root.cardHeight + Ui.Theme.contentCaptionHeight + 40
            // The clip is the parent, a little larger: a focused card grows
            // and lifts past the grid and stays whole (#838).
            clip: false
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
            onCurrentIndexChanged: root.pageIfNeeded()
            delegate: MusicCard {
                required property int index
                objectName: "musicAllCard"
                entry: root.entries[index] || ({})
                focus: GridView.isCurrentItem
                artworkWidth: root.cardWidth
                artworkHeight: root.cardHeight
                round: root.round
                playing: entry.key === root.playingKey
                playingImage: root.playingImage
                onActivated: root.entryActivated(entry)
            }
        }
        // A row scrolled up fades in the band above the grid instead of
        // showing a cut caption (#838).
        Rectangle {
            objectName: "musicAllTopFade"
            width: parent.width
            height: allClip.padTop
            z: 3
            // Never over a focused card on the top row, which lifts into it.
            visible: grid.contentY > grid.originY + 0.5 && !!grid.currentItem
                && grid.currentItem.y - grid.contentY >= grid.cellHeight / 2
            gradient: Gradient {
                GradientStop { position: 0; color: Ui.Theme.background }
                GradientStop { position: 1; color: Qt.rgba(Ui.Theme.background.r, Ui.Theme.background.g, Ui.Theme.background.b, 0) }
            }
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
