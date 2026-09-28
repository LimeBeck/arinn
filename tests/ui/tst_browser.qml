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
    SignalSpy { id: activates; signalName: "activateTab" }
    SignalSpy { id: closes; signalName: "closeTab" }
    SignalSpy { id: opens; signalName: "open" }
    SignalSpy { id: retries; signalName: "retryRequested" }

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
    function createView() {
        mouseMove(suite, 10, 10);
        view = createTemporaryObject(factory, suite);
        verify(view);
        activates.target = view; activates.clear();
        closes.target = view; closes.clear();
        opens.target = view; opens.clear();
        retries.target = view; retries.clear();
    }
    function init() { createView(); }
    function cleanup() {
        activates.target = null;
        closes.target = null;
        opens.target = null;
        retries.target = null;
    }

    function test_focus_move_stays_inside() {
        focused("tab-t2");
        compare(view.focusedWindowId, "w1");
        compare(view.activeTabId, "t2");
        verify(child("active-t2").visible);
        verify(!child("active-t1").visible);
        verify(child("audible-t2").visible);
        verify(!child("audible-t1").visible);
        verify(child("windowMark-w1").visible);
        keyClick(Qt.Key_Left);
        focused("tab-t1");
        keyClick(Qt.Key_Left);
        focused("tab-t1");
        keyClick(Qt.Key_Up);
        focused("window-w1");
        keyClick(Qt.Key_Left);
        focused("window-w1");
        keyClick(Qt.Key_Right);
        focused("window-w2");
        compare(view.focusedWindowId, "w2");
        verify(child("windowMark-w2").visible);
        keyClick(Qt.Key_Down);
        focused("tab-t3");
        keyClick(Qt.Key_Right);
        focused("tab-t3");
        keyClick(Qt.Key_Down);
        focused("activateTab");
        keyClick(Qt.Key_Right);
        focused("closeTab");
        keyClick(Qt.Key_Right);
        focused("closeTab");
        keyClick(Qt.Key_Down);
        focused("shortcut-s1");
        compare(child("shortcut-s1").text, "Known");
        keyClick(Qt.Key_Down);
        focused("shortcut-s1");
        keyClick(Qt.Key_Right);
        focused("shortcut-s1");
        keyClick(Qt.Key_Tab);
        focused("shortcut-s1");
        for (let i = 0; i < 6; ++i) {
            keyClick(Qt.Key_Up);
            keyClick(Qt.Key_Left);
            keyClick(Qt.Key_Down);
            keyClick(Qt.Key_Right);
        }
        focusInside();
    }

    function test_activate_close_and_open() {
        focused("tab-t2");
        keyClick(Qt.Key_Return);
        compare(activates.count, 1);
        compare(activates.signalArguments[0][0], "t2");
        keyClick(Qt.Key_Down);
        focused("activateTab");
        keyClick(Qt.Key_Return);
        compare(activates.count, 2);
        compare(activates.signalArguments[1][0], "t2");
        keyClick(Qt.Key_Right);
        focused("closeTab");
        keyClick(Qt.Key_Return);
        compare(closes.count, 1);
        compare(closes.signalArguments[0][0], "t2");
        keyClick(Qt.Key_Down);
        focused("shortcut-s1");
        keyClick(Qt.Key_Return);
        compare(opens.count, 1);
        compare(opens.signalArguments[0][0], "s1");
        compare(opens.signalArguments[0].length, 1);
        keyClick(Qt.Key_Up);
        keyClick(Qt.Key_Up);
        keyClick(Qt.Key_Up);
        focused("window-w1");
        keyClick(Qt.Key_Right);
        focused("window-w2");
        keyClick(Qt.Key_Down);
        focused("tab-t3");
        keyClick(Qt.Key_Down);
        keyClick(Qt.Key_Right);
        focused("closeTab");
        keyClick(Qt.Key_Return);
        compare(closes.count, 2);
        compare(closes.signalArguments[1][0], "t3");
    }

    function test_one_active_tab() {
        compare(view.activeTabId, "t2");
        view.tabs = [
            {id: "a", windowId: "w", title: "Alpha", url: "loc-a", active: true, audible: false},
            {id: "b", windowId: "w", title: "Beta", url: "loc-b", active: true, audible: true}
        ];
        compare(view.activeTabId, "a");
        focused("tab-a");
        verify(child("active-a").visible);
        verify(!child("active-b").visible);
        verify(child("audible-b").visible);
    }

    function test_empty() {
        view.browserState = "empty";
        const pane = child("browserStatus");
        tryCompare(pane, "visible", true);
        compare(pane.kind, "empty");
        compare(pane.title, "No tabs");
        verify(findChild(view, "tab-t2") === null);
        focused("statusAction");
        keyClick(Qt.Key_Return);
        compare(retries.count, 1);
        keyClick(Qt.Key_Left);
        keyClick(Qt.Key_Up);
        keyClick(Qt.Key_Down);
        focused("shortcut-s1");
        keyClick(Qt.Key_Down);
        focused("shortcut-s1");
        focusInside();
    }

    function test_unavailable() {
        view.browserState = "unavailable";
        const pane = child("browserStatus");
        tryCompare(pane, "visible", true);
        compare(pane.kind, "unavailable");
        compare(pane.title, "No browser connected");
        compare(pane.description, "No browser is connected.");
        verify(findChild(view, "tab-t2") === null);
        verify(findChild(view, "window-w1") === null);
        focused("statusAction");
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
        compare(findChild(pane, "statusAction").visible, false);
        verify(findChild(view, "shortcut-s1") === null);
        focusInside();
        view.browserState = "error";
        tryCompare(pane, "kind", "error");
        focused("statusAction");
        keyClick(Qt.Key_Return);
        compare(retries.count, 1);
        focusInside();
    }
}
