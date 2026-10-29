// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import QtQuick.Window

// The on-screen keyboard over an external application (spec §6.2, §12.5,
// ADR-0105): the one keyboard component in external mode, in its own
// window along the bottom of the screen. The window never takes focus
// (platform setup through `prepare`); the platform routes the remote's
// panel keys to press(), and typed keys leave through `typed` and `edited`.
// The suggestion row stays empty: nothing typed in another application is
// remembered.
Window {
    id: window

    // Whether the daemon's session is open and the panel may show.
    property bool active: false
    // Called once with this window before it is first shown.
    property var prepare: null

    signal typed(string text)
    signal edited(string key)
    signal hideRequested()

    function press(key) {
        if (visible)
            keyboard.press(key, false);
    }

    readonly property real canvasScale: Math.min(Screen.width / Theme.canvasWidth,
                                                 Screen.height / Theme.canvasHeight)
    width: Screen.width
    height: Math.ceil(keyboard.implicitHeight * canvasScale)
    x: Screen.virtualX
    y: Screen.virtualY + Screen.height - height
    color: "transparent"
    visible: active

    Component.onCompleted: if (prepare) prepare(window)
    onVisibleChanged: {
        if (visible)
            keyboard.start();
        else
            keyboard.finish();
    }

    OnScreenKeyboard {
        id: keyboard
        objectName: "externalOnScreenKeyboard"
        external: true
        width: Theme.canvasWidth
        x: (window.width / window.canvasScale - width) / 2 * window.canvasScale
        scale: window.canvasScale
        transformOrigin: Item.TopLeft
        memoryKey: "external"
        submitLabel: qsTr("Enter")
        suggestions: []
        emptySuggestions: ""
        onTyped: text => window.typed(text)
        onEdited: key => window.edited(key)
        onHideRequested: window.hideRequested()
    }
}
