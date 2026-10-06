// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick

FocusScope {
    id: root
    property var entries: []
    property var busyEntryIds: []
    property var openShortcutIds: []
    property bool browserUnavailable: false
    property string title: ""
    property string variant: "content"
    property bool pointerMode: false
    property bool hoverFocusEnabled: true
    property string selectedId: ""
    property int selectedIndex: 0
    readonly property int count: entries.length
    property real cellWidth: variant === "app" ? 264 : variant === "poster" ? 180 : 400
    readonly property real cellHeight: variant === "app" ? 132 : variant === "poster" ? 270 : 225
    readonly property real gap: variant === "content" ? 32 : 24
    signal selected(var entry)
    signal activated(string entryId)
    signal actionsRequested(string entryId)
    signal edge(int key)
    signal focusLost()
    signal scrolled()
    signal dpadRequested()
    height: cellHeight + 74

    function available(index) {
        return index >= 0 && index < count && entries[index].enabled !== false;
    }
    function focusIndex(index) {
        index = Math.min(Math.max(index, 0), count - 1);
        let candidate = index;
        while (candidate >= 0 && !available(candidate)) --candidate;
        if (candidate < 0) {
            candidate = index + 1;
            while (candidate < count && !available(candidate)) ++candidate;
        }
        if (!available(candidate)) return false;
        const item = cards.itemAt(candidate);
        if (!item || !item.item) return false;
        Focus.take(item.item);
        return true;
    }
    function restore() { return focusIndex(selectedIndex); }
    function reconcile() {
        const previous = selectedIndex;
        const found = entries.findIndex(entry => entry.id === selectedId);
        selectedIndex = found >= 0 ? found : Math.max(0, previous - 1);
        if (activeFocus) {
            if (!restore()) focusLost();
        }
    }
    onEntriesChanged: Qt.callLater(reconcile)
    function reveal(index) {
        const left = index * (cellWidth + gap);
        const margin = pointerMode ? 0 : cellWidth + gap;
        let next = viewport.contentX;
        if (left < next + margin) next = left - margin;
        else if (left + cellWidth > next + viewport.width - margin)
            next = left + cellWidth - viewport.width + margin;
        viewport.contentX = Math.max(0, Math.min(next, Math.max(0, viewport.contentWidth - viewport.width)));
    }
    Keys.onPressed: event => {
        if (![Qt.Key_Left, Qt.Key_Right, Qt.Key_Up, Qt.Key_Down].includes(event.key)) return;
        dpadRequested();
        event.accepted = true;
        if (event.key === Qt.Key_Left || event.key === Qt.Key_Right) {
            const step = event.key === Qt.Key_Left ? -1 : 1;
            let next = selectedIndex + step;
            while (next >= 0 && next < count && !available(next)) next += step;
            if (available(next)) focusIndex(next);
            else edge(event.key);
        } else edge(event.key);
    }
    UiText { width: parent.width; text: root.title; role: "section"; maximumLineCount: 1; elide: Text.ElideRight }
    Flickable {
        id: viewport
        objectName: root.objectName + "Viewport"
        x: -48; y: 10; width: parent.width + 96; height: root.cellHeight + 104
        clip: true
        contentWidth: cardsRow.width + 96
        contentHeight: height
        boundsBehavior: Flickable.StopAtBounds
        flickableDirection: Flickable.HorizontalFlick
        onContentXChanged: root.scrolled()
        Row {
            id: cardsRow
            x: 48; y: 48
            spacing: root.gap
            Repeater {
                id: cards
                model: root.entries
                delegate: Loader {
                    id: entryLoader
                    required property var modelData
                    required property int index
                    width: root.cellWidth; height: root.cellHeight
                    sourceComponent: root.variant === "app" ? tile : card
                    function chosen() {
                        root.selectedIndex = index;
                        root.selectedId = modelData.id;
                        root.selected(modelData);
                        root.reveal(index);
                    }
                    Component {
                        id: tile
                        AppTile {
                            objectName: "entry-" + entryLoader.modelData.id
                            width: entryLoader.width; height: entryLoader.height
                            text: entryLoader.modelData.title
                            visualIdentity: entryLoader.modelData.visualIdentity || entryLoader.modelData.id
                            monogram: entryLoader.modelData.monogram || entryLoader.modelData.title.trim().slice(0, 1).toUpperCase()
                            iconSource: entryLoader.modelData.iconSource || ""
                            subtitle: (root.browserUnavailable && entryLoader.modelData.webShortcut) ? qsTr("Unavailable") : (entryLoader.modelData.subtitle || "")
                            running: entryLoader.modelData.running || false
                            serviceOpen: root.openShortcutIds.indexOf(entryLoader.modelData.id) >= 0
                            enabled: entryLoader.modelData.enabled !== false
                            busy: entryLoader.modelData.busy || root.busyEntryIds.includes(entryLoader.modelData.id)
                            pointerMode: root.pointerMode
                            hoverFocusEnabled: root.hoverFocusEnabled
                            // An unavailable saved favorite is focusable so it can be removed.
                            unavailable: entryLoader.modelData.unavailable || (root.browserUnavailable && !!entryLoader.modelData.webShortcut)
                            onActivated: root.activated(entryLoader.modelData.id)
                            onPressAndHold: root.actionsRequested(entryLoader.modelData.id)
                            Keys.onPressed: event => {
                                if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter || event.key === Qt.Key_Space) {
                                    if (!event.isAutoRepeat && !busy) { holdHandled = false; down = true; holdTimer.restart(); }
                                    event.accepted = true;
                                }
                            }
                            property bool holdHandled: false
                            Timer {
                                id: holdTimer; interval: 650
                                onTriggered: {
                                    parent.holdHandled = true; parent.down = false;
                                    root.actionsRequested(entryLoader.modelData.id);
                                }
                            }
                            Keys.onReleased: event => {
                                if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter || event.key === Qt.Key_Space) {
                                    holdTimer.stop();
                                    if (holdHandled) { down = false; event.accepted = true; }
                                    else { const invoke = down && enabled && !busy; down = false; if (invoke) clicked(); }
                                    event.accepted = true;
                                }
                            }
                            onActiveFocusChanged: {
                                if (activeFocus) entryLoader.chosen();
                                else holdTimer.stop();
                            }
                            onDpadRequested: root.dpadRequested()
                        }
                    }
                    Component {
                        id: card
                        ContentCard {
                            objectName: "entry-" + entryLoader.modelData.id
                            width: entryLoader.width; height: entryLoader.height
                            text: entryLoader.modelData.title
                            visualIdentity: entryLoader.modelData.visualIdentity || entryLoader.modelData.id
                            subtitle: entryLoader.modelData.subtitle || ""
                            artwork: entryLoader.modelData.artwork || ""
                            running: entryLoader.modelData.running || false
                            showProgress: entryLoader.modelData.progress !== undefined
                            progress: entryLoader.modelData.progress || 0
                            enabled: entryLoader.modelData.enabled !== false
                            busy: entryLoader.modelData.busy || root.busyEntryIds.includes(entryLoader.modelData.id)
                            pointerMode: root.pointerMode
                            hoverFocusEnabled: root.hoverFocusEnabled
                            onActiveFocusChanged: if (activeFocus) entryLoader.chosen()
                            onActivated: root.activated(entryLoader.modelData.id)
                            onDpadRequested: root.dpadRequested()
                        }
                    }
                }
            }
        }
    }
}
