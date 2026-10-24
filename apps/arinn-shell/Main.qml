// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import QtQuick.Window
import Arinn.Shell 1.0
import "ui" as Ui
import "ui/screens/browser" as Browser
import "ui/screens/browse" as Browse
import "ui/screens/downloads" as Downloads
import "ui/screens/library" as Library
import "ui/screens/player" as Player
import "ui/screens/power" as Power
import "ui/screens/session" as Session
import "ui/screens/settings" as Settings

Window {
    id: root
    width: 1920; height: 1080
    visibility: (backend.homeVisible || mediaCatalog.inputLayerActive || leavingPlayback
                 || leavingStopPending || (playerOpen && leaveError !== "")) && !sessionGuard.blocked ? Window.FullScreen : Window.Hidden
    // After Back leaves playback the input layer ends before the daemon
    // reports Home again; stay on screen in between instead of vanishing
    // (#246). Cleared by that report. The bound is only a last resort for a
    // daemon that never reports: it covers a slow KWin handoff plus the
    // backend's one-second status poll with a wide margin.
    property bool leavingPlayback: false
    Timer { id: leavingBound; interval: 10000; onTriggered: root.leavingPlayback = false }
    color: playerOpen && !leavingStopPending && leaveError === "" ? "transparent" : Ui.Theme.background
    title: "Arinn"
    flags: Qt.Window | Qt.FramelessWindowHint
    readonly property bool navOpen: home.navOpen
    property bool browserOpen: false
    property bool browseOpen: false
    function showBrowse() { browseCatalog.reset(); browseOpen = true; }
    function closeBrowse() { browseOpen = false; browseCatalog.reset(); Qt.callLater(restoreFocus); }
    property bool sessionOpen: false
    property bool playerOpen: false
    property bool leavingWaitForSession: false
    Timer {
        id: leaveSessionBound
        objectName: "leaveSessionBound"
        interval: 30000
        onTriggered: {
            // Keep selection fenced: the eventual row still belongs to the
            // exact launch whose exit was requested.
            root.leaveError = qsTr("Playback is not available yet. Press Home to leave, or wait to stop the launched session.");
            playerView.osdVisible = true;
        }
    }
    property bool leavingStopPending: false
    property string leaveError: ""
    property bool sessionFromLibrary: false
    // Watch on a download whose library item the library catalog has not
    // read yet: {provider, local}, opened once the catalog lists it.
    property var pendingWatch: null
    // Each Watch request, and leaving the library, starts a new ticket; a
    // deferred open of an older ticket does nothing (#341).
    property int watchTicket: 0
    // Leaving the library abandons a pending Watch; coming back later must
    // not open an old download's Detail (review of #310).
    Connections {
        target: home
        function onScreenChanged() {
            if (home.screen === "library") return;
            root.pendingWatch = null;
            ++root.watchTicket;
        }
    }
    function openWatchTarget() {
        if (!pendingWatch || home.screen !== "library") {
            pendingWatch = null;
            return;
        }
        const entryId = libraryCatalog.entryIdFor(pendingWatch.provider, pendingWatch.local);
        if (!entryId.length) return;
        pendingWatch = null;
        const ticket = watchTicket;
        Qt.callLater(() => {
            if (ticket === root.watchTicket && home.screen === "library")
                libraryView.openEntry(entryId);
        });
    }
    property bool presentationAtHome: false
    property bool presentationRequested: false
    function showPlaybackHome() {
        if (leavingStopPending)
            return;
        leaveError = "";
        if (!mediaCatalog.inputLayerActive)
            return;
        playerView.sheetKind = "";
        presentationAtHome = true;
        playerOpen = false;
        sessionOpen = false;
        browserOpen = false;
        browseOpen = false;
        browseCatalog.reset();
        home.showHome();
    }
    onActiveChanged: {
        if (!active && mediaCatalog.presentationState === "active")
            Qt.callLater(() => {
                if (!root.active && mediaCatalog.presentationState === "active")
                    mediaCatalog.abandonPresentation();
            });
    }
    function openSession(entryId, fromLibrary) {
        if (leavingStopPending)
            return;
        leaveError = "";
        presentationAtHome = false;
        presentationRequested = false;
        sessionFromLibrary = fromLibrary;
        mediaCatalog.select(entryId);
        applyMediaSurface();
    }
    // One structured §14.1 record when playback does not get the player (#369).
    function logMediaSurface(surface, reason) {
        mediaCatalog.logSurface(surface, reason);
    }
    function applyMediaSurface() {
        // Rich flags select the OSD. A library launch whose session is not
        // in the snapshot yet is managed playback, so the OSD is shown until
        // the row arrives. A row without those flags uses the session screen.
        let player = mediaCatalog.playerSession;
        if (sessionFromLibrary && mediaCatalog.sessionState !== "ready")
            player = true;
        playerOpen = player;
        if (player && mediaCatalog.windowAvailable && !presentationRequested) {
            presentationRequested = true;
            mediaCatalog.present();
        }
        sessionOpen = !player;
        if (!player)
            logMediaSurface("session", mediaCatalog.sessionState === "ready" ? "no_player_features" : "session_not_ready");
        Qt.callLater(() => {
            if (player)
                Ui.Focus.take(playerView);
            else
                Ui.Focus.take(sessionView);
        });
    }
    function leavePlayback() {
        if (leavingStopPending)
            return;
        leaveError = "";
        if (mediaCatalog.inputLayerActive) {
            leavingPlayback = true;
            leavingBound.restart();
        }
        if (mediaCatalog.sessionState !== "ready") {
            if (sessionFromLibrary || mediaCatalog.sessionState === "loading") {
                leavingWaitForSession = true;
                leavingStopPending = true;
                leaveSessionBound.restart();
                return;
            }
            finishPlaybackExit();
            return;
        }
        leavingWaitForSession = false;
        leaveSessionBound.stop();
        leavingStopPending = true;
        mediaCatalog.stop();
        finishStopOperation();
    }
    function finishStopOperation() {
        if (!leavingStopPending || leavingWaitForSession || mediaCatalog.operationState === "pending")
            return;
        leavingStopPending = false;
        if (mediaCatalog.operationState === "succeeded")
            finishPlaybackExit();
        else {
            leaveError = qsTr("Unable to stop playback. Press Back to retry.");
            playerView.osdVisible = true;
        }
    }
    function finishPlaybackExit() {
        mediaCatalog.release();
        presentationAtHome = false;
        closePlayer();
    }
    function closePlayer() {
        playerOpen = false;
        if (sessionFromLibrary) {
            sessionFromLibrary = false;
            home.screen = "library";
            Qt.callLater(() => {
                if (libraryView.detailOpen)
                    libraryView.focusDetailAction();
                else
                    Ui.Focus.take(libraryView);
            });
        } else {
            Qt.callLater(home.showHome);
        }
    }
    // Focus ownership (ADR-0076). Main alone decides which screen holds
    // focus: each screen binds `focus` to this route, so the screen on
    // display is the focused scope and Qt restores the child it last focused.
    // Screens move focus only inside themselves, through Ui.Focus.take(),
    // which refuses hidden or disabled items. Home's navigation menu and
    // shortcut editor cover the library and settings screens, so they win.
    // Home's navigation menu opens over every screen but the player (#329).
    // Ambient covers everything and takes every key until it wakes; the power
    // scope traps focus above everything it covers (§5.7, §6.1).
    readonly property string focusedScreen: ambientScreen.active ? "ambient"
        : powerOverlay.open ? "power"
        : playerOpen ? "player"
        : home.navOpen || home.shortcutEditorOpen || home.tileActionsOpen ? "home"
        : sessionOpen ? "session"
        : browseOpen ? "browse"
        : browserOpen ? "browser"
        : home.screen === "library" ? "library"
        : home.screen === "downloads" ? "downloads"
        : home.screen === "settings" ? "settings"
        : "home"
    function screenItem(name) {
        return ({player: playerView, session: sessionView, browser: browserView, browse: browseView,
                 library: libraryView, downloads: downloadsView, settings: settingsView, home: home,
                 power: powerOverlay, ambient: ambientScreen})[name];
    }
    // Focus the screen on display at its preferred item.
    function restoreFocus() {
        switch (focusedScreen) {
        case "ambient": Ui.Focus.take(ambientScreen); break;
        case "power": powerOverlay.menu.restoreMenuFocus(); break;
        case "player": Ui.Focus.take(playerView); break;
        case "session": sessionView.restore(); break;
        case "browser": browserView.settleFocus(); break;
        case "browse": browseView.focusInitial(); break;
        case "library": libraryView.focusInitial(); break;
        case "downloads": downloadsView.restore(); break;
        case "settings": settingsView.focusInitial(); break;
        default: home.restoreFocus();
        }
    }
    function focusInside(item, scope) {
        for (let current = item; current; current = current.parent)
            if (current === scope) return true;
        return false;
    }
    // A new route whose screen did not get focus through its scope (nothing
    // focused there yet) starts at the screen's preferred item.
    // A hidden item inside the screen (a closed menu entry) does not count.
    onFocusedScreenChanged: Qt.callLater(() => {
        if (visible && (!focusInside(activeFocusItem, screenItem(focusedScreen))
                        || !activeFocusItem.visible)) restoreFocus();
    })
    // Last resort: Qt moves active focus to the root item when the focused
    // item is destroyed. Nothing handles keys there, so the remote would stop
    // working. Hand focus back to the screen on display, and say so: it
    // means some path bypassed the rules above.
    // An inactive window parks focus on the root item itself; Qt restores it
    // on activation, so only an active window counts.
    function focusFellToRoot() {
        return visible && active && (!activeFocusItem || activeFocusItem === contentItem);
    }
    property bool hadFocus: false
    onActiveFocusItemChanged: {
        if (!focusFellToRoot()) {
            hadFocus = true;
            return;
        }
        const fell = hadFocus;
        Qt.callLater(() => {
            if (!focusFellToRoot()) return;
            if (fell) console.warn("Focus fell to the window root; restoring", focusedScreen);
            restoreFocus();
        });
    }
    onVisibleChanged: if (visible) Qt.callLater(() => {
        // Returning from an external activity keeps the displayed route. Home
        // must not take focus behind Open services or another retained screen.
        if (focusedScreen === "home") home.showHome();
        else restoreFocus();
    })
    Connections {
        target: backend
        ignoreUnknownSignals: true
        function onChanged() {
            if (backend.homeVisible) root.leavingPlayback = false;
            if (!mediaCatalog.inputLayerActive) return;
            if (backend.homeVisible) root.showPlaybackHome();
            else if (root.presentationAtHome) {
                root.presentationAtHome = false;
                if (!root.active) { mediaCatalog.abandonPresentation(); return; }
                root.playerOpen = true;
                Qt.callLater(() => Ui.Focus.take(playerView));
            }
        }
    }
    Connections {
        target: sessionGuard
        ignoreUnknownSignals: true
        function onChanged() {
            if (sessionGuard.blocked) {
                mediaCatalog.abandonPresentation();
                powerOverlay.sessionBlocked();
                // A lock is not a wake: no event to consume, the screen
                // beneath comes back when the shell is shown again.
                ambientScreen.stop();
            }
        }
    }
    // Home does not show the library's operation state, so report a failed
    // resume from a Continue card here.
    property bool resumeFromHome: false
    // The library launch in flight came from Home, not from the library.
    property bool launchFromHome: false
    Connections {
        target: libraryCatalog
        // Deferred so that LibraryView's bound operation state is current.
        function onOperationChanged() {
            Qt.callLater(() => {
                if (!root.resumeFromHome || libraryView.operationPending) return;
                root.resumeFromHome = false;
                if (!libraryView.operationFailed) return;
                root.launchFromHome = false;
                feedback.text = libraryView.operationMessage;
                feedbackTimer.restart();
            });
        }
    }
    BrowserCatalog { id: browserCatalog }
    BrowseCatalog { id: browseCatalog; objectName: "browseCatalog"; tabs: browserCatalog.tabs }
    MediaCatalog { id: mediaCatalog; objectName: "mediaCatalog" }
    // Continue media cards without session artwork use the backdrop of the
    // library item they play (#365, ADR-0062). `library` only makes the
    // binding follow catalog changes.
    function continueMedia(entries, library) {
        return entries.map(entry => {
            if (entry.artwork) return entry;
            const content = mediaCatalog.libraryContent(entry.id);
            const artwork = content.local ? libraryCatalog.artworkFor(content.provider, content.local) : "";
            return artwork ? Object.assign({}, entry, {artwork: artwork}) : entry;
        });
    }
    // An imported download shows the poster of the library entry its Watch
    // opens: the first library ref, through entryIdFor (#360, ADR-0062).
    // Other rows keep the view's neutral plate. `library` only makes the
    // binding follow catalog changes.
    function downloadJobs(jobs, library) {
        return jobs.map(job => {
            const target = downloadCatalog.watchTarget(job.id);
            const poster = target.local ? libraryCatalog.posterFor(target.provider, target.local) : "";
            return poster ? Object.assign({}, job, {artwork: poster}) : job;
        });
    }
    LibraryCatalog { id: libraryCatalog; objectName: "libraryCatalog" }
    // The Activity's last frame behind Home (§12.3, ADR-0087). Optional and
    // asynchronous: Home and its focus never wait for it, and the shell's own
    // session guard clears it before any daemon reply.
    ActivityBackgroundSource {
        id: activityBackground
        objectName: "activityBackground"
        blocked: sessionGuard.blocked
    }
    SystemStatus {
        id: systemStatus
        objectName: "systemStatus"
    }
    // Ambient (§5.7, #307): an explicit start or ten idle minutes on screen,
    // never while media plays. It never suspends, pauses or closes anything.
    Power.AmbientIdle {
        id: ambientIdle
        objectName: "ambientIdle"
        // Settings › Appearance › Ambient after (ADR-0112), 10 minutes by default.
        timeout: settingsCatalog.ambientTimeoutMinutes * 60000
        shellShown: root.visible && root.active
        locked: sessionGuard.blocked
        ambientActive: ambientScreen.active
        launching: browserCatalog.opening
        mediaPlaying: root.playerOpen || ambientIdle.playing(mediaCatalog.continueEntries, browserCatalog.tabs,
                                                             mediaCatalog.inputLayerActive)
        onIdle: root.startAmbient()
    }
    // The item that had focus when ambient started; wake gives it back.
    property var ambientReturn: null
    function startAmbient() {
        if (ambientScreen.active) return;
        ambientReturn = activeFocusItem;
        ambientScreen.start();
    }
    function finishAmbient() {
        const target = ambientReturn;
        ambientReturn = null;
        Qt.callLater(() => {
            if (ambientScreen.active) return;
            if (!target || !focusInside(target, screenItem(focusedScreen)) || !Ui.Focus.take(target))
                restoreFocus();
        });
    }
    // Power actions and volume through the daemon (ADR-0108).
    SystemControl {
        id: systemControl
        objectName: "systemControl"
    }
    DownloadCatalog {
        id: downloadCatalog
        objectName: "downloadCatalog"
        onDownloadImported: message => downloadsView.showToast(message)
    }
    SettingsCatalog { id: settingsCatalog; objectName: "settingsCatalog" }
    KeyboardMonitor { id: keyboardMonitor }
    // Every key press and pointer movement in the shell feeds the one input
    // mode; it hides the cursor in D-pad mode (spec §6.1, ADR-0095).
    InputModeSource {
        pointerActive: Ui.InputMode.pointerActive
        onKeyPressed: { Ui.InputMode.keyPressed(); ambientIdle.activity(); }
        onPointerMoved: { Ui.InputMode.pointerMoved(); ambientIdle.activity(); }
        onPointerArrived: Ui.InputMode.forgetPointer()
    }
    Shortcut {
        sequences: ["Esc", "Backspace", "Back"]
        context: Qt.WindowShortcut
        autoRepeat: false
        enabled: root.active && mediaCatalog.presentationState === "ReleaseFailed"
                 && !root.playerOpen && !root.sessionOpen && !root.browserOpen && !root.browseOpen
                 && home.screen === "home" && !home.navOpen
                 && !home.shortcutEditorOpen && !sessionGuard.blocked
        onActivated: mediaCatalog.release()
    }
    // Menu and Home that the focused screen did not accept (#329, spec §6.2,
    // §6.3). Home handles both itself; these are the other screens.
    function unhandledKey(event) {
        if (event.isAutoRepeat || event.modifiers !== Qt.NoModifier) return;
        const section = focusedScreen === "library" || focusedScreen === "settings"
            || focusedScreen === "downloads";
        const overlay = focusedScreen === "browser" || focusedScreen === "browse"
            || focusedScreen === "session";
        // A Settings editor keeps Menu; Home reaches the shell through the
        // view's homeRequested, which also cancels the editor.
        if (focusedScreen === "settings" && settingsView.editingKey.length) return;
        if (event.key === Qt.Key_Menu && (section || overlay)) {
            home.openNavigation();
            event.accepted = true;
        } else if (event.key === Qt.Key_Home) {
            if (goHomeFrom(focusedScreen)) event.accepted = true;
        }
    }
    // Open services, Browse and a session give way to Home's navigation.
    function closeOverlays() {
        browserOpen = false;
        if (browseOpen) { browseOpen = false; browseCatalog.reset(); }
        if (sessionOpen) { sessionOpen = false; sessionFromLibrary = false; }
    }
    // Home from a screen that does not handle it itself; false when the
    // screen is not one of these.
    function goHomeFrom(screen) {
        if (screen === "library" || screen === "settings" || screen === "downloads") home.showHome();
        else if (screen === "browser") { browserOpen = false; Qt.callLater(home.showHome); }
        else if (screen === "browse") { browseOpen = false; browseCatalog.reset(); Qt.callLater(home.showHome); }
        else if (screen === "session") {
            sessionOpen = false;
            sessionFromLibrary = false;
            Qt.callLater(home.showHome);
        } else return false;
        return true;
    }
    Ui.LogicalCanvas {
        anchors.fill: parent
        // Keys bubble here from every screen that left them unaccepted.
        Keys.onPressed: event => root.unhandledKey(event)
        Ui.HomeView {
            id: home
            focus: root.focusedScreen === "home"
            // The navigation menu is drawn above the section screens (z 4) and
            // Open services, Browse and Session (z 5), below the player (z 6).
            z: navOpen || tileActionsOpen ? 5.5 : 0
            coveredByScreen: libraryView.visible || settingsView.visible || downloadsView.visible
                || browserView.visible || browseView.visible || sessionView.visible
            // Home remains painted beneath other screens, but its catalog
            // callbacks and hovered cards must not take their input focus.
            enabled: root.focusedScreen === "home"
            objectName: "productionHome"
            visible: !root.playerOpen
            actionsEnabled: backend.favoritesReady === true
            favoriteSaving: backend.favoriteSaving === true
            favoriteError: backend.favoriteError || ""
            onFavoriteRequested: (entryId, pinned) => backend.setFavorite(entryId, pinned)
            onTerminateRequested: entryId => backend.terminate(entryId)
            shortcutManagement: true
            keyboardAvailable: keyboardMonitor.available
            shortcutEditor: backend.shortcutEditor || ({})
            shortcutSaving: backend.shortcutSaving === true
            shortcutError: backend.shortcutError || ""
            onEditShortcutRequested: entryId => backend.editShortcut(entryId)
            onCloseShortcutEditorRequested: backend.closeShortcutEditor()
            onSaveShortcutRequested: (title, url) => backend.saveShortcut(title, url)
            onRemoveShortcutRequested: backend.removeShortcut()
            busyEntryIds: backend.busyEntryIds
            // Only explicit current evidence may replace the generic Return tile.
            returnIsWebService: backend.connected === true && backend.homeVisible && browserCatalog.presentedReturn
                && backend.daemonEpoch === browserCatalog.returnEpoch
                && backend.returnWindow === browserCatalog.returnTarget
            continueEntries: (returnIsWebService ? [] : backend.continueEntries)
                .concat(root.continueMedia(mediaCatalog.continueEntries, libraryCatalog.entries))
            favoriteEntries: [{id: "arinn-browse", title: qsTr("Site or search"), hint: qsTr("Type an address or a search"),
                               monogram: "⌕", enabled: true}].concat(backend.favorites)
            applicationEntries: backend.applications
            runningEntries: backend.items
            favoritesTitle: qsTr("Favorites")
            returnActivityId: backend.returnWindow
            activitySource: backend.homeVisible ? activityBackground.source : ""
            ambientSuppressed: activitySource.toString() !== ""
            // The shortcut editor's on-screen keyboard (KB-2, KB-3).
            shortcutAddressSuggestions: { browseCatalog.tabs; return browseCatalog.openServices(shortcutTypingText); }
            shortcutNameSuggestions: { browseCatalog.tabs; return browseCatalog.nameSuggestions(shortcutDraftUrl, shortcutTypingText); }
            openServices: browserCatalog.tabs.map(tab => ({id: tab.id, title: tab.title || tab.domain || "",
                                                            letter: tab.letter || "", icon: tab.icon || ""}))
            downloadActive: downloadCatalog.downloading
            downloadProgress: downloadCatalog.downloadProgress
            muted: systemStatus.muted
            // Settings › Appearance › Light field (ADR-0112).
            ambientEnabled: settingsCatalog.lightField
            wifi: systemStatus.wifi
            wifiLevel: systemStatus.wifiLevel
            currentServiceId: backend.homeVisible && !returnIsWebService ? "" : (browserCatalog.presentedService.id || "")
            currentServiceTitle: backend.homeVisible && !returnIsWebService ? "" : (browserCatalog.presentedService.title || "")
            currentServiceSubtitle: backend.homeVisible && !returnIsWebService ? "" : (browserCatalog.presentedService.subtitle || "")
            browserUnavailable: browserCatalog.browserState === "unavailable"
            openShortcutIds: browserCatalog.openShortcutIds
            // Site or search remains available even with no saved favorites.
            homeState: backend.homeState === "empty" ? "ready" : backend.homeState
            statusTitle: homeState === "unavailable" ? qsTr("Home is unavailable") : homeState === "loading" ? qsTr("Loading Home…") : homeState === "error" ? qsTr("Home could not be loaded") : homeState === "updated" ? qsTr("Arinn was updated") : qsTr("Your Home starts here")
            statusDescription: homeState === "empty" ? qsTr("No favorites are available yet.") : homeState === "updated" ? qsTr("Restart it to finish. Navigation remains available.") : qsTr("Navigation remains available. Home reconnects automatically.")
            sectionDescription: screen === "applications" ? ((applicationEntries.length || runningEntries.length) ? qsTr("Choose an application or an open window.") : qsTr("No applications are available.")) : qsTr("This section is not available yet.")
            clockText: Qt.formatTime(new Date(), "hh:mm")
            Timer { interval: 1000; running: true; repeat: true; onTriggered: home.clockText = Qt.formatTime(new Date(), "hh:mm") }
            onEntryRequested: entryId => { if (entryId === "arinn-browse") root.showBrowse(); else backend.launch(entryId); }
            onRunningRequested: entryId => backend.activate(entryId)
            onReturnRequested: backend.back()
            onRetryRequested: backend.refresh()
            onRestartRequested: backend.restartArinn()
            // A section picked in the navigation menu, or Home pressed there,
            // replaces an open overlay (#329, §6.3).
            onSectionRequested: route => root.closeOverlays()
            onHomeKeyPressed: root.closeOverlays()
            onOpenServicesRequested: {
                // From the menu over Browse or a session, Open services
                // replaces that overlay instead of opening beneath it.
                root.closeOverlays();
                root.browserOpen = true;
                Qt.callLater(() => Ui.Focus.take(browserView));
            }
            onServiceRequested: backend.back()
            // One card for a browser tab and its media session (ADR-0096).
            onTabRequested: tabId => browserCatalog.activateTab(tabId)
            onMediaActionRequested: (sessionId, action) => mediaCatalog.sessionAction(sessionId, action)
            onSessionRequested: entryId => {
                // A stopped film resumes through the library; the stopped
                // session itself has nothing to present (#247).
                const target = mediaCatalog.resumeTarget(entryId);
                if (target.local) {
                    root.resumeFromHome = true;
                    root.launchFromHome = true;
                    libraryCatalog.launchContent(target.provider, target.local);
                } else
                    root.openSession(entryId, false);
            }
            onCloseShellRequested: root.close()
            onPowerRequested: powerOverlay.openMenu()
        }
        Browser.BrowserView {
            id: browserView
            focus: root.focusedScreen === "browser"
            objectName: "productionBrowser"
            visible: root.browserOpen && !root.playerOpen
            enabled: root.browserOpen && !root.playerOpen
            interactive: root.browserOpen && !root.playerOpen && !root.browseOpen
            tabs: browserCatalog.tabs
            windows: browserCatalog.windows
            shortcuts: browserCatalog.shortcuts
            browserState: browserCatalog.browserState
            operationState: browserCatalog.operationState
            saveError: browserCatalog.saveError
            onActivateTab: tabId => browserCatalog.activateTab(tabId)
            onCloseTab: tabId => browserCatalog.closeTab(tabId)
            onSavePage: tabId => browserCatalog.addShortcut(tabId)
            onSaveErrorDismissed: browserCatalog.dismissSaveError()
            onOpen: shortcutId => browserCatalog.openShortcut(shortcutId)
            onRetryRequested: browserCatalog.retry()
            onBrowseRequested: root.showBrowse()
            onBackRequested: { root.browserOpen = false; Qt.callLater(home.showHome); }
        }
        Browse.BrowseView {
            id: browseView
            objectName: "productionBrowse"
            visible: root.browseOpen && !root.playerOpen
            enabled: visible
            focus: root.focusedScreen === "browse"
            z: 5
            text: browseCatalog.text
            kind: browseCatalog.kind
            resolvedUrl: browseCatalog.url
            engine: browseCatalog.engine
            openTitle: browseCatalog.openTitle
            operationState: browseCatalog.operationState
            ordinaryApplications: {
                const result = [];
                for (let i = 0; i < backend.applications.length; ++i) {
                    const row = backend.applications[i];
                    if (row.group === "online" && !row.webShortcut && row.enabled) result.push(row);
                }
                return result;
            }
            suggestions: browseCatalog.suggestions
            recent: browseCatalog.recent
            keyboardAvailable: keyboardMonitor.available
            browserUnavailable: !backend.connected || browserCatalog.retryAction === "restart"
                || browseCatalog.operationState === "Unavailable"
            onTextEdited: value => browseCatalog.text = value
            onOpenRequested: browseCatalog.open()
            onApplicationRequested: entryId => backend.launch(entryId)
            onClearRequested: browseCatalog.clearRecent()
            onRetryRequested: { browseCatalog.dismissError(); browserCatalog.retry(); browseCatalog.refreshRecent(); }
            onBackRequested: root.closeBrowse()
            onHomeRequested: root.goHomeFrom("browse")
        }
        Session.SessionView {
            id: sessionView
            focus: root.focusedScreen === "session"
            objectName: "productionSession"
            visible: root.sessionOpen && !root.playerOpen
            enabled: root.sessionOpen && !root.playerOpen
            z: 5
            sessionState: mediaCatalog.sessionState
            artwork: mediaCatalog.sessionArtwork
            title: mediaCatalog.sessionTitle
            subtitle: mediaCatalog.sessionSubtitle
            playbackState: mediaCatalog.playbackState
            durationUs: mediaCatalog.durationUs
            positionUs: mediaCatalog.positionUs
            availableActions: mediaCatalog.sessionActions
            operationState: mediaCatalog.operationState
            onPlayRequested: mediaCatalog.play()
            onPauseRequested: mediaCatalog.pause()
            onPlayPauseRequested: mediaCatalog.playPause()
            onSeekRequested: targetUs => mediaCatalog.seek(targetUs)
            onNextRequested: mediaCatalog.next()
            onPreviousRequested: mediaCatalog.previous()
            onStopRequested: mediaCatalog.stop()
            onBackRequested: {
                root.sessionOpen = false;
                if (root.sessionFromLibrary) {
                    root.sessionFromLibrary = false;
                    Qt.callLater(() => {
                        if (libraryView.detailOpen)
                            libraryView.focusDetailAction();
                        else
                            Ui.Focus.take(libraryView);
                    });
                } else {
                    Qt.callLater(home.showHome);
                }
            }
        }
        Settings.SettingsView {
            id: settingsView
            focus: root.focusedScreen === "settings"
            objectName: "productionSettings"
            visible: home.screen === "settings" && !root.playerOpen
            enabled: visible
            z: 4
            screenState: settingsCatalog.screenState
            categories: settingsCatalog.categories
            rows: settingsCatalog.rows
            keyboardAvailable: keyboardMonitor.available
            onValueChangeRequested: (providerId, settingId, value) => settingsCatalog.setValue(providerId, settingId, value)
            onErrorDismissRequested: (providerId, settingId) => settingsCatalog.dismissError(providerId, settingId)
            onDismissRequested: home.back()
            onHomeRequested: root.goHomeFrom("settings")
            onRetryRequested: settingsCatalog.refresh()
        }
        Library.LibraryView {
            id: libraryView
            focus: root.focusedScreen === "library"
            objectName: "productionLibrary"
            visible: home.screen === "library" && !root.playerOpen
            enabled: visible
            z: 4
            libraryState: libraryCatalog.libraryState
            tabs: libraryCatalog.tabs
            entries: libraryCatalog.entries
            operationState: libraryCatalog.operationState
            launchingId: libraryCatalog.launchingId
            onPlayRequested: entryId => libraryCatalog.launch(entryId)
            onEpisodeRequested: (entryId, episodeId) => libraryCatalog.launch(episodeId)
            onRetryRequested: libraryCatalog.refresh()
            onBackRequested: home.back()
            onHomeRequested: {
                root.sessionFromLibrary = false;
                home.showHome();
            }
        }
        Downloads.DownloadsView {
            id: downloadsView
            focus: root.focusedScreen === "downloads"
            objectName: "productionDownloads"
            visible: home.screen === "downloads" && !root.playerOpen
            enabled: visible
            z: 4
            autoToast: false
            viewState: downloadCatalog.downloadsState
            jobs: root.downloadJobs(downloadCatalog.jobs, libraryCatalog.entries)
            onPauseRequested: id => downloadCatalog.request(id, "pause")
            onResumeRequested: id => downloadCatalog.request(id, "resume")
            onStartNowRequested: id => downloadCatalog.request(id, "start_now")
            onRetryRequested: id => downloadCatalog.request(id, "retry")
            // Watch opens the imported item's Detail in the library, or the
            // library itself when the catalog does not list it yet.
            onWatchRequested: id => {
                const target = downloadCatalog.watchTarget(id);
                home.screen = "library";
                ++root.watchTicket;
                root.pendingWatch = target.local ? target : null;
                if (!root.pendingWatch) return;
                root.openWatchTarget();
                // Not listed yet: read again, and open it when it appears.
                if (root.pendingWatch) libraryCatalog.refresh();
            }
            onRefreshRequested: downloadCatalog.refresh()
            onBackRequested: home.back()
        }
        Player.PlayerView {
            id: playerView
            objectName: "productionPlayer"
            focus: root.focusedScreen === "player"
            visible: root.playerOpen
            enabled: root.playerOpen && (!root.leavingStopPending
                                        || (root.leavingWaitForSession && root.leaveError !== ""))
            z: 6
            pictureStandIn: false
            title: mediaCatalog.sessionTitle
            episodeLine: mediaCatalog.sessionSubtitle
            position: mediaCatalog.positionUs / 1000000
            duration: mediaCatalog.durationUs < 0 ? 0 : mediaCatalog.durationUs / 1000000
            buffered: mediaCatalog.bufferedSeconds
            chapters: mediaCatalog.chapters
            playing: mediaCatalog.playbackState === "playing"
            seekSupported: {
                const actions = mediaCatalog.sessionActions;
                if (!actions || actions.length === undefined)
                    return false;
                for (let i = 0; i < actions.length; i++)
                    if (actions[i] === "seek")
                        return true;
                return false;
            }
            tracksSupported: mediaCatalog.tracksAvailable
            timelineSupported: mediaCatalog.timelineAvailable
            audioTracks: mediaCatalog.audioTracks
            subtitleTracks: mediaCatalog.subtitleTracks
            audioTrackId: mediaCatalog.audioTrackId
            subtitleTrackId: mediaCatalog.subtitleTrackId
            thumbnail: mediaCatalog.thumbnail
            statusTitle: root.leaveError
            playbackState: {
                if (root.leavingStopPending)
                    return "loading";
                if (root.leaveError !== "")
                    return "error";
                const state = mediaCatalog.sessionState;
                if (mediaCatalog.windowAvailable && mediaCatalog.presentationState !== "active")
                    return mediaCatalog.presentationState === "pending" ? "loading" : "unavailable";
                if (state === "ready")
                    return "ready";
                if (state === "loading")
                    return "loading";
                return "unavailable";
            }
            onPlay: mediaCatalog.play()
            onPause: mediaCatalog.pause()
            onSeek: targetSeconds => mediaCatalog.seek(targetSeconds * 1000000)
            onSelectAudio: trackId => mediaCatalog.selectAudio(trackId)
            onSelectSubtitle: trackId => mediaCatalog.selectSubtitle(trackId)
            onStop: root.leavePlayback()
            onLeave: root.leavePlayback()
            onHomeRequested: {
                // A vanished/unreported launch cannot fence navigation forever.
                // Explicit Home cancels only the waiting exit, never a running Stop.
                if (root.leavingWaitForSession && root.leaveError !== "") {
                    root.leavingWaitForSession = false;
                    root.leavingStopPending = false;
                    leaveSessionBound.stop();
                }
                if (mediaCatalog.inputLayerActive) {
                    root.showPlaybackHome();
                    mediaCatalog.requestHome();
                } else {
                    root.leaveError = "";
                    root.playerOpen = false;
                    root.sessionFromLibrary = false;
                    home.showHome();
                    mediaCatalog.requestHome();
                }
            }
        }
        Binding {
            target: mediaCatalog
            property: "thumbnailWanted"
            value: root.playerOpen && playerView.timelineFocused && mediaCatalog.timelineAvailable
        }
        Connections {
            target: mediaCatalog
            function onPresentationHomeRequested() { root.showPlaybackHome(); }
            function onOperationChanged() { root.finishStopOperation(); }
            function onPresentationChanged() {
                if (root.leavingStopPending || root.leaveError !== "")
                    return;
                if (mediaCatalog.presentationState !== "" && mediaCatalog.presentationState !== "pending" && mediaCatalog.presentationState !== "active") {
                    root.logMediaSurface("home", "presentation_failed");
                    root.playerOpen = false;
                    root.presentationAtHome = false;
                    if (root.active) {
                        home.showHome();
                        mediaCatalog.requestHome();
                    }
                    feedback.text = mediaCatalog.presentationState === "ReleaseFailed"
                        ? qsTr("Unable to exit playback controls. Press Back to retry.")
                        : qsTr("Playback controls are unavailable.");
                    feedbackTimer.restart();
                }
                if (mediaCatalog.presentationState === "active" && !root.active && !backend.homeVisible && !root.presentationAtHome)
                    mediaCatalog.abandonPresentation();
                if (mediaCatalog.presentationState === "" && root.presentationRequested) {
                    root.playerOpen = false;
                    root.presentationAtHome = false;
                }
            }
            function onSessionChanged() {
                if (root.leavingWaitForSession && mediaCatalog.sessionState === "unavailable") {
                    root.leaveError = qsTr("Playback is unavailable. Press Home to leave, or wait to stop the launched session.");
                    playerView.osdVisible = true;
                }
                if (root.leavingWaitForSession && mediaCatalog.sessionState === "ready") {
                    root.leavingStopPending = false;
                    root.leavePlayback();
                    return;
                }
                if (root.leavingStopPending)
                    return;
                if (root.playerOpen && mediaCatalog.windowAvailable && !root.presentationAtHome && !root.presentationRequested) {
                    root.presentationRequested = true;
                    mediaCatalog.present();
                }
                if (!root.sessionFromLibrary || (!root.playerOpen && !root.sessionOpen))
                    return;
                const player = mediaCatalog.playerSession;
                if (root.playerOpen === player && root.sessionOpen === !player)
                    return;
                if (mediaCatalog.sessionState !== "ready" && root.playerOpen)
                    return;
                root.applyMediaSurface();
            }
        }
        Connections {
            target: libraryCatalog
            function onCatalogChanged() { if (root.pendingWatch) root.openWatchTarget(); }
            function onSessionLaunched(presentationId) {
                // A resume started from a Home Continue card returns to Home
                // when playback ends; any other library launch returns to
                // the library (§6.4).
                const fromLibrary = !root.launchFromHome;
                root.launchFromHome = false;
                root.openSession(presentationId, fromLibrary);
            }
        }
        // The power scope above every screen, and the volume HUD (§5.7).
        Power.PowerOverlay {
            id: powerOverlay
            anchors.fill: parent
            z: 7
            focus: root.focusedScreen === "power"
            control: systemControl
            runningApplications: backend.items
            registryKnown: backend.applicationsState === "ready" || backend.applicationsState === "empty"
            keysEnabled: root.visible && root.active && !sessionGuard.blocked && !ambientScreen.active
            // A late refusal never takes focus behind the launch dialog or
            // the ambient screen; it appears once they are gone.
            holdLateAnswer: launchOverlay.visible || ambientScreen.active
            onClosed: Qt.callLater(() => { if (!powerOverlay.restoreLateFocus()) root.restoreFocus(); })
            onHomeRequested: { root.closeOverlays(); Qt.callLater(home.showHome); }
            onAmbientRequested: root.startAmbient()
        }
        Power.AmbientScreen {
            id: ambientScreen
            anchors.fill: parent
            z: 30
            focus: root.focusedScreen === "ambient"
            clockText: home.clockText
            dateText: { home.clockText; return Qt.locale().toString(new Date(), "dddd, d MMMM"); }
            onActiveChanged: if (!active) root.finishAmbient()
        }
        Ui.LaunchOverlay {
            id: launchOverlay
            anchors.fill: parent
            z: 20
            active: browserCatalog.opening
            serviceTitle: browserCatalog.openingTitle
            failure: browserCatalog.openingError
            onCancelRequested: browserCatalog.cancelOpen()
            onRetryRequested: browserCatalog.retryOpen()
            onDismissRequested: browserCatalog.dismissOpenFailure()
            onHomeRequested: Qt.callLater(home.showHome)
        }
        Rectangle {
            objectName: "feedbackStrip"
            x: 420; y: 970; width: 1080; height: 72; radius: 36
            color: "#33363b"
            // Never over the picture of managed playback (#258).
            visible: (feedbackTimer.running || backend.message.length > 0) && !root.playerOpen
            Ui.UiText {
                id: feedback; objectName: "feedback"
                anchors.fill: parent; anchors.margins: 20
                horizontalAlignment: Text.AlignHCenter
                verticalAlignment: Text.AlignVCenter
                role: "caption"; maximumLineCount: 1; elide: Text.ElideRight
                text: backend.message
            }
            Timer { id: feedbackTimer; interval: 3000; onTriggered: feedback.text = Qt.binding(() => backend.message) }
        }
    }
}
