// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import QtTest
import "../../apps/arinn-shell/ui/screens/browser" as Browser

TestCase {
    id: suite
    name: "BrowserTabs"
    width: 1920
    height: 1080
    visible: true
    when: windowShown

    property var view
    Component {
        id: factory
        Browser.BrowserView {
            tabs: [
                {id: "t1", windowId: "w1", title: "First", url: "loc-1", active: false, audible: false},
                {id: "t2", windowId: "w1", title: "Second", url: "loc-2", active: true, audible: true},
                {id: "t3", windowId: "w2", title: "Third", url: "loc-3", active: false, audible: false}
            ]
            shortcuts: [{id: "s1", title: "Known", url: "not-a-location"}]
        }
    }
    Component {
        id: focusedFactory
        Browser.BrowserView {
            windows: [
                {id: "w1", focused: false},
                {id: "w2", focused: true},
                {id: "w2", focused: true}
            ]
            tabs: [
                {id: "t1", windowId: "w1", title: "First", url: "loc-1", active: true, audible: false},
                {id: "t2", windowId: "w2", title: "Second", url: "loc-2", active: true, audible: false}
            ]
        }
    }
    SignalSpy { id: activates; signalName: "activateTab" }
    SignalSpy { id: closes; signalName: "closeTab" }
    SignalSpy { id: opens; signalName: "open" }
    SignalSpy { id: retries; signalName: "retryRequested" }
    SignalSpy { id: backs; signalName: "backRequested" }

    function child(name) {
        const item = findChild(view, name);
        verify(item !== null, name);
        return item;
    }
    function focused(name) {
        tryCompare(child(name), "activeFocus", true, 5000, name);
    }
    function focusInside() {
        tryVerify(() => {
            let item = suite.Window.activeFocusItem;
            while (item && item !== view)
                item = item.parent;
            return item === view;
        }, 5000);
    }
    function bind(next) {
        view = next;
        verify(view);
        activates.target = view; activates.clear();
        closes.target = view; closes.clear();
        opens.target = view; opens.clear();
        retries.target = view; retries.clear();
        backs.target = view; backs.clear();
    }
    function createView() {
        mouseMove(suite, 10, 10);
        bind(createTemporaryObject(factory, suite));
    }
    function init() { createView(); }
    function cleanup() {
        activates.target = null;
        closes.target = null;
        opens.target = null;
        retries.target = null;
        backs.target = null;
    }

    function test_focus_move_stays_inside() {
        focused("row-t2");
        compare(view.focusedWindowId, "w1");
        compare(view.activeTabId, "t2");
        compare(view.orderedTabs[0].id, "t2");
        compare(view.orderedTabs[1].id, "t1");
        compare(view.orderedTabs[2].id, "t3");
        compare(child("screenTitle").text, "Open services");
        compare(child("serviceCount").text, "3");
        verify(child("onscreen-t2").visible);
        verify(!child("onscreen-t1").visible);
        verify(child("sound-t2").visible);
        verify(!child("sound-t1").visible);
        verify(findChild(view, "shortcut-s1") === null);
        verify(findChild(view, "window-w1") === null);
        keyClick(Qt.Key_Left);
        focused("row-t2");
        keyClick(Qt.Key_Right);
        focused("close-t2");
        keyClick(Qt.Key_Right);
        focused("close-t2");
        keyClick(Qt.Key_Left);
        focused("row-t2");
        keyClick(Qt.Key_Down);
        focused("row-t1");
        keyClick(Qt.Key_Down);
        focused("row-t3");
        keyClick(Qt.Key_Down);
        focused("row-t3");
        keyClick(Qt.Key_Right);
        focused("close-t3");
        keyClick(Qt.Key_Up);
        focused("close-t1");
        keyClick(Qt.Key_Tab);
        focused("close-t1");
        keyClick(Qt.Key_Backtab);
        focused("row-t1");
        for (let i = 0; i < 6; ++i) {
            keyClick(Qt.Key_Up);
            keyClick(Qt.Key_Left);
            keyClick(Qt.Key_Down);
            keyClick(Qt.Key_Right);
        }
        focusInside();
    }

    function test_back_requested() {
        focused("row-t2");
        keyClick(Qt.Key_Escape);
        compare(backs.count, 1);
        focused("row-t2");
        keyClick(Qt.Key_Backspace);
        compare(backs.count, 2);
        focused("row-t2");
        keyClick(Qt.Key_Back);
        compare(backs.count, 3);
        focused("row-t2");
        focusInside();
    }

    function test_activate_and_close() {
        focused("row-t2");
        keyClick(Qt.Key_Return);
        compare(activates.count, 1);
        compare(activates.signalArguments[0][0], "t2");
        keyClick(Qt.Key_Right);
        focused("close-t2");
        keyClick(Qt.Key_Return);
        compare(closes.count, 1);
        compare(closes.signalArguments[0][0], "t2");
        focused("row-t1");
        tryCompare(child("closing-t2"), "visible", true);
        tryCompare(child("entry-t2"), "opacity", 0.5, 1000);
        keyClick(Qt.Key_Down);
        focused("row-t3");
        keyClick(Qt.Key_Right);
        focused("close-t3");
        keyClick(Qt.Key_Return);
        compare(closes.count, 2);
        compare(closes.signalArguments[1][0], "t3");
        focusInside();
    }

    function test_known_shortcut_signal() {
        verify(findChild(view, "shortcut-s1") === null);
        view.requestOpen("s1");
        compare(opens.count, 1);
        compare(opens.signalArguments[0][0], "s1");
        compare(opens.signalArguments[0].length, 1);
        view.requestOpen("missing");
        compare(opens.count, 1);
    }

    function test_one_active_tab() {
        compare(view.activeTabId, "t2");
        view.tabs = [
            {id: "a", windowId: "w", title: "Alpha", url: "loc-a", active: true, audible: false},
            {id: "b", windowId: "w", title: "Beta", url: "loc-b", active: true, audible: true}
        ];
        compare(view.activeTabId, "a");
        focused("row-a");
        verify(child("onscreen-a").visible);
        verify(!child("onscreen-b").visible);
        verify(child("sound-b").visible);
        verify(!child("sound-a").visible);
    }

    function test_one_focused_window() {
        view.destroy();
        bind(createTemporaryObject(focusedFactory, suite));
        compare(view.focusedWindowId, "w2");
        compare(view.windowIds.length, 2);
        compare(view.activeTabId, "t2");
        compare(view.orderedTabs[0].id, "t2");
        focused("row-t2");
        verify(child("onscreen-t2").visible);
        keyClick(Qt.Key_Down);
        focused("row-t1");
        compare(view.focusedWindowId, "w2");
        compare(view.activeTabId, "t2");
        verify(child("onscreen-t2").visible);
        verify(!child("onscreen-t1").visible);
    }

    function test_titles_and_letter() {
        view.tabs = [
            {id: "long", windowId: "w", title: "A very long service title that should stay on one line until it is focused and then may use a second line", url: "https://hd.videos.example/film/1", active: false, audible: false},
            {id: "blank", windowId: "w", title: "", url: "https://www.example.test/a/b", active: true, audible: false}
        ];
        focused("row-blank");
        compare(child("title-blank").text, "www.example.test");
        compare(child("sub-blank").text, "https://www.example.test/a/b");
        compare(child("letter-blank").text, "E");
        compare(child("title-long").maximumLineCount, 1);
        keyClick(Qt.Key_Down);
        focused("row-long");
        compare(child("title-long").maximumLineCount, 2);
        compare(child("sub-long").text, "hd.videos.example");
        compare(child("letter-long").text, "V");
    }

    function test_scroll_position_and_hold() {
        const tabs = [];
        for (let i = 0; i < 12; ++i)
            tabs.push({id: "n" + i, windowId: "w", title: "Service " + i, url: "svc" + i + ".example/a", active: i === 0, audible: false});
        view.tabs = tabs;
        focused("row-n0");
        compare(child("serviceCount").text, "12");
        tryCompare(child("listPosition"), "visible", true);
        compare(child("listPosition").text, "1 of 12");
        view.move(Qt.Key_Down, false);
        focused("row-n1");
        compare(child("listPosition").text, "2 of 12");
        for (let i = 0; i < 4; ++i)
            view.move(Qt.Key_Down, true);
        focused("row-n6");
        compare(child("listPosition").text, "7 of 12");
        focusInside();
    }

    function test_empty() {
        view.browserState = "empty";
        const pane = child("browserStatus");
        tryCompare(pane, "visible", true);
        compare(pane.kind, "empty");
        compare(pane.title, "Nothing open");
        compare(pane.description, "Services you open from Home will appear here.");
        verify(findChild(view, "row-t2") === null);
        focused("statusAction");
        compare(child("statusAction").width > 0, true);
        keyClick(Qt.Key_Return);
        compare(backs.count, 1);
        compare(retries.count, 0);
        keyClick(Qt.Key_Left);
        keyClick(Qt.Key_Up);
        keyClick(Qt.Key_Down);
        focused("statusAction");
        verify(findChild(view, "shortcut-s1") === null);
        focusInside();
    }

    function test_unavailable() {
        view.browserState = "unavailable";
        const pane = child("browserStatus");
        tryCompare(pane, "visible", true);
        compare(pane.kind, "unavailable");
        compare(pane.title, "Browser unavailable");
        compare(pane.description, "Online services cannot be opened right now. Library, downloads and games still work.");
        verify(findChild(view, "row-t2") === null);
        focused("statusAction");
        keyClick(Qt.Key_Return);
        compare(retries.count, 1);
        keyClick(Qt.Key_Left);
        keyClick(Qt.Key_Right);
        keyClick(Qt.Key_Up);
        focused("statusAction");
        focusInside();
    }

    function test_loading_and_error() {
        view.browserState = "loading";
        const pane = child("browserStatus");
        tryCompare(pane, "kind", "loading");
        compare(pane.title, "Loading tabs");
        compare(findChild(pane, "statusAction").visible, false);
        verify(findChild(view, "row-t2") === null);
        focusInside();
        view.browserState = "error";
        tryCompare(pane, "kind", "error");
        compare(pane.title, "Tabs unavailable");
        compare(pane.description, "The tab list could not be loaded.");
        focused("statusAction");
        keyClick(Qt.Key_Return);
        compare(retries.count, 1);
        focusInside();
    }
}
