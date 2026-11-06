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
    // The Music root over every source (#800, ADR-0142 MH-1). Without it the
    // view opens on the catalogue of the browsed source, as before.
    property var room: null
    // A sheet or a dialog covers the screen: the shell's status area hides.
    readonly property bool overlayOpen: (room && roomView.sheetOpen) || pendingPlay.length > 0
        || queueOccurrence.length > 0 || deletingPlaylist || moreOpen || mode === "queue"
    property string mode: "catalogue"
    // The room entry a detail, the All list or the player was opened from:
    // Back returns to the room with focus on it (brief, M16).
    property string roomKey: ""
    // The shelf whose All tile opened the All list.
    property string roomShelf: ""
    // The detail of an album, artist or playlist opened from the room or a
    // search (#801): its entry dresses the detail header.
    property var detailEntry: ({})
    readonly property bool detailView: !!room && mode === "catalogue" && parentId.length > 0
    // The detail was opened from Find: Back returns to the results.
    property bool fromSearch: false
    // The All list a detail was opened from.
    property bool fromAll: false
    // A track or station chosen in the room waits for its source's state
    // before it starts (the play capability lives there).
    property var roomPlay: null
    Component.onCompleted: if (room)
        mode = "root"
    // `atFind`: Back from Find returns to the Find button (review of #819).
    function openRoom(atFind) {
        mode = "root";
        fromAll = false;
        roomPlay = null;
        // The detail behind a player opened from it is left too, so the
        // next Back leaves Music (review of #812).
        parentId = "";
        parentTitle = "";
        parentKind = "";
        const key = roomKey, shelf = roomShelf;
        Qt.callLater(() => {
            if (atFind === true && roomView.focusFind())
                return;
            if (!(key.length && roomView.focusEntry(key)) && !roomView.focusShelf(shelf))
                roomView.focusInitial();
        });
    }
    function openFromRoom(entry, fromList, fromFind) {
        if (!entry || !entry.provider)
            return;
        roomKey = entry.key || "";
        fromAll = fromList === true;
        fromSearch = fromFind === true;
        detailEntry = entry;
        if (!fromAll)
            roomShelf = "";
        if (catalog.sourceId !== entry.provider)
            catalog.choose(entry.provider);
        if (["album", "artist", "playlist"].includes(entry.kind)) {
            facet = entry.kind + "s";
            parentId = entry.id;
            parentTitle = entry.title || "";
            parentKind = entry.kind;
            mode = "catalogue";
            browse(facet, entry.id, "");
            Qt.callLater(focusInitial);
        } else {
            roomPlay = entry;
            Qt.callLater(startRoomPlay);
        }
    }
    function startRoomPlay() {
        const entry = roomPlay;
        if (!entry || catalog.sourceId !== entry.provider)
            return;
        // A station starts as radio, as in the catalogue list; anything else
        // plays. The card's own source must offer it: supported() reads the
        // session that plays, which may be another source (review of #812).
        const action = entry.kind === "station" ? "radio" : "play";
        const offered = selected.actions || [];
        if (!offered.includes(action)) {
            // The source answered (its state has a revision) without it:
            // nothing starts later, also when it offers no action at all
            // (Codex review of #812).
            if (offered.length || selected.revision !== undefined)
                roomPlay = null;
            return;
        }
        roomPlay = null;
        pendingAction = action;
        pendingActive = false;
        requestStart(entry.id);
    }
    Connections {
        target: root.catalog
        ignoreUnknownSignals: true
        function onStateChanged() {
            if (root.roomPlay)
                root.startRoomPlay();
        }
    }
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
            if (item && item !== root && root.containsFocusItem(item))
                root.playButtonFocused = item === playButton;
        }
    }
    Connections {
        target: root.catalog
        ignoreUnknownSignals: true
        // Only a failed page ends the opener search; a failed command
        // sets the same error line but the page may still arrive
        // (review of #798).
        function onPageFailed() {
            root.cancelRestore();
        }
        function onBrowseReset() {
            root.cancelRestore();
            root.parentId = "";
            root.parentTitle = "";
            root.parentKind = "";
            root.facet = "";
        }
        // A pending opener search waits while anything is busy (a page, a
        // command); it is checked again once that ends (review of #798).
        function onStatusChanged() {
            root.settleLike();
            Qt.callLater(root.recoverFocus);
            Qt.callLater(root.restoreOpener);
        }
        function onBusyChanged() {
            Qt.callLater(root.recoverFocus);
            Qt.callLater(root.restoreOpener);
        }
        function onTransportBusyChanged() {
            root.settleLike();
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
        roomPlay = null;
        moreOpen = false;
        replaceOpener = null;
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
    // Retry plays the failed track again. A station's own `retry` only
    // reloads its queue, so a failed station plays its next loaded track
    // instead (review of #797); with none loaded, the row's "Retry loading
    // tracks" stays the way on.
    readonly property bool canRetryPlayback: playbackProblem.length > 0
        && !transportBusy
        && (player.continuous === true
            ? supported("next") && (player.upcoming || []).length > 0
            : !!current.id)
    // Only while Retry is offered: a second OK on a hidden button must not
    // restart a track that just started (review of #797).
    function retryPlayback() {
        if (!canRetryPlayback)
            return;
        if (player.continuous === true)
            catalog.transport("next");
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
        // In Find, below the field and its keyboard button are the results
        // (Codex review of #819).
        if (mode === "search" && room) {
            searchView.focusInitial();
            return;
        }
        Ui.Focus.take(tracks.count && tracks.visible ? tracks : catalogueAction());
    }
    // 3:05, or 1:15:03 from an hour on (#756 M25).
    function timeLabel(microseconds, withHours) {
        const seconds = Math.max(0, Math.floor((microseconds || 0) / 1000000));
        const hours = Math.floor(seconds / 3600);
        const minutes = Math.floor(seconds % 3600 / 60);
        const rest = String(seconds % 60).padStart(2, "0");
        return hours > 0 || withHours === true ? hours + ":" + String(minutes).padStart(2, "0") + ":" + rest
            : minutes + ":" + rest;
    }
    function supported(action) {
        return actions.includes(action);
    }
    // Times of a track longer than an hour carry hours, elapsed too (M25).
    readonly property bool longTrack: (current.duration_us || 0) >= 3600000000
    // Now playing's context (brief mh-03): the station, or the album, and
    // the source.
    readonly property string playerContext: {
        const source = catalog.activeSourceName || "";
        const station = player.station ? (player.station.title || "") : "";
        const context = station.length ? station : current.album ? qsTr("Album") : "";
        return [context, source].filter(part => part.length).join(" · ");
    }
    // The Queue sheet's rows: the current entry first, then the upcoming
    // ones (brief *Queue*).
    readonly property var queueEntries: {
        const rows = [];
        if (player.current && player.current.track && player.current.track.id)
            rows.push({occurrence: player.current.occurrence || "current", track: current, current: true});
        for (const entry of player.upcoming || [])
            rows.push({occurrence: entry.occurrence || "", track: entry.track || ({}), current: false});
        return rows;
    }
    // Like: pending while its command is in flight, failed when it ends
    // with an error, until the track changes or Like is pressed again (M18).
    property bool likePending: false
    property bool likeFailed: false
    readonly property string currentId: current.id || ""
    // A new track ends whatever the last one's like was doing (Codex review
    // of #821).
    onCurrentIdChanged: {
        likeFailed = false;
        likePending = false;
        likeAwaitsState = false;
    }
    function like() {
        if (transportBusy || !supported("like"))
            return;
        likeFailed = false;
        likeAwaitsState = false;
        likeFeedbackBefore = player.feedback || "";
        likePending = true;
        catalog.transport("like");
        // A command the catalogue did not send ends the pending state.
        Qt.callLater(settleLike);
    }
    // A like that went through stays pending until the next state brings
    // its feedback, so the label never flashes back to "Like" (review of
    // #821); a refused one fails at once.
    property bool likeAwaitsState: false
    property string likeFeedbackBefore: ""
    function settleLike() {
        if (!likePending || catalog.transportBusy || likeAwaitsState)
            return;
        if ((catalog.errorText || "").length > 0) {
            likePending = false;
            likeFailed = true;
        } else if ((player.feedback || "") !== likeFeedbackBefore) {
            likePending = false;
        } else {
            likeAwaitPolls = 0;
            likeAwaitsState = true;
        }
    }
    // After the state's own bindings, so the label goes from "Saving…"
    // straight to the new state.
    // A poll already in flight can bring the state from before the like:
    // only a changed feedback ends the wait, or, should the provider keep
    // it, the third state after the command (Codex review of #821).
    property int likeAwaitPolls: 0
    onPlayerChanged: if (likeAwaitsState)
        Qt.callLater(finishLike)
    function finishLike() {
        if (!likeAwaitsState)
            return;
        if ((player.feedback || "") === likeFeedbackBefore && ++likeAwaitPolls < 3)
            return;
        likeAwaitsState = false;
        likePending = false;
    }
    function openQueue() {
        moreOpen = false;
        mode = "queue";
        Qt.callLater(focusInitial);
    }
    // Home pins (brief *Presence › Favourites*, #804): the [provider,
    // station] pairs on Home, and whether a pin can be written now. Pinning
    // is the shell's; the view only asks.
    property var pinnedStations: []
    property bool pinsWritable: false
    signal stationPinRequested(string providerId, string stationId, string title, string artwork, bool pinned)
    // Home stores a new pin with its title, cleaned of control characters
    // (MusicPins1): a station without one cannot be pinned, though a pin
    // Home already holds can always be removed (Codex review of #836).
    readonly property string stationPinTitle: player.station && player.station.title
        ? String(player.station.title).replace(/[\u0000-\u001f\u007f-\u009f]/g, " ").trim() : ""
    // Home membership follows the stable identity alone; a new pin needs
    // the provider's `pinnable` mark and a title (Codex review of #836).
    readonly property bool stationIdentified: !!player.station && !!player.station.id && !!catalog.activeSourceId
    readonly property bool stationPinned: stationIdentified
                                          && pinnedStations.some(pair => pair[0] === catalog.activeSourceId
                                                                         && pair[1] === player.station.id)
    readonly property bool stationPinnable: stationPinned
                                            || (stationIdentified && player.station.pinnable === true
                                                && stationPinTitle.length > 0)
    // The More sheet (brief *More sheet*): only the session's actions. The
    // Menu key keeps global navigation and never opens it.
    property bool moreOpen: false
    onModeChanged: if (mode !== "player")
        moreOpen = false
    readonly property var moreActions: {
        const rows = [];
        // A command in flight blocks every row, visibly: each sends one,
        // Radio after its confirmation (Codex review of #821).
        const busy = transportBusy;
        if (supported("dislike"))
            rows.push({id: "dislike", icon: "thumb_down", selected: player.feedback === "disliked", busy: busy,
                       label: player.feedback === "disliked" ? qsTr("Disliked") : qsTr("Dislike")});
        if (supported("radio") && !!current.id)
            rows.push({id: "radio", icon: "radio", busy: busy, label: qsTr("Radio from this track")});
        // A pinnable station pins to Home, which mirrors the provider's own
        // pin; any other station keeps only the provider's pin.
        if (stationPinnable)
            rows.push({id: "homePin", icon: "push_pin", selected: stationPinned, busy: busy || !pinsWritable,
                       label: stationPinned ? qsTr("Unpin from Home") : qsTr("Pin to Home")});
        else if (supported("pin") && player.station && player.station.id)
            rows.push({id: "pin", icon: "push_pin", selected: player.pinned === true, busy: busy,
                       label: player.pinned === true ? qsTr("Unpin station") : qsTr("Pin station")});
        if (supported("retry"))
            rows.push({id: "retry", icon: "refresh", busy: busy, label: qsTr("Retry loading tracks")});
        if (supported("stop"))
            rows.push({id: "stop", icon: "stop", busy: busy, label: qsTr("Stop")});
        return rows;
    }
    function openMore() {
        moreOpen = true;
        Qt.callLater(moreSheet.focusInitial);
    }
    function closeMore() {
        moreOpen = false;
        Qt.callLater(() => {
            if (!Ui.Focus.take(moreButton))
                focusInitial();
        });
    }
    function runMore(id) {
        if (transportBusy)
            return;
        if (id === "radio") {
            moreOpen = false;
            replaceOpener = moreButton;
            requestRadio(current.id || "", true);
            return;
        }
        if (id === "homePin") {
            if (pinsWritable)
                stationPinRequested(catalog.activeSourceId, player.station.id, stationPinTitle,
                                    player.station.artwork || "", !stationPinned);
        } else if (id === "pin")
            catalog.transport("pin", player.station ? player.station.id : "");
        else
            catalog.transport(id);
        closeMore();
    }
    // The control row is the last row: Down leaves it only for the failure's
    // Retry, never sideways inside the row (#756 M15).
    readonly property Item belowControls: retryPlaybackButton.visible ? retryPlaybackButton : null
    // Play/Pause first (#756 M13). While it is disabled (a track loading)
    // the nearest working control holds focus, and Play/Pause takes it back
    // once it works, unless the user moved on.
    property Item fallbackFocus: null
    // Play/Pause had focus last in the view; Qt drops focus from an item
    // as it is disabled, so this remembers it for the hand-off.
    property bool playButtonFocused: false
    function playerFocus() {
        if (playButton.enabled)
            return playButton;
        return [retryPlaybackButton, nextButton, previousButton, likeButton, queueButton]
            .find(item => item.visible && item.enabled) || moreButton;
    }
    function focusInitial() {
        // Find keeps focus on the result it was left from, or its first
        // result, or the field (review of #819).
        if (mode === "search" && room) {
            if (!(roomKey.length && searchView.focusKey(roomKey)) && !searchView.focusInitial())
                Ui.Focus.take(search);
            return;
        }
        // The room and its All lists keep their own focus (review of #812).
        if (mode === "root" && room) {
            if (!(roomKey.length && roomView.focusEntry(roomKey))
                    && !roomView.focusShelf(roomShelf))
                roomView.focusInitial();
            return;
        }
        if (mode === "all" && room) {
            if (!(roomKey.length && allView.focusKey(roomKey)))
                allView.focusInitial();
            return;
        }
        if (mode === "player") {
            const target = playerFocus();
            fallbackFocus = target === playButton ? null : target;
            Ui.Focus.take(target);
        } else if (mode === "queue")
            queueSheet.focusInitial();
        else if (tracks.visible && tracks.count)
            Ui.Focus.take(tracks);
        else if (detailView && (detailHeader.firstPill || playlistRow.first))
            // An empty detail hides the catalogue's actions: its own pills
            // take focus (Codex review of #819).
            Ui.Focus.take(detailHeader.firstPill || playlistRow.first);
        else
            Ui.Focus.take(catalogueAction());
    }
    Connections {
        target: playButton
        function onEnabledChanged() {
            if (playButton.enabled && root.fallbackFocus && root.fallbackFocus.activeFocus) {
                root.fallbackFocus = null;
                Ui.Focus.take(playButton);
            } else if (!playButton.enabled && root.playButtonFocused) {
                // A track loading disables Play/Pause under focus: the
                // nearest working control holds it and hands it back
                // (review of #798).
                Qt.callLater(() => {
                    // Not from under the Quick menu or another scope that
                    // holds focus now (review of #798).
                    if (!root.focusIsFree())
                        return;
                    const target = root.playerFocus();
                    root.fallbackFocus = target;
                    Ui.Focus.take(target);
                });
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
    function restoreOpener() {
        if (!restoreEntity.length)
            return;
        // An empty list with no page in flight and none to follow cannot
        // hold the opener (review of #798).
        if (!catalog.entries.length) {
            if (!catalog.busy && !catalog.more)
                cancelRestore();
            return;
        }
        const index = catalog.entries.findIndex(entry => entry.id === restoreEntity);
        // A page in flight (or any other busy work) is waited for, not
        // counted, also the last page the budget allows: loadMore() does
        // nothing meanwhile, and the status change at its end runs this
        // again (reviews of #798).
        if (index < 0 && catalog.busy)
            return;
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
        // Only while focus is still in the list, or nowhere: a user who
        // moved on, or a scope above the view, keeps it (reviews of #798).
        const item = hostWindow ? hostWindow.activeFocusItem : null;
        const inList = !!item && (item === tracks || isInside(item, tracks));
        if (tracks.visible && (inList || focusIsFree()))
            Ui.Focus.take(tracks);
    }
    // No control holds focus: none at all, or only the view itself. Focus
    // in another scope (the Quick menu, the power scope) is not free.
    function focusIsFree() {
        const item = hostWindow ? hostWindow.activeFocusItem : null;
        return !item || item === root || (hostWindow && item === hostWindow.contentItem);
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

    // The control a replacement was asked from, when it is not where
    // focusInitial() lands: Radio from this track in More returns to More
    // (review of #821).
    property Item replaceOpener: null
    function cancelReplacement() {
        pendingPlay = "";
        const opener = replaceOpener;
        replaceOpener = null;
        if (!(opener && Ui.Focus.take(opener)))
            focusInitial();
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
            Ui.Focus.take(queueSheet.list.currentItem || queueSheet.list);
        } else if (deletingPlaylist) {
            deletingPlaylist = false;
            focusInitial();
        } else if (typing) {
            finishTyping();
        } else if (pendingPlay) {
            cancelReplacement();
        } else if (moreOpen) {
            closeMore();
        } else if (mode === "queue") {
            mode = "player";
            Qt.callLater(() => {
                if (!Ui.Focus.take(queueButton))
                    focusInitial();
            });
        } else if (mode === "player" && playerFromHome) {
            playerFromHome = false;
            mode = room ? "root" : "catalogue";
            backRequested();
        } else if (mode === "player" && room) {
            openRoom();
        } else if (mode === "player") {
            mode = "catalogue";
            Qt.callLater(focusInitial);
        } else if (mode === "all") {
            openRoom();
        } else if (mode === "search") {
            search.text = "";
            searchDelay.stop();
            room.search("");
            openRoom(true);
        } else if (room && mode === "catalogue" && fromSearch) {
            parentId = "";
            parentTitle = "";
            parentKind = "";
            fromSearch = false;
            mode = "search";
            const key = roomKey;
            Qt.callLater(() => { if (!searchView.focusKey(key)) Ui.Focus.take(search); });
        } else if (room && mode === "catalogue" && fromAll) {
            parentId = "";
            parentTitle = "";
            parentKind = "";
            fromAll = false;
            mode = "all";
            const key = roomKey;
            Qt.callLater(() => { if (!allView.focusKey(key)) allView.focusInitial(); });
        } else if (room && mode === "catalogue") {
            parentId = "";
            parentTitle = "";
            parentKind = "";
            openRoom();
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
    // Now playing's floor and pool take the artwork's hue (#802). It sits
    // under every line of the screen, the heading too (review of #821).
    Ui.AmbientGlow {
        objectName: "musicPlayerGlow"
        anchors.fill: parent
        visible: playerView.visible
        veilEnabled: false
        strength: 0.7
        lit: playerView.visible && (root.current.image || "").length > 0
        source: root.current.image || ""
        anchor: playerArtwork
    }
    // The detail header (#801): backdrop, cover, title and pills.
    MusicDetailHeader {
        id: detailHeader
        objectName: "musicDetailHeader"
        visible: root.detailView
        entry: root.detailEntry
        tracks: root.catalog.entries || []
        canListen: (root.selected.actions || []).includes("play")
        canRadio: (root.selected.actions || []).includes("radio")
        onListenRequested: root.requestPlay(root.parentId)
        onRadioRequested: root.requestRadio(root.parentId)
        rightNeighbour: playlistRow.beside ? playlistRow.first : null
        onDownRequested: {
            if (!(playlistRow.below && playlistRow.first && Ui.Focus.take(playlistRow.first)))
                Ui.Focus.take(tracks.count ? tracks : root.detailAction());
        }
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
        // The room and its All lists carry their own titles.
        visible: root.mode !== "root" && root.mode !== "all" && root.mode !== "search" && !root.detailView
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
        visible: root.mode === "catalogue" && !root.detailView
        Ui.ActionButton {
            id: sourcePicker
            objectName: "musicSourcePicker"
            // The room has no source switch (MH-5): its Sources sheet holds
            // the sources.
            visible: !root.room
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
        y: root.mode === "search" ? 180 : 278
        width: root.mode === "search" ? Ui.Theme.canvasWidth - Ui.Theme.safeLeft - Ui.Theme.safeRight : catalogueActions.x - 40 - x
        height: 76
        radius: Ui.Theme.containerRadius
        color: Ui.Theme.plate
        visible: (root.mode === "catalogue" && !root.detailView) || root.mode === "search"
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
            onTextChanged: {
                searchDelay.forMode = root.mode;
                searchDelay.restart();
            }
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
                // The mode the text was typed in: a Find query is searched
                // even after a result opens, and never browses the detail;
                // a catalogue filter is dropped once that mode is left
                // (review of #819).
                property string forMode: ""
                onTriggered: if (forMode === "search")
                    root.room.search(search.text);
                else if (forMode === root.mode && !root.detailView)
                    root.browse(root.facet, root.parentId, search.text)
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
        visible: root.mode === "catalogue" && !root.detailView
        Ui.ActionButton {
            id: openPlayerButton
            objectName: "musicOpenPlayer"
            KeyNavigation.left: searchKeyboardButton
            KeyNavigation.right: settingsButton
            KeyNavigation.down: root.catalogueAction()
            width: Math.max(240, implicitWidth)
            height: 76
            visible: root.mode === "catalogue" && !root.detailView
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
            visible: root.mode === "catalogue" && !root.detailView
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
        visible: root.mode === "catalogue" && !root.room && (root.selected.actions || []).includes("disconnect")
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
        visible: root.mode === "catalogue" && !root.room && (root.selected.actions || []).includes("scan")
        text: root.selected.account === "local" ? qsTr("Rescan") : qsTr("Refresh")
        onActivated: root.catalog.command("scan")
    }
    Row {
        id: playlistRow
        objectName: "musicPlaylistActions"
        // In a detail it follows the pills on their line, never on them
        // (Codex review of #819), and joins them for the D-pad: Left goes
        // back to the pills, Down to the tracks (review of #819). Where the
        // line would pass the safe area (long translations), it takes its
        // own line under the pills (Codex review of #819).
        readonly property bool hasActions: saveGroup.visible || removeGroup.visible
        // The first action that can take focus: Save is disabled while the
        // catalogue is busy (delta review of #819).
        readonly property Item first: saveGroup.visible && saveGroup.enabled ? saveGroup
            : removeGroup.visible ? removeGroup : null
        readonly property bool beside: !root.detailView
            || detailHeader.pillsEnd + 24 + implicitWidth <= Ui.Theme.canvasWidth - Ui.Theme.safeRight
        readonly property bool below: root.detailView && visible && hasActions && !beside
        y: root.detailView ? (beside ? detailHeader.pillsY : detailHeader.contentBottom + 24) : 880
        x: root.detailView ? (beside && detailHeader.lastPill ? detailHeader.pillsEnd + 24 : 600) : 120
        spacing: 24
        visible: root.mode === "catalogue" && root.parentId.length > 0
        Ui.ActionButton {
            id: saveGroup
            objectName: "musicSavePlaylist"
            visible: (root.selected.actions || []).includes("save_playlist")
            text: qsTr("Save playlist")
            enabled: !root.catalog.busy
            onActivated: root.namePlaylist(root.parentId, false)
            KeyNavigation.up: root.detailView ? (playlistRow.below ? detailHeader.firstPill : null) : tracks
            KeyNavigation.left: root.detailView && playlistRow.beside ? detailHeader.lastPill : null
            KeyNavigation.right: removeGroup.visible ? removeGroup : root.detailView ? null : root.catalogueAction()
            KeyNavigation.down: root.detailView ? (tracks.count ? tracks : null) : root.catalogueAction()
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
            KeyNavigation.up: root.detailView ? (playlistRow.below ? detailHeader.firstPill : null) : tracks
            KeyNavigation.left: saveGroup.visible ? saveGroup
                : root.detailView ? (playlistRow.beside ? detailHeader.lastPill : null) : tracks
            KeyNavigation.down: root.detailView ? (tracks.count ? tracks : null) : root.catalogueAction()
        }
    }
    ListView {
        id: tracks
        objectName: "musicEntries"
        x: 120
        // In a detail the tracks start below the pills and end above the
        // status line (review of #819).
        // The list starts below the pills and the playlist actions, also when
        // only the actions show (Codex review of #819).
        y: root.detailView ? Math.max(detailHeader.contentBottom,
            playlistRow.visible && playlistRow.hasActions ? playlistRow.y + playlistRow.height : 0) + 40 : 385
        width: 1680
        height: root.detailView ? 960 - y : 450
        clip: true
        spacing: 12
        visible: root.mode === "catalogue" && ["local", "connected"].includes(root.selected.account)
        enabled: !root.pendingPlay
        Keys.onUpPressed: event => {
            if (currentIndex <= 0 && root.detailView)
                Ui.Focus.take(playlistRow.below && playlistRow.first ? playlistRow.first : detailHeader.firstPill || root.detailAction());
            else if (currentIndex <= 0)
                Ui.Focus.take(search);
            else
                event.accepted = false;
        }
        Keys.onDownPressed: event => {
            if (currentIndex >= count - 1) {
                if (root.catalog.more) {
                    tracks.positionViewAtEnd();
                    Qt.callLater(() => Ui.Focus.take(tracks.footerItem));
                } else if (!root.detailView)
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
            readonly property bool playingRow: root.detailView && !!modelData.id
                && root.catalog.activeSourceId === root.catalog.sourceId
                && ["playing", "paused", "loading"].includes(root.player.playback)
                && root.current.id === modelData.id
            text: modelData.title || qsTr("Untitled")
            contentItem: Item {
                Ui.UiText {
                    objectName: "musicRowDuration"
                    anchors.right: parent.right
                    anchors.rightMargin: 32
                    anchors.verticalCenter: parent.verticalCenter
                    visible: root.detailView && (entry.modelData.duration_us || 0) > 0
                    role: "meta"
                    color: Ui.Theme.textSecondary
                    font.features: ({ "tnum": 1 })
                    text: root.timeLabel(entry.modelData.duration_us)
                }
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
                        visible: !root.detailView
                    }
                    // In a detail the row carries its number, or the playing
                    // mark on the row that plays (brief mh-02).
                    Item {
                        width: 64
                        height: 64
                        anchors.verticalCenter: parent.verticalCenter
                        visible: root.detailView
                        Ui.UiText {
                            objectName: "musicRowNumber"
                            anchors.centerIn: parent
                            visible: !entry.playingRow
                            role: "meta"
                            color: Ui.Theme.textSecondary
                            font.features: ({ "tnum": 1 })
                            text: String(entry.index + 1)
                        }
                        Row {
                            objectName: "musicRowPlaying"
                            anchors.centerIn: parent
                            visible: entry.playingRow
                            spacing: 4
                            Repeater {
                                model: [14, 22, 10]
                                Rectangle {
                                    required property var modelData
                                    width: 5
                                    height: modelData
                                    radius: 2
                                    anchors.bottom: parent.bottom
                                    color: Ui.Theme.ember
                                }
                            }
                        }
                    }
                    Column {
                        width: root.detailView ? 1300 : 1400
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
                                const artist = entry.modelData.artist || "";
                                const line = root.detailView
                                    ? (artist !== (root.detailEntry.artist || "") ? artist : "")
                                    : artist || entry.modelData.album || "";
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
                    // In a detail the row becomes the detail, and the Find
                    // text never filters it (review of #819).
                    if (root.detailView)
                        root.detailEntry = Object.assign({}, modelData, {
                            provider: root.detailEntry.provider,
                            providerName: root.detailEntry.providerName,
                            local: root.detailEntry.local,
                            key: root.detailEntry.key
                        });
                    root.parentId = modelData.id;
                    root.parentKind = modelData.kind;
                    root.parentTitle = text;
                    root.browse(root.facet, modelData.id, root.detailView ? "" : search.text);
                }
            }
        }
        footer: Ui.ActionButton {
            id: loadMoreButton
            objectName: "musicLoadMore"
            Keys.onUpPressed: Ui.Focus.take(tracks.currentItem || tracks)
            Keys.onDownPressed: if (!root.detailView)
                Ui.Focus.take(root.detailAction())
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
        // A detail numbers its rows instead (review of #819).
        visible: root.mode === "catalogue" && tracks.visible && tracks.count > 0 && !root.detailView
        text: qsTr("%1 of %2").arg(tracks.currentIndex + 1).arg(tracks.count)
    }
    Column {
        x: 120
        y: 420
        width: 1300
        spacing: 28
        enabled: !root.pendingPlay
        visible: root.mode === "catalogue" && !root.room
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
        visible: root.mode === "catalogue" && !root.detailView
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
    // Now playing (brief mh-03, ADR-0142 MH-3, #802): the artwork with its
    // light, the copy, the seek line and one row of controls. The Queue and
    // More sheets open over it.
    Item {
        id: playerView
        objectName: "musicNowPlaying"
        enabled: !root.pendingPlay
        anchors.fill: parent
        visible: root.mode === "player" || root.mode === "queue"
        MusicArtwork {
            id: playerArtwork
            objectName: "musicPlayerArtwork"
            x: 120
            y: 220
            width: 560
            height: 560
            radius: Ui.Theme.stillRadius
            source: root.current.image || ""
        }
        Column {
            objectName: "musicInfoColumn"
            x: 760
            y: 220
            width: 1064
            spacing: 14
            // The context: an Ember dot, then the station or the album and
            // the source.
            Row {
                spacing: 14
                Rectangle {
                    anchors.verticalCenter: parent.verticalCenter
                    width: 12
                    height: 12
                    radius: 6
                    color: Ui.Theme.ember
                }
                Ui.UiText {
                    objectName: "musicPlayerContext"
                    width: 1030
                    role: "eyebrow"
                    text: root.playerContext
                    maximumLineCount: 1
                    elide: Text.ElideRight
                    textFormat: Text.PlainText
                    color: Ui.Theme.textSecondary
                }
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
                objectName: "musicPlayerArtist"
                width: parent.width
                role: "section"
                font.pixelSize: 32
                text: root.current.artist || ""
                textFormat: Text.PlainText
                maximumLineCount: 1
                elide: Text.ElideRight
                color: Ui.Theme.textPrimary
            }
            // The album; no year: the music Entity carries none (brief).
            Ui.UiText {
                objectName: "musicPlayerAlbum"
                width: parent.width
                role: "meta"
                font.pixelSize: 28
                text: root.current.album || ""
                textFormat: Text.PlainText
                maximumLineCount: 1
                elide: Text.ElideRight
                color: Ui.Theme.textSecondary
            }
        }
        // The seek line: a 4 px track at 20 % white with an Ember fill; the
        // knob shows only in focus. Left and Right seek (M15).
        T.Slider {
            id: seek
            objectName: "musicSeek"
            x: 760
            y: 588
            width: 1064
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
                color: Qt.rgba(1, 1, 1, 0.2)
                Rectangle {
                    objectName: "musicSeekFill"
                    width: seek.visualPosition * parent.width
                    height: parent.height
                    radius: 2
                    color: Ui.Theme.ember
                }
            }
            handle: Rectangle {
                objectName: "musicSeekKnob"
                x: seek.leftPadding + seek.visualPosition * (seek.availableWidth - width)
                y: seek.topPadding + (seek.availableHeight - height) / 2
                visible: seek.activeFocus
                width: 24
                height: 24
                radius: 12
                color: Ui.Theme.textPrimary
            }
        }
        // Elapsed and total time, in tabular figures, with hours when the
        // track is longer than an hour (M25).
        Ui.UiText {
            objectName: "musicElapsed"
            x: 760
            y: 628
            role: "meta"
            font.features: ({ "tnum": 1, "lnum": 1 })
            text: root.timeLabel(root.player.position_us, root.longTrack)
            color: Ui.Theme.textSecondary
        }
        Ui.UiText {
            objectName: "musicDuration"
            x: 760 + 1064 - width
            y: 628
            role: "meta"
            font.features: ({ "tnum": 1, "lnum": 1 })
            visible: (root.current.duration_us || 0) > 0
            text: root.timeLabel(root.current.duration_us, root.longTrack)
            color: Ui.Theme.textSecondary
        }
        // One row, vertically centred: previous, play/pause, next, Like,
        // Queue and More (MH-3). Left and Right stop at its ends; Up goes to
        // the seek line (M15). There is no Stop in the row: it is in More.
        Item {
            objectName: "musicControlsB"
            x: 760
            y: 690
            width: 1064
            height: 120
            Row {
                anchors.verticalCenter: parent.verticalCenter
                spacing: 20
                MusicButton {
                    id: previousButton
                    objectName: "musicPrevious"
                    anchors.verticalCenter: parent.verticalCenter
                    width: 96
                    height: 96
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
                    anchors.verticalCenter: parent.verticalCenter
                    KeyNavigation.left: previousButton
                    KeyNavigation.right: nextButton
                    KeyNavigation.up: seek
                    KeyNavigation.down: root.belowControls
                    objectName: "musicPlayPause"
                    width: 120
                    height: 120
                    iconName: root.playing ? "pause" : "play"
                    text: root.playing ? qsTr("Pause") : qsTr("Play")
                    // A command in flight keeps the button and its focus;
                    // a press meanwhile is ignored (Surface.busy), as on
                    // Home's card (review of #798).
                    busy: root.transportBusy
                    enabled: (root.supported("pause") || root.supported("resume")
                            || (!root.player.continuous && ["stopped", "ended"].includes(root.player.playback)
                                && !!root.current.id))
                    onActivated: root.catalog.transport(root.playing ? "pause"
                                                        : root.player.playback === "paused" ? "resume" : "play",
                                                        root.current.id || "")
                }
                MusicButton {
                    id: nextButton
                    objectName: "musicNext"
                    anchors.verticalCenter: parent.verticalCenter
                    width: 96
                    height: 96
                    KeyNavigation.left: playButton
                    KeyNavigation.right: likeButton
                    KeyNavigation.up: seek
                    KeyNavigation.down: root.belowControls
                    iconName: "skip_next"
                    text: qsTr("Next")
                    enabled: root.supported("next") && !root.transportBusy
                    onActivated: root.catalog.transport("next")
                }
                // Like shows its state as a filled heart and "Liked", not
                // only as a stronger plate (M18), with pending and failure.
                MusicPill {
                    id: likeButton
                    objectName: "musicLike"
                    anchors.verticalCenter: parent.verticalCenter
                    readonly property bool liked: root.player.feedback === "liked"
                    selected: liked
                    // Busy for its own command and for any other in flight:
                    // like() refuses then (Codex review of #821).
                    busy: root.likePending || root.transportBusy
                    visible: root.supported("like")
                    iconName: liked ? "favorite_fill" : "favorite"
                    // One width for every state, so Queue and More never move
                    // and the row stays in the safe area in every language
                    // (review of #821).
                    labels: [qsTr("Like"), qsTr("Liked"), qsTr("Saving…"), qsTr("Not saved")]
                    text: root.likePending ? qsTr("Saving…")
                        : root.likeFailed ? qsTr("Not saved")
                        : liked ? qsTr("Liked") : qsTr("Like")
                    Accessible.name: qsTr("Like")
                    Accessible.checkable: true
                    Accessible.checked: liked
                    // The state is said, not only shown (Codex review of #821).
                    // Why it is unavailable comes before an older failure
                    // (Codex review of #821).
                    Accessible.description: root.likePending ? qsTr("Saving…")
                        : root.transportBusy ? qsTr("Another command is in progress.")
                        : root.likeFailed ? qsTr("The like was not saved. Try again.") : ""
                    KeyNavigation.left: nextButton
                    KeyNavigation.right: queueButton
                    KeyNavigation.up: seek
                    KeyNavigation.down: root.belowControls
                    onActivated: root.like()
                }
                MusicPill {
                    id: queueButton
                    objectName: "musicQueue"
                    anchors.verticalCenter: parent.verticalCenter
                    iconName: "queue_music"
                    text: qsTr("Queue")
                    KeyNavigation.left: likeButton
                    KeyNavigation.right: moreButton
                    KeyNavigation.up: seek
                    KeyNavigation.down: root.belowControls
                    onActivated: root.openQueue()
                }
                MusicButton {
                    id: moreButton
                    objectName: "musicMore"
                    anchors.verticalCenter: parent.verticalCenter
                    width: 96
                    height: 96
                    iconName: "more_horiz"
                    text: qsTr("More")
                    KeyNavigation.left: queueButton
                    KeyNavigation.up: seek
                    KeyNavigation.down: root.belowControls
                    onActivated: root.openMore()
                }
            }
        }
        // Why playback stopped, and Retry (#756 M8).
        Row {
            objectName: "musicPlaybackFailure"
            x: 760
            y: 860
            spacing: 32
            visible: root.playbackProblem.length > 0
            Ui.UiText {
                objectName: "musicPlaybackProblem"
                width: retryPlaybackButton.visible ? 1064 - retryPlaybackButton.width - 32 : 1064
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
                text: root.player.continuous === true ? qsTr("Play next") : qsTr("Retry")
                KeyNavigation.up: playButton
                onActivated: root.retryPlayback()
            }
        }
        Ui.UiText {
            objectName: "musicRefill"
            x: 760
            y: 880
            width: 1064
            visible: root.player.continuous === true && !root.playbackProblem.length
            role: "caption"
            color: Ui.Theme.textSecondary
            maximumLineCount: 1
            elide: Text.ElideRight
            text: root.player.refill === "loading" ? qsTr("Loading upcoming tracks…")
                : root.player.refill === "failed" ? qsTr("Could not load more tracks. The loaded queue is preserved.")
                : root.player.refill === "exhausted" ? qsTr("No more tracks in the loaded queue. You can retry.")
                : qsTr("Upcoming tracks continue automatically")
        }
    }
    MusicQueueSheet {
        id: queueSheet
        objectName: "musicQueueView"
        z: 20
        visible: root.mode === "queue"
        enabled: !root.pendingPlay
        title: root.player.continuous === true && root.player.station ? (root.player.station.title || "") : ""
        continuous: root.player.continuous === true
        canSave: root.supported("save_playlist")
        saveEnabled: !root.transportBusy
        entries: root.queueEntries
        onSaveRequested: root.namePlaylist("", true)
        onDismissed: root.back()
        onEntryActivated: entry => {
            if (entry.current !== true && root.supported("remove")) {
                root.queueOccurrence = entry.occurrence;
                Qt.callLater(() => Ui.Focus.take(queueKeep));
            }
        }
    }
    MusicMoreSheet {
        id: moreSheet
        objectName: "musicMoreView"
        z: 20
        visible: root.moreOpen
        actions: root.moreActions
        onDismissed: root.closeMore()
        onActivated: id => root.runMore(id)
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
            onActivated: root.cancelReplacement()
        }
        Ui.ActionButton {
            id: confirmReplace
            objectName: "musicReplaceQueue"
            text: qsTr("Replace and play")
            onActivated: {
                const id = root.pendingPlay;
                root.pendingPlay = "";
                root.replaceOpener = null;
                root.startCommand(root.pendingAction, id, root.pendingActive);
                root.openPlayer();
            }
        }
    }
    // The Music root and its All lists (#800).
    MusicRoomView {
        id: roomView
        objectName: "musicRoom"
        // Under the confirm dialogs (z 20 and above), over the catalogue.
        z: 15
        visible: root.mode === "root" && !!root.room
        focus: visible
        room: root.room
        catalog: root.catalog
        onEntryActivated: entry => root.openFromRoom(entry, false)
        onAllRequested: shelfId => {
            const shelf = (root.room.shelves || []).find(item => item.id === shelfId);
            root.roomKey = "";
            root.roomShelf = shelfId;
            root.mode = "all";
            allView.open(shelfId, shelf ? shelf.title : "");
        }
        onFindRequested: {
            // Find searches every connected source (MH-10, #801).
            root.roomKey = "";
            search.text = "";
            root.room.search("");
            root.mode = "search";
            Qt.callLater(() => Ui.Focus.take(search));
        }
    }
    MusicAllView {
        id: allView
        objectName: "musicAll"
        z: 15
        visible: root.mode === "all" && !!root.room
        focus: visible
        room: root.room
        playingKey: roomView.playingKey
        onEntryActivated: entry => root.openFromRoom(entry, true)
    }
    // Find over every source (#801, MH-10).
    MusicSearchView {
        id: searchView
        objectName: "musicSearchResults"
        z: 15
        y: 280
        height: Ui.Theme.canvasHeight - y
        visible: root.mode === "search" && !!root.room
        room: root.room
        playingKey: roomView.playingKey
        onUpRequested: Ui.Focus.take(search)
        onEntryActivated: entry => root.openFromRoom(entry, false, true)
    }
}
