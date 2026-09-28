// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick

Surface {
    id: root
    property bool destructive: false
    implicitWidth: Math.max(180, label.implicitWidth + 80)
    implicitHeight: 88
    cornerRadius: height / 2
    focusScale: 1.02
    restingColor: destructive ? Qt.rgba(0.91, 0.28, 0.31, 0.22) : Theme.plate
    focusedColor: Theme.textPrimary
    contentItem: UiText {
        id: label
        text: root.busy ? qsTr("Loading…") : root.text
        role: "control"
        color: root.activeFocus ? Theme.ink : Theme.textPrimary
        horizontalAlignment: Text.AlignHCenter
        verticalAlignment: Text.AlignVCenter
        wrapMode: Text.NoWrap
    }
}
