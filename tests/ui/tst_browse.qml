import "../../apps/arinn-shell/ui/screens/browse" as Browse
// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtTest

TestCase {
    id: suite

    property var view

    function init() {
        view = createTemporaryObject(factory, suite);
        verify(view);
        opened.target = view;
        edited.target = view;
        back.target = view;
        opened.clear();
        edited.clear();
        back.clear();
        view.focus = true;
        view.focusInitial();
    }

    function child(name) {
        const item = findChild(view, name);
        verify(item, name);
        return item;
    }

    function test_enter_needs_resolution_and_backspace_edits() {
        keyClick(Qt.Key_Return);
        compare(opened.count, 0);
        view.text = "page.example";
        view.kind = "address";
        view.resolvedUrl = "https://page.example";
        tryCompare(child("browseInput"), "text", "page.example");
        keyClick(Qt.Key_Backspace);
        compare(back.count, 0);
        compare(edited.count, 1);
        keyClick(Qt.Key_Return);
        compare(opened.count, 1);
        keyClick(Qt.Key_Escape);
        compare(back.count, 1);
    }

    function test_invalid_input_does_not_open() {
        view.kind = "invalid";
        keyClick(Qt.Key_Return);
        compare(opened.count, 0);
        verify(child("browseIntent").text.indexOf("http") >= 0);
    }

    function test_recent_and_clear_are_reachable() {
        view.recent = ["one.example", "second query"];
        keyClick(Qt.Key_Down);
        tryCompare(child("browseRecent-0"), "activeFocus", true);
        keyClick(Qt.Key_Return);
        compare(edited.count, 1);
        compare(edited.signalArguments[0][0], "one.example");
        keyClick(Qt.Key_Down);
        keyClick(Qt.Key_Down);
        keyClick(Qt.Key_Down);
        tryCompare(child("browseClear"), "activeFocus", true);
        keyClick(Qt.Key_Up);
        tryCompare(child("browseRecent-1"), "activeFocus", true);
    }

    function test_remote_only_shortcut_and_printable_key() {
        view.keyboardAvailable = false;
        view.suggestions = [{
            "id": "s",
            "title": "Example",
            "subtitle": "page.example"
        }];
        view.focusInitial();
        tryCompare(child("browseShortcut-s"), "activeFocus", true);
        keyClick(Qt.Key_A);
        tryCompare(child("browseInput"), "activeFocus", true);
        compare(edited.signalArguments[0][0], "a");
        keyClick(Qt.Key_Backspace);
        compare(back.count, 0);
    }

    function test_busy_does_not_accept_programmatic_typing() {
        view.suggestions = [{
            "id": "s",
            "title": "Example",
            "subtitle": "page.example"
        }];
        keyClick(Qt.Key_Down);
        tryCompare(child("browseShortcut-s"), "activeFocus", true);
        view.operationState = "running";
        keyClick(Qt.Key_A);
        compare(edited.count, 0);
    }

    function test_many_ordinary_actions_remain_visible_when_focused() {
        view.ordinaryApplications = [{
            "id": "a",
            "title": "Browser A"
        }, {
            "id": "b",
            "title": "Browser B"
        }, {
            "id": "c",
            "title": "Browser C"
        }, {
            "id": "d",
            "title": "Browser D"
        }];
        for (let i = 0; i < 4; ++i) keyClick(Qt.Key_Down)
        const last = child("browseOrdinary-3");
        tryCompare(last, "activeFocus", true);
        const p = last.mapToItem(view, 0, 0);
        verify(p.x + last.width <= 1600);
        verify(p.y >= 438 && p.y + last.height <= 888);
    }

    function test_unavailable_focuses_retry() {
        view.browserUnavailable = true;
        view.focusInitial();
        tryCompare(child("browseRetry"), "activeFocus", true);
        keyClick(Qt.Key_Down);
        tryCompare(child("browseBack"), "activeFocus", true);
    }

    name: "BrowseInput"
    width: 1920
    height: 1080
    visible: true
    when: windowShown

    Component {
        id: factory

        Browse.BrowseView {
        }

    }

    SignalSpy {
        id: opened

        signalName: "openRequested"
    }

    SignalSpy {
        id: edited

        signalName: "textEdited"
    }

    SignalSpy {
        id: back

        signalName: "backRequested"
    }

}
