// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import QtQuick.Window
import Arinn.Shell 1.0
import "ui" as Ui
import "ui/screens/browser" as Browser
import "ui/screens/library" as Library
import "ui/screens/player" as Player
import "ui/screens/session" as Session
import "ui/screens/settings" as Settings

Window {
    id: root
    width: 1920; height: 1080
    visibility: backend.homeVisible && !sessionGuard.blocked ? Window.FullScreen : Window.Hidden
    color: playerOpen ? "transparent" : Ui.Theme.background
    title: "Arinn"
    flags: Qt.Window | Qt.FramelessWindowHint
    readonly property bool navOpen: home.navOpen
    property bool browserOpen: false
    property bool sessionOpen: false
    property bool playerOpen: false
    property bool sessionFromLibrary: false
    function openSession(entryId, fromLibrary) {
        sessionFromLibrary = fromLibrary;
        mediaCatalog.select(entryId);
        applyMediaSurface();
    }
    function applyMediaSurface() {
        // Rich flags select the OSD. A library launch whose session is not
        // in the snapshot yet is managed playback, so the OSD is shown until
        // the row arrives. A row without those flags uses the session screen.
        let player = mediaCatalog.playerSession;
        if (sessionFromLibrary && mediaCatalog.sessionState !== "ready")
            player = true;
        playerOpen = player;
        sessionOpen = !player;
        Qt.callLater(() => {
            if (player)
                playerView.forceActiveFocus();
            else
                sessionView.forceActiveFocus();
        });
    }
    function closePlayer() {
        playerOpen = false;
        if (sessionFromLibrary) {
            sessionFromLibrary = false;
            Qt.callLater(() => {
                if (libraryView.detailOpen)
                    libraryView.focusDetailAction();
                else
                    libraryView.forceActiveFocus();
            });
        } else {
            Qt.callLater(home.showHome);
        }
    }
    onVisibleChanged: if (visible) Qt.callLater(home.showHome)
    BrowserCatalog { id: browserCatalog }
    MediaCatalog { id: mediaCatalog; objectName: "mediaCatalog" }
    LibraryCatalog { id: libraryCatalog; objectName: "libraryCatalog" }
    SettingsCatalog { id: settingsCatalog; objectName: "settingsCatalog" }
    KeyboardMonitor { id: keyboardMonitor }
    Ui.LogicalCanvas {
        anchors.fill: parent
        Ui.HomeView {
            id: home
            objectName: "productionHome"
            visible: !root.playerOpen
            busyEntryIds: backend.busyEntryIds
            continueEntries: backend.continueEntries.concat(mediaCatalog.continueEntries)
            favoriteEntries: backend.favorites
            applicationEntries: backend.applications
            runningEntries: backend.items
            favoritesTitle: qsTr("Favorites")
            returnActivityId: backend.returnWindow
            openServiceCount: browserCatalog.tabs.length
            currentServiceId: browserCatalog.currentService.id || ""
            currentServiceTitle: browserCatalog.currentService.title || ""
            currentServiceSubtitle: browserCatalog.currentService.subtitle || ""
            browserUnavailable: browserCatalog.browserState === "unavailable"
            openShortcutIds: browserCatalog.openShortcutIds
            homeState: backend.homeState
            statusTitle: homeState === "unavailable" ? qsTr("Home is unavailable") : homeState === "loading" ? qsTr("Loading Home…") : homeState === "error" ? qsTr("Home could not be loaded") : qsTr("Your Home starts here")
            statusDescription: homeState === "empty" ? qsTr("No favorites are available yet.") : qsTr("Navigation remains available. Home reconnects automatically.")
            sectionDescription: screen === "applications" ? ((applicationEntries.length || runningEntries.length) ? qsTr("Choose an application or an open window.") : qsTr("No applications are available.")) : qsTr("This section is not available yet.")
            clockText: Qt.formatTime(new Date(), "hh:mm")
            Timer { interval: 1000; running: true; repeat: true; onTriggered: home.clockText = Qt.formatTime(new Date(), "hh:mm") }
            onEntryRequested: entryId => backend.launch(entryId)
            onRunningRequested: entryId => backend.activate(entryId)
            onReturnRequested: backend.back()
            onRetryRequested: backend.refresh()
            onOpenServicesRequested: {
                root.browserOpen = true;
                Qt.callLater(() => browserView.forceActiveFocus());
            }
            onServiceRequested: serviceId => {
                if (serviceId.length)
                    browserCatalog.activateTab(serviceId);
                backend.back();
            }
            onSessionRequested: entryId => root.openSession(entryId, false)
            onPowerRequested: { feedback.text = qsTr("Power controls are not available yet."); feedbackTimer.restart(); }
        }
        Browser.BrowserView {
            id: browserView
            objectName: "productionBrowser"
            visible: root.browserOpen && !root.playerOpen
            enabled: root.browserOpen && !root.playerOpen
            interactive: root.browserOpen && !root.playerOpen
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
            onBackRequested: { root.browserOpen = false; Qt.callLater(home.showHome); }
        }
        Session.SessionView {
            id: sessionView
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
                            libraryView.forceActiveFocus();
                    });
                } else {
                    Qt.callLater(home.showHome);
                }
            }
        }
        Settings.SettingsView {
            id: settingsView
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
            onRetryRequested: settingsCatalog.refresh()
        }
        Library.LibraryView {
            id: libraryView
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
        Player.PlayerView {
            id: playerView
            objectName: "productionPlayer"
            focus: root.playerOpen
            visible: root.playerOpen
            enabled: root.playerOpen
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
            playbackState: {
                const state = mediaCatalog.sessionState;
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
            onStop: mediaCatalog.stop()
            onLeave: {
                if (mediaCatalog.sessionState === "ready")
                    mediaCatalog.stop();
                root.closePlayer();
            }
            onHomeRequested: {
                root.playerOpen = false;
                root.sessionFromLibrary = false;
                home.showHome();
            }
        }
        Binding {
            target: mediaCatalog
            property: "thumbnailWanted"
            value: root.playerOpen && playerView.timelineFocused && mediaCatalog.timelineAvailable
        }
        Connections {
            target: mediaCatalog
            function onSessionChanged() {
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
            function onSessionLaunched(presentationId) {
                root.openSession(presentationId, true);
            }
        }
        Ui.LaunchOverlay {
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
            x: 420; y: 970; width: 1080; height: 72; radius: 36
            color: "#33363b"
            visible: feedbackTimer.running || backend.message.length > 0
            Ui.UiText {
                id: feedback
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
