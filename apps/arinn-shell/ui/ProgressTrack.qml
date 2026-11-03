// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick

Rectangle {
    id: root
    property real value: 0
    // The Ember line on artwork: 4 px flush with the bottom edge, a 2 px
    // Charcoal band above it, and no grey track. The detail screen keeps
    // the faint track (line: false).
    property bool line: false
    readonly property real fraction: isFinite(value) ? Math.max(0, Math.min(1, value)) : 0
    implicitWidth: 300
    implicitHeight: line ? Theme.emberLine + Theme.emberBand : 6
    radius: line ? 0 : height / 2
    color: line ? "transparent" : Qt.rgba(1, 1, 1, 0.25)
    Accessible.role: Accessible.ProgressBar
    Accessible.name: qsTr("Progress %1%").arg(Math.round(fraction * 100))
    Rectangle {
        objectName: "emberBand"
        visible: root.line && root.fraction > 0
        y: root.height - Theme.emberLine - Theme.emberBand
        width: root.width * root.fraction
        height: Theme.emberBand
        color: Theme.background
    }
    Rectangle {
        objectName: root.line ? "emberLine" : "trackFill"
        y: root.line ? root.height - Theme.emberLine : 0
        width: root.width * root.fraction
        height: root.line ? Theme.emberLine : parent.height
        radius: root.line ? 0 : parent.radius
        color: Theme.ember
    }
}
