// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick

// For a Text or TextInput in the bundled Onest that is not a UiText and sets
// font.weight: put one inside it, and its weight reaches the variable font's
// wght axis as UiText's does (Theme.applyWeight, #546). It draws nothing.
Item {
    id: root
    // The text item; its parent by default.
    property Item target: parent
    visible: false
    width: 0
    height: 0

    Connections {
        target: root.target
        ignoreUnknownSignals: true
        function onFontChanged() { Theme.applyWeight(root.target); }
    }
    onTargetChanged: Theme.applyWeight(target)
    Component.onCompleted: Theme.applyWeight(target)
}
