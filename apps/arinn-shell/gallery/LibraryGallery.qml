// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import QtQuick.Window
import "../ui" as Ui
import "../ui/screens/library" as Library

Window {
    id: window
    width: 1920; height: 1080; visible: true
    title: "Arinn — Library preview"
    color: Ui.Theme.background
    // Posters use the same placeholder the shell paints when a poster URL is
    // set. An entry with no artwork keeps the plate (noArt).
    readonly property url poster: Qt.resolvedUrl("fixture-artwork.svg")
    Library.LibraryView {
        anchors.fill: parent
        libraryState: "ready"
        tabs: [
            { id: "films", title: qsTr("Films") },
            { id: "series", title: qsTr("Series") }
        ]
        entries: [
            { id: "north", tabId: "films", title: "Northern light", year: "2019", kind: "film", artwork: window.poster },
            { id: "quiet", tabId: "films", title: "Quiet places", year: "2020", kind: "film", artwork: window.poster },
            { id: "paths", tabId: "films", title: "Mountain paths", year: "2018", kind: "film", artwork: window.poster },
            { id: "skies", tabId: "films", title: "Open skies", year: "2022", kind: "film", artwork: window.poster },
            { id: "far", tabId: "films", title: "Far away", year: "2016", kind: "film", artwork: window.poster },
            { id: "water", tabId: "films", title: "Still water", year: "2021", kind: "film", artwork: window.poster },
            { id: "late", tabId: "films", title: "Late light", year: "2017", kind: "film", noArt: true },
            { id: "serial", tabId: "series", title: "The long way home", year: "2021", kind: "series", artwork: window.poster }
        ]
    }
}
