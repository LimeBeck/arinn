// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick

Surface {
    id: root
    property bool destructive: false
    property bool primary: false
    shapeRole: "action"
    implicitWidth: Math.max(180, label.implicitWidth + 80)
    implicitHeight: 88
    cornerRadius: height / 2
    focusScale: Theme.contentFocusScale
    focusLift: 0
    liftFromBottom: false
    restingColor: destructive ? Qt.rgba(0.91, 0.28, 0.31, 0.22)
            : primary ? Theme.platePrimary : Theme.plateAction
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
