// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import "../.." as Ui

// Feature state labels and stable alternatives stay with Music; the shared
// button owns its plate, icon/label layout and activation contract.
Ui.ActionButton {
    id: root
    hoverMotionRevision: Ui.InputMode.pointerRevision
    onDpadRequested: Ui.InputMode.keyPressed()
    leftPadding: 28
    rightPadding: 28
    spacing: 14
    iconSize: 36
    busyText: root.text
    iconItem.objectName: "musicPillIcon"
    width: implicitWidth
    height: implicitHeight
}
