// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import QtQuick.Window
import "../ui" as Ui
import "../ui/screens/browser" as Browser

Window {
    id: window
    width: 1920; height: 1080; visible: true
    title: "Arinn — Open services preview"
    color: Ui.Theme.background

    function freezeClock(item) {
        if (!item)
            return false;
        if (item.objectName === "browserClock") {
            item.text = "20:42";
            return true;
        }
        const kids = item.visibleChildren || [];
        for (let i = 0; i < kids.length; ++i)
            if (freezeClock(kids[i]))
                return true;
        return false;
    }

    Browser.BrowserView {
        id: browser
        anchors.fill: parent
        tabs: [
            { id: "north", windowId: "one", title: "Northern light", url: "north", active: true, audible: false },
            { id: "quiet", windowId: "one", title: "Quiet places", url: "quiet", active: false, audible: true },
            { id: "paths", windowId: "one", title: "Mountain paths", url: "paths", active: false, audible: false }
        ]
    }
    Timer {
        interval: 400; running: true
        onTriggered: window.freezeClock(browser)
    }
}
