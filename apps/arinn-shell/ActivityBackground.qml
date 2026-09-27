// SPDX-FileCopyrightText: 2026 Arinn contributors
//
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import QtQuick.Effects

// Shared rendering for a compositor texture or an exported last-frame image.
Item {
    id: root
    property var sourceItem: null
    property url source: ""
    property bool near: false
    Rectangle { anchors.fill: parent; color: "black" }
    Image {
        id: snapshot
        anchors.fill: parent
        source: root.source
        fillMode: Image.PreserveAspectFit
        visible: false
        cache: false
    }
    MultiEffect {
        anchors.fill: parent
        source: root.sourceItem || snapshot
        scale: root.near ? 0.95 : 0.90
        blurEnabled: true
        blurMax: 36
        blur: root.near ? 8 / 36 : 1
        opacity: root.near ? 0.6 : 0.32
    }
    Rectangle {
        anchors.fill: parent
        color: "black"
        opacity: root.near ? 0.3 : 0.6
    }
}
