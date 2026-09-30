// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import QtQuick.Window
import Arinn.Shell 1.0
import "ui" as Ui
import "ui/screens/browser" as Browser
import "ui/screens/library" as Library
import "ui/screens/session" as Session

Window {
    id: root
    width: 1920; height: 1080
    visibility: backend.homeVisible && !sessionGuard.blocked ? Window.FullScreen : Window.Hidden
    color: Ui.Theme.background
    title: "Arinn"
    flags: Qt.Window | Qt.FramelessWindowHint
    readonly property bool navOpen: home.navOpen
    property bool browserOpen: false
    property bool sessionOpen: false
    property bool sessionFromLibrary: false
    onVisibleChanged: if (visible) Qt.callLater(home.showHome)
    BrowserCatalog { id: browserCatalog }
    MediaCatalog { id: mediaCatalog; objectName: "mediaCatalog" }
    LibraryCatalog { id: libraryCatalog; objectName: "libraryCatalog" }
    Ui.LogicalCanvas {
        anchors.fill: parent
        Ui.HomeView {
            id: home
            objectName: "productionHome"
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
            onSessionRequested: entryId => {
                root.sessionFromLibrary = false;
                mediaCatalog.select(entryId);
                root.sessionOpen = true;
                Qt.callLater(() => sessionView.forceActiveFocus());
            }
            onPowerRequested: { feedback.text = qsTr("Power controls are not available yet."); feedbackTimer.restart(); }
        }
        Browser.BrowserView {
            id: browserView
            objectName: "productionBrowser"
            visible: root.browserOpen
            enabled: root.browserOpen
            interactive: root.browserOpen
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
            visible: root.sessionOpen
            enabled: root.sessionOpen
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
        Library.LibraryView {
            id: libraryView
            objectName: "productionLibrary"
            visible: home.screen === "library"
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
        Connections {
            target: libraryCatalog
            function onSessionLaunched(presentationId) {
                root.sessionFromLibrary = true;
                mediaCatalog.select(presentationId);
                root.sessionOpen = true;
                Qt.callLater(() => sessionView.forceActiveFocus());
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
