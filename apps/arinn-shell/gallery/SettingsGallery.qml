// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import QtQuick.Window
import "../ui" as Ui
import "../ui/screens/settings" as Settings

Window {
    width: 1920; height: 1080; visible: true
    title: "Arinn — Settings preview"
    color: Ui.Theme.background
    // Fixture rows for the controls SettingsView paints. A path is text.
    // There is no folder browser in this screen.
    Settings.SettingsView {
        anchors.fill: parent
        screenState: "ready"
        categories: [
            { id: "media", title: qsTr("Media"), icon: "video_library" },
            { id: "playback", title: qsTr("Playback"), icon: "play_circle" },
            { id: "sound", title: qsTr("Sound"), icon: "volume_up" },
            { id: "network", title: qsTr("Network"), icon: "wifi" },
            { id: "applications", title: qsTr("Applications"), icon: "apps" },
            { id: "appearance", title: qsTr("Appearance"), icon: "palette" },
            { id: "system", title: qsTr("System"), icon: "memory" }
        ]
        rows: [
            {
                providerId: "library", providerName: "Local library", id: "remember",
                categoryId: "media", title: "Remember position",
                description: "Continue from the last position",
                type: "boolean", currentValue: true, constraints: ({}), enabled: true
            },
            {
                providerId: "library", providerName: "Local library", id: "sort",
                categoryId: "media", title: "Sort titles",
                description: "Order of the library grid",
                type: "enum", currentValue: "title",
                constraints: ({ options: [
                    { value: "title", title: "Title" },
                    { value: "year", title: "Year" }
                ] }),
                enabled: true
            },
            {
                providerId: "playback", providerName: "Playback", id: "skip",
                categoryId: "media", title: "Skip step",
                description: "Seconds",
                type: "integer", currentValue: 10,
                constraints: ({ min: 0, max: 60 }),
                enabled: true
            },
            {
                providerId: "library", providerName: "Local library", id: "folder",
                categoryId: "media", title: "Library folder",
                description: "Folder scanned for films and series",
                type: "path", currentValue: "/media/films",
                constraints: ({ directory: true }),
                enabled: true
            },
            {
                providerId: "library", providerName: "Local library", id: "roots",
                categoryId: "media", title: "Extra folders",
                description: "More places to scan",
                type: "path", currentValue: ["/media/shows"],
                constraints: ({ directory: true, multiple: true }),
                enabled: true
            }
        ]
    }
}
