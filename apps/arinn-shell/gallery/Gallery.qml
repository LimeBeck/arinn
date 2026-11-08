// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import QtQuick.Window
import "../ui" as Ui

Window {
    id: root
    width: 1920
    height: 1080
    visible: true
    color: Ui.Theme.background
    title: "Arinn — component gallery"
    property bool pointerMode: false
    property bool dialogOpen: false
    property bool sheetOpen: false
    property string lastAction: qsTr("Arrows to navigate · Enter to select")
    Component.onCompleted: Ui.Focus.take(media)
    Ui.LogicalCanvas {
        anchors.fill: parent
        Keys.onPressed: event => {
            if ([Qt.Key_Left, Qt.Key_Right, Qt.Key_Up, Qt.Key_Down].includes(event.key))
                root.pointerMode = false;
            event.accepted = false;
        }
        HoverHandler { onPointChanged: root.pointerMode = true }
        Ui.UiText { x: 120; y: 52; text: "Arinn / UI components"; role: "section" }
        Ui.UiText { x: 120; y: 108; text: qsTr("A quiet space for your content"); role: "title" }
        Ui.UiText { x: 120; y: 194; text: qsTr("Continue"); role: "eyebrow"; color: Ui.Theme.textSecondary }
        Row {
            x: 120; y: 252; spacing: 32
            Ui.ContentCard {
                id: media
                text: qsTr("Northern light")
                subtitle: qsTr("1:13:42 of 2:46:00")
                artwork: Qt.resolvedUrl("fixture-artwork.svg")
                showProgress: true; progress: 0.44
                focus: true
                pointerMode: root.pointerMode
                onDpadRequested: root.pointerMode = false
                KeyNavigation.right: activity
                KeyNavigation.down: tile
                onActivated: root.lastAction = qsTr("Resume requested — fixture only")
            }
            Ui.ContentCard {
                id: activity
                text: qsTr("Living room")
                subtitle: qsTr("Return")
                running: true
                pointerMode: root.pointerMode
                onDpadRequested: root.pointerMode = false
                KeyNavigation.right: pending
                KeyNavigation.down: tile
                onActivated: root.lastAction = qsTr("Return requested — fixture only")
            }
            Ui.ContentCard {
                id: pending
                text: qsTr("Getting ready")
                subtitle: qsTr("Waiting for content")
                busy: true
                pointerMode: root.pointerMode
                onDpadRequested: root.pointerMode = false
                KeyNavigation.down: unavailable
            }
            Ui.CaptionedCard {
                width: 280
                artworkHeight: 225
                artwork: Qt.resolvedUrl("fixture-artwork.svg")
                text: qsTr("Artwork and caption slots")
                pointerMode: root.pointerMode
                KeyNavigation.left: pending
                KeyNavigation.down: unavailable
                caption: Ui.UiText {
                    width: parent.width
                    text: qsTr("Stationary caption")
                    role: "caption"
                    maximumLineCount: 1
                    elide: Text.ElideRight
                }
                onActivated: root.lastAction = qsTr("Captioned card requested — fixture only")
            }
        }
        Ui.UiText { x: 120; y: 530; text: qsTr("Favorites / actions"); role: "eyebrow"; color: Ui.Theme.textSecondary }
        Row {
            x: 120; y: 588; spacing: 32
            Ui.AppTile {
                id: tile
                text: qsTr("Library"); monogram: "L"
                pointerMode: root.pointerMode
                onDpadRequested: root.pointerMode = false
                KeyNavigation.right: action
                KeyNavigation.up: media
                KeyNavigation.down: unavailable
                onActivated: root.lastAction = qsTr("Open requested — fixture only")
            }
            Ui.ActionButton {
                id: action
                anchors.verticalCenter: parent.verticalCenter
                text: "Продолжить просмотр"
                pointerMode: root.pointerMode
                onDpadRequested: root.pointerMode = false
                KeyNavigation.right: unavailable
                KeyNavigation.up: activity
                KeyNavigation.down: dialogLauncher
                onActivated: root.lastAction = qsTr("Primary action requested — fixture only")
            }
            Ui.ActionButton {
                anchors.verticalCenter: parent.verticalCenter
                text: qsTr("Unavailable")
                enabled: false
            }
        }
        Ui.StatusPane {
            id: unavailable
            x: 1170; y: 544; width: 620
            kind: "unavailable"
            title: qsTr("Content unavailable")
            description: qsTr("Check the connection and try again.")
            actionText: qsTr("Try again")
            pointerMode: root.pointerMode
            onDpadRequested: root.pointerMode = false
            KeyNavigation.up: pending
            KeyNavigation.left: action
            onActionRequested: root.lastAction = qsTr("Retry requested — fixture only")
        }
        Row {
            x: 120; y: 780; spacing: 24
            Ui.ActionButton {
                id: dialogLauncher
                text: qsTr("Confirmation dialog")
                iconName: "check"
                pointerMode: root.pointerMode
                onDpadRequested: root.pointerMode = false
                KeyNavigation.up: tile
                KeyNavigation.right: sheetLauncher
                onActivated: {
                    root.dialogOpen = true;
                    Qt.callLater(demoDialog.focusInitial);
                }
            }
            Ui.ActionButton {
                id: sheetLauncher
                text: qsTr("Side sheet")
                iconName: "queue_music"
                pointerMode: root.pointerMode
                onDpadRequested: root.pointerMode = false
                KeyNavigation.up: action
                KeyNavigation.left: dialogLauncher
                KeyNavigation.right: iconExample
                onActivated: {
                    root.sheetOpen = true;
                    Qt.callLater(() => Ui.Focus.take(sheetClose));
                }
            }
            Ui.IconButton {
                id: iconExample
                text: checked ? qsTr("Liked") : qsTr("Like")
                iconName: checked ? "favorite_fill" : "favorite"
                checkable: true
                selected: checked
                pointerMode: root.pointerMode
                onDpadRequested: root.pointerMode = false
                KeyNavigation.left: sheetLauncher
                KeyNavigation.up: action
                onActivated: root.lastAction = qsTr("Selection changed — fixture only")
            }
        }
        Ui.UiText {
            x: 120; y: 900; width: 900
            text: root.lastAction
            role: "meta"; color: Ui.Theme.textSecondary
        }
        Ui.UiText {
            x: 120; y: 992
            text: qsTr("Development fixtures · no applications are launched")
            role: "caption"; color: Ui.Theme.textHint
        }
        Ui.ConfirmationDialog {
            id: demoDialog
            z: 100
            visible: root.dialogOpen
            title: qsTr("Continue with this collection?")
            body: qsTr("This is a presentation fixture. The screen owns the operation and focus restoration.")
            onDismissed: {
                root.dialogOpen = false;
                Ui.Focus.take(dialogLauncher);
            }
            Ui.ActionButton {
                text: qsTr("Cancel")
                pointerMode: root.pointerMode
                onDpadRequested: root.pointerMode = false
                onActivated: demoDialog.dismissed()
            }
            Ui.ActionButton {
                text: qsTr("Continue")
                pointerMode: root.pointerMode
                onDpadRequested: root.pointerMode = false
                onActivated: {
                    root.lastAction = qsTr("Confirmation requested — fixture only");
                    demoDialog.dismissed();
                }
            }
        }
        FocusScope {
            id: demoSheetScope
            z: 100
            anchors.fill: parent
            visible: root.sheetOpen
            function dismiss() {
                root.sheetOpen = false;
                Ui.Focus.take(sheetLauncher);
            }
            Keys.onPressed: event => {
                if ([Qt.Key_Escape, Qt.Key_Back, Qt.Key_Backspace].includes(event.key)) {
                    dismiss();
                    event.accepted = true;
                }
            }
            Ui.SideSheet {
                onDismissed: demoSheetScope.dismiss()
                Ui.UiText { x: 64; y: 72; text: qsTr("Side sheet"); role: "title" }
                Ui.ActionButton {
                    id: sheetClose
                    x: 64; y: 180
                    text: qsTr("Close")
                    pointerMode: root.pointerMode
                    onDpadRequested: root.pointerMode = false
                    KeyNavigation.tab: sheetClose
                    KeyNavigation.backtab: sheetClose
                    onActivated: demoSheetScope.dismiss()
                }
            }
        }
    }
}
