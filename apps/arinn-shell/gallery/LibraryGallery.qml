// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import QtQuick.Window
import "../ui" as Ui
import "../ui/screens/library" as Library

Window {
    width: 1920; height: 1080; visible: true
    title: "Arinn — Library preview"
    color: Ui.Theme.background
    // noArt matches the shell: the catalogue discards poster ids and draws a plate.
    Library.LibraryView {
        anchors.fill: parent
        libraryState: "ready"
        tabs: [
            { id: "films", title: qsTr("Films") },
            { id: "series", title: qsTr("Series") }
        ]
        entries: [
            { id: "north", tabId: "films", title: "Northern light", year: "2019", kind: "film", noArt: true },
            { id: "quiet", tabId: "films", title: "Quiet places", year: "2020", kind: "film", noArt: true },
            { id: "paths", tabId: "films", title: "Mountain paths", year: "2018", kind: "film", noArt: true },
            { id: "skies", tabId: "films", title: "Open skies", year: "2022", kind: "film", noArt: true },
            { id: "far", tabId: "films", title: "Far away", year: "2016", kind: "film", noArt: true },
            { id: "water", tabId: "films", title: "Still water", year: "2021", kind: "film", noArt: true },
            { id: "late", tabId: "films", title: "Late light", year: "2017", kind: "film", noArt: true },
            { id: "low", tabId: "films", title: "Low country", year: "2015", kind: "film", noArt: true },
            { id: "serial", tabId: "series", title: "The long way home", year: "2021", kind: "series", noArt: true }
        ]
    }
}
