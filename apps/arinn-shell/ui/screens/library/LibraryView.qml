// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import QtQuick.Window
import "../.." as Ui

// Catalogue presentation. Properties are presentation data; signals are
// intents. This screen does not call a daemon, a bus or a provider.
FocusScope {
    id: root
    width: 1920
    height: 1080
    Accessible.name: qsTr("Library")

    property string libraryState: "ready" // ready, empty, loading, error, unavailable
    property string statusTitle: ""
    property string statusDescription: ""
    property string statusActionText: ""
    property var tabs: []
    property string currentTabId: ""
    property var entries: []
    property int columns: 0
    property string detailEntryId: ""
    property string operationState: ""
    property string launchingId: ""
    // The shell's input mode (ADR-0095); tests may assign it directly.
    property bool pointerMode: Ui.InputMode.pointerActive
    readonly property bool operationPending: operationState === "pending" || operationState === "queued" || operationState === "running"
    readonly property bool operationFailed: operationState !== "" && operationState !== "succeeded" && !operationPending
    readonly property string operationMessage: {
        if (operationState === "Unsupported")
            return qsTr("That action is not available.");
        if (operationState === "Timeout")
            return qsTr("The library did not answer in time.");
        if (operationState === "Unavailable")
            return qsTr("This title is not available.");
        if (operationState === "InvalidArgument")
            return qsTr("That title cannot be started.");
        if (operationState === "NotFound")
            return qsTr("That title is not in the library.");
        if (operationState === "Conflict")
            return qsTr("The library changed. Try again.");
        return operationState;
    }
    // Suppresses hover focus while the grid is scrolling. Local to this screen.
    readonly property int hoverDelayMs: 500
    readonly property bool hoverFocusEnabled: !hoverDelay.running
    readonly property bool detailOpen: detailEntryId.length > 0
    readonly property int posterWidth: 210
    readonly property int posterHeight: 315
    readonly property int gridGapX: 40
    readonly property int gridGapY: 48
    readonly property int marginLeft: 120
    readonly property int marginRight: 96
    // The handoff grid is seven 210-wide posters. Tests may set a smaller count.
    readonly property int columnCount: columns > 0 ? columns : 7
    readonly property var visibleEntries: {
        const tab = currentTabId;
        const source = entries || [];
        const out = [];
        for (let i = 0; i < source.length; ++i) {
            const entry = source[i];
            if (!tab.length || !entry.tabId || entry.tabId === tab)
                out.push(entry);
        }
        return out;
    }
    readonly property var detailEntry: {
        const id = detailEntryId;
        const source = entries || [];
        for (let i = 0; i < source.length; ++i)
            if (source[i].id === id)
                return source[i];
        return null;
    }
    readonly property bool launchingDetail: {
        if (!operationPending || !launchingId || !detailEntry)
            return false;
        if (launchingId === detailEntryId)
            return true;
        const episodes = detailEntry.episodes || [];
        for (let i = 0; i < episodes.length; ++i)
            if (episodes[i] && episodes[i].id === launchingId)
                return true;
        return false;
    }
    readonly property bool showGrid: libraryState === "ready" && visibleEntries.length > 0 && !detailOpen
    readonly property bool showStatus: !detailOpen && (libraryState === "empty" || libraryState === "error"
            || libraryState === "unavailable" || (libraryState === "ready" && visibleEntries.length === 0))
    readonly property string statusKind: libraryState === "ready" ? "empty" : libraryState
    readonly property string resolvedStatusTitle: {
        if (statusTitle.length)
            return statusTitle;
        if (statusKind === "empty")
            return qsTr("Nothing in the library yet");
        if (statusKind === "error")
            return qsTr("The library could not be loaded");
        if (statusKind === "unavailable")
            return qsTr("Library is unavailable");
        return "";
    }
    readonly property string resolvedStatusDescription: {
        if (statusDescription.length)
            return statusDescription;
        if (statusKind === "empty")
            return qsTr("Films and series appear here when a folder has been scanned.");
        if (statusKind === "error")
            return qsTr("Try again when you are ready.");
        if (statusKind === "unavailable")
            return qsTr("The catalogue is not connected.");
        return "";
    }
    readonly property string resolvedStatusAction: {
        if (statusActionText.length)
            return statusActionText;
        if (statusKind === "loading")
            return "";
        return qsTr("Try again");
    }
    property string focusedPosterId: ""
    property int focusedPosterIndex: 0
    property int episodeIndex: 0
    property int seasonIndex: 0
    property string currentSeasonId: ""
    // Survives delegate destruction so a removed poster can restore a neighbour.
    property string focusPlace: "tab"
    readonly property int skeletonSlots: 14
    readonly property var detailSeasons: detailEntry && detailEntry.seasons ? detailEntry.seasons : []
    readonly property var detailEpisodes: {
        const entry = detailEntry;
        if (!entry || !entry.episodes)
            return [];
        const season = currentSeasonId;
        if (!season.length)
            return entry.episodes;
        const out = [];
        for (let i = 0; i < entry.episodes.length; ++i) {
            const episode = entry.episodes[i];
            if (!episode.seasonId || episode.seasonId === season)
                out.push(episode);
        }
        return out;
    }

    signal tabRequested(string tabId)
    signal entryRequested(string entryId)
    signal playRequested(string entryId)
    signal episodeRequested(string entryId, string episodeId)
    signal removeRequested(string entryId)
    signal retryRequested()
    signal backRequested()
    signal homeRequested()

    function adoptTab() {
        if (!tabs || !tabs.length) {
            if (currentTabId.length)
                currentTabId = "";
            return;
        }
        for (let i = 0; i < tabs.length; ++i)
            if (tabs[i].id === currentTabId)
                return;
        currentTabId = tabs[0].id;
    }
    function standInColor(id) {
        let hash = 0;
        const text = String(id || "");
        for (let i = 0; i < text.length; ++i)
            hash = (hash * 33 + text.charCodeAt(i)) >>> 0;
        return Qt.hsla((hash % 360) / 360, 0.42, 0.34, 1);
    }
    // The handoff's `movie` icon on a missing-artwork plate (§05 Library).
    component FilmGlyph: Item {
        width: 44
        height: 44
        Rectangle {
            anchors.fill: parent
            radius: 6
            color: "transparent"
            border.width: 2
            border.color: Ui.Theme.textHint
        }
        Column {
            anchors.left: parent.left
            anchors.leftMargin: 2
            anchors.verticalCenter: parent.verticalCenter
            spacing: 5
            Repeater {
                model: 3
                Rectangle { width: 3; height: 6; color: Ui.Theme.textHint }
            }
        }
        Column {
            anchors.right: parent.right
            anchors.rightMargin: 2
            anchors.verticalCenter: parent.verticalCenter
            spacing: 5
            Repeater {
                model: 3
                Rectangle { width: 3; height: 6; color: Ui.Theme.textHint }
            }
        }
        Rectangle {
            anchors.centerIn: parent
            width: 14
            height: 14
            radius: 7
            color: "transparent"
            border.width: 2
            border.color: Ui.Theme.textHint
        }
    }
    function entryById(id) {
        const source = entries || [];
        for (let i = 0; i < source.length; ++i)
            if (source[i].id === id)
                return source[i];
        return null;
    }
    function posterEnabled(index) {
        const entry = visibleEntries[index];
        return !!entry && entry.enabled !== false;
    }
    function focusTab(index) {
        if (!tabs || index < 0 || index >= tabs.length)
            return false;
        const item = tabRepeater.itemAt(index);
        if (!item)
            return false;
        focusPlace = "tab";
        Ui.Focus.take(item);
        const id = tabs[index].id;
        if (currentTabId !== id) {
            currentTabId = id;
            tabRequested(id);
        }
        return true;
    }
    function focusCurrentTab() {
        if (!tabs)
            return false;
        let index = 0;
        for (let i = 0; i < tabs.length; ++i)
            if (tabs[i].id === currentTabId)
                index = i;
        const item = tabRepeater.itemAt(index);
        if (!item)
            return false;
        focusPlace = "tab";
        Ui.Focus.take(item);
        return true;
    }
    function focusPosterIndex(index) {
        const count = visibleEntries.length;
        if (!count)
            return false;
        let next = Math.max(0, Math.min(index, count - 1));
        if (!posterEnabled(next)) {
            let found = -1;
            for (let distance = 1; distance < count && found < 0; ++distance) {
                if (next - distance >= 0 && posterEnabled(next - distance))
                    found = next - distance;
                else if (next + distance < count && posterEnabled(next + distance))
                    found = next + distance;
            }
            if (found < 0)
                return false;
            next = found;
        }
        const cell = posterRepeater.itemAt(next);
        if (!cell)
            return false;
        focusedPosterIndex = next;
        focusedPosterId = visibleEntries[next].id;
        cell.focusPoster();
        revealCell(cell);
        return true;
    }
    function focusPosterById(id) {
        const index = visibleEntries.findIndex(entry => entry.id === id);
        return focusPosterIndex(index >= 0 ? index : Math.max(0, focusedPosterIndex - 1));
    }
    function revealCell(cell) {
        const top = cell.y + posterGrid.y;
        const bottom = top + cell.height;
        if (top < scroller.contentY)
            scroller.contentY = top;
        else if (bottom > scroller.contentY + scroller.height)
            scroller.contentY = Math.max(0, bottom - scroller.height);
    }
    function focusStatus() {
        if (!libraryStatus.visible)
            return false;
        focusPlace = "status";
        Ui.Focus.take(libraryStatus);
        return true;
    }
    function focusDetailAction() {
        const entry = detailEntry;
        const button = entry && entry.missing ? detailRemove : detailPlay;
        if (button && button.visible)
            Ui.Focus.take(button);
    }
    function focusSeason(index) {
        if (index < 0 || index >= detailSeasons.length)
            return false;
        const item = seasonRepeater.itemAt(index);
        if (!item)
            return false;
        seasonIndex = index;
        const season = detailSeasons[index];
        if (season && currentSeasonId !== season.id) {
            currentSeasonId = season.id;
            episodeIndex = 0;
            episodeFlick.contentX = 0;
        }
        Ui.Focus.take(item);
        return true;
    }
    function focusEpisode(index) {
        const episodes = detailEpisodes;
        if (index < 0 || index >= episodes.length)
            return false;
        const item = episodeRepeater.itemAt(index);
        if (!item)
            return false;
        episodeIndex = index;
        item.focusStill();
        return true;
    }
    function focusInitial() {
        if (detailOpen) {
            focusDetailAction();
            return;
        }
        if (tabs && tabs.length) {
            focusCurrentTab();
            return;
        }
        if (showGrid) {
            focusPosterIndex(0);
            return;
        }
        focusStatus();
    }
    function openEntry(id) {
        const entry = entryById(id);
        if (!entry || entry.enabled === false || entry.busy)
            return;
        focusedPosterId = id;
        detailEntryId = id;
        episodeIndex = 0;
        seasonIndex = 0;
        const seasons = entry.seasons || [];
        currentSeasonId = seasons.length ? seasons[0].id : "";
        entryRequested(id);
        Qt.callLater(focusDetailAction);
    }
    function closeDetail() {
        const id = detailEntryId;
        detailEntryId = "";
        Qt.callLater(() => focusPosterById(id));
    }
    function movePoster(key) {
        const count = visibleEntries.length;
        if (!count) {
            if (key === Qt.Key_Up)
                focusCurrentTab();
            return;
        }
        let index = visibleEntries.findIndex(entry => entry.id === focusedPosterId);
        if (index < 0)
            index = Math.max(0, Math.min(focusedPosterIndex, count - 1));
        const cols = columnCount;
        const row = Math.floor(index / cols);
        const col = index % cols;
        const rowStart = row * cols;
        const rowEnd = Math.min(rowStart + cols, count) - 1;
        if (key === Qt.Key_Left || key === Qt.Key_Right) {
            const step = key === Qt.Key_Left ? -1 : 1;
            let next = index + step;
            while (next >= rowStart && next <= rowEnd && !posterEnabled(next))
                next += step;
            if (next >= rowStart && next <= rowEnd)
                focusPosterIndex(next);
            return;
        }
        if (key === Qt.Key_Up) {
            if (row === 0) {
                focusCurrentTab();
                return;
            }
            focusPosterIndex((row - 1) * cols + col);
            return;
        }
        const below = (row + 1) * cols + col;
        if (below < count || Math.floor((count - 1) / cols) > row)
            focusPosterIndex(Math.min(below, count - 1));
    }
    function moveDetail(key) {
        const episodes = detailEpisodes;
        const seasons = detailSeasons;
        const focused = Window.activeFocusItem;
        const name = focused ? String(focused.objectName) : "";
        const onEpisode = name.indexOf("episode-") === 0 && name.indexOf("episode-missing-") !== 0;
        const onSeason = name.indexOf("season-") === 0;
        if (key === Qt.Key_Down && !onEpisode && !onSeason && seasons.length) {
            focusSeason(Math.min(seasonIndex, seasons.length - 1));
            return;
        }
        if (key === Qt.Key_Down && !onEpisode && episodes.length) {
            focusEpisode(Math.min(episodeIndex, episodes.length - 1));
            return;
        }
        if (key === Qt.Key_Up && onEpisode) {
            if (seasons.length)
                focusSeason(Math.min(seasonIndex, seasons.length - 1));
            else
                focusDetailAction();
            return;
        }
        if (key === Qt.Key_Up && onSeason) {
            focusDetailAction();
            return;
        }
        if (onEpisode && (key === Qt.Key_Left || key === Qt.Key_Right)) {
            const step = key === Qt.Key_Right ? 1 : -1;
            const next = episodeIndex + step;
            if (next >= 0 && next < episodes.length)
                focusEpisode(next);
            return;
        }
        if (onSeason && (key === Qt.Key_Left || key === Qt.Key_Right)) {
            const step = key === Qt.Key_Right ? 1 : -1;
            const next = seasonIndex + step;
            if (next >= 0 && next < seasons.length)
                focusSeason(next);
        }
    }
    function back() {
        if (detailOpen) {
            closeDetail();
            return;
        }
        backRequested();
    }
    function reconcilePoster() {
        if (!visible || !enabled || detailOpen || !showGrid || focusPlace !== "poster")
            return;
        if (!visibleEntries.length) {
            focusPlace = "tab";
            focusInitial();
            return;
        }
        const found = visibleEntries.findIndex(entry => entry.id === focusedPosterId);
        const index = found >= 0 ? found : Math.max(0, focusedPosterIndex - 1);
        focusPosterIndex(index);
    }

    function focusTargetVisible(item) {
        let current = item;
        while (current && current !== root) {
            if (!current.visible)
                return false;
            current = current.parent;
        }
        return !!item;
    }
    // A hidden library must not take focus from the screen on display.
    function refocusAfterState() {
        if (!visible || !enabled)
            return;
        const focused = Window.activeFocusItem;
        if (!root.activeFocus || !focusTargetVisible(focused))
            focusInitial();
    }
    onLibraryStateChanged: {
        if (libraryState !== "ready")
            detailEntryId = "";
        Qt.callLater(refocusAfterState);
    }
    onShowStatusChanged: Qt.callLater(refocusAfterState)
    onShowGridChanged: Qt.callLater(refocusAfterState)
    onEntriesChanged: Qt.callLater(reconcilePoster)
    onVisibleEntriesChanged: Qt.callLater(reconcilePoster)
    onTabsChanged: adoptTab()
    Component.onCompleted: {
        adoptTab();
        if (visible && enabled)
            Qt.callLater(focusInitial);
    }
    // Main.qml binds enabled to visible, and that binding may update after
    // this handler runs, so check both when the deferred call fires.
    onVisibleChanged: if (visible) Qt.callLater(() => { if (visible && enabled) focusInitial(); })

    Timer {
        id: hoverDelay
        interval: root.hoverDelayMs
    }
    HoverHandler {
        // Hover re-evaluated under a resting pointer (scrolling, new rows)
        // keeps the scene position; only a changed one is movement.
        property point previousPosition: Qt.point(-1, -1)
        onPointChanged: {
            const next = point.scenePosition;
            if (next.x === previousPosition.x && next.y === previousPosition.y)
                return;
            previousPosition = next;
            Ui.InputMode.pointerMoved(next);
        }
        cursorShape: root.pointerMode ? Qt.ArrowCursor : Qt.BlankCursor
    }
    Keys.onPressed: event => {
        if ([Qt.Key_Left, Qt.Key_Right, Qt.Key_Up, Qt.Key_Down].includes(event.key))
            Ui.InputMode.keyPressed();
        if (event.key === Qt.Key_Tab || event.key === Qt.Key_Backtab) {
            event.accepted = true;
            return;
        }
        if (event.key === Qt.Key_Home) {
            const restoreId = detailEntryId;
            if (detailOpen)
                detailEntryId = "";
            homeRequested();
            if (restoreId.length)
                Qt.callLater(() => focusPosterById(restoreId));
            event.accepted = true;
            return;
        }
        if (event.key === Qt.Key_Escape || event.key === Qt.Key_Backspace || event.key === Qt.Key_Back) {
            // One step per press: a held key's repeats must not walk on
            // through Detail, the library and Home into navigation (#246).
            if (!event.isAutoRepeat)
                back();
            event.accepted = true;
            return;
        }
        if (![Qt.Key_Left, Qt.Key_Right, Qt.Key_Up, Qt.Key_Down].includes(event.key))
            return;
        event.accepted = true;
        if (detailOpen) {
            moveDetail(event.key);
            return;
        }
        const focused = Window.activeFocusItem;
        const onTab = focused && String(focused.objectName).indexOf("tab-") === 0;
        const onPoster = focused && String(focused.objectName).indexOf("poster-") === 0;
        if (onTab) {
            let index = 0;
            for (let i = 0; i < tabs.length; ++i)
                if (tabs[i].id === currentTabId)
                    index = i;
            if (event.key === Qt.Key_Left || event.key === Qt.Key_Right) {
                const step = event.key === Qt.Key_Left ? -1 : 1;
                const next = index + step;
                if (next >= 0 && next < tabs.length)
                    focusTab(next);
            } else if (event.key === Qt.Key_Down) {
                if (!focusPosterIndex(0))
                    focusStatus();
            }
            return;
        }
        if (onPoster) {
            movePoster(event.key);
            return;
        }
        if (event.key === Qt.Key_Up)
            focusCurrentTab();
    }

    Rectangle {
        anchors.fill: parent
        color: Ui.Theme.background
    }
    Ui.UiText {
        id: heading
        objectName: "libraryHeading"
        x: root.marginLeft
        y: 96
        text: qsTr("Library")
        role: "title"
    }
    Row {
        id: tabRow
        x: root.marginLeft
        y: 190
        spacing: 12
        visible: root.tabs && root.tabs.length > 0
        Repeater {
            id: tabRepeater
            model: root.tabs
            delegate: Ui.Surface {
                id: tabButton
                required property var modelData
                required property int index
                objectName: "tab-" + modelData.id
                implicitWidth: Math.max(120, tabLabel.implicitWidth + 56)
                height: 64
                implicitHeight: 64
                cornerRadius: 32
                focusScale: 1.02
                text: modelData.title || ""
                restingColor: modelData.id === root.currentTabId ? Qt.rgba(1, 1, 1, 0.16) : Ui.Theme.plate
                pointerMode: root.pointerMode
                hoverFocusEnabled: root.hoverFocusEnabled
                onDpadRequested: Ui.InputMode.keyPressed()
                onActivated: root.focusTab(index)
                contentItem: Ui.UiText {
                    id: tabLabel
                    objectName: "tabLabel-" + modelData.id
                    text: tabButton.text
                    font.pixelSize: 28
                    font.weight: 600
                    horizontalAlignment: Text.AlignHCenter
                    verticalAlignment: Text.AlignVCenter
                }
                Rectangle {
                    objectName: "tabUnderline-" + modelData.id
                    anchors.left: parent.left
                    anchors.right: parent.right
                    anchors.leftMargin: 28
                    anchors.rightMargin: 28
                    anchors.bottom: parent.bottom
                    anchors.bottomMargin: 7
                    height: 4
                    radius: 2
                    color: Ui.Theme.ember
                    visible: modelData.id === root.currentTabId
                }
            }
        }
    }

    Item {
        id: libraryStage
        objectName: "libraryStage"
        y: 282
        width: parent.width
        height: parent.height - y
        clip: true

        Ui.StatusPane {
            id: libraryStatus
            objectName: "libraryStatus"
            anchors.horizontalCenter: parent.horizontalCenter
            y: 48
            width: Math.min(820, parent.width)
            visible: root.showStatus
            kind: root.statusKind
            title: root.resolvedStatusTitle
            description: root.resolvedStatusDescription
            actionText: root.resolvedStatusAction
            pointerMode: root.pointerMode
            hoverFocusEnabled: root.hoverFocusEnabled
            onDpadRequested: Ui.InputMode.keyPressed()
            onActionRequested: root.retryRequested()
        }

        Grid {
            id: skeletonGrid
            objectName: "librarySkeletons"
            x: root.marginLeft
            y: 28
            visible: root.libraryState === "loading" && !root.detailOpen
            columns: root.columnCount
            columnSpacing: root.gridGapX
            rowSpacing: root.gridGapY
            Repeater {
                model: root.skeletonSlots
                delegate: Item {
                    required property int index
                    width: root.posterWidth
                    height: root.posterHeight + 16 + 24 + 16 + 20
                    Ui.Skeleton {
                        objectName: "skeleton-" + index
                        width: root.posterWidth
                        height: root.posterHeight
                        radius: 20
                    }
                    Rectangle {
                        y: root.posterHeight + 16
                        width: parent.width * 0.8
                        height: 24
                        radius: 8
                        color: Ui.Theme.plate
                    }
                    Rectangle {
                        y: root.posterHeight + 16 + 24 + 16
                        width: parent.width * 0.36
                        height: 20
                        radius: 8
                        color: Ui.Theme.plate
                    }
                }
            }
        }

        Flickable {
            id: scroller
            objectName: "libraryGrid"
            anchors.fill: parent
            visible: root.showGrid
            clip: true
            contentWidth: posterGrid.x + posterGrid.width
            contentHeight: posterGrid.y + posterGrid.height
            boundsBehavior: Flickable.StopAtBounds
            flickableDirection: Flickable.VerticalFlick
            onContentYChanged: hoverDelay.restart()
            Grid {
                id: posterGrid
                x: root.marginLeft
                y: 28
                width: root.columnCount * root.posterWidth + Math.max(0, root.columnCount - 1) * root.gridGapX
                columns: root.columnCount
                columnSpacing: root.gridGapX
                rowSpacing: root.gridGapY
                Repeater {
                    id: posterRepeater
                    model: root.visibleEntries
                    delegate: Item {
                        id: cell
                        required property var modelData
                        required property int index
                        readonly property bool posterHasSource: !!(modelData.artwork && String(modelData.artwork).length)
                        width: root.posterWidth
                        height: 427
                        z: poster.activeFocus ? 2 : 0
                        function focusPoster() {
                            Ui.Focus.take(poster);
                        }
                        Ui.Surface {
                            id: poster
                            objectName: "poster-" + modelData.id
                            width: root.posterWidth
                            height: root.posterHeight
                            cornerRadius: 20
                            focusScale: 1.06
                            text: modelData.title || ""
                            enabled: modelData.enabled !== false
                            busy: !!modelData.busy || (root.operationPending && root.launchingId === modelData.id)
                            restingColor: modelData.noArt || !cell.posterHasSource ? Qt.rgba(0.21, 0.21, 0.23, 1) : root.standInColor(modelData.id)
                            focusedColor: restingColor
                            pointerMode: root.pointerMode
                            hoverFocusEnabled: root.hoverFocusEnabled
                            onDpadRequested: Ui.InputMode.keyPressed()
                            onActiveFocusChanged: {
                                if (activeFocus) {
                                    root.focusedPosterId = modelData.id;
                                    root.focusedPosterIndex = index;
                                    root.focusPlace = "poster";
                                }
                            }
                            onActivated: root.openEntry(modelData.id)
                            contentItem: Item {
                                Rectangle {
                                    anchors.fill: parent
                                    radius: 20
                                    visible: posterImage.status !== Image.Ready
                                    gradient: Gradient {
                                        GradientStop { position: 0.0; color: Qt.rgba(0.29, 0.29, 0.31, 1) }
                                        GradientStop { position: 1.0; color: Qt.rgba(0.21, 0.21, 0.23, 1) }
                                    }
                                }
                                Image {
                                    id: posterImage
                                    objectName: "posterImage-" + modelData.id
                                    anchors.fill: parent
                                    fillMode: Image.PreserveAspectCrop
                                    asynchronous: true
                                    cache: false
                                    source: cell.posterHasSource ? modelData.artwork : ""
                                    visible: status === Image.Ready
                                }
                                Item {
                                    objectName: "posterPlate-" + modelData.id
                                    anchors.fill: parent
                                    anchors.leftMargin: 20
                                    anchors.rightMargin: 20
                                    anchors.topMargin: 22
                                    anchors.bottomMargin: 22
                                    visible: posterImage.status !== Image.Ready
                                    FilmGlyph {
                                        objectName: "posterGlyph-" + modelData.id
                                    }
                                    Ui.UiText {
                                        objectName: "posterArtTitle-" + modelData.id
                                        anchors.left: parent.left
                                        anchors.right: parent.right
                                        anchors.bottom: parent.bottom
                                        text: modelData.title || ""
                                        font.pixelSize: 26
                                        font.weight: 600
                                        maximumLineCount: 3
                                        elide: Text.ElideRight
                                    }
                                }
                                Rectangle {
                                    objectName: "posterKind-" + modelData.id
                                    visible: modelData.kind === "series"
                                    anchors.right: parent.right
                                    anchors.top: parent.top
                                    anchors.margins: 12
                                    height: 36
                                    radius: 18
                                    width: kindLabel.implicitWidth + 24
                                    color: Qt.rgba(8 / 255, 9 / 255, 11 / 255, 0.6)
                                    Ui.UiText {
                                        id: kindLabel
                                        objectName: "posterKindLabel-" + modelData.id
                                        anchors.centerIn: parent
                                        text: qsTr("Series")
                                        font.pixelSize: 20
                                        font.weight: 600
                                    }
                                }
                            }
                            Rectangle {
                                objectName: "missing-" + modelData.id
                                visible: !!modelData.missing
                                z: 40
                                x: 14
                                y: parent.height - height - 14
                                height: 40
                                radius: 20
                                width: Math.min(parent.width - 28, missingRow.implicitWidth + 20)
                                color: Qt.rgba(8 / 255, 9 / 255, 11 / 255, 0.72)
                                Row {
                                    id: missingRow
                                    anchors.centerIn: parent
                                    spacing: 6
                                    Text {
                                        anchors.verticalCenter: parent.verticalCenter
                                        text: "!"
                                        font.pixelSize: 18
                                        font.weight: 700
                                        color: Ui.Theme.error
                                    }
                                    Ui.UiText {
                                        id: missingLabel
                                        objectName: "missingLabel-" + modelData.id
                                        text: qsTr("No file")
                                        font.pixelSize: 20
                                        font.weight: 600
                                        color: Ui.Theme.error
                                    }
                                }
                            }
                        }
                        Item {
                            id: caption
                            objectName: "posterCaption-" + modelData.id
                            property real lift: poster.activeFocus ? (root.pointerMode ? -4 : -10) : 0
                            x: 4
                            y: root.posterHeight + 16
                            width: parent.width - 8
                            height: 96
                            transform: Translate { y: caption.lift }
                            Ui.UiText {
                                id: posterLabel
                                objectName: "posterLabel-" + modelData.id
                                width: parent.width
                                text: modelData.title || ""
                                font.pixelSize: 26
                                font.weight: poster.activeFocus ? 600 : 500
                                maximumLineCount: 2
                                elide: Text.ElideRight
                            }
                            Ui.UiText {
                                objectName: "posterYear-" + modelData.id
                                anchors.top: posterLabel.bottom
                                anchors.topMargin: 4
                                width: parent.width
                                text: modelData.year !== undefined && modelData.year !== null && String(modelData.year).length
                                      ? String(modelData.year) : (modelData.subtitle || "")
                                font.pixelSize: 24
                                font.weight: 500
                                color: Ui.Theme.textHint
                                maximumLineCount: 1
                                elide: Text.ElideRight
                            }
                        }
                    }
                }
            }
        }

        Rectangle {
            z: 4
            width: parent.width
            height: 18
            gradient: Gradient {
                GradientStop { position: 0.0; color: Ui.Theme.background }
                GradientStop { position: 1.0; color: "transparent" }
            }
        }
    }

    Rectangle {
        id: detailLayer
        objectName: "detailLayer"
        anchors.fill: parent
        visible: root.detailOpen
        color: root.detailEntry ? root.standInColor(root.detailEntry.id) : Ui.Theme.background
        z: 5
        Image {
            id: detailFanart
            objectName: "detailFanart"
            anchors.fill: parent
            z: 0
            fillMode: Image.PreserveAspectCrop
            asynchronous: true
            cache: false
            source: root.detailEntry && root.detailEntry.fanart ? root.detailEntry.fanart : ""
            visible: status === Image.Ready
        }
        Rectangle {
            z: 2
            anchors.fill: parent
            gradient: Gradient {
                orientation: Gradient.Horizontal
                GradientStop { position: 0.0; color: Qt.rgba(Ui.Theme.background.r, Ui.Theme.background.g, Ui.Theme.background.b, 0.96) }
                GradientStop { position: 0.32; color: Qt.rgba(Ui.Theme.background.r, Ui.Theme.background.g, Ui.Theme.background.b, 0.80) }
                GradientStop { position: 0.68; color: Qt.rgba(Ui.Theme.background.r, Ui.Theme.background.g, Ui.Theme.background.b, 0.15) }
                GradientStop { position: 1.0; color: Qt.rgba(Ui.Theme.background.r, Ui.Theme.background.g, Ui.Theme.background.b, 0.0) }
            }
        }
        Rectangle {
            z: 2
            anchors.fill: parent
            gradient: Gradient {
                GradientStop { position: 0.45; color: Qt.rgba(Ui.Theme.background.r, Ui.Theme.background.g, Ui.Theme.background.b, 0.0) }
                GradientStop { position: 0.72; color: Qt.rgba(Ui.Theme.background.r, Ui.Theme.background.g, Ui.Theme.background.b, 0.70) }
                GradientStop { position: 1.0; color: Qt.rgba(Ui.Theme.background.r, Ui.Theme.background.g, Ui.Theme.background.b, 1.0) }
            }
        }
        Item {
            z: 3
            anchors.fill: parent
            Column {
                id: detailCopy
                x: root.marginLeft
                y: (root.detailEntry && root.detailEntry.kind === "series") || root.detailSeasons.length ? 180 : 320
                width: 1140
                spacing: 22
                Ui.UiText {
                    objectName: "detailEyebrow"
                    role: "eyebrow"
                    font.capitalization: Font.AllUppercase
                    text: root.detailEntry && root.detailEntry.kind === "series" ? qsTr("Series")
                            : root.detailEntry && root.detailEntry.kind === "film" ? qsTr("Film") : ""
                    visible: text.length > 0
                    color: Ui.Theme.textSecondary
                }
                Ui.UiText {
                    objectName: "detailTitle"
                    width: parent.width
                    role: "display"
                    text: root.detailEntry ? (root.detailEntry.title || "") : ""
                    maximumLineCount: 2
                    elide: Text.ElideRight
                }
                Ui.UiText {
                    objectName: "detailMeta"
                    width: parent.width
                    font.pixelSize: 30
                    font.weight: 500
                    color: Ui.Theme.textPrimary
                    text: {
                        const entry = root.detailEntry;
                        if (!entry)
                            return "";
                        if (entry.subtitle && String(entry.subtitle).length)
                            return String(entry.subtitle);
                        if (entry.year !== undefined && entry.year !== null && String(entry.year).length)
                            return String(entry.year);
                        return "";
                    }
                    visible: text.length > 0
                }
                Ui.UiText {
                    objectName: "detailDescription"
                    width: Math.min(960, parent.width)
                    role: "body"
                    text: root.detailEntry ? (root.detailEntry.description || "") : ""
                    color: Ui.Theme.textSecondary
                    maximumLineCount: 3
                    elide: Text.ElideRight
                    visible: text.length > 0
                }
                Rectangle {
                    objectName: "detailMissing"
                    visible: !!(root.detailEntry && root.detailEntry.missing)
                    width: Math.min(900, parent.width)
                    radius: 26
                    color: Qt.rgba(8 / 255, 9 / 255, 11 / 255, 0.5)
                    implicitHeight: missingCopy.implicitHeight + 44
                    height: implicitHeight
                    Row {
                        anchors.left: parent.left
                        anchors.leftMargin: 28
                        anchors.verticalCenter: parent.verticalCenter
                        spacing: 20
                        Text {
                            text: "!"
                            font.pixelSize: 44
                            font.weight: 700
                            color: Ui.Theme.error
                        }
                        Column {
                            id: missingCopy
                            width: 760
                            spacing: 4
                            Ui.UiText {
                                width: parent.width
                                font.pixelSize: 30
                                font.weight: 600
                                text: root.detailEntry && root.detailEntry.missingTitle
                                        ? root.detailEntry.missingTitle
                                        : qsTr("This file is no longer available")
                            }
                            Ui.UiText {
                                width: parent.width
                                font.pixelSize: 26
                                color: Ui.Theme.textSecondary
                                text: root.detailEntry && root.detailEntry.missingDescription
                                        ? root.detailEntry.missingDescription
                                        : qsTr("The entry stays in the catalogue.")
                            }
                        }
                    }
                }
                Row {
                    spacing: 20
                    Ui.ActionButton {
                        id: detailPlay
                        objectName: "detailPlay"
                        visible: !!(root.detailEntry && !root.detailEntry.missing)
                        busy: root.launchingDetail
                        text: root.detailEntry && root.detailEntry.progress > 0 ? qsTr("Continue") : qsTr("Play")
                        pointerMode: root.pointerMode
                        hoverFocusEnabled: root.hoverFocusEnabled
                        onDpadRequested: Ui.InputMode.keyPressed()
                        onActivated: root.playRequested(root.detailEntryId)
                    }
                    Ui.ActionButton {
                        id: detailRemove
                        objectName: "detailRemove"
                        visible: !!(root.detailEntry && root.detailEntry.missing)
                        text: qsTr("Remove from library")
                        pointerMode: root.pointerMode
                        hoverFocusEnabled: root.hoverFocusEnabled
                        onDpadRequested: Ui.InputMode.keyPressed()
                        onActivated: root.removeRequested(root.detailEntryId)
                    }
                }
                Row {
                    spacing: 20
                    visible: !!(root.detailEntry && !root.detailEntry.missing && root.detailEntry.progress > 0)
                    Item {
                        width: 300
                        height: 26
                        Ui.ProgressTrack {
                            id: detailProgress
                            objectName: "detailProgress"
                            anchors.verticalCenter: parent.verticalCenter
                            visible: !!(root.detailEntry && !root.detailEntry.missing && root.detailEntry.progress > 0)
                            width: 300
                            height: 6
                            value: root.detailEntry && root.detailEntry.progress ? root.detailEntry.progress : 0
                        }
                    }
                    Ui.UiText {
                        font.pixelSize: 26
                        font.weight: 500
                        color: Ui.Theme.textSecondary
                        text: root.detailEntry && root.detailEntry.progress
                              ? Math.round(root.detailEntry.progress * 100) + "%"
                              : ""
                    }
                }
            }
            Column {
                x: root.marginLeft
                width: parent.width - x
                anchors.top: detailCopy.bottom
                anchors.topMargin: root.detailSeasons.length ? 100 : 36
                spacing: 30
                visible: root.detailSeasons.length > 0 || episodeRepeater.count > 0
                Row {
                    spacing: 12
                    height: 64
                    visible: root.detailSeasons.length > 0
                    Repeater {
                        id: seasonRepeater
                        model: root.detailSeasons
                        delegate: Ui.Surface {
                            id: seasonButton
                            required property var modelData
                            required property int index
                            objectName: "season-" + modelData.id
                            implicitWidth: Math.max(120, seasonLabel.implicitWidth + 56)
                            height: 64
                            implicitHeight: 64
                            cornerRadius: 32
                            focusScale: 1.02
                            text: modelData.title || ""
                            restingColor: modelData.id === root.currentSeasonId ? Qt.rgba(1, 1, 1, 0.16) : Ui.Theme.plate
                            pointerMode: root.pointerMode
                            hoverFocusEnabled: root.hoverFocusEnabled
                            onDpadRequested: Ui.InputMode.keyPressed()
                            onActivated: root.focusSeason(index)
                            contentItem: Ui.UiText {
                                id: seasonLabel
                                text: seasonButton.text
                                font.pixelSize: 28
                                font.weight: 600
                                horizontalAlignment: Text.AlignHCenter
                                verticalAlignment: Text.AlignVCenter
                            }
                            Rectangle {
                                objectName: "seasonUnderline-" + modelData.id
                                anchors.left: parent.left
                                anchors.right: parent.right
                                anchors.leftMargin: 28
                                anchors.rightMargin: 28
                                anchors.bottom: parent.bottom
                                anchors.bottomMargin: 7
                                height: 4
                                radius: 2
                                color: Ui.Theme.ember
                                visible: modelData.id === root.currentSeasonId
                            }
                        }
                    }
                }
                Flickable {
                    id: episodeFlick
                    width: parent.width
                    height: episodeRow.height
                    visible: episodeRepeater.count > 0
                    contentWidth: episodeRow.width
                    contentHeight: height
                    clip: true
                    flickableDirection: Flickable.HorizontalFlick
                    boundsBehavior: Flickable.StopAtBounds
                    Row {
                        id: episodeRow
                        spacing: 40
                        Repeater {
                            id: episodeRepeater
                            model: root.detailEpisodes
                            delegate: Item {
                                id: episodeCell
                                required property var modelData
                                required property int index
                                width: 420
                                height: 236 + 16 + 72
                                function focusStill() {
                                    Ui.Focus.take(still);
                                }
                                Ui.Surface {
                                    id: still
                                    objectName: "episode-" + modelData.id
                                    width: 420
                                    height: 236
                                    cornerRadius: 22
                                    focusScale: 1.06
                                    text: modelData.title || ""
                                    enabled: modelData.enabled !== false
                                    busy: root.operationPending && root.launchingId === modelData.id
                                    restingColor: root.standInColor(modelData.id)
                                    focusedColor: restingColor
                                    pointerMode: root.pointerMode
                                    hoverFocusEnabled: root.hoverFocusEnabled
                                    onDpadRequested: Ui.InputMode.keyPressed()
                                    onActiveFocusChanged: {
                                        if (!activeFocus)
                                            return;
                                        root.episodeIndex = index;
                                        const left = episodeCell.x;
                                        const right = left + episodeCell.width;
                                        if (left < episodeFlick.contentX)
                                            episodeFlick.contentX = left;
                                        else if (right > episodeFlick.contentX + episodeFlick.width)
                                            episodeFlick.contentX = Math.max(0, right - episodeFlick.width);
                                    }
                                    onActivated: root.episodeRequested(root.detailEntryId, modelData.id)
                                    contentItem: Item {
                                        Rectangle {
                                            objectName: "episodePlate-" + modelData.id
                                            anchors.fill: parent
                                            radius: 22
                                            visible: stillImage.status !== Image.Ready
                                            gradient: Gradient {
                                                GradientStop { position: 0.0; color: Qt.rgba(0.29, 0.29, 0.31, 1) }
                                                GradientStop { position: 1.0; color: Qt.rgba(0.21, 0.21, 0.23, 1) }
                                            }
                                            FilmGlyph {
                                                objectName: "episodeGlyph-" + modelData.id
                                                anchors.centerIn: parent
                                            }
                                        }
                                        Image {
                                            id: stillImage
                                            objectName: "episodeImage-" + modelData.id
                                            anchors.fill: parent
                                            fillMode: Image.PreserveAspectCrop
                                            asynchronous: true
                                            cache: false
                                            source: modelData.artwork ? modelData.artwork : ""
                                            visible: status === Image.Ready
                                        }
                                        Rectangle {
                                            visible: modelData.progress > 0
                                            anchors.left: parent.left
                                            anchors.right: parent.right
                                            anchors.bottom: parent.bottom
                                            anchors.leftMargin: 20
                                            anchors.rightMargin: 20
                                            anchors.bottomMargin: 18
                                            height: 6
                                            radius: 3
                                            color: Qt.rgba(1, 1, 1, 0.25)
                                            Rectangle {
                                                width: parent.width * Math.max(0, Math.min(1, modelData.progress || 0))
                                                height: parent.height
                                                radius: parent.radius
                                                color: Ui.Theme.ember
                                            }
                                        }
                                    }
                                    Rectangle {
                                        objectName: "episode-missing-" + modelData.id
                                        visible: !!modelData.missing
                                        z: 40
                                        x: 14
                                        y: parent.height - height - 14
                                        height: 40
                                        radius: 20
                                        width: Math.min(parent.width - 28, episodeMissingLabel.implicitWidth + 36)
                                        color: Qt.rgba(8 / 255, 9 / 255, 11 / 255, 0.72)
                                        Ui.UiText {
                                            id: episodeMissingLabel
                                            anchors.centerIn: parent
                                            text: qsTr("No file")
                                            font.pixelSize: 20
                                            font.weight: 600
                                            color: Ui.Theme.error
                                        }
                                    }
                                }
                                Item {
                                    id: episodeCaption
                                    x: 4
                                    y: 236 + 16
                                    width: parent.width - 8
                                    property real lift: still.activeFocus ? (root.pointerMode ? -4 : -10) : 0
                                    transform: Translate { y: episodeCaption.lift }
                                    Ui.UiText {
                                        id: episodeLabel
                                        width: parent.width
                                        text: (modelData.number !== undefined && modelData.number !== null && String(modelData.number).length
                                               ? String(modelData.number) + ". " : "") + (modelData.title || "")
                                        font.pixelSize: 26
                                        font.weight: still.activeFocus ? 600 : 500
                                        maximumLineCount: 1
                                        elide: Text.ElideRight
                                    }
                                    Ui.UiText {
                                        objectName: "episodeDuration-" + modelData.id
                                        anchors.top: episodeLabel.bottom
                                        anchors.topMargin: 4
                                        width: parent.width
                                        text: modelData.duration || ""
                                        font.pixelSize: 24
                                        color: Ui.Theme.textHint
                                        visible: text.length > 0
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    Column {
        z: 6
        x: root.marginLeft
        y: 960
        width: 900
        spacing: 12
        visible: root.detailOpen && (root.operationPending || root.operationFailed)
        Ui.UiText {
            objectName: "operationPending"
            visible: root.operationPending
            text: qsTr("Waiting…")
            role: "meta"
            color: Ui.Theme.textSecondary
        }
        Rectangle {
            id: operationBanner
            objectName: "operationBanner"
            visible: root.operationFailed
            width: parent.width
            implicitHeight: operationBannerText.implicitHeight + 28
            height: implicitHeight
            radius: 18
            color: Qt.rgba(0.91, 0.28, 0.31, 0.16)
            Ui.UiText {
                id: operationBannerText
                objectName: "operationBannerText"
                width: parent.width - 28
                anchors.left: parent.left
                anchors.verticalCenter: parent.verticalCenter
                anchors.leftMargin: 14
                text: root.operationMessage
                role: "body"
                color: Ui.Theme.error
            }
        }
    }
}
