// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import "../.." as Ui

// The source mark (brief mh-01, MH-10): a 22 px glyph in a neutral chip, the
// same on every caption and in the Sources sheet. The shell has no provider
// icons, so it tells this computer (a screen) from a service (the globe);
// no product is named or preferred.
Rectangle {
    id: root
    property bool local: false
    property string name: ""
    property int size: 30
    width: size
    height: size
    radius: size / 2
    color: Ui.Theme.chip
    border.color: Ui.Theme.chipLine
    Accessible.name: local ? qsTr("This computer") : name
    Ui.LineIcon {
        anchors.centerIn: parent
        width: Math.round(root.size * 0.73)
        height: width
        name: root.local ? "screen" : "public"
        color: Ui.Theme.textHint
    }
}
