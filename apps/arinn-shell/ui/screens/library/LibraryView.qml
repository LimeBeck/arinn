// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import QtQuick.Window
import "../.." as Ui

// Fixture-driven catalogue. Properties are presentation data; signals are
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
    property bool pointerMode: false
    readonly property bool hoverFocusEnabled: !hoverDelay.running
    readonly property bool detailOpen: detailEntryId.length > 0
    readonly property int posterWidth: 210
    readonly property int posterHeight: 315
    readonly property int gridGapX: 40
    readonly property int gridGapY: 48
    readonly property int marginLeft: 120
    readonly property int marginRight: 96
    readonly property int columnCount: {
        if (columns > 0)
            return columns;
        const usable = width - marginLeft - marginRight + gridGapX;
        return Math.max(1, Math.floor(usable / (posterWidth + gridGapX)));
    }
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
    // Survives delegate destruction so a removed poster can restore a neighbour.
    property string focusPlace: "tab"
    readonly property int skeletonSlots: 8

    signal tabRequested(string tabId)
    signal entryRequested(string entryId)
    signal playRequested(string entryId)
    signal episodeRequested(string entryId, string episodeId)
    signal removeRequested(string entryId)
    signal retryRequested()
    signal backRequested()
    signal homeRequested()

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
        item.forceActiveFocus();
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
        item.forceActiveFocus();
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
        const top = cell.y;
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
        libraryStatus.forceActiveFocus();
        return true;
    }
    function focusDetailAction() {
        const entry = detailEntry;
        const button = entry && entry.missing ? detailRemove : detailPlay;
        if (button && button.visible)
            button.forceActiveFocus();
    }
    function focusEpisode(index) {
        const episodes = detailEntry && detailEntry.episodes ? detailEntry.episodes : [];
        if (index < 0 || index >= episodes.length)
            return false;
        const item = episodeRepeater.itemAt(index);
        if (!item)
            return false;
        episodeIndex = index;
        item.forceActiveFocus();
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
        const episodes = detailEntry && detailEntry.episodes ? detailEntry.episodes : [];
        const focused = Window.activeFocusItem;
        const onEpisode = focused && String(focused.objectName).indexOf("episode-") === 0
                && String(focused.objectName).indexOf("episode-missing-") !== 0;
        if (key === Qt.Key_Down && !onEpisode && episodes.length) {
            focusEpisode(Math.min(episodeIndex, episodes.length - 1));
            return;
        }
        if (key === Qt.Key_Up && onEpisode) {
            focusDetailAction();
            return;
        }
        if (onEpisode && (key === Qt.Key_Left || key === Qt.Key_Right)) {
            const step = key === Qt.Key_Right ? 1 : -1;
            const next = episodeIndex + step;
            if (next >= 0 && next < episodes.length)
                focusEpisode(next);
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
        if (detailOpen || !showGrid || focusPlace !== "poster")
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
    function refocusAfterState() {
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
    Component.onCompleted: {
        if (!currentTabId.length && tabs && tabs.length)
            currentTabId = tabs[0].id;
        Qt.callLater(focusInitial);
    }

    Timer {
        id: hoverDelay
        interval: 500
    }
    HoverHandler {
        onPointChanged: root.pointerMode = true
        cursorShape: root.pointerMode ? Qt.ArrowCursor : Qt.BlankCursor
    }
    Keys.onPressed: event => {
        if ([Qt.Key_Left, Qt.Key_Right, Qt.Key_Up, Qt.Key_Down].includes(event.key))
            root.pointerMode = false;
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
        if (event.key === Qt.Key_Escape || event.key === Qt.Key_Backspace) {
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
        y: 72
        text: qsTr("Library")
        role: "title"
    }
    Row {
        id: tabRow
        x: root.marginLeft
        y: 168
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
                implicitHeight: 64
                cornerRadius: 32
                focusScale: 1.02
                text: modelData.title || ""
                restingColor: modelData.id === root.currentTabId ? Qt.rgba(1, 1, 1, 0.16) : Ui.Theme.plate
                pointerMode: root.pointerMode
                hoverFocusEnabled: root.hoverFocusEnabled
                onDpadRequested: root.pointerMode = false
                onActivated: root.focusTab(index)
                contentItem: Ui.UiText {
                    id: tabLabel
                    text: tabButton.text
                    role: "control"
                    horizontalAlignment: Text.AlignHCenter
                    verticalAlignment: Text.AlignVCenter
                }
                Rectangle {
                    anchors.horizontalCenter: parent.horizontalCenter
                    anchors.bottom: parent.bottom
                    anchors.bottomMargin: 8
                    width: tabLabel.implicitWidth
                    height: 4
                    radius: 2
                    color: Ui.Theme.textPrimary
                    visible: modelData.id === root.currentTabId
                }
            }
        }
    }

    Item {
        id: body
        x: root.marginLeft
        y: 260
        width: parent.width - root.marginLeft - root.marginRight
        height: parent.height - y - 36

        Ui.StatusPane {
            id: libraryStatus
            objectName: "libraryStatus"
            anchors.horizontalCenter: parent.horizontalCenter
            width: Math.min(820, parent.width)
            visible: root.showStatus
            kind: root.statusKind
            title: root.resolvedStatusTitle
            description: root.resolvedStatusDescription
            actionText: root.resolvedStatusAction
            pointerMode: root.pointerMode
            hoverFocusEnabled: root.hoverFocusEnabled
            onDpadRequested: root.pointerMode = false
            onActionRequested: root.retryRequested()
        }

        Grid {
            id: skeletonGrid
            objectName: "librarySkeletons"
            visible: root.libraryState === "loading" && !root.detailOpen
            columns: root.columnCount
            columnSpacing: root.gridGapX
            rowSpacing: root.gridGapY
            Repeater {
                model: root.skeletonSlots
                delegate: Ui.Skeleton {
                    required property int index
                    objectName: "skeleton-" + index
                    width: root.posterWidth
                    height: root.posterHeight
                }
            }
        }

        Flickable {
            id: scroller
            objectName: "libraryGrid"
            anchors.fill: parent
            visible: root.showGrid
            clip: true
            contentWidth: posterGrid.width
            contentHeight: posterGrid.height
            boundsBehavior: Flickable.StopAtBounds
            flickableDirection: Flickable.VerticalFlick
            onContentYChanged: hoverDelay.restart()
            Grid {
                id: posterGrid
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
                        width: root.posterWidth
                        height: root.posterHeight + 88
                        function focusPoster() {
                            poster.forceActiveFocus();
                        }
                        Ui.Surface {
                            id: poster
                            objectName: "poster-" + modelData.id
                            width: root.posterWidth
                            height: root.posterHeight
                            text: modelData.title || ""
                            enabled: modelData.enabled !== false
                            busy: !!modelData.busy
                            pointerMode: root.pointerMode
                            hoverFocusEnabled: root.hoverFocusEnabled
                            onDpadRequested: root.pointerMode = false
                            onActiveFocusChanged: {
                                if (activeFocus) {
                                    root.focusedPosterId = modelData.id;
                                    root.focusedPosterIndex = index;
                                    root.focusPlace = "poster";
                                }
                            }
                            onActivated: root.openEntry(modelData.id)
                            contentItem: Item {
                                Image {
                                    anchors.fill: parent
                                    fillMode: Image.PreserveAspectCrop
                                    asynchronous: true
                                    source: modelData.artwork ? modelData.artwork : ""
                                    visible: status === Image.Ready
                                }
                                Ui.UiText {
                                    anchors.left: parent.left
                                    anchors.right: parent.right
                                    anchors.bottom: parent.bottom
                                    anchors.margins: 16
                                    text: modelData.title || ""
                                    role: "meta"
                                    maximumLineCount: 3
                                    elide: Text.ElideRight
                                    visible: !modelData.artwork
                                }
                            }
                            Rectangle {
                                objectName: "missing-" + modelData.id
                                visible: !!modelData.missing
                                z: 40
                                x: 12
                                y: parent.height - height - 12
                                height: 40
                                radius: 20
                                width: Math.min(parent.width - 24, missingLabel.implicitWidth + 28)
                                color: "#cc08090b"
                                Ui.UiText {
                                    id: missingLabel
                                    anchors.centerIn: parent
                                    text: qsTr("No file")
                                    role: "caption"
                                    color: Ui.Theme.error
                                }
                            }
                        }
                        Ui.UiText {
                            anchors.top: poster.bottom
                            anchors.topMargin: 12
                            width: parent.width
                            text: modelData.title || ""
                            role: "meta"
                            maximumLineCount: 2
                            elide: Text.ElideRight
                        }
                        Ui.UiText {
                            anchors.bottom: parent.bottom
                            width: parent.width
                            text: modelData.subtitle || ""
                            role: "caption"
                            color: Ui.Theme.textSecondary
                            maximumLineCount: 1
                            elide: Text.ElideRight
                        }
                    }
                }
            }
        }
    }

    Rectangle {
        anchors.fill: parent
        visible: root.detailOpen
        color: Ui.Theme.background
        z: 5
        Column {
            x: root.marginLeft
            y: 96
            width: 960
            spacing: 20
            Ui.UiText {
                width: parent.width
                role: "eyebrow"
                text: root.detailEntry && root.detailEntry.kind === "series" ? qsTr("Series")
                        : root.detailEntry && root.detailEntry.kind === "film" ? qsTr("Film") : ""
                visible: text.length > 0
                color: Ui.Theme.textSecondary
            }
            Ui.UiText {
                objectName: "detailTitle"
                width: parent.width
                role: "hero"
                text: root.detailEntry ? (root.detailEntry.title || "") : ""
                maximumLineCount: 2
                elide: Text.ElideRight
            }
            Ui.UiText {
                width: parent.width
                role: "body"
                text: root.detailEntry ? (root.detailEntry.subtitle || "") : ""
                color: Ui.Theme.textSecondary
                visible: text.length > 0
            }
            Ui.UiText {
                width: parent.width
                role: "body"
                text: root.detailEntry ? (root.detailEntry.description || "") : ""
                color: Ui.Theme.textSecondary
                maximumLineCount: 3
                elide: Text.ElideRight
                visible: text.length > 0
            }
            Row {
                objectName: "detailMissing"
                visible: !!(root.detailEntry && root.detailEntry.missing)
                spacing: 16
                Rectangle {
                    width: 64
                    height: 64
                    radius: 20
                    color: Ui.Theme.plate
                    Ui.UiText {
                        anchors.centerIn: parent
                        text: "!"
                        role: "title"
                        color: Ui.Theme.error
                    }
                }
                Column {
                    width: 760
                    spacing: 4
                    Ui.UiText {
                        width: parent.width
                        role: "section"
                        text: root.detailEntry && root.detailEntry.missingTitle
                                ? root.detailEntry.missingTitle
                                : qsTr("This file is no longer available")
                    }
                    Ui.UiText {
                        width: parent.width
                        role: "meta"
                        color: Ui.Theme.textSecondary
                        text: root.detailEntry && root.detailEntry.missingDescription
                                ? root.detailEntry.missingDescription
                                : qsTr("The entry stays in the catalogue.")
                    }
                }
            }
            Row {
                spacing: 20
                Ui.ActionButton {
                    id: detailPlay
                    objectName: "detailPlay"
                    visible: !!(root.detailEntry && !root.detailEntry.missing)
                    text: root.detailEntry && root.detailEntry.progress > 0 ? qsTr("Continue") : qsTr("Play")
                    pointerMode: root.pointerMode
                    hoverFocusEnabled: root.hoverFocusEnabled
                    onDpadRequested: root.pointerMode = false
                    onActivated: root.playRequested(root.detailEntryId)
                }
                Ui.ActionButton {
                    id: detailRemove
                    objectName: "detailRemove"
                    visible: !!(root.detailEntry && root.detailEntry.missing)
                    text: qsTr("Remove from library")
                    pointerMode: root.pointerMode
                    hoverFocusEnabled: root.hoverFocusEnabled
                    onDpadRequested: root.pointerMode = false
                    onActivated: root.removeRequested(root.detailEntryId)
                }
            }
            Ui.ProgressTrack {
                objectName: "detailProgress"
                visible: !!(root.detailEntry && !root.detailEntry.missing && root.detailEntry.progress > 0)
                width: 300
                value: root.detailEntry && root.detailEntry.progress ? root.detailEntry.progress : 0
            }
            Flickable {
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
                    spacing: 24
                    Repeater {
                        id: episodeRepeater
                        model: root.detailEntry && root.detailEntry.episodes ? root.detailEntry.episodes : []
                        delegate: Ui.Surface {
                            id: episodeButton
                            required property var modelData
                            required property int index
                            objectName: "episode-" + modelData.id
                            width: 280
                            height: 158
                            text: modelData.title || ""
                            enabled: modelData.enabled !== false
                            pointerMode: root.pointerMode
                            hoverFocusEnabled: root.hoverFocusEnabled
                            onDpadRequested: root.pointerMode = false
                            onActiveFocusChanged: if (activeFocus) root.episodeIndex = index
                            onActivated: root.episodeRequested(root.detailEntryId, modelData.id)
                            contentItem: Item {
                                Ui.UiText {
                                    anchors.left: parent.left
                                    anchors.right: parent.right
                                    anchors.bottom: parent.bottom
                                    anchors.margins: 16
                                    text: modelData.title || ""
                                    role: "meta"
                                    maximumLineCount: 2
                                    elide: Text.ElideRight
                                }
                            }
                            Rectangle {
                                objectName: "episode-missing-" + modelData.id
                                visible: !!modelData.missing
                                z: 40
                                x: 12
                                y: parent.height - height - 12
                                height: 36
                                radius: 18
                                width: Math.min(parent.width - 24, episodeMissingLabel.implicitWidth + 24)
                                color: "#cc08090b"
                                Ui.UiText {
                                    id: episodeMissingLabel
                                    anchors.centerIn: parent
                                    text: qsTr("No file")
                                    role: "caption"
                                    color: Ui.Theme.error
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
