// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick

// Presentation around a native editor; validation, panel placement, local
// key routing and persistence remain in the feature owner.
Item {
    id: root
    property TextField editor: null
    property bool keyboardOpen: false
    property bool keyboardEnabled: !editor || (!editor.readOnly && !editor.busy)
    property bool keyboardVisible: true
    property bool error: editor ? editor.error : false
    property bool rimEnabled: true
    property bool alwaysFocused: false
    property real outlineWidth: 0
    property color outlineColor: Theme.ring
    property real radius: 34
    property real inputInset: 32
    property real rightInset: 24
    property real gap: 24
    property real keyboardHeight: 84
    property color restingColor: Qt.rgba(1, 1, 1, 0.07)
    property color focusedColor: Qt.rgba(1, 1, 1, 0.12)
    property bool pointerMode: false
    property bool hoverFocusEnabled: true
    property int hoverMotionRevision: -1
    readonly property bool focused: alwaysFocused || keyboardOpen
        || (editor && editor.activeFocus) || keyboardButton.activeFocus
    // Reserve a newly shown counter immediately, before Row's polish pass,
    // so the caret never spends a frame underneath it.
    readonly property real trailingWidth: {
        const items = trailingRow.children.filter(item => item.visible);
        return items.reduce((sum, item) => sum + item.width, 0)
            + Math.max(0, items.length - 1) * gap;
    }
    readonly property alias keyboardButton: keyboardButton
    readonly property alias rim: rim
    readonly property alias inputItem: inputSlot
    default property alias content: inputSlot.data
    property alias trailing: trailingRow.data
    signal keyboardRequested()
    signal keyboardKeyPressed(var event)
    signal dpadRequested()
    implicitHeight: 132
    Binding {
        target: root.editor
        property: "keyboardOpen"
        value: root.keyboardOpen
        when: root.editor !== null
    }
    Rectangle {
        anchors.fill: parent
        radius: root.radius
        color: root.focused ? root.focusedColor : root.restingColor
        border.width: root.error ? 2 : root.outlineWidth
        border.color: root.error ? Theme.error : root.outlineColor
    }
    FocusRing {
        id: rim
        shown: root.rimEnabled && root.focused
        radius: root.radius
    }
    Item {
        id: inputSlot
        x: root.inputInset
        width: Math.max(0, (trailingRow.width > 0 ? trailingRow.x
            : keyboardButton.visible ? keyboardButton.x : root.width - root.rightInset) - x - root.gap)
        height: parent.height
    }
    Row {
        id: trailingRow
        width: root.trailingWidth
        x: (keyboardButton.visible ? keyboardButton.x : root.width - root.rightInset) - root.gap - width
        anchors.verticalCenter: parent.verticalCenter
        spacing: root.gap
    }
    KeyboardButton {
        id: keyboardButton
        x: parent.width - width - root.rightInset
        anchors.verticalCenter: parent.verticalCenter
        height: root.keyboardHeight
        visible: root.keyboardVisible
        enabled: root.keyboardEnabled
        panelOpen: root.keyboardOpen
        pointerMode: root.pointerMode
        hoverFocusEnabled: root.hoverFocusEnabled
        hoverMotionRevision: root.hoverMotionRevision
        onDpadRequested: root.dpadRequested()
        onActivated: root.keyboardRequested()
        Keys.onPressed: event => root.keyboardKeyPressed(event)
    }
}
