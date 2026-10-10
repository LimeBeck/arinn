// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import QtQuick.Window
import "../ui" as Ui
import "../ui/screens/browse" as Browse

// Site or search with the on-screen keyboard open (ADR-0103, handoff KB-1).
// Fictional text, history and services; no daemon.
Window {
    id: window
    width: 1920; height: 1080; visible: true
    title: "Arinn — On-screen keyboard preview"
    color: Ui.Theme.background

    Browse.BrowseView {
        id: browse
        anchors.fill: parent
        focus: true
        text: "northern"
        kind: "search"
        engine: "Example Search"
        suggestions: [
            { icon: "history", title: "northern lights tonight", value: "northern lights tonight" },
            { icon: "history", title: "northern trails map", value: "northern trails map" },
            { icon: "public", title: "Northern light", subtitle: "north.example", value: "https://north.example", open: true }
        ]
        Component.onCompleted: {
            showKeyboard();
            // A key in the middle of the letters, as a remote would leave it.
            const keys = findKeys(browse);
            keys.model.focusKey(2, 5);
        }
        function findKeys(item) {
            if (item.objectName === "browseOnScreenKeyboard")
                return item;
            for (let i = 0; i < item.children.length; ++i) {
                const found = findKeys(item.children[i]);
                if (found)
                    return found;
            }
            return null;
        }
    }
}
