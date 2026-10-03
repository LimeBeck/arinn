// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import QtQuick.Window

FocusScope {
    id: root
    width: 1920; height: 1080
    // Immutable presentation snapshots; identifiers are opaque to QML.
    property var continueEntries: []
    property var busyEntryIds: []
    property var favoriteEntries: []
    property var newEntries: []
    property var applicationEntries: []
    property var runningEntries: []
    property bool shortcutManagement: false
    property bool keyboardAvailable: true
    property var shortcutEditor: ({})
    property bool shortcutSaving: false
    property string shortcutError: ""
    onShortcutErrorChanged: {
        if (shortcutError.length && removeConfirmation.visible) {
            removeConfirmation.visible = false;
            Focus.take(shortcutDelete);
        }
    }
    readonly property bool shortcutEditorOpen: shortcutEditor.id !== undefined
    property var shortcutOpener: null
    signal editShortcutRequested(string entryId)
    signal closeShortcutEditorRequested()
    signal saveShortcutRequested(string title, string url)
    signal removeShortcutRequested()
    function editShortcut(entryId) {
        shortcutOpener = root.Window.window ? root.Window.window.activeFocusItem : null;
        editShortcutRequested(entryId);
    }
    onShortcutEditorOpenChanged: {
        if (shortcutEditorOpen) {
            shortcutTitle.text = shortcutEditor.title || "";
            shortcutUrl.text = shortcutEditor.url || "";
            removeConfirmation.visible = false;
            Qt.callLater(() => keyboardAvailable ? Focus.take(shortcutTitle) : Focus.take(shortcutCancel));
        } else {
            shortcutTitle.text = "";
            shortcutUrl.text = "";
            if (shortcutOpener && shortcutOpener.visible && shortcutOpener.enabled)
                Focus.take(shortcutOpener);
            else Qt.callLater(restoreScreen);
            shortcutOpener = null;
        }
    }
    property string applicationFocusRow: "group-online"
    // Applications-screen groups in display order (spec §5.5). Providers set
    // `group`; the C++ model maps unknown groups to "other" before QML.
    readonly property var applicationGroups: ["online", "games", "media", "other"]
    readonly property var groupTitles: ({online: qsTr("Online"), games: qsTr("Games"), media: qsTr("Media"), other: qsTr("Other")})
    property real sectionOffset: 0
    property string favoritesTitle: qsTr("Favorites")
    property string sectionDescription: qsTr("This section is not connected in the Home preview.")
    property string returnActivityId: ""
    property bool returnIsWebService: false
    property int openServiceCount: 0
    property string currentServiceId: ""
    property string currentServiceTitle: ""
    property string currentServiceSubtitle: ""
    property bool browserUnavailable: false
    property var openShortcutIds: []
    property string homeState: "ready" // ready, empty, loading, error, unavailable
    property string statusTitle: ""
    property string statusDescription: ""
    property string clockText: ""
    property bool pointerMode: false
    property bool navOpen: false
    property string screen: "home"
    property string homeRow: "continue"
    property var focusedEntry: ({})
    property int navIndex: 0
    property var navigationOpener: null
    readonly property real homeOffset: homeRow === "new" && newRow.visible ? -Math.min(360, Math.max(0, newRow.y - 570)) : 0
    onHomeOffsetChanged: hoverDelay.restart()
    readonly property bool hoverFocusEnabled: !hoverDelay.running
    readonly property var sections: [qsTr("Home"), qsTr("Library"), qsTr("Downloads"), qsTr("Applications"), qsTr("Settings")]
    readonly property var routes: ["home", "library", "downloads", "applications", "settings"]
    signal entryRequested(string entryId)
    signal runningRequested(string entryId)
    signal returnRequested(string activityId)
    signal retryRequested()
    signal sectionRequested(string route)
    signal powerRequested()
    signal openServicesRequested()
    signal serviceRequested(string serviceId)
    signal sessionRequested(string entryId)
    readonly property var continuePresentation: {
        if (openServiceCount <= 1 && !currentServiceTitle.length)
            return continueEntries;
        const shown = currentServiceTitle.length > 0;
        const service = shown ? [{
            id: "current-service",
            serviceId: currentServiceId,
            title: currentServiceTitle,
            subtitle: currentServiceSubtitle,
            running: true,
            kind: "service"
        }] : [];
        const others = openServiceCount - (shown ? 1 : 0);
        const more = openServiceCount > 1 ? [{
            id: "open-services-more",
            title: qsTr("%1 more open").arg(Math.max(others, 1)),
            subtitle: qsTr("Open services"),
            kind: "more"
        }] : [];
        const activity = !shown && continueEntries.length ? [continueEntries[0]] : [];
        const rest = shown ? continueEntries : continueEntries.slice(1);
        const further = more.slice();
        for (let i = 0; i < rest.length && further.length < 4; ++i) {
            further.push(rest[i]);
        }
        return service.concat(activity).concat(further.slice(0, 4));
    }

    function rowItems() { return [continueRow, favoritesRow, newRow]; }
    function restoreHome() {
        const rows = rowItems();
        const remembered = rows.find(row => row.objectName === homeRow);
        if (homeState === "ready" && remembered && remembered.visible && remembered.restore()) return;
        if (homeState === "ready") {
            for (const row of rows) if (row.visible && row.restore()) return;
        }
        Focus.take(menuButton);
    }
    function groupEntries(group) {
        const entries = applicationEntries.filter(entry => (entry.group || "other") === group);
        if (group === "other" && shortcutManagement)
            entries.push({id: "shortcut-add", title: qsTr("Add application"), subtitle: qsTr("Any site as an application"), shortcutAdd: true, enabled: true});
        return entries;
    }
    // Visible application rows, top to bottom: groups, then open windows.
    function applicationRows() {
        const rows = [];
        for (let i = 0; i < groupRows.count; ++i) rows.push(groupRows.itemAt(i));
        rows.push(runningRow);
        return rows.filter(row => row && row.visible);
    }
    function revealRow(row) {
        sectionOffset = Math.max(0, row.y + row.height - 700);
    }
    function focusApplicationRowBelow(fromRow) {
        const rows = applicationRows();
        for (let i = rows.indexOf(fromRow) + 1; i < rows.length; ++i) {
            if (rows[i].focusIndex(fromRow.selectedIndex) || rows[i].restore()) return true;
        }
        return false;
    }
    function applicationEdge(row, key) {
        if (key === Qt.Key_Left) { openNavigation(); return; }
        if (key === Qt.Key_Right) return;
        if (key === Qt.Key_Down && shortcutEditButton.visible) {
            Qt.callLater(() => Focus.take(shortcutEditButton));
            return;
        }
        const rows = applicationRows();
        const step = key === Qt.Key_Up ? -1 : 1;
        for (let i = rows.indexOf(row) + step; i >= 0 && i < rows.length; i += step) {
            if (rows[i].focusIndex(row.selectedIndex) || rows[i].restore()) return;
        }
        if (key === Qt.Key_Up) { sectionOffset = 0; Focus.take(menuButton); }
        else Focus.take(sectionBack);
    }
    function applicationFocusLost(row) {
        const rows = applicationRows();
        const index = rows.indexOf(row);
        for (let i = index - 1; i >= 0; --i) if (rows[i].restore()) return;
        for (let i = index + 1; i < rows.length; ++i) if (rows[i].restore()) return;
        for (const other of rows) if (other !== row && other.restore()) return;
        Focus.take(sectionBack);
    }
    function restoreScreen() {
        if (screen === "home") restoreHome();
        else if (screen === "applications") {
            const rows = applicationRows();
            const remembered = rows.find(row => row.objectName === applicationFocusRow);
            if (remembered && remembered.restore()) return;
            for (const row of rows) if (row.restore()) return;
            Focus.take(sectionBack);
        } else Focus.take(sectionBack);
    }
    // Focus whatever Home shows on top: the shortcut editor, the open
    // navigation menu or the current section.
    function restoreFocus() {
        if (shortcutEditorOpen) Focus.take(shortcutCancel);
        else if (navOpen && navItems.itemAt(navIndex)) Focus.take(navItems.itemAt(navIndex));
        else restoreScreen();
    }
    function openNavigation() {
        if (navOpen) return;
        navigationOpener = root.Window.window ? root.Window.window.activeFocusItem : null;
        navIndex = Math.max(0, routes.indexOf(screen));
        navOpen = true;
        Qt.callLater(() => Focus.take(navItems.itemAt(navIndex)));
    }
    function closeNavigation() {
        navOpen = false;
        if (navigationOpener && navigationOpener.visible && navigationOpener.enabled)
            Focus.take(navigationOpener);
        else restoreScreen();
        navigationOpener = null;
    }
    function showHome() {
        if (shortcutEditorOpen) closeShortcutEditorRequested();
        navOpen = false;
        screen = "home";
        if (returnActivityId && homeState === "ready") {
            const index = continueEntries.findIndex(entry => entry.id === returnActivityId);
            if (index >= 0 && continueRow.focusIndex(index)) return;
        }
        restoreHome();
    }
    function back() {
        if (navOpen) closeNavigation();
        else if (screen !== "home") { screen = "home"; restoreHome(); }
        else if (returnActivityId) returnRequested(returnActivityId);
        else openNavigation();
    }
    function selectSection(index) {
        sectionOffset = 0;
        screen = routes[index];
        navOpen = false;
        restoreScreen();
        sectionRequested(screen);
    }
    function rowEdge(row, key) {
        if (key === Qt.Key_Left) { openNavigation(); return; }
        if (key === Qt.Key_Right) return;
        const rows = rowItems();
        const step = key === Qt.Key_Up ? -1 : 1;
        for (let i = rows.indexOf(row) + step; i >= 0 && i < rows.length; i += step) {
            if (rows[i].visible && rows[i].focusIndex(row.selectedIndex)) return;
        }
        if (key === Qt.Key_Up || !row.count) Focus.take(menuButton);
    }
    function focusAfterRemoval(row) {
        const rows = rowItems();
        const index = rows.indexOf(row);
        for (let i = index - 1; i >= 0; --i) if (rows[i].visible && rows[i].restore()) return;
        for (let i = index + 1; i < rows.length; ++i) if (rows[i].visible && rows[i].restore()) return;
        Focus.take(menuButton);
    }
    function remember(row, entry) { homeRow = row.objectName; focusedEntry = entry; }
    function entryById(entryId) {
        const pools = [continuePresentation, favoriteEntries, newEntries, applicationEntries, runningEntries];
        for (let p = 0; p < pools.length; ++p) {
            const pool = pools[p] || [];
            for (let i = 0; i < pool.length; ++i)
                if (pool[i] && pool[i].id === entryId)
                    return pool[i];
        }
        return null;
    }
    function activate(entryId) {
        const entry = entryById(entryId);
        if (entry && entry.kind === "more") { openServicesRequested(); return; }
        if (entry && entry.kind === "service") { serviceRequested(entry.serviceId || ""); return; }
        if (entry && entry.kind === "media") { sessionRequested(entryId); return; }
        if (browserUnavailable && entry && entry.webShortcut) { openServicesRequested(); return; }
        if (entryId === returnActivityId) returnRequested(entryId);
        else entryRequested(entryId);
    }
    onHomeStateChanged: Qt.callLater(() => { if (!navOpen && !shortcutEditorOpen && screen === "home") restoreHome(); })
    onReturnActivityIdChanged: {
        if (!returnActivityId) {
            activityNotice.visible = true;
            noticeTimer.restart();
            if (navOpen && navIndex === 6)
                Focus.take(navItems.itemAt(5));
        }
    }
    Component.onCompleted: Qt.callLater(restoreScreen)
    Timer { id: hoverDelay; interval: 500 }
    Timer { id: noticeTimer; interval: 2600; onTriggered: activityNotice.visible = false }
    HoverHandler {
        property point previousPosition: Qt.point(-1, -1)
        onPointChanged: {
            if (point.scenePosition.x !== previousPosition.x || point.scenePosition.y !== previousPosition.y) {
                root.pointerMode = true;
                previousPosition = point.scenePosition;
            }
        }
        cursorShape: root.pointerMode ? Qt.ArrowCursor : Qt.BlankCursor
    }
    Keys.onPressed: event => {
        if ([Qt.Key_Left, Qt.Key_Right, Qt.Key_Up, Qt.Key_Down].includes(event.key)) root.pointerMode = false;
        if (event.key === Qt.Key_Home) { showHome(); event.accepted = true; }
        else if (event.key === Qt.Key_Menu) {
            if (shortcutManagement && !navOpen && screen === "applications" && focusedEntry.webShortcut)
                editShortcut(focusedEntry.id);
            else if (navOpen) closeNavigation(); else openNavigation();
            event.accepted = true;
        } else if (event.key === Qt.Key_Escape || event.key === Qt.Key_Backspace) {
            // Editable controls consume Backspace before it bubbles to this scope.
            // One step per press: repeats of a held key are ignored (#246).
            if (!event.isAutoRepeat) back();
            event.accepted = true;
        } else if (event.key === Qt.Key_Tab || event.key === Qt.Key_Backtab) {
            // Do not let Qt's implicit tab chain escape the active scope.
            if (navOpen) moveNavigation(event.key === Qt.Key_Backtab ? -1 : 1);
            event.accepted = true;
        }
    }
    function moveNavigation(step) {
        let next = navIndex + step;
        while (next >= 0 && next < navItems.count) {
            const item = navItems.itemAt(next);
            if (item.visible) { Focus.take(item); return; }
            next += step;
        }
    }
    Rectangle { anchors.fill: parent; color: Theme.background }
    Item {
        id: content
        anchors.fill: parent
        enabled: !root.navOpen
        scale: root.navOpen ? 0.98 : 1
        opacity: root.navOpen ? 0.55 : 1
        Behavior on scale { NumberAnimation { duration: 320 } }
        Behavior on opacity { NumberAnimation { duration: 320 } }
        BrandButton {
            id: menuButton
            objectName: "menuButton"
            x: 120; y: 44; height: 64; text: "Arinn"
            pointerMode: root.pointerMode; hoverFocusEnabled: root.hoverFocusEnabled
            onActivated: root.openNavigation()
            onDpadRequested: root.pointerMode = false
            Keys.onDownPressed: {
                if (root.screen === "home" && root.homeState !== "ready" && root.homeState !== "loading") Focus.take(statusPane);
                else root.restoreScreen();
            }
        }
        UiText { x: 1460; y: 60; width: 364; horizontalAlignment: Text.AlignRight; text: root.clockText; role: "meta" }
        Row {
            objectName: "browserStatusChip"
            x: 1180; y: 56; spacing: 12
            visible: root.browserUnavailable
            Rectangle { width: 28; height: 28; radius: 14; color: Theme.error; anchors.verticalCenter: parent.verticalCenter }
            UiText {
                objectName: "browserStatusLabel"
                text: qsTr("Browser unavailable")
                role: "caption"
                color: Theme.error
                anchors.verticalCenter: parent.verticalCenter
            }
        }
        Item {
            anchors.fill: parent
            visible: root.screen === "home"
            Item {
                id: homeContent
                anchors.fill: parent
                visible: root.homeState === "ready"
                // A translation keeps all rows in a single logical screen.
                transform: Translate {
                    y: root.homeOffset
                    Behavior on y { NumberAnimation { duration: 450; easing.type: Easing.OutCubic } }
                }
                Column {
                    x: 120; y: 136; width: 1300; spacing: 14
                    opacity: root.homeRow === "new" && newRow.visible ? 0 : 1
                    UiText { role: "eyebrow"; text: root.focusedEntry.running ? qsTr("NOW RUNNING") : root.homeRow === "favorites" ? root.favoritesTitle.toUpperCase() : qsTr("CONTINUE"); color: Theme.textSecondary }
                    UiText { width: parent.width; role: "hero"; text: root.focusedEntry.title || qsTr("Make yourself at home"); maximumLineCount: 1; elide: Text.ElideRight }
                    UiText { width: parent.width; role: "meta"; text: root.focusedEntry.subtitle || qsTr("Choose something to open"); maximumLineCount: 1; elide: Text.ElideRight; color: Theme.textSecondary }
                }
                HomeRow {
                    id: continueRow; objectName: "continue"
                    x: 120; y: 372; width: 1704
                    visible: count > 0
                    title: qsTr("Continue"); entries: root.continuePresentation; busyEntryIds: root.busyEntryIds
                    pointerMode: root.pointerMode; hoverFocusEnabled: root.hoverFocusEnabled
                    onSelected: entry => root.remember(continueRow, entry)
                    onActivated: entryId => root.activate(entryId)
                    onEdge: key => root.rowEdge(continueRow, key)
                    onFocusLost: root.focusAfterRemoval(continueRow)
                    onScrolled: hoverDelay.restart()
                    onDpadRequested: root.pointerMode = false
                }
                HomeRow {
                    id: favoritesRow; objectName: "favorites"
                    x: 120; y: continueRow.visible ? 700 : 372; width: 1704
                    visible: count > 0
                    title: root.favoritesTitle; variant: "app"; entries: root.favoriteEntries; busyEntryIds: root.busyEntryIds
                    browserUnavailable: root.browserUnavailable; openShortcutIds: root.openShortcutIds
                    pointerMode: root.pointerMode; hoverFocusEnabled: root.hoverFocusEnabled
                    onSelected: entry => root.remember(favoritesRow, entry)
                    onActivated: entryId => root.activate(entryId)
                    onEdge: key => root.rowEdge(favoritesRow, key)
                    onFocusLost: root.focusAfterRemoval(favoritesRow)
                    onScrolled: hoverDelay.restart()
                    onDpadRequested: root.pointerMode = false
                }
                HomeRow {
                    id: newRow; objectName: "new"
                    x: 120; y: favoritesRow.visible ? favoritesRow.y + 230 : continueRow.visible ? 700 : 372; width: 1704
                    visible: count > 0
                    title: activeFocus ? qsTr("New") + " · " + (root.focusedEntry.title || "") : qsTr("New")
                    variant: "poster"; entries: root.newEntries; busyEntryIds: root.busyEntryIds
                    pointerMode: root.pointerMode; hoverFocusEnabled: root.hoverFocusEnabled
                    onSelected: entry => root.remember(newRow, entry)
                    onActivated: entryId => root.activate(entryId)
                    onEdge: key => root.rowEdge(newRow, key)
                    onFocusLost: root.focusAfterRemoval(newRow)
                    onScrolled: hoverDelay.restart()
                    onDpadRequested: root.pointerMode = false
                }
            }
            StatusPane {
                id: statusPane
                Keys.onUpPressed: Focus.take(menuButton)
                objectName: "homeStatus"
                x: 550; y: 300
                visible: root.homeState !== "ready"
                kind: root.homeState
                title: root.statusTitle
                description: root.statusDescription
                actionText: root.homeState === "empty" ? qsTr("Browse sections") : qsTr("Retry")
                pointerMode: root.pointerMode; hoverFocusEnabled: root.hoverFocusEnabled
                onActionRequested: { if (root.homeState === "empty") root.openNavigation(); else root.retryRequested(); }
                onDpadRequested: root.pointerMode = false
            }
        }
        Column {
            // Scrolls so the focused group stays on screen at TV sizes.
            x: 120; y: 180 - root.sectionOffset; width: 1300; spacing: 40
            visible: root.screen !== "home"
            Behavior on y { NumberAnimation { duration: 160; easing.type: Easing.OutCubic } }
            UiText { role: "title"; text: root.sections[root.routes.indexOf(root.screen)] || "" }
            UiText { width: parent.width; text: root.sectionDescription; color: Theme.textSecondary }
            Repeater {
                id: groupRows
                model: root.applicationGroups
                HomeRow {
                    id: groupRow
                    required property string modelData
                    objectName: "group-" + modelData
                    width: 1704; variant: "app"
                    cellWidth: modelData === "other" && root.shortcutManagement ? 480 : 264
                    title: root.groupTitles[modelData]
                    visible: root.screen === "applications" && count > 0
                    entries: root.groupEntries(modelData); busyEntryIds: root.busyEntryIds
                    browserUnavailable: root.browserUnavailable; openShortcutIds: root.openShortcutIds
                    pointerMode: root.pointerMode; hoverFocusEnabled: root.hoverFocusEnabled
                    onSelected: entry => { root.focusedEntry = entry; root.applicationFocusRow = groupRow.objectName; root.revealRow(groupRow); }
                    onActivated: entryId => {
                        const entry = groupRow.entries.find(e => e.id === entryId);
                        if (entry && entry.shortcutAdd) root.editShortcut("");
                        else root.entryRequested(entryId);
                    }
                    onEdge: key => root.applicationEdge(groupRow, key)
                    onFocusLost: root.applicationFocusLost(groupRow)
                    onScrolled: hoverDelay.restart()
                    onDpadRequested: root.pointerMode = false
                }
            }
            HomeRow {
                id: runningRow; objectName: "runningApplications"
                width: 1704; variant: "app"; title: qsTr("Open windows")
                visible: root.screen === "applications" && count > 0
                entries: root.runningEntries; busyEntryIds: root.busyEntryIds
                pointerMode: root.pointerMode; hoverFocusEnabled: root.hoverFocusEnabled
                onSelected: { root.applicationFocusRow = runningRow.objectName; root.revealRow(runningRow); }
                onActivated: entryId => root.runningRequested(entryId)
                onEdge: key => root.applicationEdge(runningRow, key)
                onFocusLost: root.applicationFocusLost(runningRow)
                onScrolled: hoverDelay.restart()
                onDpadRequested: root.pointerMode = false
            }
            ActionButton {
                id: shortcutEditButton; objectName: "editShortcutButton"
                text: qsTr("Edit shortcut")
                visible: root.shortcutManagement && root.screen === "applications" && !!root.focusedEntry.webShortcut
                onActivated: root.editShortcut(root.focusedEntry.id)
                Keys.onUpPressed: root.restoreScreen()
                Keys.onDownPressed: {
                    const from = root.applicationRows().find(row => row.objectName === root.applicationFocusRow);
                    if (!from || !root.focusApplicationRowBelow(from))
                        Focus.take(sectionBack);
                }
            }
            ActionButton {
                id: sectionBack; objectName: "sectionBack"
                text: qsTr("Back to Home")
                pointerMode: root.pointerMode; hoverFocusEnabled: root.hoverFocusEnabled
                onActivated: root.showHome()
                onDpadRequested: root.pointerMode = false
                Keys.onLeftPressed: root.openNavigation()
                Keys.onUpPressed: root.restoreScreen()
            }
        }
    }
    MouseArea {
        x: 0; width: 28; height: parent.height
        visible: root.pointerMode && !root.navOpen
        onClicked: root.openNavigation()
        Rectangle { width: 6; height: 96; anchors.centerIn: parent; radius: 3; color: Theme.textHint }
    }
    Rectangle {
        id: activityNotice
        visible: false; x: 560; y: 960; width: 800; height: 80; radius: 40; color: "#33363b"
        UiText { anchors.centerIn: parent; text: qsTr("Activity is no longer available"); role: "meta" }
    }
    FocusScope {
        id: navigation
        objectName: "navigation"
        anchors.fill: parent; visible: root.navOpen
        MouseArea { anchors.fill: parent; onClicked: root.closeNavigation() }
        Rectangle {
            x: 72; y: 72; width: 500; height: 936; radius: 44; color: "#292b30"
            MouseArea { anchors.fill: parent }
            BrandLockup { objectName: "navigationBrand"; x: 40; y: 32; markSize: 40; textRole: "section" }
            Column {
                x: 28; y: 100; spacing: 8
                Repeater {
                    id: navItems
                    model: root.sections.concat([qsTr("Open services"), qsTr("Return to activity"), qsTr("Power")])
                    delegate: Surface {
                        id: navButton
                        required property int index
                        required property string modelData
                        objectName: "nav-" + index
                        width: 444
                        height: index === 5 && root.currentServiceTitle.length ? 112 : 88
                        text: modelData
                        visible: index !== 6 || (root.returnActivityId.length > 0 && !root.returnIsWebService)
                        focusScale: 1.02
                        cornerRadius: 28
                        restingColor: "transparent"
                        readonly property color labelColor: activeFocus ? Theme.textPrimary : Theme.textSecondary
                        contentItem: Item {
                            Row {
                                x: 24
                                anchors.verticalCenter: parent.verticalCenter
                                width: parent.width - 96
                                spacing: 16
                                Rectangle {
                                    visible: index === 5
                                    width: 40; height: 40; radius: 12
                                    color: Qt.rgba(1, 1, 1, 0.08)
                                    anchors.verticalCenter: parent.verticalCenter
                                    Column {
                                        anchors.centerIn: parent
                                        spacing: 3
                                        Rectangle { width: 22; height: 4; radius: 1; color: navButton.labelColor }
                                        Rectangle { width: 16; height: 4; radius: 1; color: navButton.labelColor }
                                        Rectangle { width: 10; height: 4; radius: 1; color: navButton.labelColor }
                                    }
                                }
                                Column {
                                    width: parent.width - (index === 5 ? 56 : 0)
                                    spacing: 2
                                    anchors.verticalCenter: parent.verticalCenter
                                    UiText {
                                        width: parent.width
                                        text: modelData
                                        role: "control"
                                        color: navButton.labelColor
                                        maximumLineCount: 1; elide: Text.ElideRight
                                    }
                                    UiText {
                                        objectName: index === 5 ? "openServicesSubline" : ""
                                        width: parent.width
                                        visible: index === 5 && root.currentServiceTitle.length > 0
                                        text: index === 5 ? qsTr("On screen: %1").arg(root.currentServiceTitle) : ""
                                        role: "caption"
                                        color: Theme.textSecondary
                                        maximumLineCount: 1; elide: Text.ElideRight
                                    }
                                }
                            }
                            Rectangle {
                                objectName: index === 5 ? "openServicesBadge" : ""
                                visible: index === 5 && root.openServiceCount > 0
                                anchors.right: parent.right
                                anchors.rightMargin: 20
                                anchors.verticalCenter: parent.verticalCenter
                                width: Math.max(36, openServicesCount.implicitWidth + 16)
                                height: 32
                                radius: 16
                                color: Theme.ember
                                UiText {
                                    id: openServicesCount
                                    objectName: index === 5 ? "openServicesCount" : ""
                                    anchors.centerIn: parent
                                    text: index === 5 ? String(root.openServiceCount) : ""
                                    font.pixelSize: 24
                                    font.weight: 600
                                    color: Theme.ink
                                }
                            }
                        }
                        pointerMode: root.pointerMode; hoverFocusEnabled: root.hoverFocusEnabled
                        onActiveFocusChanged: if (activeFocus) root.navIndex = index
                        onDpadRequested: root.pointerMode = false
                        onActivated: {
                            if (index < 5) root.selectSection(index);
                            else if (index === 5) { root.navOpen = false; root.openServicesRequested(); }
                            else if (index === 6) root.returnRequested(root.returnActivityId);
                            else root.powerRequested();
                        }
                        Keys.onTabPressed: root.moveNavigation(1)
                        Keys.onBacktabPressed: root.moveNavigation(-1)
                        Keys.onUpPressed: root.moveNavigation(-1)
                        Keys.onDownPressed: root.moveNavigation(1)
                        Keys.onLeftPressed: {}
                        Keys.onRightPressed: {}
                        Rectangle {
                            anchors.right: parent.right; anchors.rightMargin: 20; anchors.verticalCenter: parent.verticalCenter
                            width: 10; height: 10; radius: 5; color: Theme.ember
                            visible: index < 5 && root.routes[index] === root.screen
                        }
                    }
                }
            }
        }
    }
    // Modal user-configuration editor. QML only edits text; the daemon owns
    // address policy and C++ owns revisions, transport and mutation state.
    FocusScope {
        id: shortcutSheet
        objectName: "shortcutSheet"
        anchors.fill: parent
        z: 100
        visible: root.shortcutEditorOpen
        enabled: visible
        Keys.onPressed: event => {
            event.accepted = true;
            if (event.key === Qt.Key_Home) root.showHome();
            else if (event.key === Qt.Key_Escape || event.key === Qt.Key_Backspace) {
                if (removeConfirmation.visible) {
                    removeConfirmation.visible = false;
                    Focus.take(shortcutDelete);
                } else root.closeShortcutEditorRequested();
            }
        }
        Rectangle { anchors.fill: parent; color: "#E6131416"; MouseArea { anchors.fill: parent } }
        Rectangle {
            x: 410; y: 155; width: 1100; height: 770; radius: 40; color: Theme.background
            border.color: Theme.ring
            Column {
                x: 50; y: 38; width: 1000; spacing: 22
                enabled: !removeConfirmation.visible
                UiText { role: "title"; text: root.shortcutEditor.id ? qsTr("Edit shortcut") : qsTr("Add application") }
                UiText { text: qsTr("Title"); color: Theme.textSecondary }
                Rectangle {
                    width: 1000; height: 100; radius: 30; color: Theme.plate
                    border.width: 2; border.color: shortcutTitle.activeFocus ? Theme.ember : "transparent"
                    TextInput {
                        id: shortcutTitle; objectName: "shortcutTitle"
                        anchors.fill: parent; anchors.margins: 24
                        font.family: Theme.fontFamily; font.pixelSize: 36; color: Theme.textPrimary
                        selectByMouse: true; clip: true
                        readOnly: root.shortcutSaving || !root.keyboardAvailable
                        maximumLength: 256
                        Keys.onDownPressed: Focus.take(shortcutUrl)
                        Keys.onReturnPressed: Focus.take(shortcutUrl)
                        Keys.onEnterPressed: Focus.take(shortcutUrl)
                        Keys.onEscapePressed: root.closeShortcutEditorRequested()
                        Keys.onPressed: event => { if (event.key === Qt.Key_Home) { root.showHome(); event.accepted = true; } }
                    }
                }
                UiText { text: qsTr("Address"); color: Theme.textSecondary }
                Rectangle {
                    width: 1000; height: 100; radius: 30; color: Theme.plate
                    border.width: 2; border.color: shortcutUrl.activeFocus ? Theme.ember : "transparent"
                    TextInput {
                        id: shortcutUrl; objectName: "shortcutUrl"
                        anchors.fill: parent; anchors.margins: 24
                        font.family: Theme.fontFamily; font.pixelSize: 36; color: Theme.textPrimary
                        selectByMouse: true; clip: true
                        readOnly: root.shortcutSaving || !root.keyboardAvailable
                        maximumLength: 2048
                        Keys.onUpPressed: Focus.take(shortcutTitle)
                        Keys.onDownPressed: Focus.take(shortcutSave)
                        Keys.onReturnPressed: Focus.take(shortcutSave)
                        Keys.onEnterPressed: Focus.take(shortcutSave)
                        Keys.onEscapePressed: root.closeShortcutEditorRequested()
                        Keys.onPressed: event => { if (event.key === Qt.Key_Home) { root.showHome(); event.accepted = true; } }
                    }
                }
                UiText {
                    width: parent.width; height: 80; font.pixelSize: 28
                    text: !root.keyboardAvailable ? qsTr("A keyboard is needed to type. Without a keyboard, use arinnctl shortcuts.") : root.shortcutError
                    color: root.shortcutError.length ? Theme.error : Theme.textSecondary
                }
                Row {
                    spacing: 24
                    ActionButton {
                        id: shortcutSave; objectName: "shortcutSave"; text: qsTr("Save")
                        visible: root.keyboardAvailable; busy: root.shortcutSaving
                        onActivated: if (!root.shortcutSaving) root.saveShortcutRequested(shortcutTitle.text, shortcutUrl.text)
                        Keys.onUpPressed: Focus.take(shortcutUrl)
                        Keys.onRightPressed: Focus.take(shortcutCancel)
                    }
                    ActionButton {
                        id: shortcutCancel; objectName: "shortcutCancel"; text: qsTr("Cancel")
                        onActivated: root.closeShortcutEditorRequested()
                        Keys.onLeftPressed: if (shortcutSave.visible) Focus.take(shortcutSave)
                        Keys.onRightPressed: if (shortcutDelete.visible) Focus.take(shortcutDelete)
                        Keys.onUpPressed: if (root.keyboardAvailable) Focus.take(shortcutUrl)
                    }
                    ActionButton {
                        id: shortcutDelete; objectName: "shortcutDelete"; text: qsTr("Remove"); destructive: true
                        visible: !!root.shortcutEditor.id
                        onActivated: if (!root.shortcutSaving) { removeConfirmation.visible = true; Focus.take(confirmCancel); }
                        Keys.onLeftPressed: Focus.take(shortcutCancel)
                    }
                }
            }
        }
        Rectangle {
            id: removeConfirmation; objectName: "shortcutRemoveConfirmation"
            anchors.centerIn: parent; width: 1000; height: 380; radius: 40
            color: Theme.background; border.color: Theme.ring; visible: false
            Column {
                anchors.centerIn: parent; spacing: 40
                UiText { text: qsTr("Remove this shortcut?"); role: "section" }
                Row {
                    spacing: 28
                    ActionButton {
                        id: confirmCancel; objectName: "shortcutKeep"; text: qsTr("Cancel")
                        onActivated: { removeConfirmation.visible = false; Focus.take(shortcutDelete); }
                        Keys.onRightPressed: Focus.take(confirmRemove)
                    }
                    ActionButton {
                        id: confirmRemove; objectName: "shortcutRemove"; text: qsTr("Remove"); destructive: true
                        busy: root.shortcutSaving
                        onActivated: if (!root.shortcutSaving) root.removeShortcutRequested()
                        Keys.onLeftPressed: Focus.take(confirmCancel)
                    }
                }
            }
        }
    }

}
