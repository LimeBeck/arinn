// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import QtQuick.Window
import "../ui" as Ui

Window {
    id: window
    width: 1920; height: 1080; visible: true
    title: "Arinn — Home preview"
    color: Ui.Theme.background
    property string fixture: "populated"
    property bool showNavigation: false
    property string lastIntent: ""
    readonly property var content: [
        { id: "activity", title: "Workspace", subtitle: "Return to activity", running: true },
        { id: "film", title: "Northern light", subtitle: "42 minutes remaining", progress: 0.58, artwork: Qt.resolvedUrl("fixture-artwork.svg") },
        { id: "episode", title: "The long way home", subtitle: "Episode 4 · 28 minutes remaining", progress: 0.32, artwork: Qt.resolvedUrl("fixture-artwork.svg") },
        { id: "video", title: "Mountain paths", subtitle: "18 minutes remaining", progress: 0.16, artwork: Qt.resolvedUrl("fixture-artwork.svg") },
        { id: "last", title: "Quiet places", subtitle: "9 minutes remaining", progress: 0.7, artwork: Qt.resolvedUrl("fixture-artwork.svg") }
    ]
    Ui.LogicalCanvas {
        anchors.fill: parent
        Ui.HomeView {
            id: home
            continueEntries: window.fixture === "populated" ? window.content : []
            favoriteEntries: window.fixture === "populated" ? [
                { id: "favorite1", title: "Explore", monogram: "E" },
                { id: "favorite2", title: "Watch", monogram: "W" },
                { id: "favorite3", title: "Listen", monogram: "L" },
                { id: "favorite4", title: "Play", monogram: "P" },
                { id: "favorite5", title: "Create", monogram: "C" },
                { id: "favorite6", title: "Discover", monogram: "D" },
                { id: "favorite7", title: "Relax", monogram: "R" }
            ] : []
            newEntries: window.fixture === "populated" ? [
                { id: "new1", title: "Open skies", subtitle: "2026", artwork: Qt.resolvedUrl("fixture-artwork.svg") },
                { id: "new2", title: "Far away", subtitle: "2026", artwork: Qt.resolvedUrl("fixture-artwork.svg") }
            ] : []
            returnActivityId: window.fixture === "populated" ? "activity" : ""
            homeState: window.fixture === "populated" ? "ready" : window.fixture
            statusTitle: ({empty: "Your Home starts here", loading: "Loading Home…", error: "Home could not be loaded", unavailable: "Home is unavailable"})[homeState] || ""
            statusDescription: ({empty: "Browse sections to find something to open.", loading: "Navigation is available while content loads.", error: "Try again when you are ready.", unavailable: "The service is not connected. You can still browse sections."})[homeState] || ""
            clockText: "20:42"
            onEntryRequested: window.lastIntent = "Open requested · fixture only"
            onReturnRequested: window.lastIntent = "Return requested · fixture only"
            onRetryRequested: window.lastIntent = "Retry requested · fixture only"
            onPowerRequested: window.lastIntent = "Power is not connected in this preview"
            Component.onCompleted: if (window.showNavigation) Qt.callLater(openNavigation)
        }
        Rectangle {
            x: 1000; y: 1000; width: 824; height: 64; radius: 32
            visible: window.lastIntent.length > 0
            color: "#33363b"
            Ui.UiText { anchors.centerIn: parent; text: window.lastIntent; role: "caption" }
        }
    }
}
