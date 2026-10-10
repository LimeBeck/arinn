import "../../apps/arinn-shell/ui/screens/browse" as Browse
// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtTest
import "../../apps/arinn-shell/ui" as Ui

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
        // The panel remembers its last key per field while the shell runs.
        child("browseOnScreenKeyboard").model.forget();
    }

    function keyAt(row, column) {
        return child("keyboardKey-" + row + "-" + column);
    }

    function openKeyboard() {
        Ui.Focus.take(child("browseKeyboard"));
        keyClick(Qt.Key_Return);
        tryCompare(view, "keyboardOpen", true);
        tryCompare(child("keyboardKeys"), "activeFocus", true);
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
        // Field, keyboard button, then the list.
        keyClick(Qt.Key_Down);
        keyClick(Qt.Key_Down);
        tryCompare(child("browseRecent-0"), "activeFocus", true);
        keyClick(Qt.Key_Return);
        compare(edited.count, 1);
        compare(edited.signalArguments[0][0], "one.example");
        for (let i = 0; i < 4; ++i) keyClick(Qt.Key_Down)
        tryCompare(child("browseClear"), "activeFocus", true);
        keyClick(Qt.Key_Up);
        tryCompare(child("browseRecent-1"), "activeFocus", true);
    }

    // IN-8 is no longer a dead end: without a hardware keyboard, focus
    // starts on the keyboard button (ADR-0103).
    function test_remote_only_starts_on_the_keyboard_button() {
        view.keyboardAvailable = false;
        view.focusInitial();
        tryCompare(child("browseKeyboard"), "activeFocus", true);
        compare(child("browseKeyboard").text, "Keyboard");
        keyClick(Qt.Key_A);
        tryCompare(child("browseInput"), "activeFocus", true);
        compare(edited.signalArguments[0][0], "a");
        keyClick(Qt.Key_Backspace);
        compare(back.count, 0);
    }

    function test_busy_does_not_accept_programmatic_typing() {
        view.text = "ex";
        view.suggestions = [{
            "icon": "public",
            "title": "Example",
            "subtitle": "page.example",
            "value": "https://page.example",
            "open": true
        }];
        keyClick(Qt.Key_Down);
        tryCompare(child("browseKeyboard"), "activeFocus", true);
        keyClick(Qt.Key_Down);
        tryCompare(child("browseSuggestion-0"), "activeFocus", true);
        view.operationState = "running";
        keyClick(Qt.Key_A);
        compare(edited.count, 0);
    }

    function test_suggestion_row_while_typing_fills_the_field() {
        view.text = "pa";
        view.suggestions = [{
            "icon": "history",
            "title": "page.example",
            "value": "page.example"
        }];
        keyClick(Qt.Key_Down);
        keyClick(Qt.Key_Down);
        tryCompare(child("browseSuggestion-0"), "activeFocus", true);
        keyClick(Qt.Key_Return);
        compare(edited.signalArguments[0][0], "page.example");
        tryCompare(child("browseInput"), "activeFocus", true);
    }

    function test_keyboard_opens_types_and_hides_with_back() {
        keyClick(Qt.Key_Right);
        tryCompare(child("browseKeyboard"), "activeFocus", true);
        keyClick(Qt.Key_Return);
        tryCompare(view, "keyboardOpen", true);
        compare(child("browseKeyboard").text, "Hide");
        // KB-1: the fifth key of the middle letter row, English for an
        // address field.
        tryCompare(keyAt(2, 4), "focused", true);
        compare(keyAt(2, 4).modelData.label, "g");
        // The caret stays visible in the field while the panel types.
        verify(child("browseInput").cursorVisible);
        keyClick(Qt.Key_Return);
        compare(edited.count, 1);
        compare(edited.signalArguments[0][0], "g");
        compare(child("browseInput").text, "g");
        // Context row: https:// at the top left.
        keyClick(Qt.Key_Up);
        keyClick(Qt.Key_Up);
        tryCompare(keyAt(0, 2), "focused", true);
        keyClick(Qt.Key_Return);
        compare(child("browseInput").text, "g.ru");
        // Back hides the panel and focuses the button; the screen stays.
        keyClick(Qt.Key_Escape);
        tryCompare(view, "keyboardOpen", false);
        tryCompare(child("browseKeyboard"), "activeFocus", true);
        compare(back.count, 0);
        keyClick(Qt.Key_Escape);
        compare(back.count, 1);
    }

    function test_panel_moves_the_field_up_and_never_covers_it() {
        openKeyboard();
        const panel = child("browseOnScreenKeyboard");
        const intent = child("browseIntent");
        verify(panel.visible);
        verify(intent.y + 40 <= panel.y, "intent at " + intent.y + ", panel at " + panel.y);
        verify(!child("browseBack").visible);
        compare(panel.y + panel.height, 1080);
    }

    function test_backspace_cursor_and_main_key() {
        view.text = "page.example";
        view.kind = "address";
        openKeyboard();
        // Down twice to the action row, then right to the main key.
        keyClick(Qt.Key_Down);
        keyClick(Qt.Key_Down);
        for (let i = 0; i < 8; ++i)
            keyClick(Qt.Key_Right);
        tryCompare(keyAt(4, 7), "focused", true);
        compare(keyAt(4, 7).modelData.label, "Open");
        keyClick(Qt.Key_Return);
        compare(opened.count, 1);
        // Backspace key, then the cursor keys.
        keyClick(Qt.Key_Left);
        tryCompare(keyAt(4, 6), "focused", true);
        keyClick(Qt.Key_Return);
        compare(child("browseInput").text, "page.exampl");
        keyClick(Qt.Key_Left);
        keyClick(Qt.Key_Left);
        tryCompare(keyAt(4, 4), "focused", true);
        const before = child("browseInput").cursorPosition;
        keyClick(Qt.Key_Return);
        compare(child("browseInput").cursorPosition, before - 1);
    }

    function test_hardware_keyboard_types_while_the_panel_is_open() {
        openKeyboard();
        keyClick(Qt.Key_X);
        compare(edited.signalArguments[0][0], "x");
        verify(view.keyboardOpen);
        keyClick(Qt.Key_Backspace);
        compare(edited.signalArguments[1][0], "");
        verify(view.keyboardOpen);
        compare(back.count, 0);
    }

    function test_suggestions_row_and_leaving_up() {
        view.suggestions = [{
            "icon": "history",
            "title": "one query",
            "value": "one query"
        }, {
            "icon": "public",
            "title": "Media",
            "subtitle": "media.example",
            "value": "https://media.example",
            "open": true
        }];
        openKeyboard();
        verify(child("keyboardSuggestionOpen-1").visible);
        verify(!child("keyboardSuggestionOpen-0").visible);
        // Up from the context row reaches the suggestion above the key.
        keyClick(Qt.Key_Up);
        keyClick(Qt.Key_Up);
        keyClick(Qt.Key_Up);
        tryCompare(child("keyboardSuggestion-0"), "focused", true);
        keyClick(Qt.Key_Right);
        tryCompare(child("keyboardSuggestion-1"), "focused", true);
        keyClick(Qt.Key_Right);
        tryCompare(child("keyboardSuggestion-1"), "focused", true);
        keyClick(Qt.Key_Return);
        compare(edited.signalArguments[0][0], "https://media.example");
        verify(view.keyboardOpen);
        // Up again leaves the panel to the keyboard button.
        keyClick(Qt.Key_Up);
        tryCompare(child("browseKeyboard"), "activeFocus", true);
        verify(view.keyboardOpen);
        // Down comes back to the suggestions, and then to the keys.
        keyClick(Qt.Key_Down);
        tryCompare(child("keyboardSuggestion-0"), "focused", true);
        // The key below the first of two suggestions: www.
        keyClick(Qt.Key_Down);
        tryCompare(keyAt(0, 1), "focused", true);
    }

    function test_no_suggestions_line_and_up_to_the_button() {
        openKeyboard();
        verify(child("keyboardNoSuggestions").visible);
        keyClick(Qt.Key_Up);
        keyClick(Qt.Key_Up);
        keyClick(Qt.Key_Up);
        tryCompare(child("browseKeyboard"), "activeFocus", true);
    }

    function test_four_long_suggestions_fit_the_panel() {
        let entries = [];
        for (let i = 0; i < 4; ++i) entries.push({
            "icon": i % 2 ? "public" : "history",
            "title": "a rather long suggestion title number " + i,
            "subtitle": "long-subtitle-" + i + ".example",
            "value": "value " + i,
            "open": i % 2 === 1
        })
        view.suggestions = entries;
        openKeyboard();
        for (let i = 0; i < 4; ++i) {
            const pill = child("keyboardSuggestion-" + i);
            const p = pill.mapToItem(view, 0, 0);
            verify(p.x >= 120 && p.x + pill.width <= 1800, "pill " + i + " at " + p.x + "+" + pill.width);
        }
    }

    // Over the limit nothing changes, not even a selection the key would
    // have replaced.
    function test_over_limit_keeps_the_selection() {
        openKeyboard();
        const field = child("browseInput");
        field.maximumLength = 10;
        field.text = "0123456789";
        field.select(0, 2);
        child("browseOnScreenKeyboard").insertText("https://");
        compare(field.text, "0123456789");
        compare(field.selectedText, "01");
        // Replacing the selection with text that fits works.
        child("browseOnScreenKeyboard").insertText("ab");
        compare(field.text, "ab23456789");
    }

    // Backspace removes a whole character, never half of a surrogate pair.
    function test_backspace_removes_a_whole_emoji() {
        openKeyboard();
        const field = child("browseInput");
        field.text = "a\u{1F600}";
        field.cursorPosition = field.length;
        child("browseOnScreenKeyboard").erase();
        compare(field.text, "a");
    }

    function test_reopen_returns_to_the_last_key() {
        openKeyboard();
        keyClick(Qt.Key_Right);
        keyClick(Qt.Key_Right);
        tryCompare(keyAt(2, 6), "focused", true);
        keyClick(Qt.Key_Escape);
        tryCompare(view, "keyboardOpen", false);
        keyClick(Qt.Key_Return);
        tryCompare(view, "keyboardOpen", true);
        tryCompare(keyAt(2, 6), "focused", true);
    }

    function test_unavailable_browser_hides_the_panel() {
        openKeyboard();
        view.browserUnavailable = true;
        tryCompare(view, "keyboardOpen", false);
        tryCompare(child("browseRetry"), "activeFocus", true);
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
        for (let i = 0; i < 5; ++i) keyClick(Qt.Key_Down)
        const last = child("browseOrdinary-3");
        tryCompare(last, "activeFocus", true);
        const p = last.mapToItem(view, 0, 0);
        verify(p.x + last.width <= 1600);
        verify(p.y >= 438 && p.y + last.height <= 888);
    }

    function test_focus_move_in_the_model_change_turn_scrolls_to_the_row() {
        // #275: Columns lay out new rows on the next polish pass. Moving focus
        // in the same turn must not scroll by stale positions.
        let apps = [];
        for (let i = 0; i < 8; ++i) apps.push({
            "id": "app" + i,
            "title": "Browser " + i
        })
        view.ordinaryApplications = apps;
        for (let i = 0; i < 9; ++i) view.move(1)
        const last = child("browseOrdinary-7");
        verify(last.activeFocus);
        const p = last.mapToItem(view, 0, 0);
        verify(p.y >= 438 && p.y + last.height <= 888, "row at " + p.y);
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
