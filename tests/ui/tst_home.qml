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
    SignalSpy { id: closes; signalName: "closeShellRequested" }
    SignalSpy { id: services; signalName: "openServicesRequested" }
    SignalSpy { id: serviceCard; signalName: "serviceRequested" }
    function child(name) { const item = findChild(home, name); verify(item !== null, name); return item; }
    function focused(name) { tryCompare(child(name), "activeFocus", true, 5000, name); }
    function createHome() {
        // Qt Quick Test retains the pointer between fixtures. Park it outside
        // controls before creating Home so a queued hover cannot steal focus.
        mouseMove(suite, 10, 10);
        home = createTemporaryObject(factory, suite);
        verify(home);
        closes.target = home; closes.clear();
        returns.target = home; returns.clear();
        entries.target = home; entries.clear();
        retry.target = home; retry.clear();
        services.target = home; services.clear();
        serviceCard.target = home; serviceCard.clear();
        focused("entry-a");
    }
    function init() { createHome(); }
    function cleanup() {
        closes.target = null;
        returns.target = null; entries.target = null; retry.target = null;
        services.target = null; serviceCard.target = null;
    }

    function test_close_shell_action_data() {
        return [{tag: "keyboard", pointer: false}, {tag: "pointer", pointer: true}];
    }
    function test_close_shell_action(data) {
        // The exit is local even when the daemon cannot supply Home content.
        home.homeState = "unavailable";
        home.returnActivityId = "a";
        home.currentServiceTitle = "Synthetic service";
        home.openNavigation();
        focused("nav-0");
        const close = child("nav-8");
        compare(close.text, "Close Arinn");
        verify(close.visible && close.enabled);
        const navigation = child("navigation");
        const bottom = close.mapToItem(navigation, 0, close.height);
        verify(bottom.y < 1008, "All actions fit inside the navigation panel");
        if (data.pointer) {
            home.pointerMode = true;
            mouseClick(close, close.width / 2, close.height / 2);
        } else {
            for (let i = 0; i < 10; ++i) keyClick(Qt.Key_Down);
            focused("nav-8");
            keyClick(Qt.Key_Return);
        }
        compare(closes.count, 1);
        compare(returns.count, 0);
    }
    function test_back_from_close_action_does_not_exit() {
        home.openNavigation();
        focused("nav-0");
        for (let i = 0; i < 10; ++i) keyClick(Qt.Key_Down);
        focused("nav-8");
        keyClick(Qt.Key_Escape);
        compare(home.navOpen, false);
        compare(closes.count, 0);
        focused("entry-a");
    }

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

    function test_brand_mark_in_header_and_navigation() {
        const header = child("headerBrand");
        tryCompare(header, "markReady", true);
        const mark = findChild(header, "brandMark");
        verify(mark);
        verify(waitForRendering(mark));
        const pixels = grabImage(home);
        const ember = mark.mapToItem(home, mark.width / 2, mark.height * 0.7);
        const apex = mark.mapToItem(home, mark.width / 2, mark.height * 0.24);
        verify(pixels.red(Math.floor(ember.x), Math.floor(ember.y)) > 230);
        verify(pixels.green(Math.floor(ember.x), Math.floor(ember.y)) > 100);
        verify(pixels.blue(Math.floor(ember.x), Math.floor(ember.y)) < 100);
        verify(pixels.red(Math.floor(apex.x), Math.floor(apex.y)) > 220);
        child("menuButton").forceActiveFocus();
        focused("menuButton");
        verify(child("menuButton").focusedColor !== Ui.Theme.textPrimary);
        keyClick(Qt.Key_Return); focused("nav-0");
        const navBrand = child("navigationBrand");
        verify(navBrand.visible);
        tryCompare(navBrand, "markReady", true);
        keyClick(Qt.Key_Escape); focused("menuButton");
    }

    function test_distinct_sessions_keep_identical_titles() {
        home.currentServiceId = "service";
        home.currentServiceTitle = "Same title";
        home.continueEntries = [
            {id: "session-a", title: "Same title", kind: "media"},
            {id: "session-b", title: "Same title", kind: "media"}
        ];
        const ids = home.continuePresentation.map(entry => entry.id);
        verify(ids.includes("current-service"));
        verify(ids.includes("session-a"));
        verify(ids.includes("session-b"));
    }
    function test_return_is_not_suppressed_by_a_browser_presentation() {
        const returned = "{aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa}";
        home.currentServiceId = "{bbbbbbbb-bbbb-5bbb-8bbb-bbbbbbbbbbbb}";
        home.currentServiceTitle = "Page";
        home.continueEntries = [{id: returned, title: "Return to activity"}];
        verify(home.continuePresentation.some(entry => entry.id === returned));
        home.currentServiceTitle = "";
        compare(home.continuePresentation[0].id, returned);
    }
    function test_verified_service_replaces_return_without_losing_media_or_focus() {
        home.returnActivityId = "a";
        home.continueEntries = [{id: "a", title: "Return"}, {id: "media", title: "Page", kind: "media"}];
        home.showHome(); focused("entry-a");
        home.currentServiceId = "tab";
        home.currentServiceTitle = "Page";
        home.returnIsWebService = true;
        home.continueEntries = [{id: "media", title: "Page", kind: "media"}];
        focused("entry-current-service");
        verify(!home.continuePresentation.some(entry => entry.id === "a"));
        verify(home.continuePresentation.some(entry => entry.id === "media"));
        keyClick(Qt.Key_Return); compare(serviceCard.count, 1);
        keyClick(Qt.Key_Right); focused("entry-media");
        home.currentServiceTitle = "";
        home.returnIsWebService = false;
        home.continueEntries = [{id: "a", title: "Return"}, {id: "media", title: "Page", kind: "media"}];
        focused("entry-media");
        keyClick(Qt.Key_Home); focused("entry-a");
        keyClick(Qt.Key_Backspace); compare(returns.count, 1);
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
        focused("nav-8");
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
    function test_running_windows_and_restoration() {
        home.applicationEntries = [{id: "app", title: "Launch"}];
        home.runningEntries = [{id: "window1", title: "First window"}, {id: "window2", title: "Second window"}];
        const spy = createTemporaryObject(runningSpy, suite, {target: home});
        home.selectSection(3); focused("entry-app");
        keyClick(Qt.Key_Down); focused("entry-window1");
        keyClick(Qt.Key_Right); focused("entry-window2");
        keyClick(Qt.Key_Return); compare(spy.count, 1); compare(spy.signalArguments[0][0], "window2");
        keyClick(Qt.Key_Menu); keyClick(Qt.Key_Escape); focused("entry-window2");
        home.runningEntries = [{id: "window1", title: "First window"}]; focused("entry-window1");
        home.runningEntries = []; focused("entry-app");
    }
    Component { id: runningSpy; SignalSpy { signalName: "runningRequested" } }
    function test_application_groups() {
        // Online, Games, Media, Other; empty groups hidden; unknown -> Other.
        home.applicationEntries = [
            {id: "other1", title: "Editor"},
            {id: "game1", title: "Game", group: "games"},
            {id: "web1", title: "Browser", group: "online"},
            {id: "game2", title: "Second game", group: "games"}
        ];
        home.runningEntries = [{id: "window1", title: "Window"}];
        home.selectSection(3);
        focused("entry-web1");
        verify(child("group-online").visible);
        verify(child("group-games").visible);
        verify(!child("group-media").visible);
        verify(child("group-other").visible);
        compare(child("group-games").count, 2);
        keyClick(Qt.Key_Down); focused("entry-game1");
        keyClick(Qt.Key_Right); focused("entry-game2");
        keyClick(Qt.Key_Down); focused("entry-other1");
        keyClick(Qt.Key_Down); focused("entry-window1");
        keyClick(Qt.Key_Down); focused("sectionBack");
        keyClick(Qt.Key_Up); focused("entry-window1");
        keyClick(Qt.Key_Up); focused("entry-other1");
        keyClick(Qt.Key_Up); focused("entry-game1");
        // The group row is remembered across navigation.
        keyClick(Qt.Key_Menu); focused("nav-3");
        keyClick(Qt.Key_Escape); focused("entry-game1");
        // Removing the focused group moves focus to a neighbouring group.
        home.applicationEntries = [{id: "other1", title: "Editor"}, {id: "web1", title: "Browser", group: "online"}];
        focused("entry-web1");
        keyClick(Qt.Key_Up); focused("menuButton");
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
        for (let i = 0; i < 6; ++i) keyClick(Qt.Key_Down);
        focused("nav-6");
        home.returnActivityId = "";
        focused("nav-5");
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
    function test_continue_open_services_cards() {
        home.currentServiceId = "tab-1";
        home.currentServiceTitle = "Page";
        home.currentServiceSubtitle = "example.test · Return";
        home.openServiceCount = 1;
        const current = child("entry-current-service");
        compare(current.text, "Page");
        verify(findChild(home, "entry-open-services-more") === null);
        current.forceActiveFocus();
        focused("entry-current-service");
        keyClick(Qt.Key_Return);
        compare(serviceCard.count, 1);
        compare(serviceCard.signalArguments[0][0], "tab-1");
        home.openServiceCount = 3;
        const more = child("entry-open-services-more");
        compare(more.text, "2 more open");
        more.forceActiveFocus();
        focused("entry-open-services-more");
        keyClick(Qt.Key_Return);
        compare(services.count, 1);
        compare(entries.count, 0);
        home.openServiceCount = 1;
        verify(findChild(home, "entry-open-services-more") === null);
    }
    function test_open_services_navigation_item() {
        keyClick(Qt.Key_Menu);
        focused("nav-0");
        for (let i = 0; i < 5; ++i) keyClick(Qt.Key_Down);
        focused("nav-5");
        compare(child("nav-5").text, "Open services");
        const hiddenBadge = findChild(home, "openServicesBadge");
        verify(hiddenBadge !== null);
        compare(hiddenBadge.visible, false);
        home.openServiceCount = 4;
        home.currentServiceTitle = "Page";
        compare(child("openServicesBadge").visible, true);
        compare(child("openServicesCount").text, "4");
        compare(child("openServicesSubline").text, "On screen: Page");
        keyClick(Qt.Key_Return);
        compare(services.count, 1);
        compare(home.navOpen, false);
        home.openServiceCount = 0;
        home.currentServiceTitle = "";
        keyClick(Qt.Key_Menu);
        for (let i = 0; i < 5; ++i) keyClick(Qt.Key_Down);
        focused("nav-5");
        compare(child("openServicesBadge").visible, false);
    }
    function test_web_shortcut_open_dot_and_unavailable() {
        home.favoriteEntries = [
            {id: "f", title: "Favorite", monogram: "F", webShortcut: true},
            {id: "g", title: "App", monogram: "A"}
        ];
        home.openShortcutIds = ["f"];
        keyClick(Qt.Key_Down);
        focused("entry-f");
        const dot = findChild(child("entry-f"), "emberDot");
        verify(dot !== null);
        compare(dot.visible, true);
        compare(findChild(child("entry-g"), "emberDot").visible, false);
        home.browserUnavailable = true;
        compare(child("browserStatusChip").visible, true);
        compare(child("entry-f").opacity, 0.4);
        compare(child("entry-f").subtitle, "Unavailable");
        compare(child("entry-g").opacity, 1);
        keyClick(Qt.Key_Return);
        compare(services.count, 1);
        compare(entries.count, 0);
        keyClick(Qt.Key_Right);
        focused("entry-g");
        keyClick(Qt.Key_Return);
        compare(entries.count, 1);
        compare(entries.signalArguments[0][0], "g");
    }
}
