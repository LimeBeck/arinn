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
        // The handoff's bookmark_add: a ribbon with a plus, painted by the
        // shell rather than an outlined box.
        compare(child("bookmark-a").iconName, "bookmark_add");
        compare(child("bookmarkIcon-a").name, "bookmark_add");
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
        compare(child("bookmarkIcon-b").name, "bookmark_added");
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
        compare(child("saveError").visible, false);
        view.saveError = "InvalidArgument";
        tryCompare(child("saveError"), "visible", true);
        compare(child("saveError").text, "This address cannot be saved.");
        view.saveError = "Conflict";
        compare(child("saveError").text, "This page is already saved.");
        view.saveError = "NotFound";
        compare(child("saveError").text, "This page is no longer open.");
        view.saveError = "Unavailable";
        compare(child("saveError").text, "The page could not be saved.");
    }

    function test_focus_move_dismisses_save_error() {
        focused("row-a");
        view.saveError = "InvalidArgument";
        tryCompare(child("saveError"), "visible", true);
        view.saveErrorDismissed.connect(() => { view.saveError = ""; });
        keyClick(Qt.Key_Right);
        focused("bookmark-a");
        compare(view.saveError, "");
        compare(child("saveError").visible, false);
    }

    function test_save_error_keeps_the_list_position() {
        const many = [];
        for (let i = 0; i < 12; ++i) {
            many.push({
                id: "n" + i, windowId: "w", title: "Row " + i,
                url: "https://pages.example/" + i, active: i === 0,
                audible: false, saved: false
            });
        }
        view.tabs = many;
        tryCompare(child("listPosition"), "visible", true);
        view.saveError = "InvalidArgument";
        tryCompare(child("saveError"), "visible", true);
        compare(child("listPosition").visible, true);
        compare(child("listPosition").text, "1 of 12");
    }

    function test_bookmark_disabled_without_browser() {
        // The list is hidden while the browser is unavailable, so this does not
        // show a disabled bookmark on screen. It checks the guard OK uses
        // before that list exists: save stays off and no savePage is emitted.
        focused("row-a");
        keyClick(Qt.Key_Right);
        focused("bookmark-a");
        compare(view.saveEnabled, true);
        compare(child("bookmark-a").enabled, true);
        view.browserState = "unavailable";
        compare(view.saveEnabled, false);
        compare(view.showTabs, false);
        view.requestSave("a");
        compare(saves.count, 0);
    }
}
