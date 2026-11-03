// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick

// The focus outline Home cards draw above their artwork (Surface). Settings
// rows use the same item, so the ring stays Theme.ring and no screen
// hardcodes a focus colour.
Rectangle {
    id: ring
    objectName: "focusRing"
    property bool shown: false
    anchors.fill: parent
    z: 100
    color: "transparent"
    border.width: 3
    border.color: Theme.ring
    opacity: shown ? 1 : 0
    Behavior on opacity { NumberAnimation { duration: Theme.focusDuration } }
}
