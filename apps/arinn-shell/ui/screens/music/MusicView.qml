// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtQuick.Templates as T
import QtQuick.Window
import "../.." as Ui

FocusScope {
    id: root
    // The logical canvas, as every section screen: without a size the
    // background, the dialog scrims and the keyboard panel anchored to the
    // bottom collapsed to nothing in the shell (#756 M6, M7).
    width: Ui.Theme.canvasWidth
    height: Ui.Theme.canvasHeight
    required property var catalog
    property string mode: "catalogue"
    property string facet: ""
    property bool pickingSource: false
    property string parentId: ""
    property string parentTitle: ""
    property string parentKind: ""
    property string pendingPlay: ""
    property bool typing: false
    property var rememberedFocus: null
    readonly property var hostWindow: Window.window
    function containsFocusItem(item) {
        for (let current = item; current; current = current.parent)
            if (current === root)
                return true;
        return false;
    }
    function recoverFocus() {
        if (!root.visible || !root.enabled || !root.activeFocus || (root.mode !== "catalogue" && root.transportBusy))
            return;
        const current = hostWindow ? hostWindow.activeFocusItem : null;
        if (current && current !== root && containsFocusItem(current) && current.visible && current.enabled)
            return;
        if (rememberedFocus && rememberedFocus.visible && rememberedFocus.enabled)
            Ui.Focus.take(rememberedFocus);
        else
            focusInitial();
    }
    onSelectedChanged: {
        const facets = selected.facets || [];
        if (facets.length && !facets.includes(root.facet))
            root.facet = facets[0];
        Qt.callLater(recoverFocus);
    }
    Connections {
        target: root.hostWindow
        function onActiveFocusItemChanged() {
            const item = root.hostWindow.activeFocusItem;
            if (item && item !== root && root.containsFocusItem(item) && item.visible && item.enabled)
                root.rememberedFocus = item;
            if (item !== root.fallbackFocus)
                root.fallbackFocus = null;
        }
    }
    Connections {
        target: root.catalog
        ignoreUnknownSignals: true
        function onBrowseReset() {
            root.cancelRestore();
            root.parentId = "";
            root.parentTitle = "";
            root.parentKind = "";
            root.facet = "";
        }
        function onStatusChanged() {
            Qt.callLater(root.recoverFocus);
        }
        function onBusyChanged() {
            Qt.callLater(root.recoverFocus);
        }
        function onTransportBusyChanged() {
            Qt.callLater(root.recoverFocus);
        }
        function onEntriesChanged() {
            Qt.callLater(root.restoreOpener);
        }
    }
    function startTyping(target) {
        typingTarget = target || search;
        typing = true;
        Qt.callLater(searchKeyboard.start);
    }
    function finishTyping() {
        searchKeyboard.finish();
        typing = false;
        Ui.Focus.take(typingTarget);
    }
    onVisibleChanged: if (!visible) {
        playerFromHome = false;
        restoreEntity = "";
        restorePages = 0;
        searchKeyboard.finish();
        typing = false;
        namingPlaylist = false;
        pendingPlay = "";
        pickingSource = false;
        queueOccurrence = "";
        deletingPlaylist = false;
    }
    readonly property var selected: catalog.state || ({})
    readonly property var player: catalog.activeState || ({})
    readonly property var current: player.current ? player.current.track : ({})
    readonly property var actions: player.actions || []
    readonly property bool transportBusy: catalog.transportBusy === true
    readonly property bool playing: player.playback === "playing"
    // The state, not the failure: the cause and Retry sit under the
    // controls (#756 M8).
    readonly property string playerHeading: playing ? qsTr("Now playing")
        : player.playback === "loading" ? qsTr("Loading music…")
        : player.playback === "paused" ? qsTr("Paused")
        : qsTr("Stopped")
    readonly property string playbackProblem: catalog.playbackProblem || ""
    // Retry plays the failed track again, or asks a station for more.
    readonly property bool canRetryPlayback: playbackProblem.length > 0
        && !transportBusy
        && (supported("retry") || (!player.continuous && !!current.id))
    // Only while Retry is offered: a second OK on a hidden button must not
    // restart a track that just started (review of #797).
    function retryPlayback() {
        if (!canRetryPlayback)
            return;
        if (supported("retry"))
            catalog.transport("retry");
        else
            catalog.transport("play", current.id || "");
    }
    signal backRequested
    signal settingsRequested
    function catalogueAction() {
        if (connectButton.visible)
            return connectButton;
        if (authorizeButton.visible)
            return authorizeButton;
        if (cancelAuthButton.visible)
            return cancelAuthButton;
        if (rescanButton.visible)
            return rescanButton;
        if (disconnectButton.visible)
            return disconnectButton;
        return settingsButton;
    }
    function detailAction() {
        return saveGroup.visible ? saveGroup : removeGroup.visible ? removeGroup : catalogueAction();
    }
    function belowSearch() {
        Ui.Focus.take(tracks.count && tracks.visible ? tracks : catalogueAction());
    }
    // 3:05, or 1:15:03 from an hour on (#756 M25).
    function timeLabel(microseconds) {
        const seconds = Math.max(0, Math.floor((microseconds || 0) / 1000000));
        const hours = Math.floor(seconds / 3600);
        const minutes = Math.floor(seconds % 3600 / 60);
        const rest = String(seconds % 60).padStart(2, "0");
        return hours > 0 ? hours + ":" + String(minutes).padStart(2, "0") + ":" + rest : minutes + ":" + rest;
    }
    function supported(action) {
        return actions.includes(action);
    }
    // The control row is the last row: Down leaves it only for the failure's
    // Retry, never sideways inside the row (#756 M15).
    readonly property Item belowControls: retryPlaybackButton.visible ? retryPlaybackButton : null
    // Play/Pause first (#756 M13). While it is disabled (a command in
    // flight, a track loading) the nearest working control holds focus,
    // and Play/Pause takes it back once it works, unless the user moved on.
    property Item fallbackFocus: null
    function playerFocus() {
        if (playButton.enabled)
            return playButton;
        return [retryPlaybackButton, retryButton, stopButton, nextButton, previousButton]
            .find(item => item.visible && item.enabled) || queueButton;
    }
    function focusInitial() {
        if (mode === "player") {
            const target = playerFocus();
            fallbackFocus = target === playButton ? null : target;
            Ui.Focus.take(target);
        } else if (mode === "queue")
            Ui.Focus.take(queueList);
        else if (tracks.visible && tracks.count)
            Ui.Focus.take(tracks);
        else
            Ui.Focus.take(catalogueAction());
    }
    Connections {
        target: playButton
        function onEnabledChanged() {
            if (playButton.enabled && root.fallbackFocus && root.fallbackFocus.activeFocus) {
                root.fallbackFocus = null;
                Ui.Focus.take(playButton);
            }
        }
    }
    // Back from a player opened on Home's card returns to Home (#756 M16).
    property bool playerFromHome: false
    // Reopening the player on screen (a confirmed replacement, radio from
    // this track) keeps where it was opened from.
    function openPlayer() {
        if (mode !== "player")
            playerFromHome = false;
        mode = "player";
        Qt.callLater(focusInitial);
    }
    function openPlayerFromHome() {
        openPlayer();
        playerFromHome = true;
    }
    // The album, artist or playlist a detail was opened from; Back focuses
    // it again once the list is back (#756 M16).
    // An entity from a later page keeps the restore pending while more
    // pages load, up to a bound.
    property string restoreEntity: ""
    property int restorePages: 0
    readonly property int restorePageLimit: 10
    // Any other list the user asks for, a source change or a failed page
    // ends the search for the opener (review of #798).
    function cancelRestore() {
        restoreEntity = "";
        restorePages = 0;
    }
    function browse(facetId, parent, query) {
        cancelRestore();
        catalog.browse(facetId, parent, query);
    }
    readonly property string catalogueError: catalog.errorText || ""
    onCatalogueErrorChanged: if (catalogueError.length)
        cancelRestore()
    function restoreOpener() {
        if (!restoreEntity.length || !catalog.entries.length)
            return;
        const index = catalog.entries.findIndex(entry => entry.id === restoreEntity);
        if (index < 0 && catalog.more && restorePages < restorePageLimit) {
            restorePages++;
            catalog.loadMore();
            return;
        }
        restoreEntity = "";
        restorePages = 0;
        if (index >= 0) {
            tracks.currentIndex = index;
            tracks.positionViewAtIndex(index, ListView.Contain);
        }
        // Only while focus is still in the list, or nowhere in the view: a
        // user who moved on keeps their focus (review of #798).
        const item = hostWindow ? hostWindow.activeFocusItem : null;
        const inList = !!item && (item === tracks || isInside(item, tracks));
        const elsewhere = !!item && item !== root && containsFocusItem(item);
        if (tracks.visible && (inList || !elsewhere))
            Ui.Focus.take(tracks);
    }
    function isInside(item, scope) {
        for (let current = item; current; current = current.parent)
            if (current === scope)
                return true;
        return false;
    }
    property string pendingAction: "play"
    property bool pendingActive: false
    property bool namingPlaylist: false
    property string playlistEntity: ""
    property bool playlistActive: false
    property var playlistOpener: null
    property string queueOccurrence: ""
    property bool deletingPlaylist: false
    property var typingTarget: search
    readonly property var facetLabels: ({
            albums: qsTr("Albums"),
            artists: qsTr("Artists"),
            playlists: qsTr("Playlists"),
            tracks: qsTr("Tracks"),
            selections: qsTr("For you")
        })
    function startCommand(action, id, active) {
        if (active)
            catalog.transport(action, id);
        else
            catalog.command(action, id);
    }
    function namePlaylist(entity, active) {
        playlistEntity = entity;
        playlistActive = active;
        playlistOpener = hostWindow ? hostWindow.activeFocusItem : null;
        playlistName.text = active ? "" : root.parentTitle;
        namingPlaylist = true;
        Qt.callLater(() => Ui.Focus.take(playlistName));
    }
    function closePlaylist() {
        if (typing)
            finishTyping();
        namingPlaylist = false;
        Qt.callLater(() => {
            if (!Ui.Focus.take(playlistOpener))
                focusInitial();
        });
    }
    function confirmPlaylist() {
        catalog[playlistActive ? "transport" : "command"]("save_playlist", playlistEntity, "", 0, playlistName.text);
        closePlaylist();
    }

    function requestRadio(id, active) {
        pendingAction = "radio";
        pendingActive = active === true;
        requestStart(id);
    }
    function requestPlay(id) {
        pendingAction = "play";
        pendingActive = false;
        requestStart(id);
    }
    function requestStart(id) {
        const live = player.session && ["playing", "paused", "loading"].includes(player.playback);
        if (player.continuous === true || live) {
            pendingPlay = id;
            Qt.callLater(() => Ui.Focus.take(cancelReplace));
        } else {
            startCommand(pendingAction, id, pendingActive);
            openPlayer();
        }
    }
    function back() {
        if (namingPlaylist) {
            if (typing)
                finishTyping();
            else
                closePlaylist();
        } else if (queueOccurrence) {
            queueOccurrence = "";
            Ui.Focus.take(queueList.currentItem || queueList);
        } else if (deletingPlaylist) {
            deletingPlaylist = false;
            focusInitial();
        } else if (typing) {
            finishTyping();
        } else if (pendingPlay) {
            pendingPlay = "";
            focusInitial();
        } else if (mode === "queue") {
            mode = "player";
            Qt.callLater(() => {
                if (!Ui.Focus.take(queueButton))
                    focusInitial();
            });
        } else if (mode === "player" && playerFromHome) {
            playerFromHome = false;
            mode = "catalogue";
            backRequested();
        } else if (mode === "player") {
            mode = "catalogue";
            Qt.callLater(focusInitial);
        } else if (parentId) {
            restoreEntity = parentId;
            restorePages = 0;
            parentId = "";
            parentTitle = "";
            parentKind = "";
            catalog.browse(facet, "", search.text);
        } else
            backRequested();
    }
    Keys.onPressed: event => {
        if ((namingPlaylist && !typing) || queueOccurrence || deletingPlaylist) {
            if ([Qt.Key_Escape, Qt.Key_Back, Qt.Key_Backspace].includes(event.key))
                root.back();
            else if (namingPlaylist && [Qt.Key_Down, Qt.Key_Tab].includes(event.key))
                Ui.Focus.take(playlistSave.enabled ? playlistSave : playlistCancel);
            else if (namingPlaylist && [Qt.Key_Left, Qt.Key_Right].includes(event.key))
                Ui.Focus.take(playlistCancel.activeFocus && playlistSave.enabled ? playlistSave : playlistCancel);
            else if (queueOccurrence && [Qt.Key_Left, Qt.Key_Right, Qt.Key_Tab].includes(event.key))
                Ui.Focus.take(queueKeep.activeFocus ? queueRemove : queueKeep);
            else if (deletingPlaylist && [Qt.Key_Left, Qt.Key_Right, Qt.Key_Tab].includes(event.key))
                Ui.Focus.take(keepPlaylist.activeFocus ? deletePlaylist : keepPlaylist);
            event.accepted = true;
            return;
        }
        if (root.pickingSource && [Qt.Key_Escape, Qt.Key_Back, Qt.Key_Backspace].includes(event.key)) {
            root.pickingSource = false;
            Ui.Focus.take(sourcePicker);
            event.accepted = true;
            return;
        }
        if (root.pendingPlay) {
            if ([Qt.Key_Escape, Qt.Key_Back, Qt.Key_Backspace].includes(event.key))
                root.back();
            else if ([Qt.Key_Left, Qt.Key_Right, Qt.Key_Tab, Qt.Key_Backtab].includes(event.key))
                Ui.Focus.take(cancelReplace.activeFocus ? confirmReplace : cancelReplace);
            event.accepted = true;
            return;
        }
        if ([Qt.Key_Escape, Qt.Key_Back, Qt.Key_Backspace].includes(event.key) && !search.activeFocus) {
            back();
            event.accepted = true;
        } else if ([Qt.Key_MediaTogglePlayPause, Qt.Key_MediaPlay, Qt.Key_MediaPause].includes(event.key)) {
            catalog.transport(playing ? "pause" : "resume");
            event.accepted = true;
        }
    }
    Rectangle {
        anchors.fill: parent
        color: Ui.Theme.background
    }
    // An album, artist or playlist is content: its title is Playfair Display
    // (ADR-0137, #756 M24); screen titles stay Onest.
    Ui.UiText {
        objectName: "musicHeading"
        x: 120
        y: 80
        width: 1150
        role: "title"
        readonly property bool contentTitle: root.mode === "catalogue" && root.parentId.length > 0
        font.family: contentTitle ? Ui.Theme.contentFontFamily : Ui.Theme.fontFamily
        // Lining figures, as UiText gives the content roles (ADR-0137).
        font.features: contentTitle ? ({ "tnum": 1, "lnum": 1 }) : ({ "tnum": 1 })
        text: mode === "player" ? root.playerHeading : mode === "queue" ? qsTr("Queue") : parentTitle || qsTr("Music")
        textFormat: Text.PlainText
        maximumLineCount: 1
        elide: Text.ElideRight
    }
    // One status line with its reason; it clears once playback recovers
    // (#756 M5). The player shows a failure with Retry instead.
    Ui.UiText {
        objectName: "musicNotice"
        x: 120
        y: 975
        width: 1680
        text: root.catalog.notice || ""
        role: "caption"
        color: Ui.Theme.textSecondary
        textFormat: Text.PlainText
        maximumLineCount: 1
        elide: Text.ElideRight
        visible: text.length > 0
            && !root.selected.catalogue_loading
            && !(root.mode === "player" && root.playbackProblem.length > 0)
    }
    Row {
        x: 120
        y: 180
        spacing: 20
        enabled: !root.pendingPlay
        visible: root.mode === "catalogue"
        Ui.ActionButton {
            id: sourcePicker
            objectName: "musicSourcePicker"
            width: 360
            height: 76
            text: {
                const source = root.catalog.sources.find(s => s.id === root.catalog.sourceId);
                return source ? source.name : qsTr("Music source");
            }
            contentItem: Ui.UiText {
                text: sourcePicker.text
                role: "control"
                color: sourcePicker.activeFocus ? Ui.Theme.ink : Ui.Theme.textPrimary
                horizontalAlignment: Text.AlignHCenter
                verticalAlignment: Text.AlignVCenter
                maximumLineCount: 1
                elide: Text.ElideRight
            }
            onActivated: {
                root.pickingSource = !root.pickingSource;
                if (root.pickingSource)
                    Qt.callLater(() => Ui.Focus.take(sourceList));
            }
            Keys.onUpPressed: event => {
                event.accepted = true;
            }
            Keys.onDownPressed: Ui.Focus.take((root.selected.facets || []).length ? search : root.catalogueAction())
            Keys.onRightPressed: if (facets.count)
                Ui.Focus.take(facets.itemAt(0))
        }
        Repeater {
            // An unknown facet has no label: it is left out, never shown
            // as its raw id (#756 M25).
            id: facets
            model: (root.selected.facets || []).filter(id => !!root.facetLabels[id])
            Ui.ActionButton {
                required property string modelData
                required property int index
                objectName: "musicFacet-" + index
                Keys.onDownPressed: Ui.Focus.take(search)
                Keys.onLeftPressed: Ui.Focus.take(index ? facets.itemAt(index - 1) : sourcePicker)
                Keys.onRightPressed: if (index + 1 < facets.count)
                    Ui.Focus.take(facets.itemAt(index + 1))
                width: Math.max(220, implicitWidth)
                height: 76
                text: root.facetLabels[modelData]
                onActivated: {
                    root.facet = modelData;
                    root.parentId = "";
                    root.parentTitle = "";
                    root.browse(modelData, "", search.text);
                }
            }
        }
    }
    Rectangle {
        enabled: !root.pendingPlay
        x: 120
        y: 278
        width: catalogueActions.x - 40 - x
        height: 76
        radius: Ui.Theme.containerRadius
        color: Ui.Theme.plate
        visible: root.mode === "catalogue"
        T.TextField {
            id: search
            objectName: "musicSearch"
            anchors.fill: parent
            anchors.margins: 14
            anchors.rightMargin: searchKeyboardButton.width + 28
            placeholderText: qsTr("Search this source")
            font.pixelSize: 28
            color: Ui.Theme.textPrimary
            placeholderTextColor: Ui.Theme.textHint
            background: Rectangle {
                color: "transparent"
            }
            Ui.UiText {
                anchors.fill: parent
                visible: !search.text.length && !search.preeditText.length
                text: search.placeholderText
                role: "control"
                font.pixelSize: 28
                color: Ui.Theme.textHint
                verticalAlignment: Text.AlignVCenter
            }
            onTextChanged: searchDelay.restart()
            Keys.onDownPressed: root.belowSearch()
            Keys.onUpPressed: Ui.Focus.take(sourcePicker)
            Keys.onEscapePressed: root.back()
            Keys.onRightPressed: event => {
                if (cursorPosition === length)
                    Ui.Focus.take(searchKeyboardButton);
                else
                    event.accepted = false;
            }
            Keys.onReturnPressed: root.startTyping()
            Keys.onEnterPressed: root.startTyping()
            Timer {
                id: searchDelay
                interval: 350
                onTriggered: root.browse(root.facet, root.parentId, search.text)
            }
        }
        Ui.KeyboardButton {
            id: searchKeyboardButton
            objectName: "musicKeyboard"
            x: parent.width - width - 12
            anchors.verticalCenter: parent.verticalCenter
            height: 64
            KeyNavigation.right: detailPlay.visible ? detailPlay
                : openPlayerButton.visible ? openPlayerButton
                : settingsButton
            panelOpen: root.typing
            onActivated: root.typing ? root.finishTyping() : root.startTyping()
            Keys.onDownPressed: root.belowSearch()
            Keys.onLeftPressed: Ui.Focus.take(search)
        }
    }
    // Labels set the widths, so longer translations fit (#756 M25); the row
    // keeps its right edge on the safe area.
    Row {
        id: catalogueActions
        x: Ui.Theme.canvasWidth - Ui.Theme.safeRight - 24 - width
        y: 278
        spacing: 20
        visible: root.mode === "catalogue"
        Ui.ActionButton {
            id: openPlayerButton
            objectName: "musicOpenPlayer"
            KeyNavigation.left: searchKeyboardButton
            KeyNavigation.right: settingsButton
            KeyNavigation.down: root.catalogueAction()
            width: Math.max(240, implicitWidth)
            height: 76
            visible: root.mode === "catalogue"
                && (!!root.player.current || root.player.continuous === true)
                && !root.parentId
            text: qsTr("Open player")
            onActivated: root.openPlayer()
        }
        Ui.ActionButton {
            id: detailPlay
            objectName: "musicPlayGroup"
            width: Math.max(240, implicitWidth)
            height: 76
            visible: root.mode === "catalogue" && root.parentId.length > 0
            enabled: !root.catalog.busy && (root.selected.actions || []).includes("play")
            text: qsTr("Play")
            onActivated: root.requestPlay(root.parentId)
            KeyNavigation.left: searchKeyboardButton
            KeyNavigation.right: settingsButton
            KeyNavigation.down: tracks
        }
        Ui.ActionButton {
            id: settingsButton
            objectName: "musicSettings"
            KeyNavigation.left: detailPlay.visible ? detailPlay
                : openPlayerButton.visible ? openPlayerButton
                : searchKeyboardButton
            KeyNavigation.down: root.catalogueAction() === settingsButton ? sourcePicker : root.catalogueAction()
            width: Math.max(220, implicitWidth)
            height: 76
            visible: root.mode === "catalogue"
            text: qsTr("Settings")
            onActivated: root.settingsRequested()
        }
    }
    Ui.ActionButton {
        id: disconnectButton
        objectName: "musicDisconnect"
        KeyNavigation.up: tracks.count ? tracks : search
        KeyNavigation.left: rescanButton.visible ? rescanButton : null
        KeyNavigation.right: settingsButton
        x: 1500
        y: 880
        width: 300
        enabled: !root.pendingPlay && !root.catalog.busy
        visible: root.mode === "catalogue" && (root.selected.actions || []).includes("disconnect")
        text: qsTr("Disconnect")
        onActivated: root.catalog.command("disconnect")
    }
    Ui.ActionButton {
        id: rescanButton
        objectName: "musicRescan"
        KeyNavigation.up: tracks.count ? tracks : search
        KeyNavigation.right: disconnectButton.visible ? disconnectButton : settingsButton
        x: disconnectButton.visible ? 1176 : 1500
        y: 880
        width: 300
        enabled: !root.pendingPlay && !root.catalog.busy
        visible: root.mode === "catalogue" && (root.selected.actions || []).includes("scan")
        text: root.selected.account === "local" ? qsTr("Rescan") : qsTr("Refresh")
        onActivated: root.catalog.command("scan")
    }
    Row {
        x: 120
        y: 880
        spacing: 24
        visible: root.mode === "catalogue" && root.parentId.length > 0
        Ui.ActionButton {
            id: saveGroup
            objectName: "musicSavePlaylist"
            visible: (root.selected.actions || []).includes("save_playlist")
            text: qsTr("Save playlist")
            enabled: !root.catalog.busy
            onActivated: root.namePlaylist(root.parentId, false)
            KeyNavigation.up: tracks
            KeyNavigation.right: removeGroup.visible ? removeGroup : root.catalogueAction()
            KeyNavigation.down: root.catalogueAction()
        }
        Ui.ActionButton {
            id: removeGroup
            objectName: "musicDeletePlaylist"
            visible: (root.selected.actions || []).includes("delete_playlist") && root.parentKind === "playlist"
            text: qsTr("Delete playlist")
            destructive: true
            onActivated: {
                root.deletingPlaylist = true;
                Qt.callLater(() => Ui.Focus.take(keepPlaylist));
            }
            KeyNavigation.up: tracks
            KeyNavigation.left: saveGroup.visible ? saveGroup : tracks
            KeyNavigation.down: root.catalogueAction()
        }
    }
    ListView {
        id: tracks
        objectName: "musicEntries"
        x: 120
        y: 385
        width: 1680
        height: 450
        clip: true
        spacing: 12
        visible: root.mode === "catalogue" && ["local", "connected"].includes(root.selected.account)
        enabled: !root.pendingPlay
        Keys.onUpPressed: event => {
            if (currentIndex <= 0)
                Ui.Focus.take(search);
            else
                event.accepted = false;
        }
        Keys.onDownPressed: event => {
            if (currentIndex >= count - 1) {
                if (root.catalog.more) {
                    tracks.positionViewAtEnd();
                    Qt.callLater(() => Ui.Focus.take(tracks.footerItem));
                } else
                    Ui.Focus.take(root.detailAction());
            } else
                event.accepted = false;
        }
        model: root.catalog.entries
        keyNavigationEnabled: true
        delegate: Ui.Surface {
            id: entry
            shapeRole: "container"
            focusScale: 1
            focusLift: 0
            required property var modelData
            required property int index
            width: tracks.width - 8
            height: 90
            focus: tracks.currentIndex === index
            text: modelData.title || qsTr("Untitled")
            contentItem: Item {
                Row {
                    x: 24
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: 24
                    MusicArtwork {
                        width: 64
                        height: 64
                        radius: Ui.Theme.stillRadius
                        anchors.verticalCenter: parent.verticalCenter
                        source: entry.modelData.image || ""
                    }
                    Column {
                        width: 1400
                        anchors.verticalCenter: parent.verticalCenter
                        Ui.UiText {
                            width: parent.width
                            text: entry.text
                            textFormat: Text.PlainText
                            role: "body"
                            maximumLineCount: 1
                            elide: Text.ElideRight
                            color: Ui.Theme.textPrimary
                        }
                        Ui.UiText {
                            width: parent.width
                            text: {
                                const line = entry.modelData.artist || entry.modelData.album || "";
                                if (entry.modelData.available)
                                    return line;
                                // Readable with its reason, not a bare "Unavailable" (#756 M5).
                                const reason = qsTr("This source cannot play it");
                                return line.length ? qsTr("%1 · %2").arg(line).arg(reason) : reason;
                            }
                            textFormat: Text.PlainText
                            role: "caption"
                            maximumLineCount: 1
                            elide: Text.ElideRight
                            color: Ui.Theme.textSecondary
                        }
                    }
                }
            }
            onActivated: {
                tracks.currentIndex = index;
                if (modelData.kind === "station")
                    root.requestRadio(modelData.id);
                else if (modelData.kind === "track")
                    root.requestPlay(modelData.id);
                else {
                    root.parentId = modelData.id;
                    root.parentKind = modelData.kind;
                    root.parentTitle = text;
                    root.browse(root.facet, modelData.id, search.text);
                }
            }
        }
        footer: Ui.ActionButton {
            id: loadMoreButton
            objectName: "musicLoadMore"
            Keys.onUpPressed: Ui.Focus.take(tracks.currentItem || tracks)
            Keys.onDownPressed: Ui.Focus.take(root.detailAction())
            visible: root.catalog.more
            text: qsTr("Load more")
            enabled: !root.catalog.busy
            onActivated: root.catalog.loadMore()
        }
    }
    Ui.UiText {
        objectName: "musicListPosition"
        x: 120
        y: 850
        role: "caption"
        color: Ui.Theme.textSecondary
        visible: root.mode === "catalogue" && tracks.visible && tracks.count > 0
        text: qsTr("%1 of %2").arg(tracks.currentIndex + 1).arg(tracks.count)
    }
    Column {
        x: 120
        y: 420
        width: 1300
        spacing: 28
        enabled: !root.pendingPlay
        visible: root.mode === "catalogue"
            && (!root.selected.account || !["local", "connected"].includes(root.selected.account))
        Ui.UiText {
            objectName: "musicAccountHeading"
            width: parent.width
            role: "section"
            text: root.selected.account === "restricted" ? qsTr("Music access was denied")
                : root.selected.account === "unavailable" ? qsTr("Music is temporarily unavailable")
                : root.selected.account === "connecting" ? qsTr("Confirm sign-in")
                : root.selected.account === "unconfigured" ? qsTr("Connection is not available yet")
                : qsTr("Connect your account")
        }
        Ui.UiText {
            objectName: "musicAccountExplanation"
            width: parent.width
            role: "body"
            textFormat: Text.PlainText
            wrapMode: Text.WordWrap
            text: root.selected.account === "restricted" ? qsTr("Sign-in succeeded, but this application cannot access music.")
                : root.selected.account === "unavailable" ? qsTr("Sign-in succeeded. Music is temporarily unavailable; try again.")
                : root.selected.prompt ? root.selected.prompt.verification_url
                : root.selected.account === "connecting" ? qsTr("Complete sign-in in the browser. Arinn will check music access and return here.")
                : qsTr("Sign in on the provider's page. Your password stays there.")
        }
        Ui.UiText {
            width: parent.width
            role: "hero"
            visible: !!root.selected.prompt
            textFormat: Text.PlainText
            text: root.selected.prompt ? root.selected.prompt.user_code : ""
        }
        Ui.UiText {
            width: parent.width
            role: "caption"
            visible: !!root.selected.prompt
            text: root.selected.prompt ? qsTr("Waiting for confirmation · %1 seconds remaining").arg(root.selected.prompt.expires_in)
                : ""
        }
        Row {
            spacing: 24
            Ui.ActionButton {
                id: authorizeButton
                objectName: "musicAuthorize"
                KeyNavigation.up: sourcePicker
                KeyNavigation.right: cancelAuthButton
                visible: (root.selected.actions || []).includes("authorize")
                text: qsTr("Open sign-in in browser")
                onActivated: root.catalog.command("authorize")
            }
            Ui.ActionButton {
                id: connectButton
                objectName: "musicConnect"
                KeyNavigation.up: sourcePicker
                KeyNavigation.right: settingsButton
                visible: (root.selected.actions || []).includes("connect")
                text: qsTr("Connect")
                onActivated: root.catalog.command("connect")
            }
            Ui.ActionButton {
                id: cancelAuthButton
                objectName: "musicCancelAuth"
                KeyNavigation.up: sourcePicker
                KeyNavigation.left: authorizeButton.visible ? authorizeButton : null
                KeyNavigation.right: settingsButton
                visible: (root.selected.actions || []).includes("cancel")
                text: qsTr("Cancel")
                onActivated: root.catalog.command("cancel")
            }
        }
    }
    Ui.UiText {
        x: 120
        y: 460
        width: 1400
        visible: root.mode === "catalogue" && root.selected.catalogue_loading === true
        role: "section"
        text: qsTr("Loading music…")
    }
    Ui.UiText {
        objectName: "musicEmptyCatalogue"
        x: 120
        y: 460
        width: 1400
        visible: root.mode === "catalogue"
            && tracks.visible
            && !tracks.count
            && !root.catalog.busy
            && !root.selected.catalogue_loading
            && !root.selected.error
            && !root.catalog.errorText
        role: "section"
        text: root.selected.account === "connected" && !(root.selected.facets || []).length ? qsTr("Account connected. Playback is not available yet.")
            : search.text.length ? qsTr("Nothing found")
            : root.selected.account === "local" ? qsTr("Add music folders in Settings, then rescan.")
            : qsTr("No music here yet. Try search or another section.")
    }
    Item {
        enabled: !root.pendingPlay
        x: 120
        y: 235
        width: 1680
        height: 680
        visible: root.mode === "player"
        MusicArtwork {
            x: 0
            y: 0
            width: 500
            height: 500
            radius: Ui.Theme.stillRadius
            source: root.current.image || ""
        }
        Column {
            objectName: "musicInfoColumn"
            x: 580
            width: 1100
            spacing: 16
            Ui.UiText {
                width: parent.width
                role: "eyebrow"
                text: root.catalog.activeSourceName + (root.player.station ? " · " + root.player.station.title : "")
                maximumLineCount: 1
                elide: Text.ElideRight
                textFormat: Text.PlainText
                color: Ui.Theme.textSecondary
            }
            Ui.UiText {
                objectName: "musicTrackTitle"
                width: parent.width
                height: 176
                role: "hero"
                text: root.current.title || qsTr("Nothing is playing")
                textFormat: Text.PlainText
                fitSteps: [[96, 1], [80, 1], [64, 2]]
                elide: Text.ElideRight
                verticalAlignment: Text.AlignVCenter
            }
            Ui.UiText {
                width: parent.width
                role: "section"
                text: root.current.artist || ""
                textFormat: Text.PlainText
                maximumLineCount: 1
                elide: Text.ElideRight
                color: Ui.Theme.textSecondary
            }
            Ui.UiText {
                width: parent.width
                role: "body"
                text: root.current.album || ""
                textFormat: Text.PlainText
                maximumLineCount: 1
                elide: Text.ElideRight
                color: Ui.Theme.textSecondary
            }
            T.Slider {
                id: seek
                objectName: "musicSeek"
                height: 32
                stepSize: 10000000
                Keys.onRightPressed: {
                    seek.increase();
                    root.catalog.transport("seek", "", "", Math.round(seek.value));
                }
                Keys.onLeftPressed: {
                    seek.decrease();
                    root.catalog.transport("seek", "", "", Math.round(seek.value));
                }
                KeyNavigation.down: playButton
                width: parent.width
                from: 0
                to: root.current.duration_us || 1
                value: root.player.position_us || 0
                enabled: root.supported("seek")
                Accessible.name: qsTr("Playback position")
                onMoved: root.catalog.transport("seek", "", "", Math.round(value))
                background: Rectangle {
                    x: seek.leftPadding
                    y: seek.topPadding + (seek.availableHeight - height) / 2
                    width: seek.availableWidth
                    height: 4
                    radius: 2
                    color: Ui.Theme.plateAction
                    Rectangle {
                        width: seek.visualPosition * parent.width
                        height: parent.height
                        radius: 2
                        color: Ui.Theme.textPrimary
                    }
                }
                handle: Rectangle {
                    x: seek.leftPadding + seek.visualPosition * (seek.availableWidth - width)
                    y: seek.topPadding + (seek.availableHeight - height) / 2
                    width: seek.activeFocus ? 24 : 16
                    height: width
                    radius: width / 2
                    color: Ui.Theme.textPrimary
                }
            }
            Ui.UiText {
                role: "caption"
                objectName: "musicElapsed"
                text: root.timeLabel(root.player.position_us)
                    + (root.current.duration_us ? " / " + root.timeLabel(root.current.duration_us) : "")
                color: Ui.Theme.textSecondary
            }
        }
        Item {
            objectName: "musicControlsB"
            x: 580
            y: 460
            width: 1100
            height: 112
            Row {
                x: 0
                spacing: 16
                MusicButton {
                    id: likeButton
                    selected: root.player.feedback === "liked"
                    objectName: "musicLike"
                    KeyNavigation.right: dislikeButton
                    KeyNavigation.up: seek
                    KeyNavigation.down: root.belowControls
                    iconName: "thumb_up"
                    text: qsTr("Like")
                    visible: root.supported("like")
                    onActivated: root.catalog.transport("like")
                }
                MusicButton {
                    id: dislikeButton
                    selected: root.player.feedback === "disliked"
                    objectName: "musicDislike"
                    KeyNavigation.left: likeButton
                    KeyNavigation.right: previousButton
                    KeyNavigation.up: seek
                    KeyNavigation.down: root.belowControls
                    iconName: "thumb_down"
                    text: qsTr("Not for me")
                    visible: root.supported("dislike")
                    onActivated: root.catalog.transport("dislike")
                }
            }
            Row {
                x: 330
                spacing: 24
                MusicButton {
                    id: previousButton
                    objectName: "musicPrevious"
                    KeyNavigation.left: dislikeButton
                    KeyNavigation.right: playButton
                    KeyNavigation.down: root.belowControls
                    KeyNavigation.up: seek
                    iconName: "skip_previous"
                    text: qsTr("Previous")
                    enabled: root.supported("previous") && !root.transportBusy
                    onActivated: root.catalog.transport("previous")
                }
                MusicButton {
                    id: playButton
                    KeyNavigation.left: previousButton
                    KeyNavigation.right: nextButton
                    KeyNavigation.up: seek
                    KeyNavigation.down: root.belowControls
                    objectName: "musicPlayPause"
                    width: 96
                    height: 96
                    iconName: root.playing ? "pause" : "play"
                    text: root.playing ? qsTr("Pause") : qsTr("Play")
                    enabled: !root.transportBusy
                        && (root.supported("pause") || root.supported("resume")
                            || (!root.player.continuous && ["stopped", "ended"].includes(root.player.playback)
                                && !!root.current.id))
                    onActivated: root.catalog.transport(root.playing ? "pause"
                                                        : root.player.playback === "paused" ? "resume" : "play",
                                                        root.current.id || "")
                }
                MusicButton {
                    id: nextButton
                    objectName: "musicNext"
                    KeyNavigation.left: playButton
                    KeyNavigation.right: queueButton
                    KeyNavigation.up: seek
                    KeyNavigation.down: root.belowControls
                    iconName: "skip_next"
                    text: qsTr("Next")
                    enabled: root.supported("next") && !root.transportBusy
                    onActivated: root.catalog.transport("next")
                }
            }
            Row {
                anchors.right: parent.right
                spacing: 12
                MusicButton {
                    id: queueButton
                    width: 64
                    height: 64
                    objectName: "musicQueue"
                    KeyNavigation.left: nextButton
                    KeyNavigation.right: seekButton
                    KeyNavigation.up: seek
                    KeyNavigation.down: root.belowControls
                    iconName: "queue_music"
                    text: qsTr("Queue")
                    onActivated: {
                        root.mode = "queue";
                        Qt.callLater(root.focusInitial);
                    }
                }
                MusicButton {
                    id: seekButton
                    width: 64
                    height: 64
                    objectName: "musicSeekButton"
                    KeyNavigation.left: queueButton
                    KeyNavigation.right: stopButton
                    KeyNavigation.up: seek
                    KeyNavigation.down: root.belowControls
                    iconName: "schedule"
                    text: qsTr("Seek")
                    enabled: root.supported("seek")
                    onActivated: Ui.Focus.take(seek)
                }
                MusicButton {
                    id: stopButton
                    width: 64
                    height: 64
                    objectName: "musicStop"
                    KeyNavigation.left: seekButton
                    KeyNavigation.right: radioButton
                    KeyNavigation.up: seek
                    KeyNavigation.down: root.belowControls
                    iconName: "stop"
                    text: qsTr("Stop")
                    enabled: root.supported("stop") && !root.transportBusy
                    onActivated: root.catalog.transport("stop")
                }
                MusicButton {
                    id: radioButton
                    width: 64
                    height: 64
                    objectName: "musicRadio"
                    KeyNavigation.right: pinButton.visible ? pinButton : retryButton
                    KeyNavigation.left: stopButton
                    KeyNavigation.up: seek
                    KeyNavigation.down: root.belowControls
                    iconName: "radio"
                    text: qsTr("Radio from this track")
                    visible: root.supported("radio") && !!root.current.id
                    onActivated: root.requestRadio(root.current.id || "", true)
                }
                MusicButton {
                    id: pinButton
                    width: 64
                    height: 64
                    iconName: "push_pin"
                    visible: root.supported("pin")
                    selected: root.player.pinned === true
                    text: root.player.pinned ? qsTr("Unpin station") : qsTr("Pin station")
                    enabled: !root.transportBusy
                    onActivated: root.catalog.transport("pin", root.player.station.id)
                    KeyNavigation.left: radioButton
                    KeyNavigation.right: retryButton
                    KeyNavigation.up: seek
                    KeyNavigation.down: root.belowControls
                }
                MusicButton {
                    id: retryButton
                    width: 64
                    height: 64
                    iconName: "refresh"
                    visible: root.supported("retry")
                    text: qsTr("Retry loading tracks")
                    enabled: !root.transportBusy
                    onActivated: root.catalog.transport("retry")
                    KeyNavigation.left: pinButton.visible ? pinButton : radioButton
                    KeyNavigation.up: seek
                    KeyNavigation.down: root.belowControls
                }
            }
        }
        // Why playback stopped, and Retry (#756 M8).
        Row {
            objectName: "musicPlaybackFailure"
            x: 580
            y: 640
            spacing: 32
            visible: root.playbackProblem.length > 0
            Ui.UiText {
                objectName: "musicPlaybackProblem"
                width: retryPlaybackButton.visible ? 1100 - retryPlaybackButton.width - 32 : 1100
                anchors.verticalCenter: parent.verticalCenter
                text: qsTr("Playback failed. %1").arg(root.playbackProblem)
                role: "body"
                color: Ui.Theme.textSecondary
                textFormat: Text.PlainText
                wrapMode: Text.WordWrap
                maximumLineCount: 2
                elide: Text.ElideRight
            }
            Ui.ActionButton {
                id: retryPlaybackButton
                objectName: "musicRetryPlayback"
                visible: root.canRetryPlayback
                // Recovered or busy, the row hides; focus moves on instead of
                // staying on a hidden button (spec §6.1, review of #797).
                onVisibleChanged: if (!visible && activeFocus)
                    Qt.callLater(root.focusInitial)
                text: qsTr("Retry")
                KeyNavigation.up: playButton
                onActivated: root.retryPlayback()
            }
        }
    }
    Ui.ActionButton {
        id: saveQueue
        objectName: "musicSaveQueue"
        x: 1460
        y: 80
        width: 340
        height: 76
        visible: root.mode === "queue" && root.supported("save_playlist")
        text: qsTr("Save queue")
        enabled: !root.transportBusy
        onActivated: root.namePlaylist("", true)
        KeyNavigation.down: queueList
    }
    Ui.UiText {
        x: 700
        y: 900
        width: 1100
        visible: root.mode === "player" && root.player.continuous === true && !root.playbackProblem.length
        role: "caption"
        color: Ui.Theme.textSecondary
        maximumLineCount: 1
        elide: Text.ElideRight
        text: root.player.refill === "loading" ? qsTr("Loading upcoming tracks…")
            : root.player.refill === "failed" ? qsTr("Could not load more tracks. The loaded queue is preserved.")
            : root.player.refill === "exhausted" ? qsTr("No more tracks in the loaded queue. You can retry.")
            : qsTr("Upcoming tracks continue automatically")
    }
    ListView {
        id: queueList
        objectName: "musicQueueEntries"
        enabled: !root.pendingPlay
        x: 120
        y: 230
        width: 1680
        height: 680
        clip: true
        spacing: 12
        visible: root.mode === "queue"
        keyNavigationEnabled: true
        Keys.onUpPressed: event => {
            if (currentIndex <= 0 && saveQueue.visible)
                Ui.Focus.take(saveQueue);
            else
                event.accepted = false;
        }
        model: root.player.upcoming || []
        header: Ui.UiText {
            role: "section"
            text: root.player.continuous ? qsTr("Upcoming · continues automatically") : qsTr("Upcoming")
            height: 70
        }
        delegate: Ui.Surface {
            id: queueEntry
            required property var modelData
            required property int index
            width: queueList.width - 8
            height: 90
            focus: queueList.currentIndex === index
            text: modelData.track.title || qsTr("Untitled")
            contentItem: Ui.UiText {
                anchors.fill: parent
                anchors.margins: 24
                text: queueEntry.text
                role: "body"
                textFormat: Text.PlainText
                maximumLineCount: 1
                elide: Text.ElideRight
            }
            onActivated: if (root.supported("remove")) {
                root.queueOccurrence = modelData.occurrence;
                Qt.callLater(() => Ui.Focus.take(queueKeep));
            }
        }
    }
    MusicDialog {
        objectName: "musicRemoveDialog"
        visible: root.queueOccurrence.length > 0
        z: 30
        title: qsTr("Remove this occurrence from the queue?")
        onDismissed: root.back()
        Ui.ActionButton {
            id: queueKeep
            objectName: "musicKeepOccurrence"
            text: qsTr("Cancel")
            onActivated: root.back()
        }
        Ui.ActionButton {
            id: queueRemove
            objectName: "musicRemoveOccurrence"
            text: qsTr("Remove")
            onActivated: {
                root.catalog.transport("remove", "", root.queueOccurrence);
                root.back();
            }
        }
    }
    MusicDialog {
        objectName: "musicDeleteDialog"
        visible: root.deletingPlaylist
        z: 30
        title: qsTr("Delete this playlist?")
        body: qsTr("Audio files are kept.")
        onDismissed: root.back()
        Ui.ActionButton {
            id: keepPlaylist
            text: qsTr("Cancel")
            onActivated: root.back()
        }
        Ui.ActionButton {
            id: deletePlaylist
            text: qsTr("Delete playlist")
            destructive: true
            onActivated: {
                root.catalog.command("delete_playlist", root.parentId);
                root.deletingPlaylist = false;
                root.parentId = "";
                root.parentTitle = "";
                root.browse("playlists", "", "");
                root.focusInitial();
            }
        }
    }
    Rectangle {
        anchors.fill: parent
        visible: root.namingPlaylist
        color: Ui.Theme.scrim
        z: 30
        MouseArea {
            anchors.fill: parent
            acceptedButtons: Qt.AllButtons
        }
        Column {
            x: 360
            y: 160
            width: 1200
            spacing: 36
            Ui.UiText {
                text: qsTr("Save playlist")
                role: "title"
            }
            Rectangle {
                width: 1200
                height: 100
                radius: Ui.Theme.containerRadius
                color: Ui.Theme.plateContainer
                TextInput {
                    id: playlistName
                    objectName: "musicPlaylistName"
                    x: 24
                    y: 20
                    width: 820
                    height: 60
                    font.family: Ui.Theme.fontFamily
                    font.pixelSize: 36
                    color: Ui.Theme.textPrimary
                    clip: true
                    maximumLength: 128
                    Keys.onReturnPressed: Ui.Focus.take(playlistSave)
                    Keys.onEnterPressed: Ui.Focus.take(playlistSave)
                    Keys.onRightPressed: event => {
                        if (cursorPosition === length)
                            Ui.Focus.take(playlistKeyboard);
                        else
                            event.accepted = false;
                    }
                }
                Ui.KeyboardButton {
                    id: playlistKeyboard
                    objectName: "musicPlaylistKeyboard"
                    anchors.right: parent.right
                    anchors.rightMargin: 12
                    anchors.verticalCenter: parent.verticalCenter
                    onActivated: root.startTyping(playlistName)
                    Keys.onLeftPressed: Ui.Focus.take(playlistName)
                    Keys.onDownPressed: Ui.Focus.take(playlistSave)
                }
            }
            Row {
                spacing: 24
                Ui.ActionButton {
                    id: playlistCancel
                    text: qsTr("Cancel")
                    onActivated: root.closePlaylist()
                    KeyNavigation.right: playlistSave
                    KeyNavigation.up: playlistName
                }
                Ui.ActionButton {
                    id: playlistSave
                    objectName: "musicConfirmPlaylist"
                    text: qsTr("Save")
                    enabled: playlistName.text.trim().length > 0
                    onActivated: root.confirmPlaylist()
                    KeyNavigation.left: playlistCancel
                    KeyNavigation.up: playlistName
                }
            }
        }
    }
    Rectangle {
        x: 0
        y: root.height - height
        width: root.width
        height: searchKeyboard.implicitHeight
        color: Ui.Theme.background
        visible: root.typing
        enabled: visible
        z: 40
        Ui.OnScreenKeyboard {
            id: searchKeyboard
            objectName: "musicOnScreenKeyboard"
            anchors.fill: parent
            target: root.typingTarget
            memoryKey: root.namingPlaylist ? "music-playlist-name" : "music-search"
            maxCharacters: 256
            onSubmitted: root.finishTyping()
            onHideRequested: root.finishTyping()
            onLeaveUp: Ui.Focus.take(root.namingPlaylist ? playlistKeyboard : searchKeyboardButton)
        }
    }
    Rectangle {
        anchors.fill: parent
        color: Ui.Theme.scrim
        visible: root.pickingSource
        z: 25
        MouseArea {
            anchors.fill: parent
            onClicked: {
                root.pickingSource = false;
                Ui.Focus.take(sourcePicker);
            }
        }
        ListView {
            id: sourceList
            objectName: "musicSourceList"
            x: 120
            y: 180
            width: 520
            height: Math.min(450, count * 90)
            clip: true
            spacing: 8
            keyNavigationEnabled: true
            model: root.catalog.sources
            delegate: Ui.ActionButton {
                required property var modelData
                required property int index
                width: sourceList.width
                height: 82
                focus: sourceList.currentIndex === index
                text: modelData.name
                onActivated: {
                    root.typing = false;
                    root.facet = "";
                    root.parentId = "";
                    root.parentTitle = "";
                    root.cancelRestore();
                    root.catalog.choose(modelData.id);
                    root.pickingSource = false;
                    Ui.Focus.take(sourcePicker);
                }
            }
        }
    }
    // The standard confirm dialog over a scrim (#756 M6).
    MusicDialog {
        objectName: "musicReplaceDialog"
        visible: root.pendingPlay.length > 0
        z: 20
        title: qsTr("Replace the current queue?")
        body: qsTr("The music that plays now stops, and its queue is replaced.")
        onDismissed: root.back()
        Ui.ActionButton {
            id: cancelReplace
            objectName: "musicKeepQueue"
            text: qsTr("Cancel")
            onActivated: {
                root.pendingPlay = "";
                root.focusInitial();
            }
        }
        Ui.ActionButton {
            id: confirmReplace
            objectName: "musicReplaceQueue"
            text: qsTr("Replace and play")
            onActivated: {
                const id = root.pendingPlay;
                root.pendingPlay = "";
                root.startCommand(root.pendingAction, id, root.pendingActive);
                root.openPlayer();
            }
        }
    }
}
