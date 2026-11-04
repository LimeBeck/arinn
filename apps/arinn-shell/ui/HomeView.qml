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
    // The on-screen keyboard over the shortcut editor (KB-2, KB-3): the
    // field being typed, "title" or "url", or empty.
    property string shortcutTypingField: ""
    readonly property string shortcutTypingText: shortcutTypingInput.text
    readonly property string shortcutDraftUrl: shortcutUrl.text
    // Suggestions computed by the shell's C++ catalog from the two above.
    property var shortcutAddressSuggestions: []
    property var shortcutNameSuggestions: []
    function startShortcutTyping(field) {
        if (root.shortcutSaving) return;
        const target = field === "url" ? shortcutUrl : shortcutTitle;
        shortcutTypingField = field;
        shortcutTypingInput.maximumLength = target.maximumLength;
        shortcutTypingInput.text = target.text;
        shortcutTypingInput.cursorPosition = shortcutTypingInput.text.length;
        shortcutKeyboard.start();
    }
    // Done moves on to the next control; Back returns to the field's button.
    function finishShortcutTyping(done) {
        const field = shortcutTypingField;
        if (!field.length) return;
        shortcutKeyboard.finish();
        shortcutTypingField = "";
        if (!done) Focus.take(field === "url" ? shortcutUrlKeyboard : shortcutTitleKeyboard);
        else if (field === "url") Focus.take(shortcutSave);
        else Focus.take(keyboardAvailable ? shortcutUrl : shortcutUrlKeyboard);
    }
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
            // Without a hardware keyboard the remote types through the panel.
            Qt.callLater(() => keyboardAvailable ? Focus.take(shortcutTitle) : Focus.take(shortcutTitleKeyboard));
        } else {
            if (shortcutTypingField.length) {
                shortcutKeyboard.finish();
                shortcutTypingField = "";
            }
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
    // Connectivity: "wifi", "wired", "other", "disconnected", or empty when
    // unknown. A cable shows "wired" and a crossed-out Wi-Fi shows
    // "disconnected"; "other" and unknown show nothing (#408).
    property string network: ""
    // True while an Activity background is shown. The floor then takes the
    // Activity's identity hue instead of switching off (owner decision on
    // #756, ADR-0140; amends ADR-0027).
    property bool ambientSuppressed: false
    // The identity of the application that owns the Return window, matched
    // by its platform window token; empty when no running window matches.
    property string returnActivityIdentity: ""
    readonly property string activityIdentity: {
        if (!ambientSuppressed) return "";
        if (returnIsWebService && currentServiceId) return currentServiceId;
        if (returnActivityIdentity) return returnActivityIdentity;
        const entry = continueEntries.find(item => item && item.id === returnActivityId && item.visualIdentity);
        return entry ? entry.visualIdentity : "";
    }
    // The last frame of the Activity behind Home (§12.3), empty for none.
    // Near while the Activity's own card has focus, otherwise far (handoff
    // Home 16). Never delays focus: it only replaces the plain background.
    property url activitySource: ""
    // A Return Activity verified as a browser service is shown by the
    // current-service card instead of its opaque entry.
    readonly property bool activityNear: screen === "home" && !navOpen && homeRow === "continue"
        && returnActivityId.length > 0
        && (focusedEntry.id === returnActivityId
            || (returnIsWebService && focusedEntry.id === "current-service"))
    // Settings › Appearance › Light field (spec §15, ADR-0112).
    property bool ambientEnabled: true
    // Settings › Appearance › Light follows the time of day (spec §5.6, ADR-0136).
    property bool lightFollowsTime: true
    // The shell's input mode (ADR-0095); tests may assign it directly.
    property bool pointerMode: InputMode.pointerActive
    property bool actionsEnabled: false
    property bool favoriteSaving: false
    property string favoriteError: ""
    property string actionEntryId: ""
    property var actionOpener: null
    readonly property bool tileActionsOpen: actionEntryId.length > 0
    readonly property var actionEntry: entryById(actionEntryId) || ({})
    signal favoriteRequested(string entryId, bool pinned)
    // Close a running window: the shell's close flow asks first (CL-8).
    signal closeRequested(string windowId)
    // The running window of the application on screen, when Arinn can close
    // it: Continue's Close button (CL-3a) and the Running item's subline.
    property string activityWindowId: ""
    property string activityWindowTitle: ""
    // Windows whose close request is in flight (CL-9).
    property var closingIds: []
    readonly property int runningCount: runningEntries.length
    function restoreActionFocus() {
        if (actionsEnabled) Focus.take(favoriteAction);
        else if (favoriteRefresh.visible && favoriteRefresh.enabled) Focus.take(favoriteRefresh);
        else if (closeAppAction.visible) Focus.take(closeAppAction);
        else Focus.take(favoriteAction);
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
        actionEntryId = entryId;
        Qt.callLater(restoreActionFocus);
    }
    // Pin or unpin from the sheet: applied, then the sheet closes after
    // 180 ms (CL-2).
    function toggleFavorite() {
        favoriteRequested(actionEntryId, !actionEntry.pinned);
        sheetCloseTimer.restart();
    }
    // Close from the sheet: the sheet gives way to the shell's confirmation.
    function closeFromSheet() {
        const windowId = actionEntry.runningId || "";
        actionEntryId = "";
        const opener = actionOpener;
        actionOpener = null;
        if (opener && opener.visible && opener.enabled) Focus.take(opener);
        if (windowId.length) closeRequested(windowId);
    }
    function closeTileActions() {
        sheetCloseTimer.stop();
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
    // The hero starts at the brief's y 150 and reserves two lines at 68 px.
    // The title grows upward in that slot. The first row stays this far
    // under the block, so it does not move when the title wraps (#722).
    // The menu ends at y 108; the block stays below it. Captions sit under
    // the images, so Continue is the picture plus its caption. The shelf is
    // a fixed 125 px step, not its own height plus a gap: that puts the New
    // poster 94 px onto the 1080 canvas while Continue is focused (#725).
    readonly property real heroRowGap: 44
    readonly property real heroTwoLineSlot: Math.round(68 * 1.07) * 2
    readonly property real firstRowY: heroBox.y + heroBox.height + heroRowGap
    readonly property real homeRowStride: continueRow.height + Theme.rowGap
    readonly property real shelfStep: 125
    readonly property real newRowStep: favoritesRow.variant === "shelf" ? shelfStep : favoritesRow.height + Theme.rowGap
    // The menu button and the status area end at y 108. A row fades out as
    // its on-screen top (the animated translation, not the target offset)
    // nears this line: fully shown from 60 px below it, gone at it, so no card
    // slides under the header during or after the New row's offset (#659).
    readonly property real headerClearance: 120
    readonly property real headerFade: 60
    function rowHeaderOpacity(row) {
        const top = row.y + homeShift.y;
        return Math.max(0, Math.min(1, (top - headerClearance) / headerFade));
    }
    function rowClearsHeader(row) { return rowHeaderOpacity(row) >= 1; }
    onHomeOffsetChanged: hoverDelay.restart()
    readonly property bool hoverFocusEnabled: !hoverDelay.running
    property string musicTitle: ""
    readonly property var sections: [qsTr("Home"), qsTr("Library")].concat(musicTitle.length ? [musicTitle] : []).concat([qsTr("Downloads"), qsTr("Applications"), qsTr("Settings")])
    readonly property var routes: ["home", "library"].concat(musicTitle.length ? ["music"] : []).concat(["downloads", "applications", "settings"])
    signal musicRequested()
    signal musicTransportRequested()
    signal libraryEntryRequested(string entryId)
    signal libraryResumeRequested(string entryId)
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
        // A fresh shell may know the open tab before it has Return or media
        // evidence. The summary is hidden for one tab, so give that tab its
        // own card. Its action activates the tab; it must not imply Return.
        if (!shown && openServices.length === 1
                && !activity.some(entry => entry.serviceId === openServices[0].id)) {
            const tab = openServices[0];
            const linked = rest.findIndex(entry => entry.serviceId === tab.id);
            if (linked < 0)
                rest.unshift({id: "open-service", serviceId: tab.id, visualIdentity: tab.id,
                              title: tab.title, subtitle: tab.domain || "", running: true, kind: "tab"});
            else if (linked >= 4)
                rest.unshift(rest.splice(linked, 1)[0]);
        }
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
        // CL-3a: an application's activity card gets a Close button beside
        // it; a web-service card does not.
        const closer = !shown && lead && lead.id === returnActivityId && !returnIsWebService
                && activityWindowId.length ? [{
            id: "close:" + activityWindowId,
            windowId: activityWindowId,
            title: activityWindowTitle || lead.title || "",
            kind: "close"
        }] : [];
        return activity.concat(closer).concat(more.concat(rest.map(mediaCard)).slice(0, 4));
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
    // The hero's eyebrow for a media card: what it is, never the row label
    // above it (Hearth light brief, Home; #756 C20).
    function mediaEyebrow(entry) {
        if (entry.mediaKind === "series") {
            const parts = [qsTr("Series")];
            if (entry.season !== undefined && entry.episode !== undefined)
                parts.push(qsTr("S%1 E%2").arg(entry.season).arg(entry.episode));
            return parts.join(" · ");
        }
        if (entry.mediaKind === "film")
            return entry.year ? qsTr("Film") + " · " + entry.year : qsTr("Film");
        return entry.running ? qsTr("Playing") : qsTr("Paused");
    }
    // What the hero says about the focused Home item and what OK does
    // (#364, handoff 01–03).
    function describe(row, entry) {
        if (!entry || !entry.id)
            return {eyebrow: qsTr("Continue"), title: qsTr("Make yourself at home"), meta: qsTr("Choose something to open"), live: false};
        if (row === "favorites") {
            const group = groupTitles[entry.group];
            const open = !!entry.running || openShortcutIds.indexOf(entry.id) >= 0;
            const unavailable = entry.unavailable || (browserUnavailable && entry.webShortcut);
            return {
                eyebrow: group ? favoritesTitle + " · " + group : favoritesTitle,
                title: entry.title || "",
                meta: unavailable ? (browserUnavailable && entry.webShortcut ? qsTr("Unavailable") : entry.subtitle || "")
                    : entry.hint ? entry.hint : open ? qsTr("Running · Return") : qsTr("Opens full screen"),
                live: false
            };
        }
        if (entry.kind === "more")
            return {eyebrow: qsTr("Open services"), title: entry.title, meta: entry.names || entry.subtitle, live: false};
        if (entry.kind === "close")
            return {eyebrow: qsTr("Now running"), title: entry.title || "",
                    meta: qsTr("Close %1").arg(entry.title || ""), live: true};
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
            return {eyebrow: mediaEyebrow(entry), title: entry.title || "", meta: meta, live: false};
        }
        if (entry.kind === "tab")
            return {eyebrow: qsTr("Now running"), title: entry.title || "",
                    meta: entry.subtitle ? entry.subtitle + " · " + qsTr("Return") : qsTr("Return"), live: true};
        if (entry.running)
            return {eyebrow: qsTr("Now running"), title: entry.title || "",
                    meta: entry.kind === "service" && entry.subtitle ? entry.subtitle : qsTr("Return"), live: true};
        return {eyebrow: qsTr("Continue"), title: entry.title || "", meta: entry.subtitle || "", live: false};
    }
    // The focused entry as currently presented; a stale snapshot otherwise.
    readonly property var heroEntry: {
        const pool = homeRow === "continue" ? continuePresentation : homeRow === "favorites" ? favoriteEntries : newEntries;
        return (pool || []).find(entry => entry && entry.id === focusedEntry.id) || focusedEntry;
    }
    readonly property var hero: describe(homeRow, heroEntry)
    // The pool takes the focused content item. The open-services summary has
    // no single hue. Applications use the focused tile.
    readonly property var lightEntry: {
        if (screen === "applications")
            return focusedEntry && focusedEntry.id ? focusedEntry : null;
        if (screen === "home" && (homeRow === "continue" || homeRow === "favorites" || homeRow === "new"))
            return heroEntry && heroEntry.id && heroEntry.kind !== "more" ? heroEntry : null;
        return null;
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
            return true;
        }
        if (key === Qt.Key_Down && addApplication.visible && row.objectName !== "runningApplications") {
            const rows = applicationRows();
            const next = rows[rows.indexOf(row) + 1];
            if (!next || next.objectName === "runningApplications") {
                Focus.take(addApplication);
                return;
            }
        }
        if (key === Qt.Key_Up && row.objectName === "runningApplications" && addApplication.visible) {
            Focus.take(addApplication);
            return;
        }
        const rows = applicationRows();
        const step = key === Qt.Key_Up ? -1 : 1;
        for (let i = rows.indexOf(row) + step; i >= 0 && i < rows.length; i += step) {
            if (rows[i].focusIndex(row.selectedIndex) || rows[i].restore()) return true;
        }
        if (key === Qt.Key_Up) { sectionOffset = 0; Focus.take(menuButton); return true; }
        Focus.take(sectionBack);
        return true;
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
        interfaceSounds.playOpen();
        Qt.callLater(() => Focus.take(navItems.itemAt(navIndex)));
    }
    function closeNavigation() {
        if (!navOpen) return;
        navOpen = false;
        interfaceSounds.playClose();
        if (navigationOpener && navigationOpener.visible && navigationOpener.enabled)
            Focus.take(navigationOpener);
        else restoreScreen();
        navigationOpener = null;
    }
    // The shell closes the overlay first: Home is enabled again only then,
    // so showHome() can focus a visible control (review of #334).
    function homeKey() {
        if (tileActionsOpen) { actionEntryId = ""; actionOpener = null; }
        interfaceSounds.play("kindle");
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
    // The Running applications screen (CL-4), drawn by the shell.
    function showRunning() {
        if (shortcutEditorOpen) closeShortcutEditorRequested();
        sectionOffset = 0;
        navOpen = false;
        screen = "running";
        sectionRequested(screen);
    }
    // "On screen: {name}" while an application is on screen (CL-7).
    readonly property string runningSubline: activityWindowTitle.length && !returnIsWebService
        ? qsTr("On screen: %1").arg(activityWindowTitle) : ""
    function returnFromLibraryDetail() {
        screen = "home";
        if (newRow.visible && newRow.restore()) return;
        focusAfterRemoval(newRow);
    }
    function back() {
        if (tileActionsOpen) {
            interfaceSounds.playBack();
            closeTileActions();
        } else if (navOpen) closeNavigation();
        else if (screen !== "home") {
            interfaceSounds.playBack();
            screen = "home";
            restoreHome();
        } else if (returnActivityId) {
            interfaceSounds.playBack();
            returnRequested(returnActivityId);
        } else openNavigation();
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
            if (rows[i].visible && rows[i].focusIndex(row.selectedIndex)) return true;
        }
        if (key === Qt.Key_Up || !row.count) { Focus.take(menuButton); return true; }
        return false;
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
        interfaceSounds.play("select");
        const entry = entryById(entryId);
        if (entry && entry.kind === "music") { musicRequested(); return; }
        if (entry && entry.resumeContentId) { libraryResumeRequested(entry.resumeContentId); return; }
        if (newEntries.some(item => item.id === entryId)) { libraryEntryRequested(entryId); return; }
        if (entry && entry.launchable === false) { openTileActions(entryId); return; }
        if (entry && entry.kind === "more") { openServicesRequested(); return; }
        if (entry && entry.kind === "close") { closeRequested(entry.windowId || ""); return; }
        if (entry && entry.kind === "service") { serviceRequested(entry.serviceId || ""); return; }
        if (entry && entry.kind === "tab") { tabRequested(entry.serviceId || ""); return; }
        if (entry && entry.kind === "media") { sessionRequested(entryId); return; }
        if (browserUnavailable && entry && entry.webShortcut) { openServicesRequested(); return; }
        if (entryId === returnActivityId) returnRequested(entryId);
        else entryRequested(entryId);
    }
    onHomeStateChanged: Qt.callLater(() => { if (!navOpen && !shortcutEditorOpen && screen === "home") restoreHome(); })
    onReturnActivityIdChanged: {
        // A window the close flow is closing gets its own "closed" toast.
        if (!returnActivityId && !closingIds.length) {
            activityNotice.visible = true;
            noticeTimer.restart();
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
            menuKey();
            event.accepted = true;
        } else if (event.key === Qt.Key_Escape || event.key === Qt.Key_Backspace) {
            // Editable controls consume Backspace before it bubbles to this scope.
            // One step per press: repeats of a held key are ignored (#246).
            if (!event.isAutoRepeat) back();
            event.accepted = true;
        } else if (event.key === Qt.Key_Tab || event.key === Qt.Key_Backtab) {
            // Do not let Qt's implicit tab chain escape the active scope.
            if (navOpen) moveNavigation(event.key === Qt.Key_Backtab ? -1 : 1, event.isAutoRepeat);
            event.accepted = true;
        }
    }
    // A short Menu: toggle navigation, or the shortcut editor on a web
    // shortcut in Applications. A tile turns a long Menu into its sheet and
    // reports a short one here (HomeRow.menuTapped).
    function menuKey() {
        if (shortcutManagement && !navOpen && screen === "applications" && focusedEntry.webShortcut)
            editShortcut(focusedEntry.id);
        else if (navOpen) closeNavigation(); else openNavigation();
    }
    function navIcon(index) {
        const icons = {home: "home", library: "library", music: "volume_up", downloads: "download", applications: "apps", settings: "settings"};
        return index < root.routes.length ? icons[root.routes[index]] || ""
            : ["public", "screen", "power", "close"][index - root.routes.length] || "";
    }
    function moveNavigation(step, autoRepeat) {
        let next = navIndex + step;
        while (next >= 0 && next < navItems.count) {
            const item = navItems.itemAt(next);
            if (item.visible) {
                Focus.take(item);
                interfaceSounds.playNavigationTap(autoRepeat);
                return;
            }
            next += step;
        }
    }
    Rectangle { anchors.fill: parent; color: Theme.background; visible: !root.coveredByScreen }
    ActivityBackground {
        objectName: "activityBackground"
        anchors.fill: parent
        visible: !root.coveredByScreen && root.screen === "home" && root.activitySource.toString() !== ""
        source: root.activitySource
        near: root.activityNear
    }
    AmbientGlow {
        id: ambient
        objectName: "ambientGlow"
        anchors.fill: parent
        visible: !root.coveredByScreen
        veilParent: root
        followsTime: root.lightFollowsTime
        clockText: root.clockText
        // Home and Applications are content screens. Over an Activity the
        // floor takes its identity hue (ADR-0140). The pool takes the focused item.
        lit: root.ambientEnabled && (root.screen === "applications"
             || (root.screen === "home" && root.homeState === "ready"))
        floorTint: root.screen === "home" && root.activityIdentity ? Theme.identityColor(root.activityIdentity) : "transparent"
        // Home 1. Applications 0.6, with the Library; the prototype's 0.8 is
        // not used (owner decision on #732). Detail is 0.5.
        strength: root.screen === "applications" ? 0.6 : 1
        source: root.lightEntry ? (root.lightEntry.artwork || "") : ""
        tint: root.lightEntry ? Theme.identityColor(root.lightEntry.visualIdentity || root.lightEntry.id) : "transparent"
    }
    Item {
        id: content
        anchors.fill: parent
        visible: !root.coveredByScreen
        enabled: !root.navOpen && !root.tileActionsOpen
        opacity: root.navOpen ? 0.45 : 1
        Behavior on opacity { NumberAnimation { duration: 320 } }
        BrandButton {
            id: menuButton
            objectName: "menuButton"
            // Above the rows, which a New row offset can move into the
            // header band (#659).
            z: 1
            x: Theme.safeLeft - lockupInset; y: 44; height: 64; text: "Arinn"
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
            z: 1
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
                color: "transparent"
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
            Canvas {
                objectName: "wiredIcon"
                anchors.verticalCenter: parent.verticalCenter
                visible: root.network === "wired" && !root.wifi
                width: 32; height: 32
                Accessible.role: Accessible.StaticText
                Accessible.name: qsTr("Connected by cable")
                onPaint: {
                    // Material Symbols lan: one node above two, joined.
                    const ctx = getContext("2d");
                    ctx.clearRect(0, 0, width, height);
                    ctx.fillStyle = Theme.textSecondary;
                    ctx.strokeStyle = Theme.textSecondary;
                    ctx.lineWidth = 2.6;
                    ctx.fillRect(12, 3, 8, 8);
                    ctx.fillRect(3, 21, 8, 8);
                    ctx.fillRect(21, 21, 8, 8);
                    ctx.beginPath();
                    ctx.moveTo(16, 11); ctx.lineTo(16, 16);
                    ctx.moveTo(7, 21); ctx.lineTo(7, 16); ctx.lineTo(25, 16); ctx.lineTo(25, 21);
                    ctx.stroke();
                }
            }
            Canvas {
                objectName: "noNetworkIcon"
                anchors.verticalCenter: parent.verticalCenter
                visible: root.network === "disconnected" && !root.wifi
                width: 32; height: 32
                Accessible.role: Accessible.StaticText
                Accessible.name: qsTr("No network")
                onPaint: {
                    // Material Symbols wifi_off: the Wi-Fi glyph, crossed out.
                    const ctx = getContext("2d");
                    ctx.clearRect(0, 0, width, height);
                    ctx.lineWidth = 3;
                    ctx.lineCap = "round";
                    ctx.fillStyle = Theme.textSecondary;
                    ctx.strokeStyle = Theme.textSecondary;
                    ctx.beginPath(); ctx.arc(16, 25, 2.6, 0, Math.PI * 2); ctx.fill();
                    for (let part = 2; part <= 4; ++part) {
                        ctx.beginPath();
                        ctx.arc(16, 25, 6 + (part - 2) * 6, Math.PI * 1.22, Math.PI * 1.78);
                        ctx.stroke();
                    }
                    // The slash, with a gap cut out of the arcs around it.
                    ctx.globalCompositeOperation = "destination-out";
                    ctx.lineWidth = 7;
                    ctx.beginPath();
                    ctx.moveTo(5, 4); ctx.lineTo(27, 28);
                    ctx.stroke();
                    ctx.globalCompositeOperation = "source-over";
                    ctx.lineWidth = 3;
                    ctx.beginPath();
                    ctx.moveTo(5, 4); ctx.lineTo(27, 28);
                    ctx.stroke();
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
                    id: homeShift
                    y: root.homeOffset
                    Behavior on y { NumberAnimation { duration: 450; easing.type: Easing.OutCubic } }
                }
                // Two-line title slot at 68 px, bottom-aligned, so a one-line
                // title sits lower and a two-line title grows upward. The
                // block stays at y 150. The rows stay heroRowGap under it.
                Column {
                    id: heroBox
                    objectName: "heroBox"
                    x: 120; y: 150; width: 1300; spacing: 4
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
                    UiText {
                        objectName: "heroTitle"
                        width: parent.width
                        height: root.heroTwoLineSlot
                        verticalAlignment: Text.AlignBottom
                        role: "hero"
                        text: root.hero.title
                        fitSteps: [[96, 1], [68, 2]]
                        lineHeightMode: Text.FixedHeight
                        lineHeight: Math.round(font.pixelSize * 1.07)
                        elide: Text.ElideRight
                    }
                    UiText {
                        id: heroMeta
                        objectName: "heroMeta"
                        width: parent.width; role: "meta"; font.pixelSize: 28
                        text: root.hero.meta; maximumLineCount: 1; elide: Text.ElideRight
                        color: Theme.textSecondary
                    }
                }
                HomeRow {
                    id: continueRow; objectName: "continue"
                    opacity: root.rowHeaderOpacity(continueRow)
                    pointerBlocked: !root.rowClearsHeader(continueRow)
                    x: 120; y: root.firstRowY; width: 1704
                    visible: count > 0
                    title: qsTr("Continue"); entries: root.continuePresentation; busyEntryIds: root.busyEntryIds
                    closingIds: root.closingIds
                    castHue: isNaN(ambient.hue) ? Theme.hearthHue : ambient.hue
                    pointerMode: root.pointerMode; hoverFocusEnabled: root.hoverFocusEnabled && root.rowClearsHeader(continueRow)
                    onSelected: entry => root.remember(continueRow, entry)
                    onActivated: entryId => root.activate(entryId)
                    onSecondaryActivated: entryId => { const entry=root.entryById(entryId); if(entry && entry.kind === "music") {interfaceSounds.play("select");root.musicTransportRequested();} }
                    onEdge: (key, autoRepeat) => {
                        if (key === Qt.Key_Left || key === Qt.Key_Right) {
                            if (key === Qt.Key_Right) return;
                            root.rowEdge(continueRow, key);
                            return;
                        }
                        if (root.rowEdge(continueRow, key))
                            interfaceSounds.playNavigationTap(autoRepeat);
                    }
                    onFocusLost: root.focusAfterRemoval(continueRow)
                    onScrolled: hoverDelay.restart()
                    onDpadRequested: InputMode.keyPressed()
                }
                HomeRow {
                    id: favoritesRow; objectName: "favorites"
                    opacity: root.rowHeaderOpacity(favoritesRow)
                    pointerBlocked: !root.rowClearsHeader(favoritesRow)
                    x: 120; y: continueRow.visible ? continueRow.y + root.homeRowStride : root.firstRowY; width: 1704
                    visible: count > 0
                    title: root.favoritesTitle; variant: "shelf"
                    entries: root.favoriteEntries; busyEntryIds: root.busyEntryIds
                    browserUnavailable: root.browserUnavailable; openShortcutIds: root.openShortcutIds
                    pointerMode: root.pointerMode; hoverFocusEnabled: root.hoverFocusEnabled && root.rowClearsHeader(favoritesRow)
                    onActionsRequested: entryId => root.openTileActions(entryId)
                    onMenuTapped: root.menuKey()
                    onSelected: entry => root.remember(favoritesRow, entry)
                    onActivated: entryId => root.activate(entryId)
                    onEdge: (key, autoRepeat) => {
                        if (key === Qt.Key_Left || key === Qt.Key_Right) {
                            if (key === Qt.Key_Right) return;
                            root.rowEdge(favoritesRow, key);
                            return;
                        }
                        if (root.rowEdge(favoritesRow, key))
                            interfaceSounds.playNavigationTap(autoRepeat);
                    }
                    onFocusLost: root.focusAfterRemoval(favoritesRow)
                    onScrolled: hoverDelay.restart()
                    onDpadRequested: InputMode.keyPressed()
                }
                HomeRow {
                    id: newRow; objectName: "new"
                    x: 120; y: favoritesRow.visible ? favoritesRow.y + root.newRowStep : continueRow.visible ? continueRow.y + root.homeRowStride : root.firstRowY; width: 1704
                    visible: count > 0
                    title: activeFocus ? qsTr("New") + " · " + (root.focusedEntry.title || "") : qsTr("New")
                    variant: "poster"; entries: root.newEntries; busyEntryIds: root.busyEntryIds
                    castHue: isNaN(ambient.hue) ? Theme.hearthHue : ambient.hue
                    pointerMode: root.pointerMode; hoverFocusEnabled: root.hoverFocusEnabled
                    onSelected: entry => root.remember(newRow, entry)
                    onActivated: entryId => root.activate(entryId)
                    onEdge: (key, autoRepeat) => {
                        if (key === Qt.Key_Left || key === Qt.Key_Right) {
                            if (key === Qt.Key_Right) return;
                            root.rowEdge(newRow, key);
                            return;
                        }
                        if (root.rowEdge(newRow, key))
                            interfaceSounds.playNavigationTap(autoRepeat);
                    }
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
                    width: 1704; variant: "app"; identityLight: true
                    cellWidth: 300
                    title: root.groupTitles[modelData]
                    visible: root.screen === "applications" && count > 0
                    entries: root.groupEntries(modelData); busyEntryIds: root.busyEntryIds
                    browserUnavailable: root.browserUnavailable; openShortcutIds: root.openShortcutIds
                    pointerMode: root.pointerMode; hoverFocusEnabled: root.hoverFocusEnabled
                    onActionsRequested: entryId => root.openTileActions(entryId)
                    onMenuTapped: root.menuKey()
                    onSelected: entry => { root.focusedEntry = entry; root.applicationFocusRow = groupRow.objectName; root.revealRow(groupRow); }
                    onActivated: entryId => {
                        const entry = groupRow.entries.find(e => e.id === entryId);
                        if (entry && entry.launchable === false) root.openTileActions(entryId);
                        else root.entryRequested(entryId);
                    }
                    onEdge: (key, autoRepeat) => {
                        if (key === Qt.Key_Left || key === Qt.Key_Right) {
                            if (key === Qt.Key_Right) return;
                            root.applicationEdge(groupRow, key);
                            return;
                        }
                        if (root.applicationEdge(groupRow, key))
                            interfaceSounds.playNavigationTap(autoRepeat);
                    }
                    onFocusLost: root.applicationFocusLost(groupRow)
                    onScrolled: hoverDelay.restart()
                    onDpadRequested: InputMode.keyPressed()
                }
            }
            HomeRow {
                id: runningRow; objectName: "runningApplications"
                width: 1704; variant: "app"; identityLight: true; cellWidth: 300; title: qsTr("Open windows")
                visible: root.screen === "applications" && count > 0
                entries: root.runningEntries; busyEntryIds: root.busyEntryIds
                pointerMode: root.pointerMode; hoverFocusEnabled: root.hoverFocusEnabled
                onSelected: { root.applicationFocusRow = runningRow.objectName; root.revealRow(runningRow); }
                onMenuTapped: root.menuKey()
                onActivated: entryId => root.runningRequested(entryId)
                onEdge: (key, autoRepeat) => {
                    if (key === Qt.Key_Left || key === Qt.Key_Right) {
                        if (key === Qt.Key_Right) return;
                        root.applicationEdge(runningRow, key);
                        return;
                    }
                    if (root.applicationEdge(runningRow, key))
                        interfaceSounds.playNavigationTap(autoRepeat);
                }
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
                    if (from && root.focusApplicationRowBelow(from))
                        return;
                    if (addApplication.visible)
                        Focus.take(addApplication);
                    else
                        Focus.take(sectionBack);
                }
            }
            Surface {
                id: addApplication
                objectName: "addApplication"
                visible: root.shortcutManagement && root.screen === "applications"
                text: qsTr("Add application")
                shapeRole: "action"
                implicitWidth: Math.max(220, addLabel.implicitWidth + 96)
                implicitHeight: 88
                width: implicitWidth
                height: implicitHeight
                cornerRadius: height / 2
                focusScale: 1
                focusLift: 0
                liftFromBottom: false
                restingColor: Theme.plateAction
                focusedColor: Theme.textPrimary
                pointerMode: root.pointerMode
                hoverFocusEnabled: root.hoverFocusEnabled
                onDpadRequested: InputMode.keyPressed()
                onActivated: root.editShortcut("")
                Keys.onLeftPressed: root.openNavigation()
                Keys.onUpPressed: {
                    if (shortcutEditButton.visible) Focus.take(shortcutEditButton);
                    else root.restoreScreen();
                }
                Keys.onDownPressed: {
                    if (!(runningRow.visible && (runningRow.focusIndex(0) || runningRow.restore())))
                        Focus.take(sectionBack);
                }
                contentItem: Row {
                    anchors.centerIn: parent
                    spacing: 12
                    LineIcon {
                        width: 28; height: 28
                        anchors.verticalCenter: parent.verticalCenter
                        name: "add"
                        color: addApplication.activeFocus ? Theme.ink : Theme.textPrimary
                    }
                    UiText {
                        id: addLabel
                        text: addApplication.text
                        role: "control"
                        color: addApplication.activeFocus ? Theme.ink : Theme.textPrimary
                        wrapMode: Text.NoWrap
                    }
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
    // The tile hint (handoff 13, *Closing applications*): only while an
    // application tile has focus.
    UiText {
        objectName: "tileHint"
        x: Theme.safeLeft; y: root.height - 72
        visible: !root.coveredByScreen && !root.navOpen && !root.tileActionsOpen
                 && ((root.screen === "applications" && !!root.focusedEntry.providerId)
                     || (root.screen === "home" && root.homeRow === "favorites" && !!root.focusedEntry.providerId))
        text: qsTr("OK — open · Menu — actions")
        font.pixelSize: 24
        color: Theme.textHint
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
        // A covering section is not Home's content, so dim it here. Home's
        // own stage is dimmed by the content item (brightness 0.45).
        Rectangle { anchors.fill: parent; color: Qt.rgba(0, 0, 0, 0.55); visible: root.coveredByScreen }
        MouseArea { anchors.fill: parent; onClicked: root.closeNavigation() }
        Item {
            id: navigationPanel
            objectName: "navigationPanel"
            readonly property int sectionStep: root.musicTitle.length ? 70 : 84
            width: 660
            height: parent.height
            Rectangle {
                anchors.fill: parent
                gradient: Gradient {
                    orientation: Gradient.Horizontal
                    GradientStop { position: 0; color: Qt.rgba(Theme.background.r, Theme.background.g, Theme.background.b, 0.98) }
                    GradientStop { position: 0.72; color: Qt.rgba(Theme.background.r, Theme.background.g, Theme.background.b, 0.95) }
                    GradientStop { position: 1; color: Qt.rgba(Theme.background.r, Theme.background.g, Theme.background.b, 0) }
                }
            }
            MouseArea { anchors.fill: parent }
            BrandLockup {
                id: navBrand
                objectName: "navigationBrand"
                x: 128; y: 50; height: 40; width: implicitWidth
                markSize: 40; textRole: "section"
            }
            Rectangle {
                objectName: "navigationHairline"
                x: 104; width: 452; height: 1
                y: navBrand.y + navBrand.height + 52 + root.sections.length * navigationPanel.sectionStep + 18
                color: Qt.rgba(1, 1, 1, 0.08)
            }
            Repeater {
                id: navItems
                model: root.sections.concat([qsTr("Open services"), qsTr("Running"), qsTr("Power"), qsTr("Close Arinn")])
                delegate: Surface {
                    id: navButton
                    required property int index
                    required property string modelData
                    objectName: "nav-" + index
                    x: 104
                    y: {
                        if (index === root.sections.length + 2) return navigationPanel.height - 148 - height;
                        if (index === root.sections.length + 3) return navigationPanel.height - 56 - height;
                        const top = navBrand.y + navBrand.height + 52;
                        if (index < root.sections.length) return top + index * navigationPanel.sectionStep;
                        const after = top + root.sections.length * navigationPanel.sectionStep + 37;
                        if (index === root.sections.length) return after;
                        return after + (root.currentServiceTitle.length ? 104 : 84);
                    }
                    width: 452
                    height: index < root.sections.length ? navigationPanel.sectionStep
                          : index === root.sections.length && root.currentServiceTitle.length ? 104
                          : index === root.sections.length + 1 && root.runningSubline.length ? 104 : 84
                    text: modelData
                    // Running (CL-7) stays when nothing runs; only its count hides.
                    shapeRole: "container"
                    focusScale: 1
                    focusLift: 0
                    liftFromBottom: false
                    cornerRadius: Theme.containerRadius
                    restingColor: "transparent"
                    focusedColor: Theme.plateContainer
                    readonly property color labelColor: activeFocus ? Theme.textPrimary : Theme.textSecondary
                    readonly property bool sectionDot: index < root.sections.length && root.routes[index] === root.screen
                    readonly property bool activityDot: (index === root.sections.length && root.returnIsWebService
                            && (root.currentServiceTitle.length > 0 || root.openServiceCount > 0))
                            || (index === root.sections.length + 1 && root.runningSubline.length > 0)
                    contentItem: Item {
                        Rectangle {
                            objectName: "navCurrentDot"
                            x: -22
                            anchors.verticalCenter: parent.verticalCenter
                            width: 10; height: 10; radius: 5; color: Theme.ember
                            visible: navButton.sectionDot || navButton.activityDot
                        }
                        Row {
                            x: 24
                            anchors.verticalCenter: parent.verticalCenter
                            width: parent.width - 120
                            spacing: 22
                            LineIcon {
                                width: 36; height: 36
                                anchors.verticalCenter: parent.verticalCenter
                                name: root.navIcon(index)
                                color: navButton.activeFocus ? Theme.textPrimary : Theme.textHint
                            }
                            Column {
                                width: parent.width - 58
                                spacing: 2
                                anchors.verticalCenter: parent.verticalCenter
                                UiText {
                                    width: parent.width
                                    text: modelData
                                    role: "control"
                                    font.pixelSize: 32
                                    color: navButton.labelColor
                                    maximumLineCount: 1; elide: Text.ElideRight
                                }
                                UiText {
                                    objectName: index === root.sections.length ? "openServicesSubline"
                                              : index === root.sections.length + 1 ? "runningSubline" : ""
                                    width: parent.width
                                    visible: text.length > 0
                                    text: index === root.sections.length && root.currentServiceTitle.length
                                          ? qsTr("On screen: %1").arg(root.currentServiceTitle)
                                          : index === root.sections.length + 1 ? root.runningSubline : ""
                                    role: "caption"
                                    color: Theme.textSecondary
                                    maximumLineCount: 1; elide: Text.ElideRight
                                }
                            }
                        }
                        Rectangle {
                            objectName: index === root.sections.length ? "openServicesBadge"
                                      : index === root.sections.length + 1 ? "runningBadge" : ""
                            visible: (index === root.sections.length && root.openServiceCount > 0)
                                     || (index === root.sections.length + 1 && root.runningCount > 0)
                            anchors.right: parent.right
                            anchors.rightMargin: 20
                            anchors.verticalCenter: parent.verticalCenter
                            width: Math.max(36, openServicesCount.implicitWidth + 16)
                            height: 32
                            radius: 16
                            color: Theme.ember
                            UiText {
                                id: openServicesCount
                                objectName: index === root.sections.length ? "openServicesCount"
                                          : index === root.sections.length + 1 ? "runningCountBadge" : ""
                                anchors.centerIn: parent
                                text: index === root.sections.length ? String(root.openServiceCount)
                                      : index === root.sections.length + 1 ? String(root.runningCount) : ""
                                font.pixelSize: 24
                                font.weight: 600
                                color: Theme.ink
                            }
                        }
                        UiText {
                            objectName: root.routes[index] === "downloads" ? "downloadNavPercent" : ""
                            visible: root.routes[index] === "downloads" && root.downloadActive
                            anchors.right: parent.right
                            anchors.rightMargin: 24
                            anchors.verticalCenter: parent.verticalCenter
                            text: root.downloadProgress >= 0
                                    ? qsTr("%1%").arg(Math.floor(Math.min(1, root.downloadProgress) * 100))
                                    : qsTr("Downloading")
                            role: "caption"
                            color: Theme.textSecondary
                        }
                    }
                    pointerMode: root.pointerMode; hoverFocusEnabled: root.hoverFocusEnabled
                    onActiveFocusChanged: if (activeFocus) root.navIndex = index
                    onDpadRequested: InputMode.keyPressed()
                    onActivated: {
                        if (index < root.sections.length) root.selectSection(index);
                        else if (index === root.sections.length) { root.navOpen = false; root.openServicesRequested(); }
                        else if (index === root.sections.length + 1) root.showRunning();
                        else if (index === root.sections.length + 2) root.powerRequested();
                        else root.closeShellRequested();
                    }
                    Keys.onTabPressed: event => { root.moveNavigation(1, event.isAutoRepeat); event.accepted = true; }
                    Keys.onBacktabPressed: event => { root.moveNavigation(-1, event.isAutoRepeat); event.accepted = true; }
                    Keys.onUpPressed: event => { root.moveNavigation(-1, event.isAutoRepeat); event.accepted = true; }
                    Keys.onDownPressed: event => { root.moveNavigation(1, event.isAutoRepeat); event.accepted = true; }
                    Keys.onLeftPressed: {}
                    Keys.onRightPressed: {}
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
            else if (event.key === Qt.Key_Escape || event.key === Qt.Key_Backspace || event.key === Qt.Key_Back) {
                if (removeConfirmation.visible) {
                    removeConfirmation.visible = false;
                    Focus.take(shortcutDelete);
                } else root.closeShortcutEditorRequested();
            }
        }
        Rectangle { anchors.fill: parent; color: Qt.rgba(Theme.background.r, Theme.background.g, Theme.background.b, 0.9); MouseArea { anchors.fill: parent } }
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
                        anchors.rightMargin: shortcutTitleKeyboard.width + 32
                        font.family: Theme.fontFamily; font.pixelSize: 36; color: Theme.textPrimary
                        selectByMouse: true; clip: true
                        readOnly: root.shortcutSaving || !root.keyboardAvailable
                        maximumLength: 256
                        Keys.onDownPressed: Focus.take(shortcutUrl)
                        Keys.onReturnPressed: Focus.take(shortcutUrl)
                        Keys.onEnterPressed: Focus.take(shortcutUrl)
                        Keys.onEscapePressed: root.closeShortcutEditorRequested()
                        Keys.onPressed: event => {
                            if (event.key === Qt.Key_Home) { root.homeKey(); event.accepted = true; }
                            // Right at the end of the text reaches the keyboard button.
                            else if (event.key === Qt.Key_Right && cursorPosition === length) {
                                Focus.take(shortcutTitleKeyboard); event.accepted = true;
                            }
                        }
                    }
                    KeyboardButton {
                        id: shortcutTitleKeyboard; objectName: "shortcutTitleKeyboard"
                        x: parent.width - width - 8; anchors.verticalCenter: parent.verticalCenter
                        enabled: !root.shortcutSaving
                        onActivated: root.startShortcutTyping("title")
                        Keys.onLeftPressed: if (root.keyboardAvailable) Focus.take(shortcutTitle)
                        Keys.onDownPressed: Focus.take(root.keyboardAvailable ? shortcutUrl : shortcutUrlKeyboard)
                    }
                }
                UiText { text: qsTr("Address"); color: Theme.textSecondary }
                Rectangle {
                    width: 1000; height: 100; radius: 30; color: Theme.plate
                    border.width: 2; border.color: shortcutUrl.activeFocus ? Theme.ember : "transparent"
                    TextInput {
                        id: shortcutUrl; objectName: "shortcutUrl"
                        anchors.fill: parent; anchors.margins: 24
                        anchors.rightMargin: shortcutUrlKeyboard.width + 32
                        font.family: Theme.fontFamily; font.pixelSize: 36; color: Theme.textPrimary
                        selectByMouse: true; clip: true
                        readOnly: root.shortcutSaving || !root.keyboardAvailable
                        maximumLength: 2048
                        Keys.onUpPressed: Focus.take(shortcutTitle)
                        Keys.onDownPressed: Focus.take(shortcutSave)
                        Keys.onReturnPressed: Focus.take(shortcutSave)
                        Keys.onEnterPressed: Focus.take(shortcutSave)
                        Keys.onEscapePressed: root.closeShortcutEditorRequested()
                        Keys.onPressed: event => {
                            if (event.key === Qt.Key_Home) { root.homeKey(); event.accepted = true; }
                            else if (event.key === Qt.Key_Right && cursorPosition === length) {
                                Focus.take(shortcutUrlKeyboard); event.accepted = true;
                            }
                        }
                    }
                    KeyboardButton {
                        id: shortcutUrlKeyboard; objectName: "shortcutUrlKeyboard"
                        x: parent.width - width - 8; anchors.verticalCenter: parent.verticalCenter
                        enabled: !root.shortcutSaving
                        onActivated: root.startShortcutTyping("url")
                        Keys.onLeftPressed: if (root.keyboardAvailable) Focus.take(shortcutUrl)
                        Keys.onUpPressed: Focus.take(root.keyboardAvailable ? shortcutTitle : shortcutTitleKeyboard)
                        Keys.onDownPressed: Focus.take(shortcutSave)
                    }
                }
                UiText {
                    width: parent.width; height: 80; font.pixelSize: 28
                    text: root.shortcutError
                    color: root.shortcutError.length ? Theme.error : Theme.textSecondary
                }
                Row {
                    spacing: 24
                    ActionButton {
                        id: shortcutSave; objectName: "shortcutSave"; text: qsTr("Save")
                        busy: root.shortcutSaving
                        onActivated: if (!root.shortcutSaving) root.saveShortcutRequested(shortcutTitle.text, shortcutUrl.text)
                        Keys.onUpPressed: Focus.take(root.keyboardAvailable ? shortcutUrl : shortcutUrlKeyboard)
                        Keys.onRightPressed: Focus.take(shortcutCancel)
                    }
                    ActionButton {
                        id: shortcutCancel; objectName: "shortcutCancel"; text: qsTr("Cancel")
                        onActivated: root.closeShortcutEditorRequested()
                        Keys.onLeftPressed: Focus.take(shortcutSave)
                        Keys.onRightPressed: if (shortcutDelete.visible) Focus.take(shortcutDelete)
                        Keys.onUpPressed: Focus.take(root.keyboardAvailable ? shortcutUrl : shortcutUrlKeyboard)
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
        // Typing a field with the on-screen keyboard (KB-2, KB-3): the field
        // on top and the panel below. The text goes into the form at once;
        // Done moves on, Back returns to the form.
        FocusScope {
            id: shortcutTyping; objectName: "shortcutTyping"
            anchors.fill: parent; z: 10
            visible: root.shortcutTypingField.length > 0
            enabled: visible
            readonly property bool url: root.shortcutTypingField === "url"
            readonly property string domain: {
                const names = root.shortcutNameSuggestions || [];
                for (const entry of names) if (entry.kind === "domain") return entry.value;
                return "";
            }
            Keys.onPressed: event => {
                if (event.key === Qt.Key_Home) return;
                event.accepted = true;
                if (event.key === Qt.Key_Escape || event.key === Qt.Key_Back || event.key === Qt.Key_Backspace) {
                    if (!event.isAutoRepeat) root.finishShortcutTyping(false);
                } else if (event.key === Qt.Key_Down) {
                    shortcutKeyboard.enterFromAbove();
                } else if (event.text.length && event.text.charCodeAt(0) >= 32
                           && !(event.modifiers & (Qt.ControlModifier | Qt.AltModifier | Qt.MetaModifier))) {
                    shortcutKeyboard.insertText(event.text);
                }
            }
            Rectangle { anchors.fill: parent; color: Theme.background; MouseArea { anchors.fill: parent } }
            Row {
                x: 120; y: 64; spacing: 24
                UiText {
                    text: root.shortcutEditor.id ? qsTr("Edit shortcut") : qsTr("Add application")
                    font.pixelSize: 56; font.weight: 700; wrapMode: Text.NoWrap
                }
                UiText {
                    y: 26
                    text: shortcutTyping.url ? qsTr("Address") : qsTr("Title")
                    color: Theme.textHint; wrapMode: Text.NoWrap
                }
            }
            Rectangle {
                x: 120; y: 160; width: 1680; height: 132; radius: 34
                color: Qt.rgba(1, 1, 1, 0.12)
                border.width: 3; border.color: Theme.ring
                UiText {
                    id: shortcutTypingLabel
                    x: 32; anchors.verticalCenter: parent.verticalCenter
                    text: shortcutTyping.url ? qsTr("Address") : qsTr("Title")
                    color: Theme.textHint; wrapMode: Text.NoWrap
                }
                TextInput {
                    id: shortcutTypingInput; objectName: "shortcutTypingInput"
                    x: shortcutTypingLabel.x + shortcutTypingLabel.width + 24
                    width: shortcutTypingKeyboard.x - x - 24
                    height: 80; anchors.verticalCenter: parent.verticalCenter
                    font.family: Theme.fontFamily; font.pixelSize: 48; font.weight: 500
                    FontWeightAxis {}
                    color: Theme.textPrimary; verticalAlignment: TextInput.AlignVCenter; clip: true
                    // Focus stays on the panel; a hardware keyboard reaches
                    // the text through it.
                    cursorVisible: shortcutTyping.visible
                    cursorDelegate: Rectangle { width: 4; color: Theme.ember; visible: shortcutTypingInput.cursorVisible }
                    onTextChanged: if (shortcutTyping.visible) {
                        if (shortcutTyping.url) shortcutUrl.text = text;
                        else shortcutTitle.text = text;
                    }
                }
                KeyboardButton {
                    id: shortcutTypingKeyboard; objectName: "shortcutTypingKeyboard"
                    x: parent.width - width - 24; anchors.verticalCenter: parent.verticalCenter
                    panelOpen: true
                    onActivated: root.finishShortcutTyping(false)
                }
            }
            UiText {
                objectName: "shortcutTypingIntent"
                x: 152; y: 320; width: 1600; font.pixelSize: 30; elide: Text.ElideRight
                color: Theme.textSecondary
                // KB-3: an empty name is allowed; the host is used.
                visible: !shortcutTyping.url && !shortcutTypingInput.text.trim().length && shortcutTyping.domain.length > 0
                text: qsTr("Empty · The shortcut will be named after its domain: %1").arg(shortcutTyping.domain)
            }
            OnScreenKeyboard {
                id: shortcutKeyboard; objectName: "shortcutKeyboard"
                y: parent.height - implicitHeight
                target: shortcutTypingInput
                memoryKey: shortcutTyping.url ? "shortcut-url" : "shortcut-title"
                context: shortcutTyping.url ? "address" : ""
                submitLabel: qsTr("Done")
                suggestions: shortcutTyping.url ? root.shortcutAddressSuggestions : root.shortcutNameSuggestions
                emptySuggestions: shortcutTyping.url ? qsTr("No open services") : ""
                onSubmitted: root.finishShortcutTyping(true)
                onHideRequested: root.finishShortcutTyping(false)
                onLeaveUp: Focus.take(shortcutTypingKeyboard)
                onSuggestionChosen: index => {
                    const entry = suggestions[index];
                    if (!entry) return;
                    shortcutTypingInput.text = entry.value;
                    shortcutTypingInput.cursorPosition = shortcutTypingInput.text.length;
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

    // The tile sheet (handoff *Closing applications*, CL-1, CL-2): the right
    // glass sheet of the player's sheets, with the tile's group, chip, name
    // and running state, then pin or unpin and, while it runs, Close.
    Timer { id: sheetCloseTimer; interval: 180; onTriggered: root.closeTileActions() }
    FocusScope {
        id: tileActions
        objectName: "tileActions"
        anchors.fill: parent; visible: root.tileActionsOpen; enabled: visible; z: 20
        readonly property string group: root.screen === "home" ? root.favoritesTitle
                : (root.groupTitles[root.actionEntry.group] || root.groupTitles.other)
        readonly property var options: [favoriteAction, closeAppAction, favoriteRefresh].filter(item => item.visible)
        function step(delta) {
            const list = options;
            const index = list.findIndex(item => item.activeFocus);
            const next = index + delta;
            if (next >= 0 && next < list.length) Focus.take(list[next]);
        }
        Keys.onPressed: event => {
            if (event.key === Qt.Key_Home) root.homeKey();
            else if (event.key === Qt.Key_Up || event.key === Qt.Key_Backtab) step(-1);
            else if (event.key === Qt.Key_Down || event.key === Qt.Key_Tab) step(1);
            else if (event.key === Qt.Key_Escape || event.key === Qt.Key_Backspace || event.key === Qt.Key_Back
                     || (event.key === Qt.Key_Menu && !event.isAutoRepeat)) {
                if (!event.isAutoRepeat) root.closeTileActions();
            }
            event.accepted = true;
        }
        Keys.onReleased: event => event.accepted = true
        Rectangle { anchors.fill: parent; color: Qt.rgba(0, 0, 0, 0.45) }
        MouseArea { anchors.fill: parent; onClicked: root.closeTileActions() }
        Rectangle {
            id: actionSheet
            objectName: "tileActionSheet"
            x: parent.width - width; y: 0
            width: 720; height: parent.height
            color: Qt.rgba(0.11, 0.115, 0.13, 0.96)
            MouseArea { anchors.fill: parent }
            Column {
                x: 56; y: 112
                width: parent.width - 56 - 64
                spacing: 28
                UiText {
                    objectName: "tileActionsEyebrow"
                    text: tileActions.group
                    role: "eyebrow"
                    color: Theme.textSecondary
                }
                Row {
                    spacing: 24
                    Rectangle {
                        objectName: "tileActionsChip"
                        width: 76; height: 76; radius: 22
                        color: Theme.chip
                        border.width: 1; border.color: Theme.chipLine
                        Image {
                            id: sheetIcon
                            anchors.centerIn: parent
                            width: 48; height: 48
                            sourceSize: Qt.size(96, 96)
                            source: root.actionEntry.iconSource || ""
                            fillMode: Image.PreserveAspectFit
                            asynchronous: true
                            visible: status === Image.Ready
                        }
                        UiText {
                            anchors.centerIn: parent
                            visible: !sheetIcon.visible
                            text: root.actionEntry.monogram || String(root.actionEntry.title || "").trim().slice(0, 1).toUpperCase()
                            font.pixelSize: 34; font.weight: 700
                            color: Theme.textSecondary
                        }
                    }
                    UiText {
                        objectName: "tileActionsTitle"
                        width: actionSheet.width - 56 - 64 - 76 - 24
                        anchors.verticalCenter: parent.verticalCenter
                        text: root.actionEntry.title || qsTr("Application actions")
                        role: "title"; font.pixelSize: 52; font.weight: 700
                        maximumLineCount: 2; wrapMode: Text.WordWrap; elide: Text.ElideRight
                    }
                }
                Row {
                    spacing: 12
                    Rectangle {
                        objectName: "tileActionsRunningDot"
                        anchors.verticalCenter: parent.verticalCenter
                        width: 12; height: 12; radius: 6
                        color: Theme.ember
                        visible: !!root.actionEntry.running
                    }
                    UiText {
                        objectName: "tileActionsStatus"
                        text: root.actionEntry.running ? qsTr("Running now") : qsTr("Not running")
                        role: "meta"
                        color: Theme.textSecondary
                    }
                }
                UiText { text: root.favoriteError; visible: text.length > 0; color: Theme.error; width: parent.width; wrapMode: Text.WordWrap }
                Column {
                    width: parent.width
                    spacing: 12
                    SheetOption {
                        id: favoriteAction; objectName: "favoriteAction"
                        text: root.actionEntry.pinned ? qsTr("Unpin from Home") : qsTr("Pin to Home")
                        enabled: root.actionsEnabled && !!root.actionEntry.providerId
                        busy: root.favoriteSaving
                        onActivated: root.toggleFavorite()
                    }
                    SheetOption {
                        id: closeAppAction; objectName: "closeAppAction"
                        text: qsTr("Close")
                        description: qsTr("Arinn will ask %1 to close").arg(root.actionEntry.title || "")
                        destructive: true
                        // Only while it runs; never for a web shortcut (CL-2).
                        visible: !!root.actionEntry.runningId && !root.actionEntry.webShortcut
                        onActivated: root.closeFromSheet()
                    }
                    SheetOption {
                        id: favoriteRefresh; objectName: "favoriteRefresh"; text: qsTr("Refresh list")
                        visible: root.favoriteError.length > 0; enabled: !root.favoriteSaving
                        onActivated: root.retryRequested()
                    }
                }
            }
        }
    }
    // An option of the tile sheet: min-h104 r28, white fill with ink in
    // focus (handoff 10/11).
    component SheetOption: Surface {
        id: option
        property string description: ""
        property bool destructive: false
        width: 600
        height: Math.max(104, optionText.implicitHeight + 40)
        shapeRole: "action"
        cornerRadius: 28
        focusScale: 1
        focusLift: 0
        liftFromBottom: false
        restingColor: "transparent"
        pointerMode: root.pointerMode
        hoverFocusEnabled: root.hoverFocusEnabled
        contentItem: Item {
            Rectangle {
                id: optionIcon
                visible: option.destructive
                x: 28; anchors.verticalCenter: parent.verticalCenter
                width: 48; height: 48; radius: 24
                color: Qt.rgba(0.91, 0.28, 0.31, option.activeFocus ? 0.9 : 0.22)
                LineIcon {
                    anchors.centerIn: parent
                    width: 26; height: 26
                    name: "close"
                    color: option.activeFocus ? Theme.ink : Theme.textPrimary
                }
            }
            Column {
                id: optionText
                x: option.destructive ? optionIcon.x + optionIcon.width + 20 : 28
                width: parent.width - x - 28
                anchors.verticalCenter: parent.verticalCenter
                spacing: 4
                UiText {
                    objectName: option.objectName + "Label"
                    width: parent.width
                    text: option.busy ? qsTr("Loading…") : option.text
                    role: "control"
                    color: option.activeFocus ? Theme.ink : Theme.textPrimary
                    maximumLineCount: 1; elide: Text.ElideRight; wrapMode: Text.NoWrap
                }
                UiText {
                    objectName: option.objectName + "Description"
                    width: parent.width
                    visible: text.length > 0
                    text: option.description
                    role: "meta"
                    color: option.activeFocus ? Qt.rgba(0.07, 0.08, 0.09, 0.72) : Theme.textSecondary
                    maximumLineCount: 1; elide: Text.ElideRight; wrapMode: Text.NoWrap
                }
            }
        }
    }

}
