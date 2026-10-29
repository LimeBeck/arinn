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
    // Keep the native alpha format for every Home/playback cycle. Qt's
    // opaque window clear color revokes that request even with a default
    // alpha buffer; opaque screens belong to the scene instead.
    color: "transparent"
    Rectangle {
        objectName: "opaqueShellBackground"
        anchors.fill: parent
        z: -1
        color: Ui.Theme.background
        visible: !((root.playerOpen && !root.leavingStopPending
                    && root.leaveError === "") || root.activityBehind)
    }
    title: "Arinn"
    flags: Qt.Window | Qt.FramelessWindowHint
    readonly property bool navOpen: home.navOpen
    // The quick menu (spec §5.7, ADR-0106). Over an Activity the shell shows
    // no screen: `activityBehind` stays set while Back hands the screen back
    // to that Activity, so Home never flashes in between.
    property bool quickMenuOpen: false
    property bool activityBehind: false
    property var quickMenuOpener: null
    // The Return target the request carried. Back over an Activity uses it
    // until Status reports the Home state the request committed (#507): over
    // managed playback the window is already on screen, and Back may come
    // before that refresh.
    property string quickMenuTarget: ""
    function openQuickMenu() {
        if (sessionGuard.blocked) return;
        // During ambient the key only wakes, and that first event triggers
        // nothing, as Menu pressed inside the shell does (§5.7 Ambient).
        if (ambientScreen.active) {
            if (ambientScreen.armed) ambientScreen.wake();
            return;
        }
        if (!quickMenuOpen) {
            // The sheet is over the shell only when the shell was on screen
            // with focus when the key was pressed. Over an Activity the
            // platform activates the shell only after this request, so a
            // shell that a Status read has just shown is not active yet.
            // Managed playback is an Activity too.
            activityBehind = playerOpen || !(visible && active);
            quickMenuOpener = activityBehind ? null : activeFocusItem;
            quickMenuTarget = "";
            quickMenuOpen = true;
            // The sheet takes the place of the "Opening…" and "Starting…"
            // progress, as Home does (§6.5): the launch keeps running, and
            // only its dialog (failure or timeout) is drawn above the sheet
            // (§6.1). A stop confirmation closes and the launch goes on.
            if (launchOverlay.active && !launchOverlay.showDialog) launchOverlay.hideForHome();
            if (applicationLaunch.active && !applicationLaunch.showDialog) applicationLaunch.hideForHome();
            // The Sound row shows the platform's current level.
            if (backend.connected) systemControl.readVolume();
        }
        returnBound.stop();
        // A repeated key keeps the sheet; one under the power scope keeps
        // that scope on top.
        Qt.callLater(() => { if (focusedScreen === "quickMenu") quickMenu.start(); });
    }
    // Back: over an Activity, activate that same Activity (§6.3); over the
    // shell, return focus to the opener.
    function quickMenuBack() {
        if (!quickMenuOpen) return;
        quickMenuOpen = false;
        const target = backend.returnWindow || quickMenuTarget;
        quickMenuTarget = "";
        if (activityBehind && backend.connected && target) {
            returnBound.restart();
            returnToActivity();
            return;
        }
        if (activityBehind) { quickMenuHome(); return; }
        const opener = quickMenuOpener;
        quickMenuOpener = null;
        Qt.callLater(() => { if (!opener || !Ui.Focus.take(opener)) restoreFocus(); });
    }
    // Open Home, the Home key, or a Return that did not happen: the sheet
    // gives way to the fullscreen Home, focus on Return.
    function quickMenuHome() {
        returnBound.stop();
        quickMenuOpen = false;
        activityBehind = false;
        quickMenuOpener = null;
        quickMenuTarget = "";
        closeOverlays();
        Qt.callLater(home.showHome);
    }
    // Sleep and Power open the shell's power scope over the sheet (§5.7,
    // §6.1, ADR-0108), with its confirmations, in-flight rules and refusals.
    // The Activity stays behind; Back closes the power scope and focus
    // returns to the sheet's row (§6.3).
    function quickMenuPower(sleep) {
        if (!quickMenuOpen) return;
        powerOverlay.openMenu();
        // A request already in flight is shown as it is; SystemControl sends
        // one request at a time.
        if (sleep) systemControl.request("suspend");
    }
    // The remote's power key (spec §5.7 *Power button*, ADR-0111). Over an
    // Activity the platform shows the shell as for Home, and the power scope
    // opens over it; Back and Cancel then leave Home on screen (§6.3). A
    // quick menu over an Activity gives way to Home as for the Home key; one
    // over the shell keeps the sheet under the scope, as its Power row does.
    // Ambient ends without a consumed wake. A repeated key keeps the scope as
    // it is. Nothing happens while the session is locked or unknown.
    function openPowerFromKey() {
        if (sessionGuard.blocked) return;
        ambientIdle.activity();
        if (ambientScreen.active) ambientScreen.stop();
        // No toggle and no change underneath: an open scope (also one the
        // sheet's Power row opened over an Activity) stays exactly as it is.
        // openMenu() alone would keep the scope but let the Activity case
        // below replace the sheet with Home.
        if (powerOverlay.open) return;
        // As for the quick menu: the scope takes the place of the "Opening…"
        // and "Starting…" progress, as Home does (§6.5); the launch goes on,
        // a stop confirmation closes, and only a launch's dialog (failure or
        // timeout) is drawn above the scope (§6.1).
        if (launchOverlay.active && !launchOverlay.showDialog) launchOverlay.hideForHome();
        if (applicationLaunch.active && !applicationLaunch.showDialog) applicationLaunch.hideForHome();
        if (quickMenuOpen && !activityBehind) {
            quickMenuPower(false);
            return;
        }
        if (quickMenuOpen || activityBehind) {
            quickMenuHome();
            // After Home has taken focus, so the scope keeps it.
            Qt.callLater(powerOverlay.openMenu);
            return;
        }
        powerOverlay.openMenu();
    }
    // A Return the platform does not carry out leaves Home on screen.
    Timer { id: returnBound; interval: 3000; onTriggered: if (root.visible) root.quickMenuHome() }
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
    property bool sessionFromLaunch: false
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
    property int presentationActivationSerial: 0
    property int presentationHandoffSerial: 0
    property bool presentationReturning: false
    property bool presentationFocusConfirmed: false
    property bool presentationWaitingForHomeExit: false
    property double presentationNativeRevision: Infinity
    function requestPresentationFocus() {
        presentationNativeRevision = Infinity;
        if (backend.nativeFocusSupported === true)
            presentationNativeRevision = backend.requestNativeFocus();
    }
    function nativePresentationFocus() {
        return backend.nativeFocusSupported === true
                && backend.nativeFocusRevision >= presentationNativeRevision
                ? backend.nativeShellFocus : "unknown";
    }
    // The compositor's acknowledgement can precede Qt's activation event.
    // Wait for that event without requesting activation or retrying Present.
    Timer {
        id: presentationFocusBound
        objectName: "presentationFocusBound"
        interval: 1000
        onTriggered: {
            const returning = root.presentationReturning;
            root.presentationReturning = false;
            if (mediaCatalog.presentationState !== "active" || root.leavingStopPending
                    || sessionGuard.blocked) return;
            if (root.nativePresentationFocus() === "shell"
                    || (root.nativePresentationFocus() === "unknown" && root.active))
                root.presentationFocusConfirmed = true;
            else if (!root.presentationAtHome || returning)
                mediaCatalog.abandonPresentation("presentation_focus_timeout");
        }
    }
    function checkPresentationFocus() {
        if (mediaCatalog.presentationState !== "active"
                || leavingStopPending || sessionGuard.blocked) {
            presentationFocusBound.stop();
            return;
        }
        const nativeFocus = nativePresentationFocus();
        // Status contains a platform cache: a post-intent request can still
        // precede ReportWindow. Preserve the handoff grace until confirmed.
        if (nativeFocus === "other" && presentationFocusConfirmed) {
            mediaCatalog.abandonPresentation("focus_lost");
            return;
        }
        if (presentationAtHome && !presentationReturning) {
            presentationFocusBound.stop();
            return;
        }
        if (nativeFocus === "shell" || (nativeFocus === "unknown" && active
                && (presentationFocusConfirmed
                    || presentationActivationSerial > presentationHandoffSerial))) {
            presentationFocusConfirmed = true;
            if (!presentationAtHome) {
                presentationReturning = false;
                presentationFocusBound.stop();
                return;
            }
        }
        if (!presentationFocusBound.running)
            presentationFocusBound.start();
    }
    function returnToActivity() {
        if (mediaCatalog.inputLayerActive && mediaCatalog.presentationState === "active") {
            // Record the intent before the platform can change Qt activation.
            presentationReturning = true;
            presentationNativeRevision = Infinity;
            presentationFocusConfirmed = false;
            presentationHandoffSerial = presentationActivationSerial;
            presentationFocusBound.restart();
        }
        backend.back();
    }
    function presentPlayer() {
        presentationReturning = false;
        presentationRequested = true;
        presentationFocusConfirmed = false;
        presentationHandoffSerial = presentationActivationSerial;
        presentationWaitingForHomeExit = true;
        presentationFocusBound.stop();
        mediaCatalog.present();
        checkPresentationFocus();
    }
    function showPlaybackHome() {
        if (leavingStopPending)
            return;
        leaveError = "";
        if (!mediaCatalog.inputLayerActive)
            return;
        presentationReturning = false;
        presentationFocusBound.stop();
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
        if (active) {
            ++presentationActivationSerial;
            checkPresentationFocus();
        } else if (mediaCatalog.presentationState === "active") {
            Qt.callLater(() => {
                if (!root.active && mediaCatalog.presentationState === "active") {
                    if (backend.nativeFocusSupported === true) {
                        root.requestPresentationFocus();
                        root.checkPresentationFocus();
                    } else if (root.presentationFocusConfirmed || (root.presentationAtHome && !root.presentationReturning)) {
                        mediaCatalog.abandonPresentation("focus_lost");
                    } else {
                        root.checkPresentationFocus();
                    }
                }
            });
        }
    }
    function openSession(entryId, fromLibrary) {
        openMediaSession(entryId, fromLibrary, fromLibrary);
    }
    function openMediaSession(entryId, fromLibrary, launched) {
        if (leavingStopPending)
            return;
        leaveError = "";
        presentationReturning = false;
        presentationFocusBound.stop();
        presentationFocusConfirmed = false;
        presentationWaitingForHomeExit = false;
        presentationAtHome = false;
        presentationRequested = false;
        sessionFromLibrary = fromLibrary;
        sessionFromLaunch = launched;
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
        if (sessionFromLaunch && mediaCatalog.sessionState !== "ready")
            player = true;
        playerOpen = player;
        if (player && mediaCatalog.windowAvailable && !presentationRequested) {
            presentPlayer();
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
        presentationReturning = false;
        presentationFocusBound.stop();
        leaveError = "";
        if (mediaCatalog.inputLayerActive) {
            leavingPlayback = true;
            leavingBound.restart();
        }
        if (mediaCatalog.sessionState !== "ready") {
            if (sessionFromLibrary || sessionFromLaunch || mediaCatalog.sessionState === "loading") {
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
        sessionFromLaunch = false;
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
    // scope traps focus above everything it covers (§5.7, §6.1), the quick
    // menu's Power entry included.
    readonly property string focusedScreen: ambientScreen.active ? "ambient"
        : powerOverlay.open ? "power"
        : quickMenuOpen ? "quickMenu"
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
                 power: powerOverlay, ambient: ambientScreen, quickMenu: quickMenu})[name];
    }
    // Focus the screen on display at its preferred item.
    function restoreFocus() {
        switch (focusedScreen) {
        case "ambient": Ui.Focus.take(ambientScreen); break;
        case "power": powerOverlay.menu.restoreMenuFocus(); break;
        case "quickMenu": quickMenu.restoreFocus(); break;
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
    onFocusedScreenChanged: {
        if (mediaCatalog && (sessionFromLibrary || playerOpen || mediaCatalog.inputLayerActive))
            logMediaSurface(focusedScreen, "screen_route_changed");
        Qt.callLater(() => {
            if (visible && (!focusInside(activeFocusItem, screenItem(focusedScreen))
                            || !activeFocusItem.visible)) restoreFocus();
        });
    }
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
    onVisibleChanged: if (!visible) {
        // Hidden by Return, lock or dismissal: the quick menu does not survive.
        returnBound.stop();
        quickMenuOpen = false;
        activityBehind = false;
        quickMenuOpener = null;
        quickMenuTarget = "";
    } else {
        Qt.callLater(() => {
            // Returning from an external activity keeps the displayed route. Home
            // must not take focus behind Open services or another retained screen.
            if (focusedScreen === "home") home.showHome();
            else restoreFocus();
        });
    }
    Connections {
        target: backend
        ignoreUnknownSignals: true
        function onNativeFocusChanged() { root.checkPresentationFocus(); }
        function onChanged() {
            // The Return of the quick menu is done: the Activity has the
            // screen again, including a managed player (ADR-0106).
            // Status has caught up with the quick-menu request (#507).
            if (backend.homeVisible) root.quickMenuTarget = "";
            if (!backend.homeVisible && !root.quickMenuOpen && root.activityBehind) {
                returnBound.stop();
                root.activityBehind = false;
            }
            if (backend.homeVisible) root.leavingPlayback = false;
            if (!mediaCatalog.inputLayerActive) return;
            if (!backend.homeVisible) root.presentationWaitingForHomeExit = false;
            if (backend.homeVisible) {
                // Until Status has observed Present's Home exit, an old Home
                // snapshot is not a new Home intent. Explicit Home still wins.
                if (!root.presentationReturning && (!root.presentationWaitingForHomeExit || root.presentationAtHome))
                    root.showPlaybackHome();
            } else if (root.presentationAtHome) {
                if (!root.presentationReturning) {
                    // A platform-originated Return has no earlier shell intent.
                    root.presentationFocusConfirmed = false;
                    root.presentationHandoffSerial = root.presentationActivationSerial;
                    presentationFocusBound.restart();
                }
                root.presentationAtHome = false;
                root.requestPresentationFocus();
                root.playerOpen = true;
                Qt.callLater(() => Ui.Focus.take(playerView));
            }
            root.checkPresentationFocus();
        }
    }
    // Explicit platform requests (ADR-0069, ADR-0106, ADR-0111).
    Connections {
        target: backend
        ignoreUnknownSignals: true
        function onQuickMenuRequested(target) {
            root.openQuickMenu();
            if (root.quickMenuOpen && !backend.homeVisible && !root.quickMenuTarget)
                root.quickMenuTarget = target || "";
        }
        function onPowerMenuRequested() { root.openPowerFromKey(); }
        // The Home key replaces the quick menu with Home (§6.3). With the
        // power scope open over the sheet it leaves as Home inside that scope
        // does: a request in flight goes on and a late refusal comes back.
        function onHomeRequested() {
            if (!root.quickMenuOpen && !root.activityBehind) return;
            if (powerOverlay.open) powerOverlay.leaveHome();
            else root.quickMenuHome();
        }
    }
    Connections {
        target: sessionGuard
        ignoreUnknownSignals: true
        function onChanged() {
            if (sessionGuard.blocked) {
                mediaCatalog.abandonPresentation("session_blocked");
                powerOverlay.sessionBlocked();
                // A lock is not a wake: no event to consume, the screen
                // beneath comes back when the shell is shown again.
                ambientScreen.stop();
                // Nothing is typed into an application while locked (ADR-0105).
                externalKeyboard.close();
            }
        }
    }
    // The on-screen keyboard over an external application (spec §6.2,
    // ADR-0105): a separate window that never takes focus. It shows only
    // while the shell is off screen: Home, the quick menu's sheet, the power
    // scope, ambient, both launch overlays (an application's included, also
    // "Starting…" and "Stopping…", ADR-0110) and the player all live in the
    // shell window, so the shell coming on screen for any of them, or a
    // lock, hides the panel at once and closes the session. The platform
    // also ends the session as focus leaves the application.
    ExternalKeyboard { id: externalKeyboard }
    readonly property bool shellOnScreen: backend.homeVisible || root.visible
    Ui.KeyboardPanel {
        id: keyboardPanel
        objectName: "keyboardPanelWindow"
        transientParent: null
        active: externalKeyboard.open && !root.shellOnScreen && !sessionGuard.blocked
        prepare: window => externalKeyboard.preparePanel(window)
        onTyped: text => externalKeyboard.type(text)
        onEdited: key => externalKeyboard.press(key)
        onHideRequested: externalKeyboard.close()
    }
    Connections {
        target: externalKeyboard
        function onInput(key) { keyboardPanel.press(key); }
        // A session that opens while the shell is on screen has no panel to
        // show: end it rather than leave the key's second press to close it.
        function onOpenChanged() {
            if (externalKeyboard.open && (root.shellOnScreen || sessionGuard.blocked))
                externalKeyboard.close();
        }
    }
    onShellOnScreenChanged: if (shellOnScreen) externalKeyboard.close()
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
        // An Activity behind the quick menu is in the foreground (§5.7).
        shellShown: root.visible && root.active && !root.activityBehind
        locked: sessionGuard.blocked
        ambientActive: ambientScreen.active
        // An application launch only while its overlay is on screen: it may
        // keep running hidden for hours (§6.5).
        launching: browserCatalog.opening || applicationLaunch.visible
        // Never over a power request in flight: its answer would take focus
        // behind ambient (#487).
        powerPending: systemControl.pendingAction.length > 0
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
        objectName: "shellScreens"
        // Over an Activity the quick menu shows none of the screens.
        visible: !root.activityBehind
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
            onReturnRequested: root.returnToActivity()
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
            onServiceRequested: root.returnToActivity()
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
            // `~` in a path names this folder (#479); unset in test hosts.
            homeFolder: typeof shellHomeFolder === "string" ? shellHomeFolder : ""
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
            value: root.visible && root.focusedScreen === "player"
                && playerView.visible && playerView.osdVisible && playerView.transportReady
                && playerView.timelineFocused && mediaCatalog.timelineAvailable
        }
        Connections {
            target: mediaCatalog
            function onPresentationHomeRequested() { root.showPlaybackHome(); }
            function onOperationChanged() { root.finishStopOperation(); }
            function onPresentationChanged() {
                if (mediaCatalog.presentationState !== "active") {
                    root.presentationReturning = false;
                    presentationFocusBound.stop();
                    root.presentationFocusConfirmed = false;
                    root.presentationNativeRevision = Infinity;
                    if (backend.nativeFocusSupported === true)
                        backend.stopNativeFocusObservation();
                }
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
                if (mediaCatalog.presentationState === "active") {
                    root.requestPresentationFocus();
                    root.checkPresentationFocus();
                }
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
                    root.presentPlayer();
                }
                if ((!root.sessionFromLibrary && !root.sessionFromLaunch)
                        || (!root.playerOpen && !root.sessionOpen))
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
                root.openMediaSession(presentationId, fromLibrary, true);
            }
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
    // The quick menu sheet above every screen (ADR-0106).
    Ui.LogicalCanvas {
        anchors.fill: parent
        Ui.QuickMenu {
            id: quickMenu
            anchors.fill: parent
            z: 10
            visible: root.quickMenuOpen
            enabled: visible
            focus: root.focusedScreen === "quickMenu"
            overActivity: root.activityBehind
            connected: backend.connected === true
            nowPlaying: {
                const media = mediaCatalog.continueEntries.filter(entry => entry.state === "playing" || entry.state === "paused");
                return media.find(entry => entry.state === "playing") || media[0] || null;
            }
            audio: systemStatus.audio
            volumeControl: backend.connected === true
            volumeKnown: systemControl.volumeKnown
            volumeMuted: systemControl.volumeMuted
            volumeLevel: systemControl.volumeLevel
            network: systemStatus.network
            wifiLevel: systemStatus.wifiLevel
            clockText: home.clockText
            onPlayPauseRequested: sessionId => mediaCatalog.sessionAction(sessionId, "playPause")
            // The volume keys' semantics: Right turns the sound back on, then
            // raises it (owner decision of 2026-10-21 on #477).
            onMuteToggleRequested: systemControl.toggleMuted()
            onVolumeStepRequested: step => { if (step > 0) systemControl.raiseVolume(); else systemControl.stepVolume(-1); }
            onSleepRequested: root.quickMenuPower(true)
            onPowerMenuRequested: root.quickMenuPower(false)
            onHomeRequested: root.quickMenuHome()
            onCloseShellRequested: root.close()
            onBackRequested: root.quickMenuBack()
        }
    }
    // The system scopes above the screens and the sheet, also over an
    // Activity: power, the launch dialog, then ambient (§6.1).
    Ui.LogicalCanvas {
        anchors.fill: parent
        // Keys they leave unaccepted bubble as from the screens.
        Keys.onPressed: event => root.unhandledKey(event)
        // The power scope above every screen and the quick menu, and the
        // volume HUD (§5.7, §6.1: dialog › power › sheet).
        Power.PowerOverlay {
            id: powerOverlay
            anchors.fill: parent
            z: 7
            hudLayer: volumeHudLayer
            focus: root.focusedScreen === "power"
            control: systemControl
            runningApplications: backend.items
            registryKnown: backend.applicationsState === "ready" || backend.applicationsState === "empty"
            keysEnabled: root.visible && root.active && !sessionGuard.blocked && !ambientScreen.active
            // A late refusal never takes focus behind a launch overlay or
            // the ambient screen; it appears once they are gone. A launch
            // still in its reveal delay (active, not hidden for Home) holds
            // it too: its overlay is about to be drawn above (#534).
            holdLateAnswer: launchOverlay.visible || applicationLaunch.visible || ambientScreen.active
                || (launchOverlay.active && !launchOverlay.homeHidden)
                || (applicationLaunch.active && !applicationLaunch.homeHidden)
            onClosed: Qt.callLater(() => { if (!powerOverlay.restoreLateFocus()) root.restoreFocus(); })
            onHomeRequested: {
                if (root.quickMenuOpen || root.activityBehind) {
                    root.quickMenuHome();
                    return;
                }
                root.closeOverlays();
                Qt.callLater(home.showHome);
            }
            onAmbientRequested: root.startAmbient()
        }
        // The launch dialog is drawn above the power scope (ui-power.md).
        Ui.LaunchOverlay {
            id: launchOverlay
            objectName: "launchOverlay"
            anchors.fill: parent
            z: 20
            active: browserCatalog.opening
            serviceTitle: browserCatalog.openingTitle
            failure: browserCatalog.openingError
            onCancelRequested: browserCatalog.cancelOpen()
            onRetryRequested: browserCatalog.retryOpen()
            onDismissRequested: browserCatalog.dismissOpenFailure()
            // Home replaces a quick menu under the dialog too (§6.3).
            onHomeRequested: {
                if (root.quickMenuOpen || root.activityBehind) root.quickMenuHome();
                else Qt.callLater(home.showHome);
            }
        }
        // An application launch (spec §6.5, ADR-0110): it ends when a window
        // appears, so it has no timeout here; after 8 s it reads
        // "Starting <app>…" with a confirmed Cancel. Drawn like the launch
        // dialog above, and its dialog stays above the sheet too.
        Ui.LaunchOverlay {
            id: applicationLaunch
            objectName: "applicationLaunch"
            anchors.fill: parent
            z: 20
            kind: "application"
            timeoutDelay: 0
            active: backend.launchActive === true
            serviceTitle: backend.launchTitle || ""
            failure: backend.launchError || ""
            cancellable: backend.launchCancellable === true
            stopping: backend.launchStopping === true
            onCancelRequested: backend.cancelLaunch()
            onRetryRequested: backend.retryLaunch()
            onDismissRequested: backend.dismissLaunchFailure()
            onHomeRequested: {
                if (root.quickMenuOpen || root.activityBehind) root.quickMenuHome();
                else Qt.callLater(home.showHome);
            }
            // What the backend needs to put this screen back after another
            // launch's failure dialog, and to hold that dialog while the
            // stop confirmation is open (§6.1, §6.5).
            onHomeHiddenChanged: if (typeof backend.setLaunchHidden === "function") backend.setLaunchHidden(homeHidden)
            onConfirmingChanged: if (typeof backend.setLaunchConfirming === "function") backend.setLaunchConfirming(confirming)
            Connections {
                target: backend
                // Test doubles of the backend may lack the signal.
                ignoreUnknownSignals: true
                function onLaunchShown() { applicationLaunch.showAgain(backend.launchElapsed()); }
                // A launch that replaces another on screen starts over.
                function onLaunchStarted() { applicationLaunch.arm(); }
                // The launch that takes the screen after a failure or after
                // the launch on screen ended had been hidden: keep it hidden.
                function onLaunchHidden() { applicationLaunch.hideForHome(); }
            }
        }
        // The volume HUD of the power overlay, drawn above the launch
        // overlays so a volume change or its failure stays visible during a
        // launch (#487). It never takes focus or input; ambient covers it.
        Item {
            id: volumeHudLayer
            objectName: "volumeHudLayer"
            anchors.fill: parent
            z: 25
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
    }
}
