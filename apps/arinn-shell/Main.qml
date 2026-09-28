// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import QtQuick.Window
import "ui" as Ui

Window {
    id: root
    width: 1920; height: 1080
    visibility: backend.homeVisible && !sessionGuard.blocked ? Window.FullScreen : Window.Hidden
    color: Ui.Theme.background
    title: "Arinn"
    flags: Qt.Window | Qt.FramelessWindowHint
    readonly property bool navOpen: home.navOpen
    onVisibleChanged: if (visible) Qt.callLater(home.showHome)
    Ui.LogicalCanvas {
        anchors.fill: parent
        Ui.HomeView {
            id: home
            objectName: "productionHome"
            busyEntryIds: backend.busyEntryIds
            continueEntries: backend.continueEntries
            favoriteEntries: backend.webShortcuts
            applicationEntries: backend.applications
            runningEntries: backend.items
            favoritesTitle: qsTr("Favorites")
            returnActivityId: backend.returnWindow
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
            onPowerRequested: { feedback.text = qsTr("Power controls are not available yet."); feedbackTimer.restart(); }
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
