// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick

Rectangle {
    id: root
    property real value: 0
    readonly property real fraction: isFinite(value) ? Math.max(0, Math.min(1, value)) : 0
    implicitWidth: 300
    implicitHeight: 6
    radius: height / 2
    color: Qt.rgba(1, 1, 1, 0.25)
    Accessible.role: Accessible.ProgressBar
    Accessible.name: qsTr("Progress %1%").arg(Math.round(fraction * 100))
    Rectangle {
        width: root.width * root.fraction
        height: parent.height
        radius: parent.radius
        color: Theme.ember
    }
}
