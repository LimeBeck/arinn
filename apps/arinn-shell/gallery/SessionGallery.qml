// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import QtQuick.Window
import "../ui" as Ui
import "../ui/screens/session" as Session

Window {
    width: 1920; height: 1080; visible: true
    title: "Arinn — Now playing preview"
    color: Ui.Theme.background
    Session.SessionView {
        anchors.fill: parent
        sessionState: "ready"
        title: "Northern light"
        subtitle: "42 minutes remaining"
        playbackState: "playing"
        // 2:04:00 of 2:46:00, so 42 minutes remain. Microseconds.
        durationUs: 9960000000
        positionUs: 7440000000
        availableActions: ["previous", "playPause", "next", "seek", "stop"]
    }
}
