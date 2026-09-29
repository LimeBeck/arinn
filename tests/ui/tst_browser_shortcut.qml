// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import QtTest
import "../../apps/arinn-shell/ui/screens/browser" as Browser

TestCase {
    id: suite
    name: "BrowserShortcutSave"
    width: 1920
    height: 1080
    visible: true
    when: windowShown

    property var view
    Component {
        id: factory
        Browser.BrowserView {
            tabs: [
                {id: "a", windowId: "w", title: "Page", url: "https://pages.example/a", active: true, audible: false, saved: false},
                {id: "b", windowId: "w2", title: "Kept", url: "https://kept.example/a", active: true, audible: false, saved: true}
            ]
        }
    }
    SignalSpy { id: saves; signalName: "savePage" }

    function child(name) {
        const item = findChild(view, name);
        verify(item !== null, name);
        return item;
    }
    function focused(name) {
        tryCompare(child(name), "activeFocus", true, 5000, name);
    }
    function init() {
        mouseMove(suite, 10, 10);
        view = createTemporaryObject(factory, suite);
        verify(view);
        saves.target = view;
        saves.clear();
    }
    function cleanup() {
        saves.target = null;
        if (view)
            view.destroy();
    }

    function test_focus_order_is_row_bookmark_close() {
        focused("row-a");
        keyClick(Qt.Key_Right);
        focused("bookmark-a");
        compare(child("bookmark-a").iconName, "bookmark");
        verify(!child("mark-a").visible);
        keyClick(Qt.Key_Right);
        focused("close-a");
        keyClick(Qt.Key_Right);
        focused("close-a");
        keyClick(Qt.Key_Left);
        focused("bookmark-a");
        keyClick(Qt.Key_Left);
        focused("row-a");
        keyClick(Qt.Key_Down);
        focused("row-b");
        keyClick(Qt.Key_Right);
        focused("bookmark-b");
        keyClick(Qt.Key_Up);
        focused("bookmark-a");
        keyClick(Qt.Key_Down);
        focused("bookmark-b");
        compare(child("bookmark-b").iconName, "bookmark_added");
        compare(child("mark-b").text, "Shortcut");
        verify(child("mark-b").visible);
        keyClick(Qt.Key_Return);
        compare(saves.count, 0);
        keyClick(Qt.Key_Up);
        focused("bookmark-a");
        keyClick(Qt.Key_Return);
        compare(saves.count, 1);
        compare(saves.signalArguments[0][0], "a");
    }

    function test_rejected_address_shows_daemon_error() {
        focused("row-a");
        compare(child("saveError").visible, false);
        view.operationState = "InvalidArgument";
        tryCompare(child("saveError"), "visible", true);
        compare(child("saveError").text, "InvalidArgument");
    }

    function test_bookmark_disabled_without_browser() {
        focused("row-a");
        keyClick(Qt.Key_Right);
        focused("bookmark-a");
        compare(view.saveEnabled, true);
        compare(child("bookmark-a").enabled, true);
        view.browserState = "unavailable";
        compare(view.saveEnabled, false);
        view.requestSave("a");
        compare(saves.count, 0);
    }
}
