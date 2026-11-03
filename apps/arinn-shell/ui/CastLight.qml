// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick

// Soft light under a focused image (ADR-0136): 84% of the image width, 34 px
// high. Stacked ellipses stand in for a 20 px blur on the software renderer.
// Place it behind the image, overlapping the bottom edge.
Item {
    id: cast
    property bool shown: false
    property real hue: Theme.hearthHue
    readonly property color ink: Theme.castColor(hue)
    opacity: shown ? 1 : 0
    implicitWidth: 200
    implicitHeight: Theme.castHeight
    Behavior on opacity { NumberAnimation { duration: Theme.focusDuration } }
    Repeater {
        model: 6
        Rectangle {
            required property int index
            readonly property real spread: (index + 1) / 6
            anchors.centerIn: parent
            width: cast.width * (0.5 + 0.5 * spread)
            height: cast.height * (0.4 + 0.6 * spread)
            radius: height / 2
            color: Qt.rgba(cast.ink.r, cast.ink.g, cast.ink.b, 0.28 * (1 - index / 6))
        }
    }
}
