// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick

// The idle timer of spec §5.7 (#307). `idle()` fires after `timeout` without
// input while automatic ambient is allowed: the shell is on screen and
// active, the session is not locked, ambient is not already on, nothing is
// launching, and no media plays. It only starts ambient; it never requests
// OS suspend, pauses playback or closes applications.
QtObject {
    id: root
    // Ten minutes by default (spec §5.7).
    property int timeout: 600000
    property bool shellShown: false
    property bool locked: false
    property bool ambientActive: false
    property bool launching: false
    property bool mediaPlaying: false
    readonly property bool eligible: shellShown && !locked && !ambientActive && !launching && !mediaPlaying
    readonly property bool counting: timer.running

    signal idle()

    // Any key or genuine pointer movement in the shell starts the count again.
    function activity() {
        if (eligible)
            timer.restart();
    }
    // Media plays when a media session is playing, the managed player's input
    // layer is up, or a browser tab makes sound.
    function playing(mediaEntries, tabs, inputLayer) {
        if (inputLayer)
            return true;
        for (const entry of mediaEntries || [])
            if (entry && entry.running === true)
                return true;
        for (const tab of tabs || [])
            if (tab && tab.audible === true)
                return true;
        return false;
    }

    property Timer timer: Timer {
        interval: root.timeout
        onTriggered: if (root.eligible) root.idle()
    }
    // Becoming eligible starts a full timeout; losing it forgets the count.
    onEligibleChanged: eligible ? timer.restart() : timer.stop()
    onTimeoutChanged: if (timer.running) timer.restart()
    Component.onCompleted: if (eligible) timer.restart()
}
