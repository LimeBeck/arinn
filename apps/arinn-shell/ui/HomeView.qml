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
    property string applicationFocusRow: "group-online"
    // Applications-screen groups in display order (spec §5.5). Providers set
    // `group`; the C++ model maps unknown groups to "other" before QML.
    readonly property var applicationGroups: ["online", "games", "media", "other"]
    readonly property var groupTitles: ({online: qsTr("Online"), games: qsTr("Games"), media: qsTr("Media"), other: qsTr("Other")})
    property real sectionOffset: 0
    property string favoritesTitle: qsTr("Favorites")
    property string sectionDescription: qsTr("This section is not connected in the Home preview.")
    property string returnActivityId: ""
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

    function rowItems() { return [continueRow, favoritesRow, newRow]; }
    function restoreHome() {
        const rows = rowItems();
        const remembered = rows.find(row => row.objectName === homeRow);
        if (homeState === "ready" && remembered && remembered.visible && remembered.restore()) return;
        if (homeState === "ready") {
            for (const row of rows) if (row.visible && row.restore()) return;
        }
        menuButton.forceActiveFocus();
    }
    function groupEntries(group) {
        return applicationEntries.filter(entry => (entry.group || "other") === group);
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
    function applicationEdge(row, key) {
        if (key === Qt.Key_Left) { openNavigation(); return; }
        if (key === Qt.Key_Right) return;
        const rows = applicationRows();
        const step = key === Qt.Key_Up ? -1 : 1;
        for (let i = rows.indexOf(row) + step; i >= 0 && i < rows.length; i += step) {
            if (rows[i].focusIndex(row.selectedIndex) || rows[i].restore()) return;
        }
        if (key === Qt.Key_Up) { sectionOffset = 0; menuButton.forceActiveFocus(); }
        else sectionBack.forceActiveFocus();
    }
    function applicationFocusLost(row) {
        const rows = applicationRows();
        const index = rows.indexOf(row);
        for (let i = index - 1; i >= 0; --i) if (rows[i].restore()) return;
        for (let i = index + 1; i < rows.length; ++i) if (rows[i].restore()) return;
        for (const other of rows) if (other !== row && other.restore()) return;
        sectionBack.forceActiveFocus();
    }
    function restoreScreen() {
        if (screen === "home") restoreHome();
        else if (screen === "applications") {
            const rows = applicationRows();
            const remembered = rows.find(row => row.objectName === applicationFocusRow);
            if (remembered && remembered.restore()) return;
            for (const row of rows) if (row.restore()) return;
            sectionBack.forceActiveFocus();
        } else sectionBack.forceActiveFocus();
    }
    function openNavigation() {
        if (navOpen) return;
        navigationOpener = root.Window.window ? root.Window.window.activeFocusItem : null;
        navIndex = Math.max(0, routes.indexOf(screen));
        navOpen = true;
        Qt.callLater(() => navItems.itemAt(navIndex).forceActiveFocus());
    }
    function closeNavigation() {
        navOpen = false;
        if (navigationOpener && navigationOpener.visible && navigationOpener.enabled)
            navigationOpener.forceActiveFocus();
        else restoreScreen();
        navigationOpener = null;
    }
    function showHome() {
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
        if (key === Qt.Key_Up || !row.count) menuButton.forceActiveFocus();
    }
    function focusAfterRemoval(row) {
        const rows = rowItems();
        const index = rows.indexOf(row);
        for (let i = index - 1; i >= 0; --i) if (rows[i].visible && rows[i].restore()) return;
        for (let i = index + 1; i < rows.length; ++i) if (rows[i].visible && rows[i].restore()) return;
        menuButton.forceActiveFocus();
    }
    function remember(row, entry) { homeRow = row.objectName; focusedEntry = entry; }
    function activate(entryId) {
        if (entryId === returnActivityId) returnRequested(entryId);
        else entryRequested(entryId);
    }
    onHomeStateChanged: Qt.callLater(() => { if (!navOpen && screen === "home") restoreHome(); })
    onReturnActivityIdChanged: {
        if (!returnActivityId) {
            activityNotice.visible = true;
            noticeTimer.restart();
            if (navOpen && navIndex === 5) navItems.itemAt(4).forceActiveFocus();
        }
    }
    Component.onCompleted: Qt.callLater(restoreHome)
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
            if (navOpen) closeNavigation(); else openNavigation();
            event.accepted = true;
        } else if (event.key === Qt.Key_Escape || event.key === Qt.Key_Backspace) {
            // Editable controls consume Backspace before it bubbles to this scope.
            back(); event.accepted = true;
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
            if (item.visible) { item.forceActiveFocus(); return; }
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
                if (root.screen === "home" && root.homeState !== "ready" && root.homeState !== "loading") statusPane.forceActiveFocus();
                else root.restoreScreen();
            }
        }
        UiText { x: 1460; y: 60; width: 364; horizontalAlignment: Text.AlignRight; text: root.clockText; role: "meta" }
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
                    title: qsTr("Continue"); entries: root.continueEntries; busyEntryIds: root.busyEntryIds
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
                Keys.onUpPressed: menuButton.forceActiveFocus()
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
                    title: root.groupTitles[modelData]
                    visible: root.screen === "applications" && count > 0
                    entries: root.groupEntries(modelData); busyEntryIds: root.busyEntryIds
                    pointerMode: root.pointerMode; hoverFocusEnabled: root.hoverFocusEnabled
                    onSelected: { root.applicationFocusRow = groupRow.objectName; root.revealRow(groupRow); }
                    onActivated: entryId => root.entryRequested(entryId)
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
            x: 72; y: 112; width: 500; height: 856; radius: 44; color: "#292b30"
            MouseArea { anchors.fill: parent }
            BrandLockup { objectName: "navigationBrand"; x: 40; y: 32; markSize: 40; textRole: "section" }
            Column {
                x: 28; y: 100; spacing: 8
                Repeater {
                    id: navItems
                    model: root.sections.concat([qsTr("Return to activity"), qsTr("Power")])
                    delegate: Surface {
                        required property int index
                        required property string modelData
                        objectName: "nav-" + index
                        width: 444; height: 88; text: modelData
                        visible: index !== 5 || root.returnActivityId.length > 0
                        focusScale: 1.02
                        cornerRadius: 28
                        restingColor: "transparent"
                        contentItem: UiText {
                            leftPadding: 32; rightPadding: 44
                            text: parent.text
                            role: "control"
                            color: parent.activeFocus ? Theme.textPrimary : Theme.textSecondary
                            verticalAlignment: Text.AlignVCenter
                            maximumLineCount: 1; elide: Text.ElideRight
                        }
                        pointerMode: root.pointerMode; hoverFocusEnabled: root.hoverFocusEnabled
                        onActiveFocusChanged: if (activeFocus) root.navIndex = index
                        onDpadRequested: root.pointerMode = false
                        onActivated: {
                            if (index < 5) root.selectSection(index);
                            else if (index === 5) root.returnRequested(root.returnActivityId);
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
}
