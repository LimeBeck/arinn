// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import "../../" as Ui

// Open services list. One row per tab from the caller. The active tab of the
// focused window is on screen and is ordered first. OK on a row activates it.
// OK on its close control closes it. This screen does not talk to a daemon.
FocusScope {
    id: root
    width: 1920
    height: 1080

    property var tabs: []
    property var windows: []
    property var shortcuts: []
    property bool interactive: true
    property string browserState: "populated" // populated, empty, loading, error, unavailable
    property string operationState: ""
    property string saveError: ""
    readonly property bool saveEnabled: browserState !== "unavailable"
    readonly property string saveErrorText: {
        if (saveError === "InvalidArgument")
            return qsTr("This address cannot be saved.");
        if (saveError === "Conflict")
            return qsTr("This page is already saved.");
        if (saveError === "NotFound")
            return qsTr("This page is no longer open.");
        return saveError.length ? qsTr("The page could not be saved.") : "";
    }
    property string statusTitle: ""
    property string statusDescription: ""
    property bool pointerMode: false
    property bool hoverFocusEnabled: true

    readonly property bool showTabs: browserState === "populated" && orderedTabs.length > 0
    readonly property string statusKind: {
        if (showTabs)
            return "populated";
        if (browserState === "populated" || browserState === "empty")
            return "empty";
        if (browserState === "loading" || browserState === "error" || browserState === "unavailable")
            return browserState;
        return "unavailable";
    }
    readonly property string statusActionText: {
        if (showTabs || statusKind === "loading")
            return "";
        return statusKind === "empty" ? qsTr("Go to Home") : qsTr("Retry");
    }
    readonly property var stateCopy: ({
        empty: [qsTr("Nothing open"), qsTr("Services you open from Home will appear here.")],
        loading: [qsTr("Loading tabs"), qsTr("The tab list is loading.")],
        error: [qsTr("Tabs unavailable"), qsTr("The tab list could not be loaded.")],
        unavailable: [qsTr("Browser unavailable"), qsTr("Online services cannot be opened right now. Library, downloads and games still work.")]
    })
    readonly property string resolvedTitle: statusTitle.length ? statusTitle : (stateCopy[statusKind] ? stateCopy[statusKind][0] : "")
    readonly property string resolvedDescription: statusDescription.length ? statusDescription : (stateCopy[statusKind] ? stateCopy[statusKind][1] : "")
    readonly property bool listOverflow: showTabs && services.contentHeight > services.height + 1

    property var presentationTabs: []
    property var orderedTabs: []
    property var windowIds: []
    property var shortcutEntries: []
    property string activeTabId: ""
    property string focusedWindowId: ""
    property string focusedEntryId: ""
    property string closingId: ""
    property string focusRow: "list"
    property string listSide: "row"
    property int listIndex: 0
    property string announcedSide: ""
    property int announcedIndex: -1
    property int repeatRun: 0

    signal activateTab(string tabId)
    signal closeTab(string tabId)
    signal savePage(string tabId)
    signal saveErrorDismissed()
    signal open(string shortcutId)
    signal retryRequested()
    signal backRequested()
    signal browseRequested()

    function hostOf(url) {
        const value = url !== undefined && url !== null ? String(url) : "";
        const stripped = value.replace(/^[a-z][a-z0-9+.-]*:\/\//i, "");
        const cut = stripped.search(/[/?#]/);
        return cut >= 0 ? stripped.slice(0, cut) : stripped;
    }

    function letterOf(url) {
        const host = hostOf(url).replace(/^(www|m|hd)\./i, "");
        return host.length ? host.charAt(0).toUpperCase() : "";
    }

    function chipLetter(tab) {
        if (tab && tab.letter !== undefined && tab.letter !== null && String(tab.letter).length)
            return String(tab.letter);
        return letterOf(tab ? tab.url : "");
    }

    function chipIcon(tab) {
        return tab && tab.icon ? String(tab.icon) : "";
    }

    function displayTitle(tab) {
        const title = tab && tab.title ? String(tab.title) : "";
        return title.length ? title : hostOf(tab ? tab.url : "");
    }

    function displaySub(tab) {
        const title = tab && tab.title ? String(tab.title) : "";
        if (title.length)
            return hostOf(tab ? tab.url : "");
        return tab && tab.url !== undefined && tab.url !== null ? String(tab.url) : "";
    }

    // A C++ QVariantList is array-like in QML. Array.isArray is false for it.
    function asList(input) {
        if (!input)
            return [];
        if (Array.isArray(input))
            return input;
        const length = input.length;
        if (typeof length !== "number")
            return [];
        const result = [];
        for (let i = 0; i < length; ++i)
            result.push(input[i]);
        return result;
    }

    function normalizeTabs(input) {
        const source = asList(input);
        const seen = {};
        const activeWindows = {};
        const result = [];
        for (let i = 0; i < source.length; ++i) {
            const tab = source[i] || {};
            const id = tab.id !== undefined && tab.id !== null ? String(tab.id) : "";
            if (!id || seen[id])
                continue;
            seen[id] = true;
            const windowId = tab.windowId !== undefined && tab.windowId !== null ? String(tab.windowId) : "";
            const active = !!tab.active && !activeWindows[windowId];
            if (active)
                activeWindows[windowId] = true;
            const row = {
                id: id,
                windowId: windowId,
                title: tab.title !== undefined && tab.title !== null ? String(tab.title) : "",
                url: tab.url !== undefined && tab.url !== null ? String(tab.url) : "",
                active: active,
                audible: !!tab.audible,
                saved: !!tab.saved
            };
            if (tab.letter !== undefined && tab.letter !== null)
                row.letter = String(tab.letter);
            if (tab.icon)
                row.icon = String(tab.icon);
            result.push(row);
        }
        return result;
    }

    function normalizeWindows(input) {
        const source = asList(input);
        const seen = {};
        let focusedAssigned = false;
        const result = [];
        for (let i = 0; i < source.length; ++i) {
            const entry = source[i] || {};
            const id = entry.id !== undefined && entry.id !== null ? String(entry.id) : "";
            if (!id || seen[id])
                continue;
            seen[id] = true;
            const focused = !!entry.focused && !focusedAssigned;
            if (focused)
                focusedAssigned = true;
            result.push({id: id, focused: focused});
        }
        return result;
    }

    function normalizeShortcuts(input) {
        const source = asList(input);
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

    function onScreenTab(nextTabs, windowId) {
        for (let i = 0; i < nextTabs.length; ++i) {
            if (nextTabs[i].windowId === windowId && nextTabs[i].active)
                return nextTabs[i];
        }
        for (let i = 0; i < nextTabs.length; ++i) {
            if (nextTabs[i].active)
                return nextTabs[i];
        }
        return null;
    }

    function orderTabs(nextTabs, onScreenId) {
        const rest = [];
        let first = null;
        for (let i = 0; i < nextTabs.length; ++i) {
            if (!first && nextTabs[i].id === onScreenId)
                first = nextTabs[i];
            else
                rest.push(nextTabs[i]);
        }
        return first ? [first].concat(rest) : rest;
    }

    function rebuild() {
        const nextTabs = normalizeTabs(tabs);
        const explicit = normalizeWindows(windows);
        const ids = [];
        const seen = {};
        const source = explicit.length ? explicit.map(entry => entry.id) : nextTabs.map(tab => tab.windowId);
        for (let i = 0; i < source.length; ++i) {
            if (!source[i] || seen[source[i]])
                continue;
            seen[source[i]] = true;
            ids.push(source[i]);
        }
        for (let i = 0; i < nextTabs.length; ++i) {
            const windowId = nextTabs[i].windowId;
            if (!windowId || seen[windowId])
                continue;
            seen[windowId] = true;
            ids.push(windowId);
        }
        const marked = explicit.find(entry => entry.focused);
        const active = nextTabs.find(tab => tab.active);
        presentationTabs = nextTabs;
        windowIds = ids;
        shortcutEntries = normalizeShortcuts(shortcuts);
        // The service on screen follows the focused window from the model.
        // Moving the highlight does not change it; a later snapshot does.
        const fallback = marked && ids.indexOf(marked.id) >= 0 ? marked.id
                : (active ? active.windowId : (ids.length ? ids[0] : ""));
        focusedWindowId = fallback;
        const onScreen = onScreenTab(nextTabs, fallback);
        activeTabId = onScreen ? onScreen.id : "";
        const ordered = orderTabs(nextTabs, activeTabId);
        orderedTabs = ordered;
        if (closingId.length && !ordered.some(tab => tab.id === closingId))
            closingId = "";
        if (!ordered.some(tab => tab.id === focusedEntryId)) {
            focusedEntryId = activeTabId || (ordered.length ? ordered[0].id : "");
            listSide = "row";
        }
        listIndex = Math.max(0, ordered.findIndex(tab => tab.id === focusedEntryId));
        syncRows();
    }

    // Update the list model in place, keyed by tab id: a changed field updates
    // its row and a reorder moves rows, so delegates and their focus survive
    // snapshot updates. Replacing the whole model rebuilt every row on each
    // update and froze the screen with many tabs (#200).
    function syncRows() {
        const next = showTabs ? orderedTabs : [];
        const wanted = {};
        for (let i = 0; i < next.length; ++i)
            wanted[next[i].id] = true;
        for (let i = serviceRows.count - 1; i >= 0; --i) {
            if (!wanted[serviceRows.get(i).rowId])
                serviceRows.remove(i);
        }
        for (let i = 0; i < next.length; ++i) {
            const id = next[i].id;
            const json = JSON.stringify(next[i]);
            if (i >= serviceRows.count || serviceRows.get(i).rowId !== id) {
                let from = -1;
                for (let k = i + 1; k < serviceRows.count; ++k) {
                    if (serviceRows.get(k).rowId === id) {
                        from = k;
                        break;
                    }
                }
                if (from < 0) {
                    serviceRows.insert(i, { rowId: id, rowJson: json });
                    continue;
                }
                serviceRows.move(from, i, 1);
            }
            if (serviceRows.get(i).rowJson !== json)
                serviceRows.setProperty(i, "rowJson", json);
        }
        // Create delegates for inserted rows now, as a model reset did, so
        // focus can move to them in the same turn.
        services.forceLayout();
    }

    function settleFocus() {
        if (!interactive)
            return;
        // Nothing to do when the focused row is still the one to focus.
        if (showTabs && focusRow === "list") {
            const current = itemFor(listIndex, listSide);
            if (current && current.activeFocus)
                return;
        }
        muteTabStops(root);
        if (showTabs)
            focusEntry(listIndex, listSide);
        else if (statusActionText.length)
            focusStatus();
        else
            Ui.Focus.take(root);
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

    function itemFor(index, side) {
        const entry = services.itemAtIndex(index);
        if (!entry || index < 0 || index >= orderedTabs.length)
            return null;
        const id = orderedTabs[index].id;
        const prefix = side === "close" ? "close-" : side === "bookmark" ? "bookmark-" : "row-";
        return findNamed(entry, prefix + id);
    }

    function focusWhenReady(index, side, tries) {
        const item = itemFor(index, side);
        if (item) {
            Ui.Focus.take(item);
            reveal(index);
            return;
        }
        if (tries > 0)
            Qt.callLater(() => focusWhenReady(index, side, tries - 1));
    }

    function focusEntry(index, side) {
        if (!showTabs || !orderedTabs.length)
            return false;
        index = Math.max(0, Math.min(index, orderedTabs.length - 1));
        if (side !== "close" && side !== "bookmark")
            side = "row";
        listIndex = index;
        listSide = side;
        focusRow = "list";
        focusedEntryId = orderedTabs[index].id;
        focusWhenReady(index, side, 8);
        return true;
    }

    function focusStatus() {
        focusRow = "status";
        const apply = tries => {
            const item = findNamed(messagePane, "statusAction");
            if (item && item.visible) {
                Ui.Focus.take(item);
                return;
            }
            if (tries > 0)
                Qt.callLater(() => apply(tries - 1));
            else
                Ui.Focus.take(root);
        };
        apply(8);
    }

    // Keep the focused row about two row-heights above the bottom edge.
    function reveal(index) {
        const entry = services.itemAtIndex(index);
        if (!entry)
            return;
        const marginBelow = 288;
        const marginAbove = 144;
        const top = entry.y;
        const bottom = entry.y + entry.height;
        let y = services.contentY;
        const limit = Math.max(0, services.contentHeight - services.height);
        if (bottom > y + services.height - marginBelow)
            y = bottom - (services.height - marginBelow);
        if (top < y + marginAbove)
            y = top - marginAbove;
        y = Math.max(0, Math.min(y, limit));
        if (Math.abs(services.contentY - y) > 0.5)
            services.contentY = y;
    }

    function stepCount(autoRepeat) {
        if (!autoRepeat) {
            repeatRun = 0;
            return 1;
        }
        repeatRun += 1;
        if (repeatRun >= 8)
            return 3;
        if (repeatRun >= 4)
            return 2;
        return 1;
    }

    function move(key, autoRepeat) {
        pointerMode = false;
        if (key === Qt.Key_Left || key === Qt.Key_Right)
            repeatRun = 0;
        if (!showTabs) {
            if (statusActionText.length)
                focusStatus();
            else
                Ui.Focus.take(root);
            return;
        }
        const order = ["row", "bookmark", "close"];
        let at = order.indexOf(listSide);
        if (at < 0)
            at = 0;
        if (key === Qt.Key_Left) {
            focusEntry(listIndex, order[Math.max(0, at - 1)]);
            return;
        }
        if (key === Qt.Key_Right) {
            focusEntry(listIndex, order[Math.min(order.length - 1, at + 1)]);
            return;
        }
        const direction = key === Qt.Key_Up ? -1 : 1;
        const next = listIndex + direction * stepCount(!!autoRepeat);
        focusEntry(next, listSide);
    }

    function isBackKey(key) {
        return key === Qt.Key_Escape || key === Qt.Key_Backspace || key === Qt.Key_Back;
    }

    function localKey(event) {
        if (event.key === Qt.Key_Up && !browseButton.activeFocus && ((!showTabs) || listIndex === 0)) {
            Ui.Focus.take(browseButton); event.accepted = true; return;
        }
        if (isBackKey(event.key)) {
            event.accepted = true;
            if (!event.isAutoRepeat)
                backRequested();
            return;
        }
        if (event.key === Qt.Key_Tab || event.key === Qt.Key_Backtab
                || event.key === Qt.Key_Left || event.key === Qt.Key_Right
                || event.key === Qt.Key_Up || event.key === Qt.Key_Down) {
            event.accepted = true;
            const key = event.key === Qt.Key_Tab ? Qt.Key_Right
                    : event.key === Qt.Key_Backtab ? Qt.Key_Left : event.key;
            move(key, event.isAutoRepeat);
        }
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

    function requestSave(tabId) {
        if (!saveEnabled || !tabId)
            return;
        const tab = orderedTabs.find(entry => entry.id === tabId);
        if (!tab || tab.saved)
            return;
        savePage(tabId);
    }

    function requestOpen(shortcutId) {
        for (let i = 0; i < shortcutEntries.length; ++i) {
            if (shortcutEntries[i].id === shortcutId) {
                open(shortcutId);
                return;
            }
        }
    }

    function beginClose(tabId) {
        if (!tabId)
            return;
        const already = closingId === tabId;
        closingId = tabId;
        if (!already)
            closeTab(tabId);
        const index = orderedTabs.findIndex(tab => tab.id === tabId);
        let next = index;
        if (index >= 0 && index + 1 < orderedTabs.length)
            next = index + 1;
        else if (index > 0)
            next = index - 1;
        focusEntry(Math.max(0, next), "row");
    }

    function activateStatus() {
        if (statusKind === "empty")
            backRequested();
        else if (statusKind === "error" || statusKind === "unavailable")
            retryRequested();
    }

    function noteRowFocus(index, side) {
        const moved = announcedSide !== side || announcedIndex !== index;
        announcedSide = side;
        announcedIndex = index;
        focusRow = "list";
        listSide = side;
        listIndex = index;
        if (index >= 0 && index < orderedTabs.length)
            focusedEntryId = orderedTabs[index].id;
        if (moved && saveError.length)
            saveErrorDismissed();
    }

    onTabsChanged: {
        const keepFocus = activeFocus;
        rebuild();
        if (interactive && keepFocus)
            Qt.callLater(settleFocus);
    }
    onWindowsChanged: {
        const keepFocus = activeFocus;
        rebuild();
        if (interactive && keepFocus)
            Qt.callLater(settleFocus);
    }
    onShortcutsChanged: {
        const keepFocus = activeFocus;
        rebuild();
        if (interactive && keepFocus)
            Qt.callLater(settleFocus);
    }
    onBrowserStateChanged: Qt.callLater(settleFocus)
    onShowTabsChanged: syncRows()
    onInteractiveChanged: if (interactive) Qt.callLater(settleFocus)
    onOperationStateChanged: {
        if (!closingId.length)
            return;
        if (operationState === "" || operationState === "queued" || operationState === "running" || operationState === "succeeded")
            return;
        if (orderedTabs.some(tab => tab.id === closingId))
            closingId = "";
    }
    Component.onCompleted: {
        rebuild();
        Qt.callLater(settleFocus);
    }

    Keys.onPressed: event => root.localKey(event)

    Ui.LogicalCanvas {
        anchors.fill: parent
        Item {
            anchors.fill: parent
            Rectangle {
                anchors.fill: parent
                color: Ui.Theme.background
            }

            Text {
                id: clock
                objectName: "browserClock"
                y: 44
                height: 64
                anchors.right: parent.right
                anchors.rightMargin: 96
                verticalAlignment: Text.AlignVCenter
                font.family: Ui.Theme.fontFamily
                font.pixelSize: 30
                font.weight: 500
                font.features: ({"tnum": 1})
                color: Ui.Theme.textSecondary
                text: Qt.formatTime(new Date(), "hh:mm")
                Timer {
                    interval: 30000
                    running: true
                    repeat: true
                    triggeredOnStart: true
                    onTriggered: clock.text = Qt.formatTime(new Date(), "hh:mm")
                }
            }

            Ui.ActionButton {
                id: browseButton
                objectName: "browserBrowse"
                x: 1220; y: 96; height: 80
                text: qsTr("Site or search")
                onActivated: root.browseRequested()
                Keys.onDownPressed: root.settleFocus()
            }
            Item {
                x: 120
                y: 96
                width: parent.width - 120 - 760
                height: 72
                Ui.UiText {
                    id: screenTitle
                    objectName: "screenTitle"
                    text: qsTr("Open services")
                    role: "title"
                    maximumLineCount: 1
                }
                Ui.UiText {
                    objectName: "saveError"
                    visible: root.saveErrorText.length > 0
                    anchors.baseline: screenTitle.baseline
                    anchors.right: parent.right
                    anchors.rightMargin: listPosition.visible ? listPosition.implicitWidth + 24 : 0
                    width: Math.min(implicitWidth, Math.max(0, parent.width - screenTitle.width - 160 - anchors.rightMargin))
                    role: "meta"
                    color: Ui.Theme.error
                    text: root.saveErrorText
                    maximumLineCount: 1
                    elide: Text.ElideRight
                }
                Text {
                    objectName: "serviceCount"
                    visible: root.showTabs
                    x: screenTitle.width + 20
                    anchors.baseline: screenTitle.baseline
                    text: String(root.orderedTabs.length)
                    font.family: Ui.Theme.fontFamily
                    font.pixelSize: 40
                    font.weight: 600
                    font.features: ({"tnum": 1})
                    color: Ui.Theme.textHint
                }
                Ui.UiText {
                    id: listPosition
                    objectName: "listPosition"
                    visible: root.listOverflow
                    anchors.baseline: screenTitle.baseline
                    anchors.right: parent.right
                    role: "meta"
                    color: Ui.Theme.textSecondary
                    text: qsTr("%1 of %2").arg(root.listIndex + 1).arg(root.orderedTabs.length)
                }
            }

            ListView {
                id: services
                objectName: "serviceList"
                x: 96
                y: 196
                width: 1548
                height: 734
                visible: root.showTabs
                clip: true
                interactive: false
                keyNavigationEnabled: false
                highlightFollowsCurrentItem: false
                reuseItems: false
                model: ListModel { id: serviceRows }
                spacing: 16
                topMargin: 14
                bottomMargin: 14
                leftMargin: 24
                rightMargin: 24
                displaced: Transition {
                    NumberAnimation { properties: "y"; duration: Ui.Theme.focusDuration }
                }
                delegate: Item {
                    id: entry
                    required property string rowJson
                    required property int index
                    readonly property var modelData: JSON.parse(rowJson)
                    objectName: "entry-" + modelData.id
                    Component.onCompleted: root.muteTabStops(entry)
                    width: ListView.view.width - 48
                    height: Math.max(plate.height, bookmarkBtn.height, closeBtn.height)
                    opacity: root.closingId === modelData.id ? 0.5 : 1
                    z: plate.activeFocus || bookmarkBtn.activeFocus || closeBtn.activeFocus ? 3 : 1
                    Behavior on opacity { NumberAnimation { duration: Ui.Theme.focusDuration } }

                    // Only a visible list animates a removed row; leaving the
                    // list (empty, error) drops the rows at once.
                    ListView.onRemove: if (root.showTabs) removeAnim.start()
                    SequentialAnimation {
                        id: removeAnim
                        PropertyAction { target: entry; property: "ListView.delayRemove"; value: true }
                        ParallelAnimation {
                            NumberAnimation { target: entry; property: "opacity"; to: 0; duration: Ui.Theme.focusDuration }
                            NumberAnimation { target: entry; property: "height"; to: 0; duration: Ui.Theme.focusDuration }
                        }
                        PropertyAction { target: entry; property: "ListView.delayRemove"; value: false }
                    }

                    Item {
                        id: plate
                        objectName: "row-" + modelData.id
                        width: entry.width - 288
                        height: Math.max(128, textCol.implicitHeight + 44)
                        anchors.verticalCenter: parent.verticalCenter
                        scale: activeFocus ? (pressed ? 0.98 : 1.02) : 1
                        activeFocusOnTab: false
                        property bool pressed: false

                        Behavior on scale {
                            NumberAnimation { duration: plate.pressed ? Ui.Theme.pressDuration : Ui.Theme.focusDuration }
                        }
                        Rectangle {
                            anchors.fill: parent
                            radius: 26
                            color: plate.activeFocus ? Ui.Theme.plateFocus : Ui.Theme.plate
                            Behavior on color { ColorAnimation { duration: Ui.Theme.focusDuration } }
                        }
                        Rectangle {
                            anchors.left: parent.left
                            anchors.right: parent.right
                            anchors.top: parent.top
                            height: 1
                            color: Qt.rgba(1, 1, 1, 0.08)
                            visible: !plate.activeFocus
                        }
                        Rectangle {
                            anchors.fill: parent
                            radius: 26
                            color: "transparent"
                            border.width: 3
                            border.color: Ui.Theme.ring
                            opacity: plate.activeFocus ? 1 : 0
                        }
                        Item {
                            objectName: "chip-" + modelData.id
                            x: 28
                            width: 76
                            height: 76
                            anchors.verticalCenter: parent.verticalCenter
                            readonly property string iconSource: root.chipIcon(modelData)
                            Rectangle {
                                anchors.fill: parent
                                radius: 22
                                color: Qt.rgba(1, 1, 1, 0.12)
                            }
                            Image {
                                id: chipImage
                                anchors.fill: parent
                                anchors.margins: 8
                                fillMode: Image.PreserveAspectFit
                                asynchronous: true
                                source: {
                                    const icon = parent.iconSource;
                                    if (icon.startsWith("/") || icon.startsWith("file:") || icon.startsWith("qrc:") || icon.startsWith("image:"))
                                        return icon;
                                    return "";
                                }
                                visible: status === Image.Ready
                            }
                            Ui.UiText {
                                objectName: "letter-" + modelData.id
                                anchors.centerIn: parent
                                visible: !chipImage.visible
                                text: root.chipLetter(modelData)
                                font.pixelSize: 34
                                font.weight: 700
                                color: Ui.Theme.textPrimary
                            }
                        }
                        Column {
                            id: textCol
                            x: 128
                            width: parent.width - 156
                            anchors.verticalCenter: parent.verticalCenter
                            spacing: 8
                            Ui.UiText {
                                id: titleText
                                objectName: "title-" + modelData.id
                                width: textCol.width
                                text: root.displayTitle(modelData)
                                role: "section"
                                lineHeight: 1.25
                                lineHeightMode: Text.ProportionalHeight
                                wrapMode: Text.WordWrap
                                maximumLineCount: plate.activeFocus ? 2 : 1
                                elide: Text.ElideRight
                            }
                            Row {
                                id: hostLine
                                width: textCol.width
                                spacing: 24
                                Ui.UiText {
                                    objectName: "sub-" + modelData.id
                                    width: Math.max(0, hostLine.width - statusMarks.implicitWidth - (statusMarks.implicitWidth > 0 ? hostLine.spacing : 0))
                                    text: root.displaySub(modelData)
                                    role: "caption"
                                    color: Ui.Theme.textSecondary
                                    elide: Text.ElideRight
                                    maximumLineCount: 1
                                    wrapMode: Text.NoWrap
                                    anchors.verticalCenter: parent.verticalCenter
                                }
                                Row {
                                    id: statusMarks
                                    spacing: 24
                                    anchors.verticalCenter: parent.verticalCenter
                                    Row {
                                        objectName: "onscreen-" + modelData.id
                                        spacing: 10
                                        visible: modelData.id === root.activeTabId
                                        anchors.verticalCenter: parent.verticalCenter
                                        Rectangle {
                                            width: 12
                                            height: 12
                                            radius: 6
                                            color: Ui.Theme.ember
                                            anchors.verticalCenter: parent.verticalCenter
                                        }
                                        Ui.UiText {
                                            text: qsTr("On screen")
                                            role: "caption"
                                            color: Ui.Theme.textPrimary
                                            anchors.verticalCenter: parent.verticalCenter
                                        }
                                    }
                                    Row {
                                        objectName: "sound-" + modelData.id
                                        spacing: 8
                                        visible: !!modelData.audible
                                        anchors.verticalCenter: parent.verticalCenter
                                        Rectangle {
                                            width: 8
                                            height: 14
                                            radius: 2
                                            color: Ui.Theme.ember
                                            anchors.verticalCenter: parent.verticalCenter
                                        }
                                        Rectangle {
                                            width: 4
                                            height: 22
                                            radius: 2
                                            color: Ui.Theme.ember
                                            anchors.verticalCenter: parent.verticalCenter
                                        }
                                        Ui.UiText {
                                            text: qsTr("Sound")
                                            role: "caption"
                                            color: Ui.Theme.textPrimary
                                            anchors.verticalCenter: parent.verticalCenter
                                        }
                                    }
                                    Ui.UiText {
                                        objectName: "mark-" + modelData.id
                                        visible: !!modelData.saved
                                        text: qsTr("Shortcut")
                                        role: "caption"
                                        color: Ui.Theme.textPrimary
                                        anchors.verticalCenter: parent.verticalCenter
                                    }
                                    Ui.UiText {
                                        objectName: "closing-" + modelData.id
                                        visible: root.closingId === modelData.id
                                        text: qsTr("Closing…")
                                        role: "caption"
                                        color: Ui.Theme.textPrimary
                                        anchors.verticalCenter: parent.verticalCenter
                                    }
                                }
                            }
                        }
                        Keys.priority: Keys.BeforeItem
                        Keys.onPressed: event => {
                            if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) {
                                if (!event.isAutoRepeat)
                                    pressed = true;
                                event.accepted = true;
                                return;
                            }
                            root.localKey(event);
                        }
                        Keys.onReleased: event => {
                            if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) {
                                const go = pressed && !event.isAutoRepeat;
                                pressed = false;
                                if (go)
                                    root.activateTab(modelData.id);
                                event.accepted = true;
                            }
                        }
                        onActiveFocusChanged: if (activeFocus) root.noteRowFocus(index, "row")
                        TapHandler { onTapped: root.activateTab(modelData.id) }
                        HoverHandler {
                            onHoveredChanged: if (hovered && root.pointerMode && root.hoverFocusEnabled) Ui.Focus.take(plate)
                        }
                    }

                    Item {
                        id: bookmarkBtn
                        objectName: "bookmark-" + modelData.id
                        x: plate.width + 16
                        width: 128
                        height: 128
                        anchors.verticalCenter: parent.verticalCenter
                        enabled: root.saveEnabled
                        scale: activeFocus ? (pressed ? 0.98 : 1.06) : 1
                        activeFocusOnTab: false
                        property bool pressed: false
                        readonly property bool saved: !!modelData.saved
                        readonly property string iconName: saved ? "bookmark_added" : "bookmark"

                        Behavior on scale {
                            NumberAnimation { duration: bookmarkBtn.pressed ? Ui.Theme.pressDuration : Ui.Theme.focusDuration }
                        }
                        Rectangle {
                            anchors.fill: parent
                            anchors.topMargin: 18
                            radius: 26
                            color: Qt.rgba(0, 0, 0, bookmarkBtn.activeFocus ? 0.45 : 0)
                            z: -1
                        }
                        Rectangle {
                            anchors.fill: parent
                            radius: 26
                            color: bookmarkBtn.activeFocus ? Ui.Theme.textPrimary : Ui.Theme.plate
                            opacity: bookmarkBtn.enabled ? 1 : 0.4
                            Behavior on color { ColorAnimation { duration: Ui.Theme.focusDuration } }
                        }
                        Item {
                            anchors.centerIn: parent
                            width: 28
                            height: 36
                            Rectangle {
                                anchors.fill: parent
                                radius: 3
                                color: bookmarkBtn.saved ? Ui.Theme.ember : "transparent"
                                border.width: bookmarkBtn.saved ? 0 : 3
                                border.color: bookmarkBtn.activeFocus ? Ui.Theme.ink : Ui.Theme.textSecondary
                            }
                            Rectangle {
                                visible: bookmarkBtn.saved
                                width: 14
                                height: 3
                                radius: 1
                                rotation: 40
                                x: 4
                                y: 18
                                color: Ui.Theme.ink
                            }
                            Rectangle {
                                visible: bookmarkBtn.saved
                                width: 8
                                height: 3
                                radius: 1
                                rotation: -40
                                x: 12
                                y: 20
                                color: Ui.Theme.ink
                            }
                        }
                        Keys.priority: Keys.BeforeItem
                        Keys.onPressed: event => {
                            if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) {
                                if (!event.isAutoRepeat && bookmarkBtn.enabled && !bookmarkBtn.saved)
                                    pressed = true;
                                event.accepted = true;
                                return;
                            }
                            root.localKey(event);
                        }
                        Keys.onReleased: event => {
                            if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) {
                                const go = pressed && !event.isAutoRepeat;
                                pressed = false;
                                if (go)
                                    root.requestSave(modelData.id);
                                event.accepted = true;
                            }
                        }
                        onActiveFocusChanged: if (activeFocus) root.noteRowFocus(index, "bookmark")
                        TapHandler {
                            enabled: bookmarkBtn.enabled && !bookmarkBtn.saved
                            onTapped: root.requestSave(modelData.id)
                        }
                        HoverHandler {
                            onHoveredChanged: if (hovered && bookmarkBtn.enabled && root.pointerMode && root.hoverFocusEnabled) Ui.Focus.take(bookmarkBtn)
                        }
                    }

                    Item {
                        id: closeBtn
                        objectName: "close-" + modelData.id
                        x: plate.width + 160
                        width: 128
                        height: 128
                        anchors.verticalCenter: parent.verticalCenter
                        scale: activeFocus ? (pressed ? 0.98 : 1.06) : 1
                        activeFocusOnTab: false
                        property bool pressed: false
                        readonly property bool closing: root.closingId === modelData.id

                        Behavior on scale {
                            NumberAnimation { duration: closeBtn.pressed ? Ui.Theme.pressDuration : Ui.Theme.focusDuration }
                        }
                        Rectangle {
                            anchors.fill: parent
                            anchors.topMargin: 18
                            radius: 26
                            color: Qt.rgba(0, 0, 0, closeBtn.activeFocus ? 0.45 : 0)
                            z: -1
                        }
                        Rectangle {
                            anchors.fill: parent
                            radius: 26
                            color: closeBtn.activeFocus ? Ui.Theme.textPrimary : Ui.Theme.plate
                            Behavior on color { ColorAnimation { duration: Ui.Theme.focusDuration } }
                        }
                        Ui.UiText {
                            anchors.centerIn: parent
                            visible: !closeBtn.closing
                            text: "×"
                            font.pixelSize: 44
                            color: closeBtn.activeFocus ? Ui.Theme.ink : Ui.Theme.textSecondary
                        }
                        Item {
                            anchors.centerIn: parent
                            width: 44
                            height: 44
                            visible: closeBtn.closing
                            RotationAnimation on rotation {
                                from: 0
                                to: 360
                                duration: 900
                                loops: Animation.Infinite
                                running: closeBtn.closing
                            }
                            Rectangle {
                                anchors.fill: parent
                                radius: 22
                                color: "transparent"
                                border.width: 4
                                border.color: Qt.rgba(1, 1, 1, 0.18)
                            }
                            Rectangle {
                                x: 20
                                y: 2
                                width: 4
                                height: 14
                                radius: 2
                                color: Ui.Theme.textSecondary
                            }
                        }
                        Keys.priority: Keys.BeforeItem
                        Keys.onPressed: event => {
                            if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) {
                                if (!event.isAutoRepeat)
                                    pressed = true;
                                event.accepted = true;
                                return;
                            }
                            root.localKey(event);
                        }
                        Keys.onReleased: event => {
                            if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) {
                                const go = pressed && !event.isAutoRepeat;
                                pressed = false;
                                if (go)
                                    root.beginClose(modelData.id);
                                event.accepted = true;
                            }
                        }
                        onActiveFocusChanged: if (activeFocus) root.noteRowFocus(index, "close")
                        TapHandler { onTapped: root.beginClose(modelData.id) }
                        HoverHandler {
                            onHoveredChanged: if (hovered && root.pointerMode && root.hoverFocusEnabled) Ui.Focus.take(closeBtn)
                        }
                    }
                }
            }

            Rectangle {
                z: 2
                x: 96
                y: 196
                width: 1548
                height: 60
                visible: root.listOverflow
                gradient: Gradient {
                    GradientStop { position: 0.0; color: Ui.Theme.background }
                    GradientStop { position: 1.0; color: "transparent" }
                }
            }
            Rectangle {
                z: 2
                x: 96
                y: 196 + 734 - 90
                width: 1548
                height: 90
                visible: root.listOverflow
                gradient: Gradient {
                    GradientStop { position: 0.0; color: "transparent" }
                    GradientStop { position: 1.0; color: Ui.Theme.background }
                }
            }

            Row {
                x: 120
                anchors.bottom: parent.bottom
                anchors.bottomMargin: 64
                spacing: 44
                visible: root.showTabs
                Row {
                    spacing: 12
                    Rectangle {
                        radius: 10
                        color: Qt.rgba(1, 1, 1, 0.1)
                        width: okKey.implicitWidth + 24
                        height: 36
                        Ui.UiText {
                            id: okKey
                            anchors.centerIn: parent
                            text: qsTr("OK")
                            role: "caption"
                            font.weight: 600
                            color: Ui.Theme.textPrimary
                        }
                    }
                    Ui.UiText {
                        text: qsTr("Go to")
                        role: "caption"
                        color: Ui.Theme.textHint
                        anchors.verticalCenter: parent.verticalCenter
                    }
                }
                Row {
                    spacing: 12
                    Rectangle {
                        radius: 10
                        color: Qt.rgba(1, 1, 1, 0.1)
                        width: arrowKey.implicitWidth + 24
                        height: 36
                        Ui.UiText {
                            id: arrowKey
                            anchors.centerIn: parent
                            text: "→"
                            role: "caption"
                            font.weight: 600
                            color: Ui.Theme.textPrimary
                        }
                    }
                    Ui.UiText {
                        text: qsTr("Save")
                        role: "caption"
                        color: Ui.Theme.textHint
                        anchors.verticalCenter: parent.verticalCenter
                    }
                }
                Row {
                    spacing: 12
                    Rectangle {
                        radius: 10
                        color: Qt.rgba(1, 1, 1, 0.1)
                        width: backKey.implicitWidth + 24
                        height: 36
                        Ui.UiText {
                            id: backKey
                            anchors.centerIn: parent
                            text: qsTr("Back")
                            role: "caption"
                            font.weight: 600
                            color: Ui.Theme.textPrimary
                        }
                    }
                    Ui.UiText {
                        text: qsTr("Exit")
                        role: "caption"
                        color: Ui.Theme.textHint
                        anchors.verticalCenter: parent.verticalCenter
                    }
                }
            }

            Column {
                id: messagePane
                objectName: "browserStatus"
                property string kind: root.statusKind
                property string title: root.resolvedTitle
                property string description: root.resolvedDescription
                x: 120
                y: 360
                width: 980
                spacing: 24
                visible: !root.showTabs
                Item {
                    objectName: "statusIcon"
                    width: 88
                    height: 88
                    Column {
                        visible: root.statusKind === "empty"
                        anchors.verticalCenter: parent.verticalCenter
                        spacing: 10
                        Rectangle { width: 72; height: 18; radius: 4; color: Ui.Theme.textHint }
                        Rectangle { width: 56; height: 18; radius: 4; color: Ui.Theme.textHint }
                        Rectangle { width: 40; height: 18; radius: 4; color: Ui.Theme.textHint }
                    }
                    Item {
                        anchors.fill: parent
                        visible: root.statusKind === "loading"
                        RotationAnimation on rotation {
                            from: 0; to: 360; duration: 900; loops: Animation.Infinite
                            running: root.statusKind === "loading"
                        }
                        Rectangle {
                            anchors.fill: parent
                            radius: 44
                            color: "transparent"
                            border.width: 6
                            border.color: Qt.rgba(1, 1, 1, 0.18)
                        }
                    }
                    Item {
                        anchors.fill: parent
                        visible: root.statusKind === "error" || root.statusKind === "unavailable"
                        Rectangle {
                            anchors.fill: parent
                            radius: 44
                            color: "transparent"
                            border.width: 6
                            border.color: Ui.Theme.error
                        }
                        Rectangle {
                            width: 6
                            height: 78
                            radius: 3
                            anchors.centerIn: parent
                            rotation: -35
                            color: Ui.Theme.error
                        }
                    }
                }
                Ui.UiText {
                    objectName: "statusTitle"
                    width: parent.width
                    text: messagePane.title
                    font.pixelSize: 48
                    font.weight: 700
                    font.letterSpacing: -0.96
                    wrapMode: Text.WordWrap
                }
                Ui.UiText {
                    objectName: "statusBody"
                    width: parent.width
                    text: messagePane.description
                    role: "body"
                    lineHeight: 1.45
                    lineHeightMode: Text.ProportionalHeight
                    color: Ui.Theme.textSecondary
                }
                Item { width: 1; height: 16; visible: statusButton.visible }
                Item {
                    id: statusButton
                    objectName: "statusAction"
                    width: statusLabel.implicitWidth + 88
                    height: 88
                    visible: root.statusActionText.length > 0
                    scale: activeFocus ? 1.06 : 1
                    transformOrigin: Item.Left
                    activeFocusOnTab: false
                    property bool pressed: false
                    Behavior on scale {
                        NumberAnimation { duration: statusButton.pressed ? Ui.Theme.pressDuration : Ui.Theme.focusDuration }
                    }
                    Rectangle {
                        anchors.fill: parent
                        anchors.topMargin: 12
                        radius: height / 2
                        color: Qt.rgba(0, 0, 0, statusButton.activeFocus ? 0.45 : 0)
                        z: -1
                    }
                    Rectangle {
                        anchors.fill: parent
                        radius: height / 2
                        color: Ui.Theme.textPrimary
                    }
                    Ui.UiText {
                        id: statusLabel
                        objectName: "statusActionLabel"
                        anchors.centerIn: parent
                        text: root.statusActionText
                        role: "control"
                        color: Ui.Theme.ink
                    }
                    Keys.priority: Keys.BeforeItem
                    Keys.onPressed: event => {
                        if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) {
                            if (!event.isAutoRepeat)
                                pressed = true;
                            event.accepted = true;
                            return;
                        }
                        root.localKey(event);
                    }
                    Keys.onReleased: event => {
                        if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) {
                            const go = pressed && !event.isAutoRepeat;
                            pressed = false;
                            if (go)
                                root.activateStatus();
                            event.accepted = true;
                        }
                    }
                    TapHandler { onTapped: root.activateStatus() }
                }
            }
        }
    }
}
