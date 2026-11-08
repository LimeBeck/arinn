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
    property bool pointerMode: Ui.InputMode.pointerActive
    property bool hoverFocusEnabled: true
    signal scrolled()
    property var room: null
    // The music catalogue, for the session that plays.
    property var catalog: null
    readonly property var shelves: room ? room.shelves : []
    // Arinn never shows an empty room (brief: first use). Once every source
    // has answered and nothing is loading, a room without shelves says why,
    // from the source rows (reviews of #812):
    // - every source unreachable: the reason, and Sources;
    // - a source that needs sign-in and none with music: Connect a source;
    // - otherwise the sources have no music yet (a local library without
    //   folders, an empty service): No music yet, and Sources.
    readonly property bool firstUse: !!room && room.settled !== false && !room.loading
        && shelves.length === 0 && rows.length > 0
    readonly property var rows: room ? room.sources : []
    // Turned-off sources (#842) say nothing about why the room is empty,
    // unless every source is off: then the room points to Sources.
    readonly property var liveRows: rows.filter(source => source.status !== "off")
    readonly property bool allOff: firstUse && liveRows.length === 0
    readonly property var unreachable: liveRows.filter(source => source.status === "unavailable")
    readonly property bool allUnreachable: firstUse && unreachable.length > 0 && unreachable.length === liveRows.length
    readonly property bool needsSignIn: firstUse && !allOff && !allUnreachable
        && liveRows.some(source => source.status === "signin")
    readonly property bool connectedEmpty: firstUse && !allOff && !allUnreachable && !needsSignIn
    readonly property bool localEmpty: connectedEmpty && liveRows.some(source => source.local === true)
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
    // The cover of the track that plays, for a playing station without its
    // own (#843).
    readonly property string playingImage: player.current && player.current.track
        ? (player.current.track.image || "") : ""
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
    function focusFind() {
        return findButton.visible && Ui.Focus.take(findButton);
    }
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
    // The band at the top of the shelves where rows scrolling up fade out
    // (#838, as Home's header band): a revealed shelf starts below it.
    readonly property real topFade: 60
    function revealShelf(shelf) {
        const top = shelf.y;
        if (top - (top > 0 ? topFade : 0) < shelfArea.contentY)
            shelfArea.contentY = Math.max(0, top - topFade);
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
        // The artwork the card shows: a playing station without its own
        // cover shows the playing track's (#843, Codex review of #848).
        source: root.focusedEntry ? (root.focusedEntry.image
            || (root.focusedEntry.kind === "station" && root.focusedEntry.key === root.playingKey
                ? root.playingImage : "")) : ""
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
            text: root.allOff ? qsTr("Music sources are off")
                : root.allUnreachable ? qsTr("Music is unavailable")
                : root.connectedEmpty ? qsTr("No music yet")
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
                if (root.allOff)
                    return qsTr("Turn a source on in Sources.");
                if (root.allUnreachable)
                    return root.unreachable[0].reason || "";
                if (root.connectedEmpty && root.liveRows.some(source => source.status === "scanning"))
                    return qsTr("Scanning the library…");
                if (root.localEmpty)
                    return qsTr("Add folders with music in Settings.");
                if (root.connectedEmpty)
                    return qsTr("Your sources have no music to show yet.");
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
            pointerMode: root.pointerMode
            hoverFocusEnabled: root.hoverFocusEnabled && !root.sheetOpen
            hoverMotionRevision: Ui.InputMode.pointerRevision
            onDpadRequested: Ui.InputMode.keyPressed()
            objectName: "musicConnectFirst"
            y: 190
            visible: root.firstUse
            primary: true
            text: root.allOff || root.allUnreachable || root.connectedEmpty ? qsTr("Sources") : qsTr("Connect a source")
            KeyNavigation.up: sourcesButton
            onVisibleChanged: if (!visible) Qt.callLater(root.settleFocus)
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
            pointerMode: root.pointerMode
            hoverFocusEnabled: root.hoverFocusEnabled && !root.sheetOpen
            hoverMotionRevision: Ui.InputMode.pointerRevision
            onDpadRequested: Ui.InputMode.keyPressed()
            objectName: "musicSources"
            text: qsTr("Sources")
            KeyNavigation.right: findButton
            Keys.onDownPressed: root.focusInitial()
            onActivated: root.openSources()
        }
        Ui.ActionButton {
            id: findButton
            pointerMode: root.pointerMode
            hoverFocusEnabled: root.hoverFocusEnabled && !root.sheetOpen
            hoverMotionRevision: Ui.InputMode.pointerRevision
            onDpadRequested: Ui.InputMode.keyPressed()
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
        onContentXChanged: if (visible) root.scrolled()
        onContentYChanged: if (visible) root.scrolled()
        objectName: "musicShelves"
        // Full width: a shelf insets its own content by the safe area, so a
        // focused first card grows past it unclipped, and cards scrolled
        // left fade out at the screen edge (#838).
        x: 0
        y: 420
        width: parent.width
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
                    pointerMode: root.pointerMode
                    hoverFocusEnabled: root.hoverFocusEnabled && !root.sheetOpen
                    onScrolled: root.scrolled()
                    required property string shelfKey
                    required property int index
                    objectName: "musicShelf-" + shelfKey
                    width: shelfColumn.width
                    room: root.room
                    shelf: root.shelves.find(item => item.id === shelfKey) || ({})
                    playingKey: root.playingKey
                    playingImage: root.playingImage
                    rowBrightness: activeFocus ? 0.82 : 0.62
                    onActiveFocusChanged: if (activeFocus) {
                        root.focusedShelf = shelfItem;
                        root.revealShelf(shelfItem);
                    }
                    // A shelf that arrived above moves the focused one down
                    // once the column lays it out: keep it in view (reviews
                    // of #812).
                    onYChanged: if (activeFocus)
                        root.revealShelf(shelfItem)
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
    // A shelf that left while it held focus hands focus on, and so does a
    // control that hid under focus (an item keeps focus as it hides): the
    // first-use action once the room fills, Find while nothing browses
    // (review of #812).
    function settleFocus() {
        const held = control => control.visible && control.activeFocus;
        if (visible && activeFocus && !sourcesSheet.visible
                && (!focusedShelf || !focusedShelf.activeFocus)
                && !held(sourcesButton) && !held(findButton) && !held(connectFirst))
            focusInitial();
    }
    onFirstUseChanged: Qt.callLater(settleFocus)
    onShelvesChanged: { scrolled(); syncShelves(); }
    Component.onCompleted: syncShelves()
    // Rows scrolling up fade out under the hero instead of a hard cut at
    // the shelves' top (#838, as Home's header band).
    Rectangle {
        objectName: "musicShelvesTopFade"
        x: 0
        y: shelfArea.y
        z: 1
        width: parent.width
        height: root.topFade
        visible: shelfArea.visible && shelfArea.contentY > 0
        gradient: Gradient {
            GradientStop { position: 0; color: Ui.Theme.background }
            GradientStop { position: 1; color: Qt.rgba(Ui.Theme.background.r, Ui.Theme.background.g, Ui.Theme.background.b, 0) }
        }
    }
    MusicSourcesSheet {
        id: sourcesSheet
        pointerMode: root.pointerMode
        hoverFocusEnabled: root.hoverFocusEnabled
        onScrolled: root.scrolled()
        objectName: "musicSourcesOverlay"
        z: 10
        room: root.room
        // The source whose session plays or is paused: turning it off asks
        // first, then stops the music (#842).
        playingSource: ["playing", "paused", "loading"].includes(root.player.playback) ? root.activeSource : ""
        catalog: root.catalog
        onClosed: Ui.Focus.take(sourcesButton.visible && !root.firstUse ? sourcesButton : connectFirst)
    }
}
