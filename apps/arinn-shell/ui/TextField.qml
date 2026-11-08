// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import QtQuick.Templates as T

// Native TextInput editing/selection remains intact. Owners handle routing
// keys and validation; this primitive supplies text, placeholder and caret.
T.TextField {
    id: root
    property bool keyboardOpen: false
    property bool busy: false
    property string errorText: ""
    property bool error: errorText.length > 0
    property string accessibleLabel: placeholderText
    signal submitted()
    padding: 0
    activeFocusOnTab: false
    font.family: Theme.fontFamily
    font.pixelSize: 36
    color: Theme.textPrimary
    placeholderTextColor: Theme.textHint
    verticalAlignment: TextInput.AlignVCenter
    clip: true
    background: null
    readOnly: busy
    cursorVisible: activeFocus || keyboardOpen
    cursorDelegate: Rectangle {
        width: 4
        color: Theme.ember
        visible: root.cursorVisible
    }
    FontWeightAxis {}
    Accessible.name: accessibleLabel
    Accessible.description: errorText
    onAccepted: if (enabled && !readOnly && !busy) submitted()
    Keys.onPressed: event => {
        // A held remote OK must not submit repeatedly. Feature handlers may
        // handle Return themselves when it means another local intent.
        if (event.isAutoRepeat && (event.key === Qt.Key_Return || event.key === Qt.Key_Enter))
            event.accepted = true;
    }
    UiText {
        objectName: "textFieldPlaceholder"
        x: root.leftPadding
        y: root.topPadding
        width: Math.max(0, root.width - root.leftPadding - root.rightPadding)
        height: Math.max(0, root.height - root.topPadding - root.bottomPadding)
        text: root.placeholderText
        textFormat: Text.PlainText
        color: root.placeholderTextColor
        font: root.font
        verticalAlignment: Text.AlignVCenter
        maximumLineCount: 1
        elide: Text.ElideRight
        visible: !root.text.length && !root.preeditText.length
    }
}
