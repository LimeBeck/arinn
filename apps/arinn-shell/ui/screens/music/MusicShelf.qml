// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import "../.." as Ui

// One shelf of the Music root (brief mh-01): a label, the cards and an *All*
// tile at the end. Late entries never move focus: while the shelf holds
// focus, the cards it shows keep their places and new ones join at the end
// (MusicRoom.stable); once focus leaves, the shelf takes the room's order.
FocusScope {
    id: root
    property var room: null
    property var shelf: ({})
    property var shown: []
    // The key of the entry that plays, for the playing chip.
    property string playingKey: ""
    // The cover of the track that plays, for a playing station (#843).
    property string playingImage: ""
    property real rowBrightness: 0.62
    // The All tile ends a room shelf; search results have none.
    property bool showAll: true
    readonly property string shelfId: shelf.id || ""
    readonly property bool wide: shelfId === "stations"
    readonly property bool round: shelfId === "artists"
    readonly property real cardWidth: wide ? 448 : 240
    readonly property real cardHeight: wide ? 252 : 240
    readonly property int count: shown.length
    // The entry under focus, or null on the All tile.
    readonly property var focusedEntry: list.currentIndex >= 0 && list.currentIndex < shown.length
        ? shown[list.currentIndex] : null
    readonly property Item focusedCard: list.currentItem
    // The shelf spans the screen; its title and first card start at the
    // safe area (#838).
    readonly property real inset: Ui.Theme.safeLeft
    signal activated(var entry)
    signal allRequested(string shelfId)
    height: cardHeight + Ui.Theme.contentCaptionHeight + 72
    function sync() {
        const desired = shelf.entries || [];
        const key = focusedEntry ? focusedEntry.key : "";
        const index = list.currentIndex;
        const onAll = index >= shown.length;
        const next = activeFocus && room ? room.stable(shown, desired) : desired;
        if (JSON.stringify(next) === JSON.stringify(shown))
            return;
        shown = next;
        let at = key ? next.findIndex(entry => entry.key === key) : -1;
        // The All tile stays the All tile when cards join (review of #812);
        // a card that left hands focus to its nearest neighbour, the last
        // card when a shelf without the tile loses its last one (Codex
        // review of #819).
        if (!key && onAll && showAll)
            at = next.length;
        else if (at < 0)
            at = Math.max(0, Math.min(index, showAll ? next.length : next.length - 1));
        list.currentIndex = at;
        reveal();
    }
    // Focus on a card by entry key; false when the shelf has no such card.
    function focusKey(key) {
        const at = shown.findIndex(entry => entry.key === key);
        if (at < 0)
            return false;
        list.currentIndex = at;
        reveal();
        Ui.Focus.take(list);
        return true;
    }
    // The current card stays between the safe areas: never in the left band
    // where scrolled cards fade, nor past the right one (#838).
    // A scrolled card stops right of the fade by its focus growth, mat and
    // rim, so its left rim is never dimmed (review of #848).
    readonly property real focusBleed: Math.ceil(cardWidth * (Ui.Theme.contentFocusScale - 1) / 2
        + Ui.Theme.matThickness + Ui.Theme.rimThickness)
    function reveal() {
        list.forceLayout();
        const item = list.currentItem;
        if (!item)
            return;
        const left = item.x - list.contentX;
        const start = Math.max(root.inset, Ui.Theme.edgeFadeEnd + focusBleed);
        if (left < start)
            list.contentX = Math.max(-list.leftMargin, item.x - start);
        else if (left + item.width > list.width - Ui.Theme.safeRight)
            list.contentX = item.x + item.width - list.width + Ui.Theme.safeRight;
    }
    onShelfChanged: sync()
    onActiveFocusChanged: if (!activeFocus) Qt.callLater(sync)
    Component.onCompleted: sync()
    Ui.UiText {
        objectName: "musicShelfTitle"
        x: root.inset
        y: 0
        role: "section"
        color: root.activeFocus ? Ui.Theme.textPrimary : Ui.Theme.textSecondary
        text: root.shelf.title || ""
    }
    ListView {
        id: list
        objectName: "musicShelfCards"
        y: 56
        width: Ui.Theme.canvasWidth
        // The first card starts at the safe area; a focused one grows past it
        // and stays whole (#838).
        leftMargin: root.inset
        height: root.cardHeight + Ui.Theme.contentCaptionHeight + 16
        orientation: ListView.Horizontal
        spacing: 32
        focus: true
        clip: false
        keyNavigationEnabled: true
        highlightFollowsCurrentItem: false
        boundsBehavior: Flickable.StopAtBounds
        onCurrentIndexChanged: root.reveal()
        model: root.shown.length + (root.showAll ? 1 : 0)
        delegate: FocusScope {
            id: slot
            required property int index
            readonly property bool allTile: index >= root.shown.length
            readonly property var entry: allTile ? null : root.shown[index]
            width: allTile ? 240 : root.cardWidth
            height: root.cardHeight + Ui.Theme.contentCaptionHeight
            focus: ListView.isCurrentItem
            MusicCard {
                anchors.fill: parent
                visible: !slot.allTile
                focus: !slot.allTile
                objectName: "musicShelfCard"
                entry: slot.entry || ({})
                artworkWidth: root.cardWidth
                artworkHeight: root.cardHeight
                round: root.round
                rowBrightness: root.rowBrightness
                playing: !!slot.entry && slot.entry.key === root.playingKey
                playingImage: root.playingImage
                onActivated: root.activated(slot.entry)
            }
            // The All tile opens the whole list for this shelf (mh-11).
            Ui.Surface {
                objectName: "musicShelfAll"
                visible: slot.allTile
                focus: slot.allTile
                width: 240
                height: root.cardHeight
                shapeRole: "container"
                cornerRadius: Ui.Theme.containerRadius
                rowBrightness: root.rowBrightness
                text: qsTr("All")
                onActivated: root.allRequested(root.shelfId)
                contentItem: Item {
                    Ui.UiText {
                        objectName: "musicShelfAllLabel"
                        anchors.centerIn: parent
                        role: "control"
                        text: qsTr("All")
                        // A container: its focused plate is dark, so the label
                        // stays light; ink is for a white action pill (#838).
                        color: Ui.Theme.textPrimary
                    }
                }
            }
        }
    }
    // Cards scrolled left stay visible as they cross the screen edge and
    // fade out from x 112 to x 24, as on Home (HomeRow rowEdgeFade,
    // ADR-0136); left of x 24 the fade is opaque to the screen edge, so no
    // sliver shows there (Codex review of #848).
    Rectangle {
        objectName: "musicShelfEdgeFade"
        z: 80
        x: 0
        y: list.y
        width: Ui.Theme.edgeFadeEnd
        height: list.height
        visible: list.contentX > -list.leftMargin + 0.5
        gradient: Gradient {
            orientation: Gradient.Horizontal
            GradientStop { position: 0; color: Ui.Theme.background }
            GradientStop { position: Ui.Theme.edgeFadeStart / Ui.Theme.edgeFadeEnd; color: Ui.Theme.background }
            GradientStop { position: 1; color: Qt.rgba(Ui.Theme.background.r, Ui.Theme.background.g, Ui.Theme.background.b, 0) }
        }
    }
}
