// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import "../../apps/arinn-shell/ui" as Ui

Window {
    visible: true
    width: 264; height: 132
    Ui.AppTile {
        objectName: "iconTile"
        anchors.fill: parent
        text: "Fixture"
        iconSource: iconUrl
    }
}
