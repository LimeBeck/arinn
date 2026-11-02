// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import "../.." as Ui
Ui.Surface {
    id: root
    property string iconName: "play"
    implicitWidth: 80; implicitHeight: 80; width: 80; height: 80
    cornerRadius: 40
    contentItem: MusicIcon {
        anchors.centerIn: parent
        width: 32; height: 32
        name: root.iconName
        color: root.activeFocus ? Ui.Theme.ink : Ui.Theme.textPrimary
    }
}
