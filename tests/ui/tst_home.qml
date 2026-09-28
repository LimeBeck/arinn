// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import QtTest
import "../../apps/arinn-shell/ui" as Ui

TestCase {
    id: suite
    name: "HomeNavigation"
    width: 1920; height: 1080
    visible: true
    when: windowShown
    property var home
    Component {
        id: factory
        Ui.HomeView {
            continueEntries: [
                {id: "a", title: "Activity", subtitle: "Return", running: true},
                {id: "b", title: "Second"}, {id: "c", title: "Third"},
                {id: "d", title: "Fourth"}, {id: "e", title: "Fifth"}
            ]
            favoriteEntries: [{id: "f", title: "Favorite", monogram: "F"}, {id: "g", title: "Another", monogram: "A"}]
            newEntries: [{id: "n", title: "New"}]
        }
    }
    SignalSpy { id: returns; signalName: "returnRequested" }
    SignalSpy { id: entries; signalName: "entryRequested" }
    SignalSpy { id: retry; signalName: "retryRequested" }
    function child(name) { const item = findChild(home, name); verify(item !== null, name); return item; }
    function focused(name) { tryCompare(child(name), "activeFocus", true, 5000, name); }
    function createHome() {
        // Qt Quick Test retains the pointer between fixtures. Park it outside
        // controls before creating Home so a queued hover cannot steal focus.
        mouseMove(suite, 10, 10);
        home = createTemporaryObject(factory, suite);
        verify(home);
        returns.target = home; returns.clear();
        entries.target = home; entries.clear();
        retry.target = home; retry.clear();
        focused("entry-a");
    }
    function init() { createHome(); }
    function cleanup() { returns.target = null; entries.target = null; retry.target = null; }

    function test_fixture_resets_pointer_data() {
        return [{tag: "second", entry: "b"}, {tag: "third", entry: "c"}];
    }
    function test_fixture_resets_pointer(data) {
        mouseMove(child("entry-" + data.entry), 100, 100);
        focused("entry-" + data.entry);
        home.destroy();
        wait(0);
        createHome();
        verify(waitForRendering(home));
        wait(0);
        focused("entry-a");
        keyClick(Qt.Key_Right); focused("entry-b");
        keyClick(Qt.Key_Right); focused("entry-c");
        home.continueEntries = [{id: "a", title: "Activity"}, {id: "b", title: "Second"}, {id: "d", title: "Fourth"}];
        focused("entry-b");
    }

    function test_home_back_activity() {
        home.returnActivityId = "a";
        keyClick(Qt.Key_Right); focused("entry-b");
        keyClick(Qt.Key_Home); focused("entry-a");
        compare(returns.count, 0);
        keyClick(Qt.Key_Home); focused("entry-a");
        keyClick(Qt.Key_Backspace);
        compare(returns.count, 1); compare(returns.signalArguments[0][0], "a");
        compare(home.navOpen, false);
        keyClick(Qt.Key_Return); compare(returns.count, 2);
    }
    function test_navigation_restores_opener_and_traps_focus() {
        keyClick(Qt.Key_Right); focused("entry-b");
        keyClick(Qt.Key_Escape); focused("nav-0");
        for (let i = 0; i < 10; ++i) keyClick(Qt.Key_Up);
        focused("nav-0");
        keyClick(Qt.Key_Left); keyClick(Qt.Key_Right); focused("nav-0");
        for (let i = 0; i < 10; ++i) keyClick(Qt.Key_Tab);
        focused("nav-6");
        keyClick(Qt.Key_Escape); focused("entry-b");
    }
    function test_section_back_and_home_close_navigation() {
        keyClick(Qt.Key_Menu); keyClick(Qt.Key_Down); keyClick(Qt.Key_Return);
        compare(home.screen, "library"); focused("sectionBack");
        keyClick(Qt.Key_Backspace); compare(home.screen, "home"); focused("entry-a");
        keyClick(Qt.Key_Menu); keyClick(Qt.Key_Down); keyClick(Qt.Key_Home);
        compare(home.screen, "home"); compare(home.navOpen, false); focused("entry-a");
    }
    function test_rows_and_horizontal_scroll() {
        keyClick(Qt.Key_Down); focused("entry-f");
        tryCompare(home, "hoverFocusEnabled", true);
        keyClick(Qt.Key_Down); focused("entry-n");
        compare(home.hoverFocusEnabled, false);
        keyClick(Qt.Key_Up); focused("entry-f");
        keyClick(Qt.Key_Up); focused("entry-a");
        for (let i = 0; i < 4; ++i) keyClick(Qt.Key_Right);
        focused("entry-e");
        verify(child("continueViewport").contentX > 0);
        compare(home.hoverFocusEnabled, false);
        tryCompare(home, "hoverFocusEnabled", true);
        keyClick(Qt.Key_Return); compare(entries.count, 1); compare(entries.signalArguments[0][0], "e");
    }
    function test_removed_entry_focus_previous() {
        keyClick(Qt.Key_Right); keyClick(Qt.Key_Right); focused("entry-c");
        home.continueEntries = [{id: "a", title: "Activity"}, {id: "b", title: "Second"}, {id: "d", title: "Fourth"}];
        focused("entry-b");
    }
    function test_reordered_snapshot_keeps_id() {
        keyClick(Qt.Key_Right); focused("entry-b");
        home.continueEntries = [{id: "b", title: "Second"}, {id: "a", title: "Activity"}];
        focused("entry-b");
        keyClick(Qt.Key_Right); focused("entry-a");
    }
    function test_removed_activity_and_row_fallback() {
        home.returnActivityId = "a";
        home.returnActivityId = "";
        home.continueEntries = [];
        focused("entry-f");
        keyClick(Qt.Key_Escape); compare(home.navOpen, true); compare(returns.count, 0);
    }
    function test_model_update_while_navigation_open() {
        keyClick(Qt.Key_Right); keyClick(Qt.Key_Menu); focused("nav-0");
        home.continueEntries = [{id: "a", title: "Activity"}];
        wait(20); focused("nav-0");
        keyClick(Qt.Key_Escape); focused("entry-a");
    }
    function test_busy_disabled_entries() {
        home.continueEntries = [{id: "a", title: "Busy", busy: true}, {id: "b", title: "Disabled", enabled: false}, {id: "c", title: "Available"}];
        focused("entry-a"); keyClick(Qt.Key_Return); compare(entries.count, 0);
        keyClick(Qt.Key_Right); focused("entry-c");
    }
    function test_application_section_navigation_and_updates() {
        home.applicationEntries = [{id: "app-one", title: "First"}, {id: "app-two", title: "Second"}];
        home.selectSection(3);
        focused("entry-app-one");
        keyClick(Qt.Key_Right); focused("entry-app-two");
        keyClick(Qt.Key_Return);
        compare(entries.count, 1); compare(entries.signalArguments[0][0], "app-two");
        keyClick(Qt.Key_Menu); focused("nav-3");
        keyClick(Qt.Key_Escape); focused("entry-app-two");
        home.applicationEntries = [{id: "app-one", title: "First"}];
        focused("entry-app-one");
        home.applicationEntries = [];
        focused("sectionBack");
        keyClick(Qt.Key_Backspace); compare(home.screen, "home"); focused("entry-a");
    }
    function test_states_data() {
        return [{tag: "loading"}, {tag: "empty"}, {tag: "error"}, {tag: "unavailable"}];
    }
    function test_states(data) {
        home.homeState = data.tag;
        home.statusTitle = data.tag;
        focused("menuButton");
        verify(child("homeStatus").visible);
        keyClick(Qt.Key_Menu); focused("nav-0");
        keyClick(Qt.Key_Escape); focused("menuButton");
    }
    function test_pointer_and_dpad() {
        tryCompare(home, "hoverFocusEnabled", true);
        mouseMove(home, 10, 10);
        keyClick(Qt.Key_Up);
        mouseMove(child("entry-b"), 100, 100);
        tryCompare(home, "pointerMode", true);
        focused("entry-b");
        mouseClick(child("entry-b"), 100, 100);
        compare(entries.count, 1);
        keyClick(Qt.Key_Left);
        compare(home.pointerMode, false);
        focused("entry-a");
        child("continueViewport").contentX = 80;
        compare(home.hoverFocusEnabled, false);
        mouseMove(child("entry-c"), 100, 100);
        focused("entry-a");
        tryCompare(home, "hoverFocusEnabled", true);
        mouseMove(child("entry-c"), 101, 100);
        focused("entry-c");
    }
    function test_new_without_other_rows() {
        home.continueEntries = [];
        home.favoriteEntries = [];
        focused("entry-n");
        const card = child("entry-n");
        const position = card.mapToItem(home, 0, 0);
        verify(position.y > 108);
        verify(position.y + card.height < home.height);
    }
    function test_all_entries_disabled() {
        home.continueEntries = [{id: "a", title: "Unavailable", enabled: false}];
        focused("entry-f");
    }
    function test_menu_button_opener() {
        keyClick(Qt.Key_Up); focused("menuButton");
        keyClick(Qt.Key_Return); focused("nav-0");
        keyClick(Qt.Key_Escape); focused("menuButton");
    }
    function test_retry_from_dpad() {
        home.homeState = "error";
        focused("menuButton");
        keyClick(Qt.Key_Down); focused("statusAction");
        keyClick(Qt.Key_Return); compare(retry.count, 1);
        keyClick(Qt.Key_Menu); focused("nav-0");
        keyClick(Qt.Key_Escape); focused("statusAction");
        keyClick(Qt.Key_Up); focused("menuButton");
    }
    function test_last_row_removed() {
        keyClick(Qt.Key_Down); keyClick(Qt.Key_Down); focused("entry-n");
        home.newEntries = [];
        focused("entry-f");
    }
    function test_navigation_activity_removed() {
        home.returnActivityId = "a";
        keyClick(Qt.Key_Menu);
        for (let i = 0; i < 5; ++i) keyClick(Qt.Key_Down);
        focused("nav-5");
        home.returnActivityId = "";
        focused("nav-4");
    }
    function test_scrim_restores_focus() {
        keyClick(Qt.Key_Right); keyClick(Qt.Key_Menu); focused("nav-0");
        mouseClick(home, 1800, 500);
        compare(home.navOpen, false); focused("entry-b");
    }
    function test_backspace_in_editor() {
        const editor = Qt.createQmlObject('import QtQuick; TextInput { text: "abc"; width: 200; height: 50 }', home);
        editor.forceActiveFocus(); editor.cursorPosition = 3;
        keyClick(Qt.Key_Backspace); compare(editor.text, "ab"); compare(home.navOpen, false);
        keyClick(Qt.Key_Escape); compare(home.navOpen, true);
        editor.destroy();
    }
}
