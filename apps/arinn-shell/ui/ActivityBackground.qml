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
        asynchronous: true
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
    // Near, the application's own controls in its top bar and along its
    // bottom edge would read through Home's header and lowest row. Dark
    // bands keep them under the shell (owner decision on #756, ADR-0140).
    Rectangle {
        objectName: "activityTopBand"
        anchors { left: parent.left; right: parent.right; top: parent.top }
        height: parent.height * 180 / 1080
        visible: root.near
        gradient: Gradient {
            GradientStop { position: 0; color: Qt.rgba(0, 0, 0, 0.85) }
            GradientStop { position: 0.7; color: Qt.rgba(0, 0, 0, 0.6) }
            GradientStop { position: 1; color: Qt.rgba(0, 0, 0, 0) }
        }
    }
    Rectangle {
        objectName: "activityBottomBand"
        anchors { left: parent.left; right: parent.right; bottom: parent.bottom }
        height: parent.height * 140 / 1080
        visible: root.near
        gradient: Gradient {
            GradientStop { position: 0; color: Qt.rgba(0, 0, 0, 0) }
            GradientStop { position: 0.3; color: Qt.rgba(0, 0, 0, 0.6) }
            GradientStop { position: 1; color: Qt.rgba(0, 0, 0, 0.85) }
        }
    }
}
