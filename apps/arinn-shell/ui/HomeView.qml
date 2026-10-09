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
    // Open web services ({id, title, letter, icon}), the one on screen first.
    // `id` is the identity a Continue entry carries as `serviceId`.
    property var openServices: []
    readonly property int openServiceCount: openServices.length
    property string currentServiceId: ""
    property string currentServiceTitle: ""
    property string currentServiceSubtitle: ""
    property bool browserUnavailable: false
    property var openShortcutIds: []
    property string homeState: "ready" // ready, empty, loading, error, unavailable
    property string statusTitle: ""
    property string statusDescription: ""
    property string clockText: ""
    // Aggregate of active downloads: the status chip shows while downloading.
    // Progress is 0–1, or below 0 when no active download knows its size.
    property bool downloadActive: false
    property real downloadProgress: -1
    // Platform state (ADR-0100), shown only while known: the default output
    // is muted; connected over Wi-Fi, with a level 1–4 or 0 when unknown.
    property bool muted: false
    property bool wifi: false
    property int wifiLevel: 0
    // True while an Activity background is shown (ADR-0027): the ambient
    // light field then stays off.
    property bool ambientSuppressed: false
    // The shell's input mode (ADR-0095); tests may assign it directly.
    property bool pointerMode: InputMode.pointerActive
    property bool actionsEnabled: false
    property bool favoriteSaving: false
    property string favoriteError: ""
    property string actionEntryId: ""
    property bool confirmAppClose: false
    property var actionOpener: null
    readonly property bool tileActionsOpen: actionEntryId.length > 0
    readonly property var actionEntry: entryById(actionEntryId) || ({})
    signal favoriteRequested(string entryId, bool pinned)
    signal terminateRequested(string entryId)
    function restoreActionFocus() {
        if (confirmAppClose) Focus.take(closeAppCancel);
        else if (actionsEnabled) Focus.take(favoriteAction);
        else if (favoriteRefresh.visible && favoriteRefresh.enabled) Focus.take(favoriteRefresh);
        else Focus.take(actionCancel);
    }
    function settleActionFocus() {
        if (!tileActionsOpen || !root.Window.window) return;
        const item = root.Window.window.activeFocusItem;
        if (item && item.visible && item.enabled && item !== tileActions && item !== root
                && item !== root.Window.window.contentItem) return;
        restoreActionFocus();
    }
    onActionsEnabledChanged: Qt.callLater(settleActionFocus)
    onFavoriteErrorChanged: Qt.callLater(settleActionFocus)
    function openTileActions(entryId) {
        const entry = entryById(entryId);
        if (!actionsEnabled || !entry || !entry.providerId) return;
        actionOpener = root.Window.window ? root.Window.window.activeFocusItem : null;
        confirmAppClose = false;
        actionEntryId = entryId;
        Qt.callLater(() => Focus.take(favoriteAction));
    }
    function closeTileActions() {
        confirmAppClose = false;
        actionEntryId = "";
        Qt.callLater(() => {
            if (actionOpener && actionOpener.visible && actionOpener.enabled) Focus.take(actionOpener);
            else restoreScreen();
            actionOpener = null;
        });
    }
    onFavoriteEntriesChanged: if (tileActionsOpen && (screen === "home" ? !favoriteEntries.some(entry => entry.id === actionEntryId) : !entryById(actionEntryId))) closeTileActions()
    onApplicationEntriesChanged: if (tileActionsOpen && !entryById(actionEntryId)) closeTileActions()
    property bool navOpen: false
    // A section the shell draws itself (Library, Settings) covers Home's own
    // content. Home is then raised above it only to show the navigation
    // menu, so its background and section content stay hidden (#329).
    property bool coveredByScreen: false
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
    // The Home key was pressed in Home or its navigation menu. The shell
    // closes what the menu was opened over (§6.3, #329).
    signal homeKeyPressed()
    signal closeShellRequested()
    // The daemon runs another build: restart Arinn (ADR-0097, ADR-0098).
    signal restartRequested()
    signal powerRequested()
    signal openServicesRequested()
    signal serviceRequested(string serviceId)
    signal sessionRequested(string entryId)
    // A tab card of a linked browser media session (ADR-0096).
    signal tabRequested(string tabId)
    // action: "play", "pause" or "playPause", as the key asked.
    signal mediaActionRequested(string sessionId, string action)
    // Continue as shown: one activity (the service on screen, or the Return
    // activity) and at most four further entries, the open-services summary
    // first (spec §5.1, handoff 02, H-1).
    readonly property var continuePresentation: {
        const shown = currentServiceTitle.length > 0;
        // A browser's media session linked to a current tab (ADR-0096) is one
        // card with that tab. A link to a tab that is not open is ignored.
        const tabs = openServices.map(entry => entry.id);
        const entries = continueEntries.map(entry => entry.kind === "media" && entry.serviceId && !tabs.includes(entry.serviceId)
            ? Object.assign({}, entry, {serviceId: ""}) : entry);
        const merged = shown ? entries.find(entry => entry.kind === "media" && entry.serviceId === currentServiceId) : null;
        const service = shown ? [Object.assign({
            id: "current-service",
            serviceId: currentServiceId,
            // The colour follows the service, not the fixed presentation id.
            visualIdentity: currentServiceId,
            title: currentServiceTitle,
            subtitle: currentServiceSubtitle,
            running: true,
            kind: "service"
        }, merged ? linkedState(merged, currentServiceSubtitle) : {})] : [];
        const own = entries.filter(entry => entry !== merged);
        const lead = own.length && own[0].kind !== "media" ? own[0] : null;
        const activity = shown ? service : lead ? [lead] : [];
        const rest = (shown || !lead ? own : own.slice(1))
            .map(entry => entry.kind === "media" && entry.serviceId ? tabCard(entry) : entry);
        // Services that already have a card in this row, by identity only,
        // never by title (#358, #207). Only cards that fit count: with the
        // summary, three further entries remain; without it nothing is left
        // over, so a fourth cannot change the result.
        const represented = activity.concat(rest.slice(0, 3)).map(entry => entry.serviceId).filter(id => !!id);
        const others = openServices.filter(entry => !represented.includes(entry.id));
        const more = openServices.length > 1 && others.length ? [{
            id: "open-services-more",
            // "N more" once a service has its own card, "Open: N" otherwise.
            title: represented.length ? qsTr("%n more open", "", others.length) : qsTr("Open: %n", "", others.length),
            subtitle: qsTr("All open services"),
            names: others.map(entry => entry.title).filter(title => !!title).join(" · "),
            stack: others.slice(0, 3).map(entry => ({id: entry.id, letter: entry.letter || "", icon: entry.icon || ""})),
            kind: "more"
        }] : [];
        return activity.concat(more.concat(rest.map(mediaCard)).slice(0, 4));
    }
    // The playing or paused state a linked media session lends its tab card;
    // play/pause goes to `mediaId` (ADR-0096).
    function linkedState(entry, line) {
        const card = mediaCard(entry) || {};
        const state = entry.running ? qsTr("Playing") : qsTr("Paused");
        return {
            mediaId: entry.id,
            playing: !!entry.running,
            subtitle: line ? state + " · " + line : state,
            progress: card.progress
        };
    }
    // A linked session whose tab is not on screen: the tab's card, which
    // opens that tab.
    function tabCard(entry) {
        const tab = openServices.find(service => service.id === entry.serviceId) || {};
        return Object.assign({
            id: entry.id,
            serviceId: entry.serviceId,
            visualIdentity: entry.serviceId,
            title: tab.title || entry.title || "",
            artwork: entry.artwork || "",
            running: true,
            kind: "tab"
        }, linkedState(entry, ""));
    }
    // A media entry's card: the time left as its subtitle when it has none,
    // and its progress (#365).
    function mediaCard(entry) {
        if (!entry || entry.kind !== "media" || !(entry.durationUs > 0)) return entry;
        const position = Math.max(0, Math.min(entry.positionUs || 0, entry.durationUs));
        return Object.assign({}, entry, {
            subtitle: entry.subtitle || remainingText(entry.durationUs - position),
            detail: entry.subtitle || "",
            progress: position / entry.durationUs
        });
    }
    // 1:13:42, or 37:05 under an hour.
    function formatTime(us) {
        const total = Math.max(0, Math.floor(us / 1000000));
        const h = Math.floor(total / 3600), m = Math.floor(total % 3600 / 60), sec = total % 60;
        const two = n => (n < 10 ? "0" : "") + n;
        return h > 0 ? h + ":" + two(m) + ":" + two(sec) : m + ":" + two(sec);
    }
    function remainingText(us) {
        const minutes = Math.max(0, Math.round(us / 60000000));
        const h = Math.floor(minutes / 60), m = minutes % 60;
        if (minutes < 1) return qsTr("Less than a minute left");
        if (!h) return qsTr("%1 min left").arg(m);
        if (!m) return qsTr("%1 h left").arg(h);
        return qsTr("%1 h %2 min left").arg(h).arg(m);
    }
    // What the hero says about the focused Home item and what OK does
    // (#364, handoff 01–03).
    function describe(row, entry) {
        if (!entry || !entry.id)
            return {eyebrow: qsTr("CONTINUE"), title: qsTr("Make yourself at home"), meta: qsTr("Choose something to open"), live: false};
        if (row === "favorites") {
            const group = groupTitles[entry.group];
            const open = !!entry.running || openShortcutIds.indexOf(entry.id) >= 0;
            const unavailable = entry.unavailable || (browserUnavailable && entry.webShortcut);
            return {
                eyebrow: (group ? favoritesTitle + " · " + group : favoritesTitle).toUpperCase(),
                title: entry.title || "",
                meta: unavailable ? (browserUnavailable && entry.webShortcut ? qsTr("Unavailable") : entry.subtitle || "")
                    : entry.hint ? entry.hint : open ? qsTr("Running · Return") : qsTr("Opens full screen"),
                live: false
            };
        }
        if (entry.kind === "more")
            return {eyebrow: qsTr("OPEN SERVICES"), title: entry.title, meta: entry.names || entry.subtitle, live: false};
        if (entry.kind === "media") {
            // The provider's own line (an episode, an artist), not the card's.
            const detail = entry.detail !== undefined ? entry.detail : entry.subtitle || "";
            let meta = detail;
            // A playing session's last reported position is already behind.
            if (!entry.running && entry.durationUs > 0) {
                const position = Math.max(0, Math.min(entry.positionUs || 0, entry.durationUs));
                const left = remainingText(entry.durationUs - position);
                meta = detail ? detail + " · " + left
                    : qsTr("%1 of %2").arg(formatTime(position)).arg(formatTime(entry.durationUs)) + " · " + left;
            } else if (entry.running && !meta)
                meta = qsTr("Playing");
            return {eyebrow: qsTr("CONTINUE"), title: entry.title || "", meta: meta, live: false};
        }
        if (entry.kind === "tab")
            return {eyebrow: qsTr("NOW RUNNING"), title: entry.title || "",
                    meta: entry.subtitle + " · " + qsTr("Return"), live: true};
        if (entry.running)
            return {eyebrow: qsTr("NOW RUNNING"), title: entry.title || "",
                    meta: entry.kind === "service" && entry.subtitle ? entry.subtitle : qsTr("Return"), live: true};
        return {eyebrow: qsTr("CONTINUE"), title: entry.title || "", meta: entry.subtitle || "", live: false};
    }
    // The focused entry as currently presented; a stale snapshot otherwise.
    readonly property var heroEntry: {
        const pool = homeRow === "continue" ? continuePresentation : homeRow === "favorites" ? favoriteEntries : newEntries;
        return (pool || []).find(entry => entry && entry.id === focusedEntry.id) || focusedEntry;
    }
    readonly property var hero: describe(homeRow, heroEntry)
    // The light field follows the focused Continue or Favorites item; the
    // open-services summary has no single hue.
    readonly property var ambientEntry: (homeRow === "continue" || homeRow === "favorites")
        && heroEntry && heroEntry.id && heroEntry.kind !== "more" ? heroEntry : null

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
        if (tileActionsOpen) restoreActionFocus();
        else if (shortcutEditorOpen) Focus.take(shortcutCancel);
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
    // The shell closes the overlay first: Home is enabled again only then,
    // so showHome() can focus a visible control (review of #334).
    function homeKey() {
        if (tileActionsOpen) { actionEntryId = ""; actionOpener = null; }
        homeKeyPressed();
        showHome();
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
        if (tileActionsOpen) closeTileActions();
        else if (navOpen) closeNavigation();
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
        if (entry && entry.launchable === false) { openTileActions(entryId); return; }
        if (entry && entry.kind === "more") { openServicesRequested(); return; }
        if (entry && entry.kind === "service") { serviceRequested(entry.serviceId || ""); return; }
        if (entry && entry.kind === "tab") { tabRequested(entry.serviceId || ""); return; }
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
                InputMode.pointerMoved(point.scenePosition);
                previousPosition = point.scenePosition;
            }
        }
        cursorShape: root.pointerMode ? Qt.ArrowCursor : Qt.BlankCursor
    }
    Keys.onPressed: event => {
        if ([Qt.Key_Left, Qt.Key_Right, Qt.Key_Up, Qt.Key_Down].includes(event.key)) InputMode.keyPressed();
        if (event.key === Qt.Key_Home) { homeKey(); event.accepted = true; }
        else if ([Qt.Key_MediaTogglePlayPause, Qt.Key_MediaPlay, Qt.Key_MediaPause].includes(event.key)) {
            // Play/pause on a card linked to a media session goes to it.
            // Dedicated Play and Pause keep their meaning; only the toggle
            // key toggles.
            if (screen === "home" && !navOpen && heroEntry && heroEntry.mediaId) {
                const action = event.key === Qt.Key_MediaPlay ? "play"
                             : event.key === Qt.Key_MediaPause ? "pause" : "playPause";
                mediaActionRequested(heroEntry.mediaId, action);
                event.accepted = true;
            }
        }
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
    Rectangle { anchors.fill: parent; color: Theme.background; visible: !root.coveredByScreen }
    AmbientGlow {
        id: ambient
        objectName: "ambientGlow"
        anchors.fill: parent
        visible: !root.coveredByScreen
        lit: root.screen === "home" && root.homeState === "ready" && !root.ambientSuppressed
             && root.ambientEntry !== null && root.homeRow !== "new"
        source: root.ambientEntry ? (root.ambientEntry.artwork || "") : ""
        tint: root.ambientEntry ? Theme.identityColor(root.ambientEntry.visualIdentity || root.ambientEntry.id) : "transparent"
    }
    Item {
        id: content
        anchors.fill: parent
        visible: !root.coveredByScreen
        enabled: !root.navOpen && !root.tileActionsOpen
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
            onDpadRequested: InputMode.keyPressed()
            Keys.onDownPressed: {
                if (root.screen === "home" && root.homeState !== "ready" && root.homeState !== "loading") Focus.take(statusPane);
                else root.restoreScreen();
            }
        }
        // Status area, right to left: clock, Wi-Fi, mute, downloads, browser
        // state (handoff 01). Unknown platform state shows nothing (#387).
        Row {
            objectName: "statusArea"
            anchors.right: parent.right; anchors.rightMargin: Theme.safeRight
            y: 44; height: 64; spacing: 24
            Row {
                objectName: "browserStatusChip"
                anchors.verticalCenter: parent.verticalCenter
                spacing: 12
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
            Rectangle {
                objectName: "downloadChip"
                anchors.verticalCenter: parent.verticalCenter
                visible: root.downloadActive
                height: 48; radius: 24
                width: downloadLabel.x + downloadLabel.width + 22
                color: Theme.plate
                Accessible.role: Accessible.StaticText
                Accessible.name: downloadLabel.text
                UiText {
                    id: downloadArrow
                    x: 20; anchors.verticalCenter: parent.verticalCenter
                    text: "↓"; role: "caption"; font.weight: 700; color: Theme.ember
                }
                UiText {
                    id: downloadLabel
                    objectName: "downloadChipLabel"
                    x: downloadArrow.x + downloadArrow.width + 8
                    anchors.verticalCenter: parent.verticalCenter
                    role: "caption"; font.weight: 600; wrapMode: Text.NoWrap
                    text: root.downloadProgress >= 0
                        ? qsTr("%1%").arg(Math.floor(Math.min(1, root.downloadProgress) * 100))
                        : qsTr("Downloading")
                }
            }
            Canvas {
                objectName: "muteIcon"
                anchors.verticalCenter: parent.verticalCenter
                visible: root.muted
                width: 32; height: 32
                Accessible.role: Accessible.StaticText
                Accessible.name: qsTr("Sound muted")
                onPaint: {
                    const ctx = getContext("2d");
                    ctx.clearRect(0, 0, width, height);
                    ctx.fillStyle = Theme.textSecondary;
                    ctx.strokeStyle = Theme.textSecondary;
                    ctx.lineWidth = 2.6;
                    ctx.lineCap = "round";
                    // A speaker and a cross: Material Symbols volume_off.
                    ctx.beginPath();
                    ctx.moveTo(4, 12); ctx.lineTo(9, 12); ctx.lineTo(16, 6);
                    ctx.lineTo(16, 26); ctx.lineTo(9, 20); ctx.lineTo(4, 20);
                    ctx.closePath();
                    ctx.fill();
                    ctx.beginPath();
                    ctx.moveTo(20, 12); ctx.lineTo(28, 20);
                    ctx.moveTo(28, 12); ctx.lineTo(20, 20);
                    ctx.stroke();
                }
            }
            Canvas {
                objectName: "wifiIcon"
                anchors.verticalCenter: parent.verticalCenter
                visible: root.wifi
                width: 32; height: 32
                // Dot and three arcs light up to the level; all of them
                // when the level is unknown.
                readonly property int lit: root.wifiLevel > 0 ? root.wifiLevel : 4
                onLitChanged: requestPaint()
                Accessible.role: Accessible.StaticText
                Accessible.name: qsTr("Wi-Fi connected")
                onPaint: {
                    const ctx = getContext("2d");
                    ctx.clearRect(0, 0, width, height);
                    ctx.lineWidth = 3;
                    ctx.lineCap = "round";
                    const tone = part => part <= lit ? Theme.textSecondary : Theme.plateFocus;
                    ctx.fillStyle = tone(1);
                    ctx.beginPath(); ctx.arc(16, 25, 2.6, 0, Math.PI * 2); ctx.fill();
                    for (let part = 2; part <= 4; ++part) {
                        ctx.strokeStyle = tone(part);
                        ctx.beginPath();
                        ctx.arc(16, 25, 6 + (part - 2) * 6, Math.PI * 1.22, Math.PI * 1.78);
                        ctx.stroke();
                    }
                }
            }
            UiText {
                objectName: "clock"
                anchors.verticalCenter: parent.verticalCenter
                text: root.clockText; role: "meta"; font.pixelSize: 30; wrapMode: Text.NoWrap
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
                    Row {
                        spacing: 12
                        Rectangle {
                            objectName: "heroLiveDot"
                            anchors.verticalCenter: parent.verticalCenter
                            width: 12; height: 12; radius: 6; color: Theme.ember
                            visible: root.hero.live
                        }
                        UiText { objectName: "heroEyebrow"; role: "eyebrow"; text: root.hero.eyebrow; color: Theme.textSecondary }
                    }
                    UiText { objectName: "heroTitle"; width: parent.width; role: "hero"; text: root.hero.title; maximumLineCount: 1; elide: Text.ElideRight }
                    UiText { objectName: "heroMeta"; width: parent.width; role: "meta"; font.pixelSize: 28; text: root.hero.meta; maximumLineCount: 1; elide: Text.ElideRight; color: Theme.textSecondary }
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
                    onDpadRequested: InputMode.keyPressed()
                }
                HomeRow {
                    id: favoritesRow; objectName: "favorites"
                    x: 120; y: continueRow.visible ? 700 : 372; width: 1704
                    visible: count > 0
                    title: root.favoritesTitle; variant: "app"; entries: root.favoriteEntries; busyEntryIds: root.busyEntryIds
                    browserUnavailable: root.browserUnavailable; openShortcutIds: root.openShortcutIds
                    pointerMode: root.pointerMode; hoverFocusEnabled: root.hoverFocusEnabled
                    onActionsRequested: entryId => root.openTileActions(entryId)
                    onSelected: entry => root.remember(favoritesRow, entry)
                    onActivated: entryId => root.activate(entryId)
                    onEdge: key => root.rowEdge(favoritesRow, key)
                    onFocusLost: root.focusAfterRemoval(favoritesRow)
                    onScrolled: hoverDelay.restart()
                    onDpadRequested: InputMode.keyPressed()
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
                    onDpadRequested: InputMode.keyPressed()
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
                actionText: root.homeState === "empty" ? qsTr("Browse sections") : root.homeState === "updated" ? qsTr("Restart") : qsTr("Retry")
                pointerMode: root.pointerMode; hoverFocusEnabled: root.hoverFocusEnabled
                onActionRequested: {
                    if (root.homeState === "empty") root.openNavigation();
                    else if (root.homeState === "updated") root.restartRequested();
                    else root.retryRequested();
                }
                onDpadRequested: InputMode.keyPressed()
            }
        }
        Column {
            // Scrolls so the focused group stays on screen at TV sizes.
            x: 120; y: 180 - root.sectionOffset; width: 1300; spacing: 40
            visible: root.screen !== "home"
            enabled: !root.tileActionsOpen
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
                    onActionsRequested: entryId => root.openTileActions(entryId)
                    onSelected: entry => { root.focusedEntry = entry; root.applicationFocusRow = groupRow.objectName; root.revealRow(groupRow); }
                    onActivated: entryId => {
                        const entry = groupRow.entries.find(e => e.id === entryId);
                        if (entry && entry.shortcutAdd) root.editShortcut("");
                        else if (entry && entry.launchable === false) root.openTileActions(entryId);
                        else root.entryRequested(entryId);
                    }
                    onEdge: key => root.applicationEdge(groupRow, key)
                    onFocusLost: root.applicationFocusLost(groupRow)
                    onScrolled: hoverDelay.restart()
                    onDpadRequested: InputMode.keyPressed()
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
                onDpadRequested: InputMode.keyPressed()
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
                onDpadRequested: InputMode.keyPressed()
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
        // Dims the covering section the way Home's own content is dimmed.
        Rectangle { anchors.fill: parent; color: Qt.rgba(0, 0, 0, 0.45); visible: root.coveredByScreen }
        MouseArea { anchors.fill: parent; onClicked: root.closeNavigation() }
        Rectangle {
            x: 72; y: 72; width: 500; height: 936; radius: 44; color: "#292b30"
            MouseArea { anchors.fill: parent }
            BrandLockup { objectName: "navigationBrand"; x: 40; y: 32; markSize: 40; textRole: "section" }
            Column {
                x: 28; y: 100; spacing: 4
                Repeater {
                    id: navItems
                    model: root.sections.concat([qsTr("Open services"), qsTr("Return to activity"), qsTr("Power"), qsTr("Close Arinn")])
                    delegate: Surface {
                        id: navButton
                        required property int index
                        required property string modelData
                        objectName: "nav-" + index
                        width: 444
                        height: index === 5 && root.currentServiceTitle.length ? 112 : 84
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
                        onDpadRequested: InputMode.keyPressed()
                        onActivated: {
                            if (index < 5) root.selectSection(index);
                            else if (index === 5) { root.navOpen = false; root.openServicesRequested(); }
                            else if (index === 6) root.returnRequested(root.returnActivityId);
                            else if (index === 7) root.powerRequested();
                            else root.closeShellRequested();
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
            if (event.key === Qt.Key_Home) root.homeKey();
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
                        Keys.onPressed: event => { if (event.key === Qt.Key_Home) { root.homeKey(); event.accepted = true; } }
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
                        Keys.onPressed: event => { if (event.key === Qt.Key_Home) { root.homeKey(); event.accepted = true; } }
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

    FocusScope {
        id: tileActions
        objectName: "tileActions"
        anchors.fill: parent; visible: root.tileActionsOpen; enabled: visible; z: 20
        Keys.onPressed: event => {
            if (event.key === Qt.Key_Home) root.homeKey();
            else if ((event.key === Qt.Key_Tab || event.key === Qt.Key_Backtab) && root.confirmAppClose)
                Focus.take(closeAppCancel.activeFocus ? closeAppConfirm : closeAppCancel);
            else if (event.key === Qt.Key_Escape || event.key === Qt.Key_Backspace || event.key === Qt.Key_Back || event.key === Qt.Key_Menu) {
                if (root.confirmAppClose) { root.confirmAppClose = false; Focus.take(closeAppAction); }
                else root.closeTileActions();
            }
            event.accepted = true;
        }
        Rectangle { anchors.fill: parent; color: Qt.rgba(0, 0, 0, 0.65) }
        MouseArea { anchors.fill: parent; onClicked: root.closeTileActions() }
        Rectangle {
            anchors.centerIn: parent; width: 1000; height: 560; radius: 40; color: Theme.background
            enabled: !root.confirmAppClose
            Column {
                anchors.fill: parent; anchors.margins: 48; spacing: 24
                UiText { text: root.actionEntry.title || qsTr("Application actions"); role: "section"; width: parent.width }
                UiText { text: root.favoriteError; visible: text.length > 0; color: Theme.error; width: parent.width }
                Grid {
                    columns: 2; columnSpacing: 24; rowSpacing: 20
                    ActionButton {
                        width: 400
                        id: favoriteAction; objectName: "favoriteAction"
                        text: root.actionEntry.pinned ? qsTr("Unpin from Home") : qsTr("Pin to Home")
                        enabled: root.actionsEnabled && !!root.actionEntry.providerId
                        busy: root.favoriteSaving
                        onActivated: root.favoriteRequested(root.actionEntryId, !root.actionEntry.pinned)
                        Keys.onRightPressed: Focus.take(closeAppAction.visible ? closeAppAction : favoriteRefresh.visible ? favoriteRefresh : actionCancel)
                        Keys.onDownPressed: Focus.take(favoriteRefresh.visible ? favoriteRefresh : actionCancel)
                    }
                    ActionButton {
                        width: 400
                        id: closeAppAction; objectName: "closeAppAction"; text: qsTr("Close application"); destructive: true
                        visible: !!root.actionEntry.runningId
                        onActivated: { root.confirmAppClose = true; Qt.callLater(() => Focus.take(closeAppCancel)); }
                        Keys.onLeftPressed: Focus.take(favoriteAction)
                        Keys.onRightPressed: Focus.take(favoriteRefresh.visible ? favoriteRefresh : actionCancel)
                        Keys.onDownPressed: Focus.take(actionCancel)
                    }
                    ActionButton {
                        width: 400
                        id: favoriteRefresh; objectName: "favoriteRefresh"; text: qsTr("Refresh list")
                        visible: root.favoriteError.length > 0; enabled: !root.favoriteSaving
                        onActivated: root.retryRequested()
                        Keys.onLeftPressed: Focus.take(favoriteAction)
                        Keys.onRightPressed: Focus.take(actionCancel)
                        Keys.onUpPressed: Focus.take(favoriteAction)
                    }
                    ActionButton {
                        width: 400
                        id: actionCancel; objectName: "actionCancel"; text: qsTr("Back")
                        onActivated: root.closeTileActions()
                        Keys.onLeftPressed: Focus.take(favoriteRefresh.visible ? favoriteRefresh : closeAppAction.visible ? closeAppAction : favoriteAction)
                        Keys.onUpPressed: Focus.take(closeAppAction.visible ? closeAppAction : favoriteAction)
                    }
                }
            }
        }
        Rectangle {
            objectName: "closeAppConfirmation"
            visible: root.confirmAppClose; z: 1
            anchors.fill: parent; color: Qt.rgba(0, 0, 0, 0.75)
            MouseArea { anchors.fill: parent; acceptedButtons: Qt.AllButtons }
            Rectangle {
                anchors.centerIn: parent; width: 1000; height: 360; radius: 40; color: Theme.background
                Column {
                    anchors.centerIn: parent; spacing: 40
                    UiText { text: qsTr("Close this application?"); role: "section" }
                    Row {
                        spacing: 24
                        ActionButton {
                            id: closeAppCancel; objectName: "closeAppCancel"; text: qsTr("Cancel")
                            onActivated: { root.confirmAppClose = false; Focus.take(closeAppAction); }
                            Keys.onRightPressed: Focus.take(closeAppConfirm)
                        }
                        ActionButton {
                            id: closeAppConfirm; objectName: "closeAppConfirm"; text: qsTr("Close"); destructive: true
                            onActivated: { root.terminateRequested(root.actionEntry.runningId); root.closeTileActions(); }
                            Keys.onLeftPressed: Focus.take(closeAppCancel)
                        }
                    }
                }
            }
        }

    }

}
