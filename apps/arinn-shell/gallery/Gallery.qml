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
        Ui.UiText { x: 120; y: 194; text: qsTr("CONTINUE"); role: "eyebrow"; color: Ui.Theme.textSecondary }
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
            Ui.Skeleton { width: 352 }
        }
        Ui.UiText { x: 120; y: 530; text: qsTr("FAVORITES / ACTIONS"); role: "eyebrow"; color: Ui.Theme.textSecondary }
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
    }
}
