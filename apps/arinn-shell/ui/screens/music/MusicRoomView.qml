// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtQuick.Window
import "../.." as Ui

// The Music root (brief mh-01, ADR-0142 MH-1, MH-5, MH-9): one room over every
// connected source. A hero describes the focused entry, shelves gather the
// sources' entries, and the light follows the focused artwork. The Sources
// sheet holds the housekeeping; browsing never selects a source.
FocusScope {
    id: root
    property var room: null
    // The music catalogue, for the session that plays.
    property var catalog: null
    readonly property var shelves: room ? room.shelves : []
    // No source connected once every source has answered (brief: first use).
    readonly property bool firstUse: !!room && !room.connected && !room.loading && room.settled !== false
    // Every source is there but none can be reached: say why instead of
    // asking to connect (review of #812).
    readonly property var unreachable: room ? room.sources.filter(source => source.status === "unavailable") : []
    readonly property bool allUnreachable: firstUse && unreachable.length > 0
        && unreachable.length === (room ? room.sources.length : 0)
    readonly property bool sheetOpen: sourcesSheet.visible
    // The shelf and entry under focus.
    property Item focusedShelf: null
    readonly property var focusedEntry: focusedShelf ? focusedShelf.focusedEntry : null
    readonly property var player: catalog ? (catalog.activeState || ({})) : ({})
    readonly property string activeSource: catalog ? (catalog.activeSourceId || "") : ""
    // The entry that plays: the station of a continuous session, else the
    // current track. Albums and playlists carry no reference to it.
    readonly property string playingKey: {
        if (!activeSource.length || !["playing", "paused", "loading"].includes(player.playback))
            return "";
        const station = player.station ? player.station.id : "";
        const track = player.current && player.current.track ? player.current.track.id : "";
        return activeSource + "\n" + (station || track || "");
    }
    signal entryActivated(var entry)
    signal allRequested(string shelfId)
    signal findRequested()
    width: Ui.Theme.canvasWidth
    height: Ui.Theme.canvasHeight

    function kindName(kind) {
        return kind === "station" ? qsTr("Station")
            : kind === "album" ? qsTr("Album")
            : kind === "playlist" ? qsTr("Playlist")
            : kind === "artist" ? qsTr("Artist")
            : qsTr("Track");
    }
    function sourceName(entry) {
        return entry.local === true ? qsTr("This computer") : (entry.providerName || "");
    }
    // Focus the card of an entry key, else the first shelf, else the header.
    function focusEntry(key) {
        for (let i = 0; i < shelfRepeater.count; ++i) {
            const shelf = shelfRepeater.itemAt(i);
            if (shelf && key && shelf.focusKey(key)) {
                revealShelf(shelf);
                return true;
            }
        }
        return false;
    }
    // Focus a shelf where it was left (its All tile after the All list).
    function focusShelf(id) {
        for (let i = 0; i < shelfRepeater.count; ++i) {
            const shelf = shelfRepeater.itemAt(i);
            if (shelf && id && shelf.shelfId === id) {
                Ui.Focus.take(shelf);
                revealShelf(shelf);
                return true;
            }
        }
        return false;
    }
    function focusInitial() {
        if (firstUse)
            return Ui.Focus.take(connectFirst);
        const shelf = shelfRepeater.count ? shelfRepeater.itemAt(0) : null;
        if (shelf) {
            Ui.Focus.take(shelf);
            revealShelf(shelf);
            return true;
        }
        return Ui.Focus.take(sourcesButton);
    }
    function revealShelf(shelf) {
        const top = shelf.y;
        if (top < shelfArea.contentY)
            shelfArea.contentY = top;
        else if (top + shelf.height > shelfArea.contentY + shelfArea.height)
            shelfArea.contentY = Math.max(0, top + shelf.height - shelfArea.height);
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
    function openSources() {
        sourcesSheet.open();
    }

    Ui.AmbientGlow {
        objectName: "musicRoomGlow"
        anchors.fill: parent
        veilEnabled: false
        strength: 0.6
        lit: !!root.focusedEntry && !root.sheetOpen
        source: root.focusedEntry ? (root.focusedEntry.image || "") : ""
        anchor: root.focusedShelf ? root.focusedShelf.focusedCard : null
    }
    // The top bar as on Home: the lockup; the status area is the shell's.
    Ui.BrandLockup {
        objectName: "musicLockup"
        x: Ui.Theme.safeLeft
        y: 50
        height: 40
        width: implicitWidth
        markSize: 40
        textRole: "section"
    }
    // The hero describes the focused entry and never repeats the shelf label.
    Item {
        objectName: "musicHero"
        x: Ui.Theme.safeLeft
        y: 150
        width: 1200
        height: 240
        Row {
            id: eyebrow
            spacing: 12
            Rectangle {
                objectName: "musicHeroPlaying"
                anchors.verticalCenter: parent.verticalCenter
                width: 12
                height: 12
                radius: 6
                color: Ui.Theme.ember
                visible: !!root.focusedEntry && root.focusedEntry.key === root.playingKey
            }
            Ui.UiText {
                objectName: "musicHeroEyebrow"
                role: "eyebrow"
                color: Ui.Theme.textSecondary
                textFormat: Text.PlainText
                text: root.firstUse ? qsTr("Music")
                    : !root.focusedEntry ? ""
                    : root.focusedEntry.key === root.playingKey
                        ? qsTr("%1 · playing").arg(root.kindName(root.focusedEntry.kind))
                        : root.kindName(root.focusedEntry.kind)
            }
        }
        Ui.UiText {
            id: heroTitle
            objectName: "musicHeroTitle"
            y: 44
            width: parent.width
            role: "hero"
            fitSteps: [[96, 1], [68, 2]]
            textFormat: Text.PlainText
            text: root.allUnreachable ? qsTr("Music is unavailable")
                : root.firstUse ? qsTr("Connect a source")
                : root.focusedEntry ? (root.focusedEntry.title || "") : qsTr("Music")
        }
        Ui.UiText {
            objectName: "musicHeroMeta"
            y: 44 + heroTitle.height + 8
            width: parent.width
            role: "meta"
            font.pixelSize: 28
            color: Ui.Theme.textSecondary
            textFormat: Text.PlainText
            maximumLineCount: 1
            elide: Text.ElideRight
            text: {
                if (root.allUnreachable)
                    return root.unreachable[0].reason || "";
                if (root.firstUse)
                    return qsTr("Connect a music service or add folders with music in Settings.");
                const entry = root.focusedEntry;
                if (!entry)
                    return "";
                const parts = [];
                if (entry.artist && entry.kind !== "artist")
                    parts.push(entry.artist);
                if (entry.album && entry.kind === "track")
                    parts.push(entry.album);
                parts.push(root.sourceName(entry));
                return parts.filter(part => part.length).join(" · ");
            }
        }
        // First use offers the one next step.
        Ui.ActionButton {
            id: connectFirst
            objectName: "musicConnectFirst"
            y: 190
            visible: root.firstUse
            primary: true
            text: root.allUnreachable ? qsTr("Sources") : qsTr("Connect a source")
            onActivated: root.openSources()
        }
    }
    // Header controls on the hero line (MH-5).
    Row {
        objectName: "musicHeader"
        anchors.right: parent.right
        anchors.rightMargin: Ui.Theme.safeRight
        y: 150
        spacing: 16
        Ui.ActionButton {
            id: sourcesButton
            objectName: "musicSources"
            text: qsTr("Sources")
            KeyNavigation.right: findButton
            Keys.onDownPressed: root.focusInitial()
            onActivated: root.openSources()
        }
        Ui.ActionButton {
            id: findButton
            objectName: "musicFind"
            text: qsTr("Find")
            visible: !!root.room && root.room.connected
            KeyNavigation.left: sourcesButton
            Keys.onDownPressed: root.focusInitial()
            onActivated: root.findRequested()
        }
    }
    // Loading: artwork-shaped plates, as in the Library (M19).
    Row {
        objectName: "musicRoomLoading"
        x: Ui.Theme.safeLeft
        y: 476
        spacing: 32
        visible: !!root.room && root.room.loading && !root.shelves.length
        Repeater {
            model: 6
            Ui.Skeleton {
                width: 240
                height: 240
                radius: Ui.Theme.stillRadius
                color: Qt.rgba(1, 1, 1, 0.04)
            }
        }
    }
    Flickable {
        id: shelfArea
        objectName: "musicShelves"
        x: Ui.Theme.safeLeft
        y: 420
        width: parent.width - Ui.Theme.safeLeft
        height: parent.height - y
        contentHeight: shelfColumn.height + 120
        interactive: false
        clip: true
        visible: !root.firstUse
        Behavior on contentY { NumberAnimation { duration: Ui.Theme.focusDuration; easing.type: Easing.OutCubic } }
        Column {
            id: shelfColumn
            width: parent.width
            spacing: 24
            Repeater {
                id: shelfRepeater
                model: shelfModel
                MusicShelf {
                    id: shelfItem
                    required property string shelfKey
                    required property int index
                    objectName: "musicShelf-" + shelfKey
                    width: shelfColumn.width
                    room: root.room
                    shelf: root.shelves.find(item => item.id === shelfKey) || ({})
                    playingKey: root.playingKey
                    rowBrightness: activeFocus ? 0.82 : 0.62
                    onActiveFocusChanged: if (activeFocus) {
                        root.focusedShelf = shelfItem;
                        root.revealShelf(shelfItem);
                    }
                    Keys.onUpPressed: event => {
                        const previous = shelfRepeater.itemAt(index - 1);
                        if (previous) {
                            Ui.Focus.take(previous);
                            root.revealShelf(previous);
                        } else {
                            Ui.Focus.take(sourcesButton);
                        }
                    }
                    Keys.onDownPressed: event => {
                        const next = shelfRepeater.itemAt(index + 1);
                        if (next) {
                            Ui.Focus.take(next);
                            root.revealShelf(next);
                        }
                    }
                    onActivated: entry => root.entryActivated(entry)
                    onAllRequested: shelfId => root.allRequested(shelfId)
                }
            }
        }
    }
    // Shelves are keyed by id, so a shelf keeps its cards and its focus
    // when the room's shelves change; only a shelf that comes or goes is
    // created or removed.
    ListModel { id: shelfModel }
    function syncShelves() {
        const ids = shelves.map(shelf => shelf.id);
        for (let i = shelfModel.count - 1; i >= 0; --i)
            if (!ids.includes(shelfModel.get(i).shelfKey))
                shelfModel.remove(i);
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
        Qt.callLater(settleFocus);
    }
    // A shelf that left while it held focus hands focus on.
    function settleFocus() {
        if (visible && activeFocus && !sourcesSheet.visible
                && (!focusedShelf || !focusedShelf.activeFocus)
                && !sourcesButton.activeFocus && !findButton.activeFocus && !connectFirst.activeFocus)
            focusInitial();
    }
    onShelvesChanged: syncShelves()
    Component.onCompleted: syncShelves()
    MusicSourcesSheet {
        id: sourcesSheet
        objectName: "musicSourcesOverlay"
        z: 10
        room: root.room
        onClosed: Ui.Focus.take(sourcesButton.visible && !root.firstUse ? sourcesButton : connectFirst)
    }
}
