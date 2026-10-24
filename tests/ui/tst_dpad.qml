// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import QtQuick.Window
import QtTest
import "dpad.js" as Dpad
import "../../apps/arinn-shell/ui" as Ui
import "../../apps/arinn-shell/ui/screens/browser" as Browser
import "../../apps/arinn-shell/ui/screens/downloads" as Downloads
import "../../apps/arinn-shell/ui/screens/library" as Library
import "../../apps/arinn-shell/ui/screens/player" as Player
import "../../apps/arinn-shell/ui/screens/power" as Power
import "../../apps/arinn-shell/ui/screens/session" as Session
import "../../apps/arinn-shell/ui/screens/settings" as Settings

// AC-02: every action of every screen state is reachable with arrows alone
// (#147). Each test builds a state from the fixtures the screen suites use
// and walks it with the arrows from the initial focus.
TestCase {
    id: suite
    name: "DpadReachability"
    width: 1920; height: 1080
    visible: true
    when: windowShown

    function focusedItem() { return suite.Window.activeFocusItem; }
    // `build()` returns {view, scope} in the screen's starting state.
    // `exempt` maps an action's label to the remote key that reaches it
    // instead of the arrows.
    function expectAllReachable(build, exempt) {
        const missing = Dpad.unreachable(suite, build, focusedItem)
            .filter(name => !(exempt && exempt[name]));
        compare(missing, [], "actions not reachable with arrows: " + missing.join(", "));
    }
    function create(component, props) {
        // A queued hover must not steal the initial focus.
        mouseMove(suite, 1, 1);
        const item = component.createObject(suite, props || {});
        verify(item);
        return item;
    }
    function child(root, name) {
        const item = findChild(root, name);
        verify(item !== null, name);
        return item;
    }
    function focused(root, name) {
        tryCompare(child(root, name), "activeFocus", true, 5000, name);
    }
    function screen(view, targets) { return {view: view, scope: view, targets: targets}; }
    // The deepest item that contains both, for a sheet without a name.
    function commonAncestor(a, b) {
        for (let current = a; current; current = current.parent) {
            for (let other = b; other; other = other.parent) {
                if (other === current)
                    return current;
            }
        }
        return null;
    }

    // Home -----------------------------------------------------------------
    Component {
        id: homeFactory
        Ui.HomeView {
            continueEntries: [
                {id: "a", title: "Activity", subtitle: "Return", running: true},
                {id: "b", title: "Second"}, {id: "c", title: "Third"},
                {id: "d", title: "Fourth"}, {id: "e", title: "Fifth"}
            ]
            favoriteEntries: [{id: "f", title: "Favorite", monogram: "F"}, {id: "g", title: "Another", monogram: "A"}]
            newEntries: [{id: "n", title: "New"}]
            shortcutManagement: true
            applicationEntries: [
                {id: "web1", title: "Browser", group: "online", webShortcut: true},
                {id: "game1", title: "Game", group: "games"},
                {id: "game2", title: "Second game", group: "games"},
                {id: "other1", title: "Editor"}
            ]
            runningEntries: [{id: "window1", title: "Window"}]
            onEditShortcutRequested: entryId => shortcutEditor = {id: entryId, title: "Example", url: "https://example.test"}
            onCloseShortcutEditorRequested: shortcutEditor = ({})
        }
    }
    function test_home_rows() {
        // Left at a row edge opens navigation, so the menu is part of this
        // walk too.
        expectAllReachable(() => {
            const home = create(homeFactory);
            focused(home, "entry-a");
            return screen(home);
        });
    }
    function test_home_applications() {
        expectAllReachable(() => {
            const home = create(homeFactory);
            focused(home, "entry-a");
            home.selectSection(3);
            focused(home, "entry-web1");
            return screen(home);
        });
    }
    function test_home_shortcut_editor() {
        expectAllReachable(() => {
            const home = create(homeFactory);
            focused(home, "entry-a");
            home.selectSection(3);
            focused(home, "entry-web1");
            keyClick(Qt.Key_Menu);
            verify(home.shortcutEditorOpen);
            return {view: home, scope: child(home, "shortcutSheet")};
        });
    }

    // Library --------------------------------------------------------------
    Component {
        id: libraryFactory
        Library.LibraryView {}
    }
    function catalogue() {
        return {
            columns: 4,
            tabs: [{id: "films", title: "Films"}, {id: "series", title: "Series"}],
            entries: [
                {id: "a", tabId: "films", title: "Alpha", subtitle: "2001", kind: "film"},
                {id: "b", tabId: "films", title: "Bravo", subtitle: "2002", kind: "film"},
                {id: "c", tabId: "films", title: "Charlie", subtitle: "2003", kind: "film", enabled: false},
                {id: "d", tabId: "films", title: "Delta", subtitle: "2004", kind: "film", missing: true},
                {id: "e", tabId: "films", title: "Echo", subtitle: "2005", kind: "film", progress: 0.4,
                    description: "A resumed entry.", episodes: [{id: "e1", title: "One"}, {id: "e2", title: "Two", missing: true}]},
                {id: "f", tabId: "films", title: "Foxtrot", subtitle: "2006", kind: "film"},
                {id: "g", tabId: "films", title: "Golf", subtitle: "2007", kind: "film"},
                {id: "h", tabId: "films", title: "Hotel", subtitle: "2008", kind: "film"},
                {id: "s", tabId: "series", title: "Serial", subtitle: "2010", kind: "series",
                    description: "A seasonal entry.",
                    seasons: [{id: "s1", title: "Season 1"}, {id: "s2", title: "Season 2"}],
                    episodes: [
                        {id: "p1", seasonId: "s1", title: "Pilot", duration: "40 min", number: 1},
                        {id: "p2", seasonId: "s2", title: "Next", duration: "41 min", number: 1, missing: true}
                    ]}
            ]
        };
    }
    function test_library_grid() {
        // Focusing a tab selects it, so the walk covers every tab's grid.
        expectAllReachable(() => {
            const library = create(libraryFactory, catalogue());
            focused(library, "tab-films");
            return screen(library);
        });
    }
    function test_library_detail_data() {
        return [{tag: "film", entry: "e"}, {tag: "series", entry: "s"}];
    }
    function test_library_detail(data) {
        expectAllReachable(() => {
            const library = create(libraryFactory, catalogue());
            focused(library, "tab-films");
            library.openEntry(data.entry);
            focused(library, "detailPlay");
            // The detail covers the grid; only its own actions count.
            return {view: library, scope: child(library, "detailLayer")};
        });
    }

    // Settings -------------------------------------------------------------
    Component {
        id: settingsFactory
        Settings.SettingsView {
            categories: [
                { id: "media", title: "Media", icon: "video_library" },
                { id: "playback", title: "Playback", icon: "play_circle" },
                { id: "sound", title: "Sound", icon: "volume_up" },
                { id: "network", title: "Network", icon: "wifi" },
                { id: "applications", title: "Applications", icon: "apps" },
                { id: "appearance", title: "Appearance", icon: "palette" },
                { id: "system", title: "System", icon: "memory" }
            ]
            rows: [
                { providerId: "library", providerName: "Library", id: "scan", categoryId: "media",
                  title: "Scan folders", description: "Look for new files", type: "boolean",
                  currentValue: false, constraints: {}, enabled: true },
                { providerId: "library", providerName: "Library", id: "roots", categoryId: "media",
                  title: "Folders", description: "Places to read", type: "path",
                  currentValue: ["/videos"], constraints: { directory: true, multiple: true, maxItems: 3 },
                  enabled: true },
                { providerId: "library", providerName: "Library", id: "folder", categoryId: "media",
                  title: "Download location", description: "Where files go", type: "path",
                  currentValue: "", constraints: { directory: true, multiple: false }, enabled: true },
                { providerId: "audio", providerName: "Audio", id: "output", categoryId: "sound",
                  title: "Output", description: "Audio output", type: "enum", currentValue: "stereo",
                  constraints: { options: [{ value: "stereo", title: "Stereo" }, { value: "surround", title: "Surround" }] },
                  enabled: true },
                { providerId: "shell", providerName: "Shell", id: "label", categoryId: "applications",
                  title: "Label", description: "Shown under the icon", type: "string", currentValue: "note",
                  constraints: { maxLength: 40 }, enabled: true },
                { providerId: "shell", providerName: "Shell", id: "locked", categoryId: "applications",
                  title: "Locked", description: "Cannot be changed", type: "string", currentValue: "fixed",
                  constraints: { maxLength: 40 }, enabled: false },
                { providerId: "shell", providerName: "Shell", id: "held", categoryId: "appearance",
                  title: "Theme", description: "Appearance name", type: "string", currentValue: "dark",
                  constraints: { maxLength: 20 }, enabled: true, pending: true },
                { providerId: "shell", providerName: "Shell", id: "broken", categoryId: "appearance",
                  title: "Scale", description: "Interface scale", type: "integer", currentValue: 1,
                  constraints: { min: 1, max: 3 }, enabled: true, error: "InvalidArgument" },
                { providerId: "shell", providerName: "Shell", id: "idle", categoryId: "system",
                  title: "Idle minutes", description: "Wait before the ambient screen", type: "integer",
                  currentValue: 10, constraints: { min: 1, max: 60 }, enabled: true }
            ]
        }
    }
    function test_settings() {
        // Moving through the rail selects each category, so the walk covers
        // the rows of every category.
        expectAllReachable(() => {
            const view = create(settingsFactory);
            focused(view, "category-media");
            return screen(view);
        });
    }

    function test_settings_string_sheet() {
        expectAllReachable(() => {
            const view = create(settingsFactory);
            focused(view, "category-media");
            for (let i = 0; i < 4; ++i)
                keyClick(Qt.Key_Down);
            focused(view, "category-applications");
            keyClick(Qt.Key_Right);
            focused(view, "setting-shell-label");
            keyClick(Qt.Key_Return);
            verify(view.stringSheet);
            const field = child(view, "editor-shell-label");
            tryCompare(field, "activeFocus", true);
            // Save and Cancel sit under the field in the sheet.
            return {view: view, scope: commonAncestor(field, child(view, "stringSave"))};
        });
    }
    function test_settings_integer_sheet() {
        expectAllReachable(() => {
            const view = create(settingsFactory);
            focused(view, "category-media");
            for (let i = 0; i < 6; ++i)
                keyClick(Qt.Key_Down);
            focused(view, "category-system");
            keyClick(Qt.Key_Right);
            focused(view, "setting-shell-idle");
            keyClick(Qt.Key_Return);
            focused(view, "integerStepper");
            // The stepper takes Left/Right and Select in its key handler.
            return {view: view, scope: commonAncestor(child(view, "integerStepper"),
                                                      child(view, "integerBounds")),
                    targets: /^integerStepper$/};
        });
    }

    // Session, player, downloads, power, browser ---------------------------
    Component {
        id: sessionFactory
        Session.SessionView {
            title: "Fixture title"
            subtitle: "Fixture artist"
            playbackState: "playing"
            durationUs: 125000000
            positionUs: 65000000
            artworkHue: "#336699"
            availableActions: ["play", "pause", "playPause", "seek", "previous", "stop"]
        }
    }
    function test_session() {
        expectAllReachable(() => {
            const view = create(sessionFactory);
            view.restore();
            tryVerify(() => Dpad.owner(focusedItem()) !== null);
            // The seek track handles Left, Right and Select in its key handler.
            return screen(view, /^seekTrack$/);
        });
    }
    Component {
        id: playerFactory
        Player.PlayerView {
            width: 1920
            height: 1080
            title: "North"
            position: 30
            duration: 120
            playing: false
            seekSupported: true
            audioTracks: [{id: "a1", label: "One"}, {id: "a2", label: "Two"}]
            subtitleTracks: [{id: "s1", label: "Notes"}]
            audioTrackId: "a1"
            subtitleTrackId: "s1"
        }
    }
    // The OSD keeps one focus scope and moves a virtual focus (`osdFocus`)
    // between its targets, so walk that value instead of focus items.
    function test_player_osd() {
        mouseMove(suite, 1, 1);
        const view = createTemporaryObject(playerFactory, suite);
        verify(view);
        view.forceActiveFocus();
        tryCompare(view, "activeFocus", true);
        const targets = ["timeline", "subtitles", "audio", "more"];
        const reached = new Set(["timeline"]);
        const queue = ["timeline"];
        while (queue.length) {
            const from = queue.shift();
            for (const key of [Qt.Key_Left, Qt.Key_Right, Qt.Key_Up, Qt.Key_Down]) {
                view.osdVisible = true;
                view.sheetKind = "";
                view.osdFocus = from;
                keyClick(key);
                if (!reached.has(view.osdFocus)) {
                    reached.add(view.osdFocus);
                    queue.push(view.osdFocus);
                }
            }
        }
        const missing = targets.filter(target => !reached.has(target));
        compare(missing, [], "OSD targets not reachable with arrows: " + missing.join(", "));
    }
    Component {
        id: downloadsFactory
        Downloads.DownloadsView {
            jobs: [
                {id: "active", title: "Feature", subtitle: "Part two", state: "downloading", progress: 0.82, statusText: "82%", availableActions: ["pause"], importState: "pending"},
                {id: "waiting", title: "Queued item", state: "queued", availableActions: ["start_now"], importState: "pending"},
                {id: "held", title: "Held item", state: "paused", progress: 0.4, availableActions: ["resume"], importState: "pending"},
                {id: "failed", title: "Failed item", state: "error", errorText: "Could not download", availableActions: ["retry"], importState: "pending"},
                {id: "ready", title: "Ready item", state: "completed", importState: "imported"},
                {id: "extra", title: "Extra file", state: "completed", importState: "unsupported"}
            ]
        }
    }
    function test_downloads() {
        expectAllReachable(() => {
            const view = create(downloadsFactory);
            focused(view, "job-active");
            // Each job row handles Select; its action button is NoFocus.
            return screen(view, /^job-/);
        });
    }
    Component {
        id: powerFactory
        Power.PowerMenu {
            closingApplications: ["One", "Two"]
        }
    }
    function test_power() {
        expectAllReachable(() => {
            const view = create(powerFactory);
            focused(view, "sleepButton");
            return screen(view);
        });
    }
    Component {
        id: browserFactory
        Browser.BrowserView {
            tabs: [
                {id: "t1", windowId: "w1", title: "First", url: "loc-1", active: false, audible: false},
                {id: "t2", windowId: "w1", title: "Second", url: "loc-2", active: true, audible: true},
                {id: "t3", windowId: "w2", title: "Third", url: "loc-3", active: false, audible: false}
            ]
            shortcuts: [{id: "s1", title: "Known", url: "not-a-location"}]
        }
    }
    function test_browser() {
        expectAllReachable(() => {
            const view = create(browserFactory);
            view.settleFocus();
            tryVerify(() => Dpad.owner(focusedItem()) !== null);
            // Rows, bookmark and close controls handle Select in key handlers.
            return screen(view, /^(row|bookmark|close)-|^statusAction$/);
        });
    }
}
