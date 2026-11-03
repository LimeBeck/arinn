// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import "../.." as Ui
Ui.Surface {
    id: root
    property bool selected: false
    property string iconName: "play"
    implicitWidth: 80; implicitHeight: 80; width: 80; height: 80
    cornerRadius: 40
    shapeRole:"action"
    restingColor: selected ? Ui.Theme.platePrimary : Ui.Theme.plateAction
    focusedColor: Ui.Theme.textPrimary
    Ui.UiText {anchors.top:parent.bottom;anchors.topMargin:14;anchors.horizontalCenter:parent.horizontalCenter;width:260;visible:root.activeFocus;text:root.text;role:"caption";horizontalAlignment:Text.AlignHCenter;maximumLineCount:1;elide:Text.ElideRight}
    contentItem: Item {
        MusicIcon {
            anchors.centerIn: parent
            width: root.width >= 96 ? 48 : 32; height: width
            name: root.iconName
            color: root.activeFocus ? Ui.Theme.ink : Ui.Theme.textPrimary
        }
    }
}
