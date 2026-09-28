// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick

Rectangle {
    id: root
    implicitWidth: 400
    implicitHeight: 225
    radius: Theme.cardRadius
    color: Theme.plate
    Accessible.name: qsTr("Loading")
    SequentialAnimation on opacity {
        running: root.visible
        loops: Animation.Infinite
        NumberAnimation { from: 0.45; to: 1; duration: 700 }
        NumberAnimation { from: 1; to: 0.45; duration: 700 }
    }
}
