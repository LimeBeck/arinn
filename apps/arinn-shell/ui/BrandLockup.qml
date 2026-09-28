// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick

Item {
    id: root
    property real markSize: 32
    property string textRole: "control"
    readonly property bool markReady: mark.status === Image.Ready
    implicitWidth: mark.width + 12 + wordmark.implicitWidth
    implicitHeight: Math.max(mark.height, wordmark.implicitHeight)
    Image {
        id: mark
        objectName: "brandMark"
        width: root.markSize; height: root.markSize
        anchors.verticalCenter: parent.verticalCenter
        source: "qrc:/brand/arinn-mark.svg"
        sourceSize: Qt.size(256, 256)
        fillMode: Image.PreserveAspectFit
        mipmap: true
    }
    UiText {
        id: wordmark
        x: mark.width + 12
        anchors.verticalCenter: parent.verticalCenter
        text: "Arinn"
        role: root.textRole
        color: Theme.textPrimary
    }
}
