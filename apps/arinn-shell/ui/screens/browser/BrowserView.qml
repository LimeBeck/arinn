// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import "../../" as Ui

// Fixture presentation of one browser capability's windows and tabs.
// Callers pass explicit properties; this screen does not talk to a daemon.
FocusScope {
    id: root
    width: 1920
    height: 1080

    property var tabs: []
    property var shortcuts: []
    property string browserState: "populated" // populated, empty, loading, error, unavailable
    property string statusTitle: ""
    property string statusDescription: ""
    property bool pointerMode: false
    property bool hoverFocusEnabled: true

    readonly property bool showTabs: browserState === "populated" && presentationTabs.length > 0
    readonly property string statusKind: {
        if (browserState === "populated")
            return "empty";
        if (browserState === "empty" || browserState === "loading" || browserState === "error" || browserState === "unavailable")
            return browserState;
        return "unavailable";
    }
    readonly property string statusActionText: statusKind === "loading" ? "" : statusKind === "empty" ? qsTr("Refresh") : qsTr("Retry")
    readonly property var stateCopy: ({
        empty: [qsTr("No tabs"), qsTr("No tabs are open.")],
        loading: [qsTr("Loading tabs"), qsTr("The tab list is loading.")],
        error: [qsTr("Tabs unavailable"), qsTr("The tab list could not be loaded.")],
        unavailable: [qsTr("No browser connected"), qsTr("No browser is connected.")]
    })
    readonly property string resolvedTitle: statusTitle.length ? statusTitle : (stateCopy[statusKind] ? stateCopy[statusKind][0] : "")
    readonly property string resolvedDescription: statusDescription.length ? statusDescription : (stateCopy[statusKind] ? stateCopy[statusKind][1] : "")

    property var presentationTabs: []
    property var windowIds: []
    property var visibleTabs: []
    property var shortcutEntries: []
    property string activeTabId: ""
    property string focusedWindowId: ""
    property string focusedTabId: ""
    property string focusRow: "tabs"
    property int windowIndex: 0
    property int tabIndex: 0
    property int actionIndex: 0
    property int shortcutIndex: 0

    signal activateTab(string tabId)
    signal closeTab(string tabId)
    signal open(string shortcutId)
    signal retryRequested()

    function normalizeTabs(input) {
        const source = Array.isArray(input) ? input : [];
        const seen = {};
        let activeAssigned = false;
        const result = [];
        for (let i = 0; i < source.length; ++i) {
            const tab = source[i] || {};
            const id = tab.id !== undefined && tab.id !== null ? String(tab.id) : "";
            if (!id || seen[id])
                continue;
            seen[id] = true;
            const active = !!tab.active && !activeAssigned;
            if (active)
                activeAssigned = true;
            result.push({
                id: id,
                windowId: tab.windowId !== undefined && tab.windowId !== null ? String(tab.windowId) : "",
                title: tab.title !== undefined && tab.title !== null ? String(tab.title) : "",
                url: tab.url !== undefined && tab.url !== null ? String(tab.url) : "",
                active: active,
                audible: !!tab.audible
            });
        }
        return result;
    }

    function normalizeShortcuts(input) {
        const source = Array.isArray(input) ? input : [];
        const seen = {};
        const result = [];
        for (let i = 0; i < source.length; ++i) {
            const entry = source[i] || {};
            const id = entry.id !== undefined && entry.id !== null ? String(entry.id) : "";
            if (!id || seen[id])
                continue;
            seen[id] = true;
            const title = entry.title !== undefined && entry.title !== null && String(entry.title).length
                    ? String(entry.title) : qsTr("Shortcut");
            result.push({id: id, title: title});
        }
        return result;
    }

    function syncVisibleTabs() {
        const visible = presentationTabs.filter(tab => tab.windowId === focusedWindowId);
        visibleTabs = visible;
        if (!visible.some(tab => tab.id === focusedTabId)) {
            const preferred = visible.find(tab => tab.active) || visible[0];
            focusedTabId = preferred ? preferred.id : "";
        }
        tabIndex = Math.max(0, visible.findIndex(tab => tab.id === focusedTabId));
    }

    function rebuild() {
        const nextTabs = normalizeTabs(tabs);
        const ids = [];
        const seen = {};
        for (let i = 0; i < nextTabs.length; ++i) {
            const windowId = nextTabs[i].windowId;
            if (seen[windowId])
                continue;
            seen[windowId] = true;
            ids.push(windowId);
        }
        const active = nextTabs.find(tab => tab.active);
        presentationTabs = nextTabs;
        windowIds = ids;
        shortcutEntries = normalizeShortcuts(shortcuts);
        activeTabId = active ? active.id : "";
        const nextWindow = ids.indexOf(focusedWindowId) >= 0
                ? focusedWindowId
                : (active ? active.windowId : (ids.length ? ids[0] : ""));
        const windowChanged = focusedWindowId !== nextWindow;
        focusedWindowId = nextWindow;
        windowIndex = Math.max(0, ids.indexOf(focusedWindowId));
        if (!windowChanged)
            syncVisibleTabs();
    }

    function rowNames() {
        const rows = [];
        if (showTabs) {
            if (windowIds.length)
                rows.push("windows");
            if (visibleTabs.length)
                rows.push("tabs");
            if (visibleTabs.length)
                rows.push("actions");
        } else if (statusActionText.length) {
            rows.push("status");
        }
        if (shortcutEntries.length && browserState !== "loading")
            rows.push("shortcuts");
        return rows;
    }

    function preferredRow() {
        const rows = rowNames();
        if (rows.indexOf("tabs") >= 0)
            return "tabs";
        if (rows.indexOf("status") >= 0)
            return "status";
        return rows.length ? rows[0] : "";
    }

    function indexFor(row) {
        if (row === "windows")
            return windowIndex;
        if (row === "tabs")
            return tabIndex;
        if (row === "actions")
            return actionIndex;
        if (row === "shortcuts")
            return shortcutIndex;
        return 0;
    }

    function countFor(row) {
        if (row === "windows")
            return windowIds.length;
        if (row === "tabs")
            return visibleTabs.length;
        if (row === "actions")
            return visibleTabs.length ? 2 : 0;
        if (row === "shortcuts")
            return shortcutEntries.length;
        if (row === "status")
            return statusActionText.length ? 1 : 0;
        return 0;
    }

    function findNamed(node, name) {
        if (!node)
            return null;
        if (node.objectName === name)
            return node;
        const kids = node.children || [];
        for (let i = 0; i < kids.length; ++i) {
            const found = findNamed(kids[i], name);
            if (found)
                return found;
        }
        return null;
    }

    function itemFor(row, index) {
        if (row === "windows")
            return windowRepeater.itemAt(index);
        if (row === "tabs")
            return tabRepeater.itemAt(index);
        if (row === "actions")
            return index === 0 ? activateButton : closeButton;
        if (row === "shortcuts")
            return shortcutRepeater.itemAt(index);
        if (row === "status")
            return findNamed(statusPane, "statusAction");
        return null;
    }

    function selectWindow(index) {
        if (index < 0 || index >= windowIds.length)
            return;
        windowIndex = index;
        focusedWindowId = windowIds[index];
    }

    function noteWindowFocus(index) {
        focusRow = "windows";
        if (windowIndex === index && focusedWindowId === windowIds[index])
            return;
        selectWindow(index);
    }

    function trapTab(event) {
        if (event.key === Qt.Key_Tab || event.key === Qt.Key_Backtab) {
            pointerMode = false;
            event.accepted = true;
            move(event.key === Qt.Key_Tab ? Qt.Key_Right : Qt.Key_Left);
            return;
        }
        if (event.key === Qt.Key_Left || event.key === Qt.Key_Right || event.key === Qt.Key_Up || event.key === Qt.Key_Down)
            event.accepted = false;
    }

    function muteTabStops(node) {
        if (!node)
            return;
        if (node !== root && node.activeFocusOnTab)
            node.activeFocusOnTab = false;
        const kids = node.children || [];
        for (let i = 0; i < kids.length; ++i)
            muteTabStops(kids[i]);
    }

    function focusCell(row, index) {
        const count = countFor(row);
        if (count <= 0)
            return false;
        index = Math.max(0, Math.min(index, count - 1));
        if (row === "windows")
            selectWindow(index);
        else if (row === "tabs") {
            tabIndex = index;
            focusedTabId = visibleTabs[index].id;
        } else if (row === "actions") {
            actionIndex = index;
        } else if (row === "shortcuts") {
            shortcutIndex = index;
        }
        focusRow = row;
        const item = itemFor(row, index);
        if (!item)
            return false;
        item.forceActiveFocus();
        return true;
    }

    function restoreFocus() {
        const rows = rowNames();
        if (!rows.length) {
            forceActiveFocus();
            return;
        }
        const row = rows.indexOf(focusRow) >= 0 ? focusRow : preferredRow();
        if (!focusCell(row, indexFor(row)) && row !== preferredRow())
            focusCell(preferredRow(), indexFor(preferredRow()));
    }

    function move(key) {
        const rows = rowNames();
        if (!rows.length)
            return;
        let row = rows.indexOf(focusRow) >= 0 ? focusRow : rows[0];
        const rowPos = rows.indexOf(row);
        if (key === Qt.Key_Left || key === Qt.Key_Right) {
            const delta = key === Qt.Key_Left ? -1 : 1;
            const next = indexFor(row) + delta;
            if (next >= 0 && next < countFor(row))
                focusCell(row, next);
            else
                focusCell(row, indexFor(row));
            return;
        }
        const delta = key === Qt.Key_Up ? -1 : 1;
        const nextRowPos = rowPos + delta;
        if (nextRowPos < 0 || nextRowPos >= rows.length) {
            focusCell(row, indexFor(row));
            return;
        }
        focusCell(rows[nextRowPos], indexFor(rows[nextRowPos]));
    }

    function requestOpen(shortcutId) {
        for (let i = 0; i < shortcutEntries.length; ++i) {
            if (shortcutEntries[i].id === shortcutId) {
                open(shortcutId);
                return;
            }
        }
    }

    onTabsChanged: {
        const keepFocus = activeFocus;
        rebuild();
        if (keepFocus)
            Qt.callLater(restoreFocus);
    }
    onShortcutsChanged: {
        const keepFocus = activeFocus;
        rebuild();
        if (keepFocus)
            Qt.callLater(restoreFocus);
    }
    onFocusedWindowIdChanged: syncVisibleTabs()
    onBrowserStateChanged: {
        focusRow = preferredRow();
        Qt.callLater(() => {
            muteTabStops(root);
            focusRow = preferredRow();
            restoreFocus();
        });
    }
    Component.onCompleted: {
        rebuild();
        Qt.callLater(() => {
            muteTabStops(root);
            focusRow = preferredRow();
            restoreFocus();
        });
    }

    Keys.onPressed: event => {
        const arrows = [Qt.Key_Left, Qt.Key_Right, Qt.Key_Up, Qt.Key_Down];
        if (arrows.indexOf(event.key) < 0 && event.key !== Qt.Key_Tab && event.key !== Qt.Key_Backtab)
            return;
        pointerMode = false;
        event.accepted = true;
        if (event.key === Qt.Key_Tab)
            move(Qt.Key_Right);
        else if (event.key === Qt.Key_Backtab)
            move(Qt.Key_Left);
        else
            move(event.key);
    }

    Ui.LogicalCanvas {
        anchors.fill: parent
        Item {
            anchors.fill: parent
            Rectangle {
                anchors.fill: parent
                color: Ui.Theme.background
            }
            Ui.UiText {
                x: Ui.Theme.safeLeft
                y: 48
                width: parent.width - Ui.Theme.safeLeft - Ui.Theme.safeRight
                role: "title"
                text: qsTr("Browser")
                maximumLineCount: 1
                elide: Text.ElideRight
            }

            Column {
                id: listColumn
                x: Ui.Theme.safeLeft
                y: 160
                width: parent.width - Ui.Theme.safeLeft - Ui.Theme.safeRight
                spacing: 16
                visible: root.showTabs

                Ui.UiText { width: parent.width; role: "section"; text: qsTr("Windows"); maximumLineCount: 1 }
                Row {
                    spacing: 24
                    Repeater {
                        id: windowRepeater
                        model: root.showTabs ? root.windowIds : []
                        delegate: Ui.ActionButton {
                            required property string modelData
                            required property int index
                            objectName: "window-" + modelData
                            text: qsTr("Window %1").arg(index + 1)
                            activeFocusOnTab: false
                            pointerMode: root.pointerMode
                            hoverFocusEnabled: root.hoverFocusEnabled
                            Keys.priority: Keys.BeforeItem
                            Keys.onPressed: event => root.trapTab(event)
                            onActiveFocusChanged: if (activeFocus) root.noteWindowFocus(index)
                            onDpadRequested: root.pointerMode = false
                            Rectangle {
                                objectName: "windowMark-" + modelData
                                anchors.left: parent.left
                                anchors.right: parent.right
                                anchors.bottom: parent.bottom
                                height: 4
                                radius: 2
                                color: Ui.Theme.ember
                                visible: root.focusedWindowId === modelData
                            }
                        }
                    }
                }

                Ui.UiText { width: parent.width; role: "section"; text: qsTr("Tabs"); maximumLineCount: 1 }
                Item {
                    width: parent.width
                    height: 250
                    clip: true
                    Row {
                        spacing: 32
                        Repeater {
                            id: tabRepeater
                            model: root.showTabs ? root.visibleTabs : []
                            delegate: Ui.ContentCard {
                                required property var modelData
                                required property int index
                                objectName: "tab-" + modelData.id
                                width: 400
                                height: 225
                                text: modelData.title
                                subtitle: modelData.url
                                activeFocusOnTab: false
                                pointerMode: root.pointerMode
                                hoverFocusEnabled: root.hoverFocusEnabled
                                Keys.priority: Keys.BeforeItem
                                Keys.onPressed: event => root.trapTab(event)
                                onActiveFocusChanged: if (activeFocus) {
                                    root.focusRow = "tabs";
                                    root.tabIndex = index;
                                    root.focusedTabId = modelData.id;
                                }
                                onActivated: root.activateTab(modelData.id)
                                onDpadRequested: root.pointerMode = false
                                Ui.UiText {
                                    objectName: "active-" + modelData.id
                                    x: 24
                                    y: 20
                                    z: 50
                                    role: "caption"
                                    text: modelData.active ? qsTr("Active") : ""
                                    color: Ui.Theme.ember
                                    visible: modelData.active
                                }
                                Ui.UiText {
                                    objectName: "audible-" + modelData.id
                                    anchors.right: parent.right
                                    anchors.top: parent.top
                                    anchors.rightMargin: 24
                                    anchors.topMargin: 20
                                    z: 50
                                    role: "caption"
                                    text: modelData.audible ? qsTr("Audible") : ""
                                    color: Ui.Theme.textSecondary
                                    visible: modelData.audible
                                }
                            }
                        }
                    }
                }

                Row {
                    spacing: 24
                    Ui.ActionButton {
                        id: activateButton
                        objectName: "activateTab"
                        text: qsTr("Activate")
                        activeFocusOnTab: false
                        pointerMode: root.pointerMode
                        hoverFocusEnabled: root.hoverFocusEnabled
                        Keys.priority: Keys.BeforeItem
                        Keys.onPressed: event => root.trapTab(event)
                        onActiveFocusChanged: if (activeFocus) {
                            root.focusRow = "actions";
                            root.actionIndex = 0;
                        }
                        onActivated: if (root.focusedTabId.length) root.activateTab(root.focusedTabId)
                        onDpadRequested: root.pointerMode = false
                    }
                    Ui.ActionButton {
                        id: closeButton
                        objectName: "closeTab"
                        text: qsTr("Close")
                        destructive: true
                        activeFocusOnTab: false
                        pointerMode: root.pointerMode
                        hoverFocusEnabled: root.hoverFocusEnabled
                        Keys.priority: Keys.BeforeItem
                        Keys.onPressed: event => root.trapTab(event)
                        onActiveFocusChanged: if (activeFocus) {
                            root.focusRow = "actions";
                            root.actionIndex = 1;
                        }
                        onActivated: if (root.focusedTabId.length) root.closeTab(root.focusedTabId)
                        onDpadRequested: root.pointerMode = false
                    }
                }
            }

            Ui.StatusPane {
                id: statusPane
                objectName: "browserStatus"
                anchors.horizontalCenter: parent.horizontalCenter
                y: 200
                width: 900
                visible: !root.showTabs
                kind: root.statusKind
                title: root.resolvedTitle
                description: root.resolvedDescription
                actionText: root.statusActionText
                pointerMode: root.pointerMode
                hoverFocusEnabled: root.hoverFocusEnabled
                onActionRequested: root.retryRequested()
                onDpadRequested: root.pointerMode = false
                Keys.priority: Keys.BeforeItem
                Keys.onPressed: event => root.trapTab(event)
            }

            Column {
                x: Ui.Theme.safeLeft
                y: 860
                width: parent.width - Ui.Theme.safeLeft - Ui.Theme.safeRight
                spacing: 12
                visible: root.shortcutEntries.length > 0 && root.browserState !== "loading"
                Ui.UiText { width: parent.width; role: "section"; text: qsTr("Shortcuts"); maximumLineCount: 1 }
                Row {
                    spacing: 24
                    Repeater {
                        id: shortcutRepeater
                        model: root.browserState === "loading" ? [] : root.shortcutEntries
                        delegate: Ui.ActionButton {
                            required property var modelData
                            required property int index
                            objectName: "shortcut-" + modelData.id
                            text: modelData.title
                            activeFocusOnTab: false
                            pointerMode: root.pointerMode
                            hoverFocusEnabled: root.hoverFocusEnabled
                            Keys.priority: Keys.BeforeItem
                            Keys.onPressed: event => root.trapTab(event)
                            onActiveFocusChanged: if (activeFocus) {
                                root.focusRow = "shortcuts";
                                root.shortcutIndex = index;
                            }
                            onActivated: root.requestOpen(modelData.id)
                            onDpadRequested: root.pointerMode = false
                        }
                    }
                }
            }
        }
    }
}
