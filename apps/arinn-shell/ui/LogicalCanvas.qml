// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick

Item {
    id: root
    default property alias content: stage.data
    readonly property real canvasScale: Math.min(width / Theme.canvasWidth, height / Theme.canvasHeight)
    Item {
        id: stage
        width: Theme.canvasWidth
        height: Theme.canvasHeight
        anchors.centerIn: parent
        scale: root.canvasScale
    }
}
