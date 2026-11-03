// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import QtQuick.Window
import "../.." as Ui
import "../../artwork.js" as Artwork

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
    property string descriptionEntryId: ""
    property string descriptionText: ""
    property var entries: []
    property var folderEntries: []
    readonly property bool folderMode: currentTabId === "@folders"
    readonly property var catalogueTabs: (tabs || []).filter(t => t.id !== "@folders")
    readonly property bool hasFolderView: (tabs || []).some(t => t.id === "@folders")
    property string lastCatalogueTab: ""
    property var folderTrail: []
    property var viewPositions: ({})
    readonly property string currentFolderId: folderTrail.length ? folderTrail[folderTrail.length - 1].id : ""
    readonly property var currentFolder: (folderEntries || []).find(e => e.kind === "folder" && e.id === currentFolderId) || null
    readonly property bool folderEmpty: folderMode && libraryState === "ready" && visibleEntries.length === 0 && !detailOpen
    readonly property string folderBreadcrumb: {
        let labels = [qsTr("Folders")];
        for (const step of folderTrail) {
            const row = (folderEntries || []).find(e => e.kind === "folder" && e.id === step.id);
            labels.push(row ? row.title : step.title);
        }
        return labels.join("  /  ");
    }
    property int columns: 0
    property string detailEntryId: ""
    property string detailOpenerId: ""
    property bool detailRailActive: false
    // True while an episode still has focus, so the rest of that rail dims.
    property bool episodeStillFocused: false
    onDetailRailActiveChanged: hoverDelay.restart()
    onDetailEntryIdChanged: {
        if (episodeFlick) episodeFlick.resetScroll();
        detailRailActive = false;
        episodeStillFocused = false;
        if (!detailEntryId.length) {
            detailOpenerId = "";
        } else if (detailOpenerId.length && detailOpenerId !== detailEntryId) {
            const target = entryById(detailEntryId);
            if (!target || !(target.episodes || []).some(episode => episode.id === detailOpenerId))
                detailOpenerId = "";
        }
    }
    property string operationState: ""
    property string launchingId: ""
    // The shell's input mode (ADR-0095); tests may assign it directly.
    property bool pointerMode: Ui.InputMode.pointerActive
    // False while another screen or overlay holds focus above this one. The
    // view stays visible and enabled under Home's navigation menu, so data
    // updates must not take focus then (#608). Main.qml binds it.
    property bool ownsFocus: true
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
        if (operationState === "PermissionDenied")
            return qsTr("That is not allowed.");
        if (operationState === "Cancelled" || operationState === "cancelled")
            return qsTr("Cancelled.");
        // Internal, a failed state or a code this shell does not know (§15).
        return qsTr("Something went wrong. Try again.");
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
    readonly property int marginRight: 120
    // The handoff grid is seven 210-wide posters. Tests may set a smaller count.
    readonly property int columnCount: columns > 0 ? columns : 7
    readonly property var visibleEntries: {
        const tab = currentTabId;
        const source = folderMode ? (folderEntries || []) : (entries || []);
        const folder = currentFolderId;
        const out = [];
        for (let i = 0; i < source.length; ++i) {
            const entry = source[i];
            if (folderMode ? (entry.folderId || "") === folder : (!tab.length || !entry.tabId || entry.tabId === tab))
                out.push(entry);
        }
        return out;
    }
    readonly property var detailEntry: {
        const id = detailEntryId;
        const source = (entries || []).concat(folderEntries || []);
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
            return folderMode ? qsTr("This folder is empty") : qsTr("Nothing in the library yet");
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
            return folderMode ? qsTr("Go back to choose another folder.") : qsTr("Films and series appear here when a folder has been scanned.");
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
    // Settings › Appearance (ADR-0112, ADR-0136). Main binds both.
    property bool ambientEnabled: true
    property bool lightFollowsTime: true
    property string clockText: ""
    readonly property var libraryLightEntry: {
        if (!focusedPosterId)
            return null;
        return (visibleEntries || []).find(entry => entry && entry.id === focusedPosterId) || null;
    }
    property int focusedPosterIndex: 0
    property int episodeIndex: 0
    property int seasonIndex: 0
    property string currentSeasonId: ""
    onCurrentSeasonIdChanged: if (episodeFlick) episodeFlick.resetScroll()
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
            if ((episode.seasonId || "unknown") === season)
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
    // A rounded backing does not clip an Image. Paint the crop through
    // the same software-compatible mask used by ContentCard and Downloads.
    component RoundedArtwork: Item {
        id: artworkSlot
        property alias source: picture.source
        // Preserve the Image-like contract used by native image consumers.
        // Ready means the rounded canvas has drawn the current source.
        readonly property int status: picture.status === Image.Ready && !ready ? Image.Loading : picture.status
        visible: picture.status === Image.Ready
        property real cornerRadius: 0
        readonly property bool ready: picture.status === Image.Ready && artworkCanvas.showsArtwork
                && artworkCanvas.loadedSource === picture.source.toString()
        onCornerRadiusChanged: artworkCanvas.requestPaint()
        Image {
            id: picture
            anchors.fill: parent
            asynchronous: true
            cache: false
            visible: false
            // Decode only what the cell covers, not the full poster (#611).
            sourceSize: Artwork.sourceSize(source, width, height)
            onStatusChanged: artworkCanvas.syncSource()
            onSourceChanged: Qt.callLater(artworkCanvas.syncSource)
        }
        Canvas {
            id: artworkCanvas
            objectName: "roundedArtworkCanvas"
            anchors.fill: parent
            opacity: artworkSlot.ready ? 1 : 0
            property string loadedSource: ""
            property bool showsArtwork: false
            // The canvas copy is decoded at the Image's decoded size, which
            // covers the cell (#611), so the crop uses the same pixels.
            property size loadedSize: Qt.size(0, 0)
            function syncSource() {
                const ready = picture.status === Image.Ready;
                const next = ready ? picture.source.toString() : "";
                const size = ready ? Qt.size(picture.implicitWidth, picture.implicitHeight) : Qt.size(0, 0);
                if (loadedSource !== next || loadedSize.width !== size.width
                        || loadedSize.height !== size.height) {
                    showsArtwork = false;
                    if (loadedSource.length) unloadImage(loadedSource);
                    loadedSource = next;
                    loadedSize = size;
                }
                if (available && next.length && !isImageLoaded(next)
                        && !isImageLoading(next) && !isImageError(next))
                    Artwork.loadCopy(artworkCanvas, picture);
                requestPaint();
            }
            onAvailableChanged: syncSource()
            onImageLoaded: requestPaint()
            onWidthChanged: requestPaint()
            onHeightChanged: requestPaint()
            onPaint: {
                const ctx = getContext("2d");
                ctx.reset();
                const image = loadedSource;
                if (!image.length || picture.status !== Image.Ready
                        || image !== picture.source.toString() || !isImageLoaded(image)) {
                    showsArtwork = false;
                    return;
                }
                Artwork.clipRounded(ctx, width, height, artworkSlot.cornerRadius);
                showsArtwork = Artwork.drawCover(ctx, image, picture.implicitWidth,
                                                 picture.implicitHeight, width, height);
            }
        }
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
    component FolderGlyph: Canvas {
        width: 80; height: 64
        onPaint: {
            const ctx = getContext("2d");
            ctx.reset();
            ctx.strokeStyle = Ui.Theme.textHint;
            ctx.lineWidth = 6;
            ctx.lineJoin = "round";
            ctx.beginPath();
            ctx.moveTo(6, 3); ctx.lineTo(29, 3); ctx.lineTo(37, 11);
            ctx.lineTo(74, 11); ctx.lineTo(77, 14); ctx.lineTo(77, 58);
            ctx.lineTo(74, 61); ctx.lineTo(6, 61); ctx.lineTo(3, 58);
            ctx.lineTo(3, 6); ctx.closePath(); ctx.stroke();
        }
    }
    // View, category and season pills are containers: no plate at rest, an
    // 8% plate and a rim in focus (ADR-0136). The pill radius stays 32.
    component LibraryPill: Ui.Surface {
        id: pill
        property bool selected: false
        property string labelName: objectName + "Label"
        property string underlineName: objectName + "Underline"
        implicitWidth: label.implicitWidth + 56
        implicitHeight: 64
        height: 64
        cornerRadius: 32
        focusScale: 1
        focusLift: 0
        liftFromBottom: false
        shapeRole: "container"
        restingColor: "transparent"
        focusedColor: Ui.Theme.plateContainer
        transform: Translate { y: 0 }
        background: Rectangle { radius: 32; color: pill.activeFocus ? pill.focusedColor : "transparent" }
        contentItem: Ui.UiText {
            id: label
            objectName: pill.labelName
            text: pill.text
            font.pixelSize: 28
            font.weight: 600
            color: pill.activeFocus || pill.selected ? Ui.Theme.textPrimary : Ui.Theme.textSecondary
            horizontalAlignment: Text.AlignHCenter
            verticalAlignment: Text.AlignVCenter
        }
        Rectangle {
            objectName: pill.underlineName
            x: 28; width: parent.width - 56
            y: parent.height - 11; height: 4; radius: 2
            color: Ui.Theme.ember
            visible: pill.selected && !pill.activeFocus
        }
    }
    function chooseView(folders) {
        if (folders && !hasFolderView) return;
        if (folders === folderMode) {
            focusPlace = "mode";
            Ui.Focus.take(folders ? folderModeButton : catalogueModeButton);
            return;
        }
        if (folders !== folderMode) {
            const positions = Object.assign({}, viewPositions);
            positions[folderMode ? "folders" : "catalogue"] = {id: focusedPosterId, index: focusedPosterIndex, scrollY: scroller.contentY};
            viewPositions = positions;
        }
        const remembered = catalogueTabs.some(t => t.id === lastCatalogueTab) ? lastCatalogueTab : "";
        focusPlace = "mode";
        currentTabId = folders ? "@folders" : (remembered || (catalogueTabs.length ? catalogueTabs[0].id : ""));
        const position = viewPositions[folders ? "folders" : "catalogue"];
        focusedPosterId = position ? position.id : "";
        focusedPosterIndex = position ? position.index : 0;
        posterGrid.forceLayout();
        scroller.contentY = position ? position.scrollY : 0;
        Ui.Focus.take(folders ? folderModeButton : catalogueModeButton);
    }
    function openFolder(entry) {
        folderTrail = folderTrail.concat([{id: entry.id, title: entry.title, openerId: entry.id, scrollY: scroller.contentY}]);
        scroller.contentY = 0;
        focusedPosterId = "";
        Qt.callLater(() => { if (ownsFocus && visible && enabled && !focusPosterIndex(0)) focusStatus(); });
    }
    function upFolder() {
        if (!folderTrail.length) return false;
        const step = folderTrail[folderTrail.length - 1];
        folderTrail = folderTrail.slice(0, -1);
        Qt.callLater(() => {
            scroller.contentY = step.scrollY;
            if (ownsFocus && visible && enabled) focusPosterById(step.openerId);
        });
        return true;
    }
    function reconcileFolder() {
        const source = folderEntries || [];
        let keep = 0;
        for (const step of folderTrail) {
            if (!source.some(e => e.kind === "folder" && e.id === step.id)) break;
            ++keep;
        }
        if (keep !== folderTrail.length) folderTrail = folderTrail.slice(0, keep);
        reconcilePoster();
    }
    function entryById(id) {
        const source = (entries || []).concat(folderEntries || []);
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
        if (!catalogueTabs || index < 0 || index >= catalogueTabs.length)
            return false;
        const item = tabRepeater.itemAt(index);
        if (!item)
            return false;
        focusPlace = "tab";
        Ui.Focus.take(item);
        const id = catalogueTabs[index].id;
        if (currentTabId !== id) {
            currentTabId = id;
            tabRequested(id);
        }
        return true;
    }
    function focusCurrentTab() {
        if (folderMode) { focusPlace = "mode"; Ui.Focus.take(folderModeButton); return true; }
        if (!catalogueTabs)
            return false;
        let index = 0;
        for (let i = 0; i < catalogueTabs.length; ++i)
            if (catalogueTabs[i].id === currentTabId)
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
        // A mode change replaces delegates before the positioner's next
        // polish; reveal the new card only after its row has been laid out.
        posterGrid.forceLayout();
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
        if (folderEmpty) {
            focusPlace = "status";
            Ui.Focus.take(folderBackButton.visible ? folderBackButton : folderModeButton);
            return true;
        }
        if (!libraryStatus.visible)
            return false;
        focusPlace = "status";
        Ui.Focus.take(libraryStatus);
        return true;
    }
    function focusDetailAction() {
        detailRailActive = false;
        episodeStillFocused = false;
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
        detailRailActive = true;
        seasonIndex = index;
        const season = detailSeasons[index];
        if (season && currentSeasonId !== season.id) {
            currentSeasonId = season.id;
            episodeIndex = 0;
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
        detailRailActive = true;
        episodeIndex = index;
        item.focusStill();
        return true;
    }
    function focusInitial() {
        if (!ownsFocus || !visible || !enabled) return;
        if (detailOpen) {
            focusDetailAction();
            return;
        }
        if (showGrid && focusPlace === "poster" && focusedPosterId.length) {
            focusPosterById(focusedPosterId);
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
        if (entry.kind === "folder") { openFolder(entry); return; }
        const series = folderMode ? (entries || []).find(candidate => candidate.kind === "series"
                && (candidate.episodes || []).some(episode => episode.id === id)) : null;
        const selectedEpisode = series ? series.episodes.find(episode => episode.id === id) : null;
        const target = series || entry;
        focusedPosterId = id;
        detailOpenerId = id;
        episodeFlick.resetScroll();
        detailRailActive = false;
        episodeStillFocused = false;
        detailEntryId = target.id;
        episodeIndex = 0;
        seasonIndex = 0;
        const seasons = target.seasons || [];
        currentSeasonId = selectedEpisode ? (selectedEpisode.seasonId || "unknown") : seasons.length ? seasons[0].id : "";
        if (selectedEpisode && !seasons.length) currentSeasonId = "";
        seasonIndex = Math.max(0, seasons.findIndex(season => season.id === currentSeasonId));
        entryRequested(target.id);
        Qt.callLater(() => {
            if (!ownsFocus || !visible || !enabled || detailEntryId !== target.id) return;
            if (selectedEpisode) {
                const index = detailEpisodes.findIndex(episode => episode.id === id);
                if (focusEpisode(index)) return;
            }
            focusDetailAction();
        });
    }
    function closeDetail() {
        const id = detailOpenerId || detailEntryId;
        detailRailActive = false;
        episodeStillFocused = false;
        detailEntryId = "";
        detailOpenerId = "";
        Qt.callLater(() => { if (ownsFocus && visible && enabled) focusPosterById(id); });
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
            if (detailRailActive) {
                focusDetailAction();
                return;
            }
            closeDetail();
            return;
        }
        if (folderMode && upFolder()) return;
        backRequested();
    }
    function reconcilePoster() {
        if (!ownsFocus || !visible || !enabled || detailOpen || !showGrid || focusPlace !== "poster")
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
        if (!ownsFocus || !visible || !enabled)
            return;
        const focused = Window.activeFocusItem;
        // The scope itself holds focus when its focused child was destroyed
        // while an overlay was open.
        if (!root.activeFocus || focused === root || !focusTargetVisible(focused))
            focusInitial();
    }
    onLibraryStateChanged: {
        if (libraryState !== "ready")
            detailEntryId = "";
        Qt.callLater(refocusAfterState);
    }
    onShowStatusChanged: Qt.callLater(refocusAfterState)
    onShowGridChanged: Qt.callLater(refocusAfterState)
    // The remembered poster first: the fallback would focus a tab and
    // forget the grid position.
    onOwnsFocusChanged: if (ownsFocus) Qt.callLater(() => { reconcilePoster(); refocusAfterState(); })
    onEntriesChanged: Qt.callLater(reconcilePoster)
    onFolderEntriesChanged: Qt.callLater(reconcileFolder)
    onVisibleEntriesChanged: Qt.callLater(reconcilePoster)
    onTabsChanged: adoptTab()
    onCurrentTabIdChanged: if (currentTabId !== "@folders") lastCatalogueTab = currentTabId
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
            const restoreId = detailOpenerId || detailEntryId;
            if (detailOpen)
                detailEntryId = "";
            homeRequested();
            if (restoreId.length)
                Qt.callLater(() => { if (ownsFocus && visible && enabled) focusPosterById(restoreId); });
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
        const onMode = focused && String(focused.objectName).indexOf("libraryMode-") === 0;
        if (onMode) {
            if (event.key === Qt.Key_Left) chooseView(false);
            else if (event.key === Qt.Key_Right) chooseView(true);
            else if (event.key === Qt.Key_Down) {
                if (showGrid) focusPosterById(focusedPosterId);
                else focusStatus();
            }
            return;
        }
        if (onTab) {
            let index = 0;
            for (let i = 0; i < catalogueTabs.length; ++i)
                if (catalogueTabs[i].id === currentTabId)
                    index = i;
            if (event.key === Qt.Key_Left || event.key === Qt.Key_Right) {
                const step = event.key === Qt.Key_Left ? -1 : 1;
                const next = index + step;
                if (next >= 0 && next < catalogueTabs.length)
                    focusTab(next);
            } else if (event.key === Qt.Key_Up && hasFolderView) {
                focusPlace = "mode"; Ui.Focus.take(catalogueModeButton);
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
    // Floor and pool behind the catalogue (ADR-0136). Strength 0.6. The
    // veil is the one for this screen; the detail host does not add another.
    Ui.AmbientGlow {
        objectName: "libraryGlow"
        anchors.fill: parent
        veilParent: root
        veilZ: 40
        followsTime: root.lightFollowsTime
        clockText: root.clockText
        lit: root.ambientEnabled && !root.detailOpen
        strength: 0.6
        source: root.libraryLightEntry ? (root.libraryLightEntry.artwork || "") : ""
        tint: root.libraryLightEntry && root.libraryLightEntry.id
              ? Ui.Theme.identityColor(root.libraryLightEntry.id) : "transparent"
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
        visible: !root.folderMode && root.catalogueTabs.length > 0
        Repeater {
            id: tabRepeater
            model: root.catalogueTabs
            delegate: LibraryPill {
                required property var modelData
                required property int index
                objectName: "tab-" + modelData.id
                labelName: "tabLabel-" + modelData.id
                underlineName: "tabUnderline-" + modelData.id
                selected: modelData.id === root.currentTabId
                text: modelData.title || ""
                pointerMode: root.pointerMode
                hoverFocusEnabled: root.hoverFocusEnabled
                onDpadRequested: Ui.InputMode.keyPressed()
                onActivated: root.focusTab(index)
            }
        }
    }

    Row {
        objectName: "libraryViewModes"
        anchors.right: parent.right
        anchors.rightMargin: root.marginRight
        y: 96
        spacing: 12
        visible: root.hasFolderView && !root.detailOpen
        LibraryPill {
            id: catalogueModeButton
            objectName: "libraryMode-catalogue"
            text: qsTr("Catalogue")
            selected: !root.folderMode
            pointerMode: root.pointerMode
            hoverFocusEnabled: root.hoverFocusEnabled
            onActivated: root.chooseView(false)
            onDpadRequested: Ui.InputMode.keyPressed()
        }
        LibraryPill {
            id: folderModeButton
            objectName: "libraryMode-folders"
            text: qsTr("Folders")
            selected: root.folderMode
            pointerMode: root.pointerMode
            hoverFocusEnabled: root.hoverFocusEnabled
            onActivated: root.chooseView(true)
            onDpadRequested: Ui.InputMode.keyPressed()
        }
    }
    Ui.UiText {
        objectName: "libraryFolderBreadcrumb"
        x: root.marginLeft; y: 206
        width: root.width - root.marginLeft - root.marginRight
        visible: root.folderMode && !root.detailOpen
        text: root.folderBreadcrumb
        font.pixelSize: 26
        color: Ui.Theme.textHint
        maximumLineCount: 1
        elide: Text.ElideLeft
    }
    Ui.UiText {
        objectName: "libraryFolderUnavailable"
        x: root.marginLeft; y: 264
        visible: root.folderMode && !!root.currentFolder && !!root.currentFolder.missing && !root.detailOpen
        text: qsTr("Folder unavailable · check the connection")
        font.pixelSize: 24
        color: Ui.Theme.error
    }
    Ui.UiText {
        objectName: "libraryFolderEmpty"
        x: root.marginLeft; y: 390
        visible: root.folderEmpty
        text: qsTr("There are no videos in this folder yet")
        font.pixelSize: 36
        color: Ui.Theme.textHint
    }
    Ui.Surface {
        id: folderBackButton
        objectName: "libraryFolderBack"
        x: root.marginLeft; y: root.height - height - 40
        implicitWidth: folderBackLabel.implicitWidth + 100
        height: 64
        cornerRadius: 32
        focusScale: Ui.Theme.contentFocusScale
        focusLift: 0
        liftFromBottom: false
        rimEnabled: false
        shapeRole: "action"
        transform: Translate { y: 0 }
        restingColor: Ui.Theme.plateAction
        focusedColor: Ui.Theme.textPrimary
        z: 4
        visible: root.folderMode && root.folderTrail.length > 0 && !root.detailOpen
        text: qsTr("Up one level")
        pointerMode: root.pointerMode
        hoverFocusEnabled: root.hoverFocusEnabled
        contentItem: Row {
            leftPadding: 28
            spacing: 12
            Ui.UiText {
                anchors.verticalCenter: parent.verticalCenter
                width: 32
                text: "←"
                font.pixelSize: 32
                color: folderBackButton.activeFocus ? Ui.Theme.ink : Ui.Theme.textPrimary
            }
            Ui.UiText {
                id: folderBackLabel
                anchors.verticalCenter: parent.verticalCenter
                text: folderBackButton.text
                font.pixelSize: 26
                font.weight: 600
                color: folderBackButton.activeFocus ? Ui.Theme.ink : Ui.Theme.textPrimary
            }
        }
        onActivated: root.upFolder()
        onDpadRequested: Ui.InputMode.keyPressed()
        Keys.onUpPressed: { if (!root.focusPosterIndex(root.focusedPosterIndex)) root.focusCurrentTab(); }
    }
    Item {
        id: libraryStage
        objectName: "libraryStage"
        y: 282
        width: parent.width
        height: parent.height - y - (root.folderMode && root.folderTrail.length ? 124 : 0)
        clip: true

        Ui.StatusPane {
            id: libraryStatus
            objectName: "libraryStatus"
            anchors.horizontalCenter: parent.horizontalCenter
            y: 48
            width: Math.min(820, parent.width)
            visible: root.showStatus && !root.folderEmpty
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
                        radius: Ui.Theme.posterRadius
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
                        // `noArt` wins over an artwork URL: the plate stays.
                        readonly property bool posterHasSource: !modelData.noArt
                            && !!(modelData.artwork && String(modelData.artwork).length)
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
                            cornerRadius: Ui.Theme.posterRadius
                            focusScale: modelData.kind === "folder" ? 1.02 : Ui.Theme.contentFocusScale
                            focusLift: modelData.kind === "folder" ? 0 : Ui.Theme.contentFocusLift
                            liftFromBottom: modelData.kind !== "folder"
                            rimEnabled: modelData.kind === "folder"
                            shapeRole: modelData.kind === "folder" ? "container" : "content"
                            transform: Translate { y: modelData.kind === "folder" ? 0 : poster.lift }
                            text: modelData.title || ""
                            enabled: modelData.enabled !== false
                            busy: !!modelData.busy || (root.operationPending && root.launchingId === modelData.id)
                            restingColor: modelData.kind === "folder" ? "transparent"
                                    : !cell.posterHasSource ? Qt.rgba(0.21, 0.21, 0.23, 1) : root.standInColor(modelData.id)
                            focusedColor: modelData.kind === "folder" ? Ui.Theme.plateContainer : restingColor
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
                                    radius: poster.cornerRadius
                                    visible: !posterImage.ready
                                    gradient: Gradient {
                                        GradientStop { position: 0.0; color: modelData.kind === "folder" ? "#292c31" : Qt.rgba(0.29, 0.29, 0.31, 1) }
                                        GradientStop { position: 1.0; color: modelData.kind === "folder" ? "#17191d" : Qt.rgba(0.21, 0.21, 0.23, 1) }
                                    }
                                }
                                RoundedArtwork {
                                    id: posterImage
                                    objectName: "posterImage-" + modelData.id
                                    anchors.fill: parent
                                    cornerRadius: poster.cornerRadius
                                    source: cell.posterHasSource ? modelData.artwork : ""
                                }
                                Ui.ArtworkFocus {
                                    shown: poster.activeFocus && modelData.kind !== "folder"
                                    artworkRadius: poster.cornerRadius
                                }
                                Ui.CastLight {
                                    z: -1
                                    shown: poster.activeFocus && modelData.kind !== "folder"
                                    width: parent.width * Ui.Theme.castWidthRatio
                                    anchors.horizontalCenter: parent.horizontalCenter
                                    anchors.top: parent.bottom
                                    anchors.topMargin: -Math.round(Ui.Theme.castHeight * 0.4)
                                }
                                Item {
                                    objectName: "posterPlate-" + modelData.id
                                    anchors.fill: parent
                                    anchors.leftMargin: 20
                                    anchors.rightMargin: 20
                                    anchors.topMargin: 22
                                    anchors.bottomMargin: 22
                                    visible: !posterImage.ready
                                    FilmGlyph {
                                        objectName: "posterGlyph-" + modelData.id
                                        visible: modelData.kind !== "folder"
                                    }
                                    FolderGlyph {
                                        objectName: "folderGlyph-" + modelData.id
                                        visible: modelData.kind === "folder"
                                        anchors.centerIn: parent
                                    }
                                    Ui.UiText {
                                        objectName: "posterArtTitle-" + modelData.id
                                        visible: modelData.kind !== "folder"
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
                                        text: modelData.kind === "folder" ? qsTr("Unavailable") : qsTr("No file")
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
                            property real lift: poster.lift
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
                                color: poster.activeFocus ? Ui.Theme.textPrimary : Ui.Theme.textSecondary
                                maximumLineCount: 2
                                elide: Text.ElideRight
                            }
                            Ui.UiText {
                                objectName: "posterYear-" + modelData.id
                                anchors.top: posterLabel.bottom
                                anchors.topMargin: 4
                                width: parent.width
                                text: modelData.kind === "folder" ? (modelData.missing ? qsTr("Unavailable") : qsTr("Folder"))
                                    : modelData.year !== undefined && modelData.year !== null && String(modelData.year).length
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
        // Detail strength 0.5, above the fanart scrim and under the copy.
        // The catalogue glow already lays the veil over this screen.
        Ui.AmbientGlow {
            objectName: "detailGlow"
            anchors.fill: parent
            z: 2.5
            visible: root.detailOpen
            veilEnabled: false
            followsTime: root.lightFollowsTime
            clockText: root.clockText
            lit: root.ambientEnabled
            strength: 0.5
            source: root.detailEntry ? (root.detailEntry.fanart || root.detailEntry.artwork || "") : ""
            tint: root.detailEntry && root.detailEntry.id
                  ? Ui.Theme.identityColor(root.detailEntry.id) : "transparent"
        }
        Image {
            id: detailFanart
            objectName: "detailFanart"
            anchors.fill: parent
            z: 0
            fillMode: Image.PreserveAspectCrop
            asynchronous: true
            cache: false
            sourceSize: Qt.size(Math.ceil(width), Math.ceil(height))
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
            transform: Translate {
                y: root.detailRailActive ? Math.min(0, 170 - detailRail.y) : 0
                Behavior on y { NumberAnimation { duration: 450; easing.type: Easing.InOutCubic } }
            }
            Column {
                id: detailCopy
                opacity: root.detailRailActive ? 0.18 : 1
                x: root.marginLeft
                y: (root.detailEntry && root.detailEntry.kind === "series") || root.detailSeasons.length ? 180 : 320
                width: 1140
                spacing: 22
                Ui.UiText {
                    objectName: "detailEyebrow"
                    role: "eyebrow"
                    text: root.detailEntry && root.detailEntry.kind === "series" ? qsTr("Series")
                            : root.detailEntry && root.detailEntry.kind === "film" ? qsTr("Film") : ""
                    visible: text.length > 0
                    color: Ui.Theme.textSecondary
                }
                Ui.UiText {
                    objectName: "detailTitle"
                    width: parent.width
                    role: "detailTitle"
                    text: root.detailEntry ? (root.detailEntry.title || "") : ""
                    // 104 on up to two lines, 80 on up to three (ADR-0136).
                    fitSteps: [[104, 2], [80, 3]]
                    lineHeightMode: Text.FixedHeight
                    lineHeight: Math.round(font.pixelSize * 1.07)
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
                    text: root.descriptionEntryId === root.detailEntryId ? root.descriptionText
                          : root.detailEntry ? (root.detailEntry.description || "") : ""
                    textFormat: Text.PlainText
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
                        primary: true
                        text: root.detailEntry && root.detailEntry.progress > 0 ? qsTr("Continue") : qsTr("Play")
                        pointerMode: root.pointerMode
                        hoverFocusEnabled: root.hoverFocusEnabled
                        onDpadRequested: Ui.InputMode.keyPressed()
                        onActiveFocusChanged: if (activeFocus) {
                            root.detailRailActive = false;
                            root.episodeStillFocused = false;
                        }
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
                id: detailRail
                objectName: "detailRail"
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
                        delegate: LibraryPill {
                            required property var modelData
                            required property int index
                            objectName: "season-" + modelData.id
                            underlineName: "seasonUnderline-" + modelData.id
                            selected: modelData.id === root.currentSeasonId
                            text: modelData.title || ""
                            pointerMode: root.pointerMode
                            hoverFocusEnabled: root.hoverFocusEnabled
                            onDpadRequested: Ui.InputMode.keyPressed()
                            onActiveFocusChanged: if (activeFocus) {
                                root.detailRailActive = true;
                                root.episodeStillFocused = false;
                            }
                            onActivated: root.focusSeason(index)
                        }
                    }
                }
                Flickable {
                    id: episodeFlick
                    objectName: "episodeScroller"
                    // Keep the scaled/lifted first and last still inside the
                    // clip. Offsetting the row preserves its normal position.
                    readonly property int focusPadding: 16
                    x: -focusPadding
                    width: parent.width + focusPadding
                    height: episodeRow.height + 2 * focusPadding
                    transform: Translate { y: -episodeFlick.focusPadding }
                    visible: episodeRepeater.count > 0
                    contentWidth: episodeRow.width + 2 * focusPadding
                    contentHeight: height
                    clip: true
                    flickableDirection: Flickable.HorizontalFlick
                    boundsBehavior: Flickable.StopAtBounds
                    // Animate focus reveals only. Native pointer flicks retain
                    // their own physics and can interrupt a keyboard reveal.
                    function resetScroll() {
                        episodeScroll.stop();
                        cancelFlick();
                        contentX = 0;
                    }
                    function reveal(cell) {
                        episodeScroll.stop();
                        cancelFlick();
                        const left = cell.x;
                        const right = left + cell.width + 2 * focusPadding;
                        const target = Math.max(0, Math.min(contentWidth - width,
                                left < contentX ? left : right > contentX + width ? right - width : contentX));
                        if (Math.abs(target - contentX) < 0.5)
                            return;
                        episodeScroll.to = target;
                        episodeScroll.start();
                    }
                    onContentXChanged: hoverDelay.restart()
                    onDraggingChanged: if (dragging) episodeScroll.stop()
                    onFlickingChanged: if (flicking) episodeScroll.stop()
                    onContentWidthChanged: {
                        episodeScroll.stop();
                        contentX = Math.max(0, Math.min(contentX, contentWidth - width));
                    }
                    NumberAnimation {
                        id: episodeScroll
                        target: episodeFlick
                        property: "contentX"
                        duration: 450
                        easing.type: Easing.BezierSpline
                        easing.bezierCurve: Ui.Theme.easing
                    }
                    Row {
                        id: episodeRow
                        x: episodeFlick.focusPadding
                        y: episodeFlick.focusPadding
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
                                    cornerRadius: Ui.Theme.stillRadius
                                    focusScale: Ui.Theme.contentFocusScale
                                    focusLift: Ui.Theme.contentFocusLift
                                    liftFromBottom: true
                                    rimEnabled: false
                                    shapeRole: "content"
                                    rowBrightness: still.activeFocus || !root.episodeStillFocused ? 1
                                            : Ui.Theme.rowLitBrightness
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
                                        root.detailRailActive = true;
                                        root.episodeStillFocused = true;
                                        root.episodeIndex = index;
                                        episodeFlick.reveal(episodeCell);
                                    }
                                    onActivated: root.episodeRequested(root.detailEntryId, modelData.id)
                                    contentItem: Item {
                                        Rectangle {
                                            objectName: "episodePlate-" + modelData.id
                                            anchors.fill: parent
                                            radius: still.cornerRadius
                                            visible: !stillImage.ready
                                            gradient: Gradient {
                                                GradientStop { position: 0.0; color: Qt.rgba(0.29, 0.29, 0.31, 1) }
                                                GradientStop { position: 1.0; color: Qt.rgba(0.21, 0.21, 0.23, 1) }
                                            }
                                            FilmGlyph {
                                                objectName: "episodeGlyph-" + modelData.id
                                                anchors.centerIn: parent
                                            }
                                        }
                                        RoundedArtwork {
                                            id: stillImage
                                            objectName: "episodeImage-" + modelData.id
                                            anchors.fill: parent
                                            cornerRadius: still.cornerRadius
                                            source: modelData.artwork ? modelData.artwork : ""
                                        }
                                        Rectangle {
                                            objectName: "episodeShade-" + modelData.id
                                            anchors.fill: parent
                                            radius: still.cornerRadius
                                            color: "black"
                                            opacity: Math.max(0, 1 - still.rowBrightness)
                                            z: 8
                                        }
                                        Ui.ProgressTrack {
                                            objectName: "episodeProgress-" + modelData.id
                                            anchors.left: parent.left
                                            anchors.right: parent.right
                                            anchors.bottom: parent.bottom
                                            line: true
                                            visible: (modelData.progress || 0) > 0
                                            value: modelData.progress || 0
                                            z: 9
                                        }
                                        Ui.ArtworkFocus {
                                            shown: still.activeFocus
                                            artworkRadius: still.cornerRadius
                                        }
                                        Ui.CastLight {
                                            z: -1
                                            shown: still.activeFocus
                                            width: parent.width * Ui.Theme.castWidthRatio
                                            anchors.horizontalCenter: parent.horizontalCenter
                                            anchors.top: parent.bottom
                                            anchors.topMargin: -Math.round(Ui.Theme.castHeight * 0.4)
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
                                    property real lift: still.lift
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
