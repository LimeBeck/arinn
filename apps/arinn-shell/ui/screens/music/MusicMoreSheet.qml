// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import "../.." as Ui

// Now playing's More sheet (brief *More sheet*, #802): a right-hand sheet
// 720 wide that lists only the actions the session supports, as container
// rows with a line icon. Back closes it; the view restores focus to More.
FocusScope {
    id: root
    property bool pointerMode: Ui.InputMode.pointerActive
    property bool hoverFocusEnabled: true
    signal scrolled()
    // [{id, label, icon, selected}], in the brief's order.
    property var actions: []
    signal activated(string id)
    signal dismissed()
    anchors.fill: parent
    // Rows are keyed by action id: the state poll republishes the session
    // every second, and a rebuilt list would drop the focused row.
    ListModel { id: rowModel }
    function syncRows() {
        const ids = actions.map(action => action.id);
        for (let i = rowModel.count - 1; i >= 0; --i)
            if (!ids.includes(rowModel.get(i).actionKey))
                rowModel.remove(i);
        for (let i = 0; i < ids.length; ++i) {
            let at = -1;
            for (let j = i; j < rowModel.count && at < 0; ++j)
                if (rowModel.get(j).actionKey === ids[i])
                    at = j;
            if (at < 0)
                rowModel.insert(i, {actionKey: ids[i]});
            else if (at !== i)
                rowModel.move(at, i, 1);
        }
    }
    onActionsChanged: syncRows()
    Component.onCompleted: syncRows()
    function focusInitial() {
        list.currentIndex = 0;
        Ui.Focus.take(list.count ? list : root);
    }
    // The sheet keeps Left and Right: they never leave it sideways.
    Keys.onLeftPressed: {}
    Keys.onRightPressed: {}
    Rectangle {
        anchors.fill: parent
        color: Ui.Theme.scrim
        MouseArea {
            anchors.fill: parent
            onClicked: root.dismissed()
        }
    }
    Rectangle {
        objectName: "musicMoreSheet"
        x: parent.width - width
        width: 720
        height: parent.height
        color: Ui.Theme.dialogSurface
        border.color: Ui.Theme.dialogLine
        MouseArea { anchors.fill: parent }
        Ui.UiText {
            x: 64
            y: 72
            role: "title"
            text: qsTr("More")
        }
        Ui.UiText {
            objectName: "musicMoreEmpty"
            x: 64
            y: 180
            width: parent.width - 128
            visible: !list.count
            role: "body"
            color: Ui.Theme.textSecondary
            wrapMode: Text.WordWrap
            text: qsTr("Nothing more for this track.")
        }
        ListView {
            id: list
            onContentYChanged: if (visible) root.scrolled()
            objectName: "musicMoreActions"
            x: 48
            y: 180
            width: parent.width - 96
            height: parent.height - y - 64
            spacing: 12
            clip: true
            keyNavigationEnabled: true
            model: rowModel
            delegate: Ui.Surface {
                id: row
                pointerMode: root.pointerMode
                hoverFocusEnabled: root.hoverFocusEnabled
                hoverMotionRevision: Ui.InputMode.pointerRevision
                onDpadRequested: Ui.InputMode.keyPressed()
                required property string actionKey
                required property int index
                readonly property var moreAction: root.actions.find(item => item.id === actionKey) || ({})
                objectName: "musicMore-" + actionKey
                shapeRole: "container"
                cornerRadius: Ui.Theme.containerRadius
                focusScale: 1
                focusLift: 0
                width: list.width
                height: 96
                focus: ListView.isCurrentItem
                onActiveFocusChanged: if (activeFocus) list.currentIndex = index
                text: moreAction.label || ""
                // While a command is in flight the row ignores presses and
                // shows it.
                busy: moreAction.busy === true
                Accessible.checkable: moreAction.selected !== undefined
                Accessible.checked: moreAction.selected === true
                contentItem: Item {
                    opacity: row.busy ? 0.45 : 1
                    Row {
                        x: 28
                        anchors.verticalCenter: parent.verticalCenter
                        spacing: 24
                        Ui.LineIcon {
                            anchors.verticalCenter: parent.verticalCenter
                            width: 36
                            height: 36
                            name: row.moreAction.icon || ""
                            color: row.moreAction.selected ? Ui.Theme.ember : Ui.Theme.textPrimary
                        }
                        Ui.UiText {
                            anchors.verticalCenter: parent.verticalCenter
                            width: list.width - 140
                            text: row.text
                            role: "body"
                            textFormat: Text.PlainText
                            maximumLineCount: 1
                            elide: Text.ElideRight
                            color: Ui.Theme.textPrimary
                        }
                    }
                }
                onActivated: root.activated(actionKey)
            }
        }
    }
}
