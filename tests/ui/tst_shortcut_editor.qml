// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import QtTest
import "../../apps/arinn-shell/ui" as Ui

TestCase {
    id: suite
    name: "ShortcutEditor"
    width: 1920; height: 1080
    visible: true
    when: windowShown
    property var home
    Component {
        id: factory
        Ui.HomeView {
            shortcutManagement: true
            applicationEntries: [{id: "one", title: "Example", group: "online", webShortcut: true}]
            onEditShortcutRequested: entryId => shortcutEditor = {id: entryId, title: entryId ? "Example" : "", url: entryId ? "https://example.test" : ""}
            onCloseShortcutEditorRequested: shortcutEditor = ({})
        }
    }
    SignalSpy { id: saves; signalName: "saveShortcutRequested" }
    SignalSpy { id: removals; signalName: "removeShortcutRequested" }
    function child(name) { const item = findChild(home, name); verify(item !== null, name); return item; }
    function focused(name) { tryCompare(child(name), "activeFocus", true, 5000, name); }
    function init() {
        mouseMove(suite, 10, 10);
        home = createTemporaryObject(factory, suite);
        verify(home);
        saves.target = home; saves.clear();
        removals.target = home; removals.clear();
        home.selectSection(3);
        focused("entry-one");
    }
    function cleanup() { saves.target = null; removals.target = null; home.destroy(); }
    function openEditor() { keyClick(Qt.Key_Menu); focused("shortcutTitle"); }
    function test_edit_save_error_and_backspace() {
        openEditor();
        compare(child("shortcutTitle").text, "Example");
        child("shortcutTitle").cursorPosition = 7;
        keyClick(Qt.Key_Backspace);
        compare(child("shortcutTitle").text, "Exampl");
        verify(home.shortcutEditorOpen);
        keyClick(Qt.Key_Down); focused("shortcutUrl");
        keyClick(Qt.Key_Down); focused("shortcutSave");
        keyClick(Qt.Key_Return);
        compare(saves.count, 1);
        compare(saves.signalArguments[0][0], "Exampl");
        home.shortcutError = "Conflict";
        compare(child("shortcutTitle").text, "Exampl");
        home.shortcutSaving = true;
        keyClick(Qt.Key_Return);
        compare(saves.count, 1);
        keyClick(Qt.Key_Escape);
        verify(!home.shortcutEditorOpen);
        focused("entry-one");
    }
    function test_remove_confirmation_defaults_to_cancel() {
        openEditor();
        keyClick(Qt.Key_Down); keyClick(Qt.Key_Down); keyClick(Qt.Key_Right); keyClick(Qt.Key_Right);
        focused("shortcutDelete"); keyClick(Qt.Key_Return);
        focused("shortcutKeep");
        compare(removals.count, 0);
        keyClick(Qt.Key_Escape); focused("shortcutDelete");
        keyClick(Qt.Key_Return); keyClick(Qt.Key_Right); focused("shortcutRemove");
        keyClick(Qt.Key_Return); compare(removals.count, 1);
    }
    function test_add_tile_and_home_discards_draft() {
        keyClick(Qt.Key_Down); focused("editShortcutButton");
        keyClick(Qt.Key_Down); focused("entry-shortcut-add");
        keyClick(Qt.Key_Return); focused("shortcutTitle");
        compare(child("shortcutTitle").text, "");
        verify(!child("shortcutDelete").visible);
        keyClick(Qt.Key_Home);
        verify(!home.shortcutEditorOpen);
        compare(home.screen, "home");
    }
    function key(row, column) { return child("keyboardKey-" + row + "-" + column); }
    // Without a hardware keyboard the editor is no longer a dead end: the
    // remote types both fields through the on-screen keyboard (KB-2, KB-3).
    function test_remote_types_both_fields_and_saves() {
        home.keyboardAvailable = false;
        keyClick(Qt.Key_Menu); focused("shortcutTitleKeyboard");
        verify(child("shortcutTitle").readOnly);
        verify(child("shortcutSave").visible);
        child("shortcutKeyboard").model.forget();
        keyClick(Qt.Key_Return);
        tryCompare(home, "shortcutTypingField", "title");
        focused("keyboardKeys");
        compare(child("shortcutTypingInput").text, "Example");
        // The name field has no context row: the middle letter row is row 1.
        tryCompare(key(1, 4), "focused", true);
        keyClick(Qt.Key_Return);
        compare(child("shortcutTitle").text, "Exampleg");
        // Done (the main key) moves on to the address field's button.
        keyClick(Qt.Key_Down); keyClick(Qt.Key_Down);
        for (let i = 0; i < 8; ++i) keyClick(Qt.Key_Right);
        tryCompare(key(3, 7), "focused", true);
        compare(key(3, 7).modelData.label, "Done");
        keyClick(Qt.Key_Return);
        tryCompare(home, "shortcutTypingField", "");
        focused("shortcutUrlKeyboard");
        // The address field types with the address row; Back keeps the text.
        child("shortcutKeyboard").model.forget();
        keyClick(Qt.Key_Return);
        tryCompare(home, "shortcutTypingField", "url");
        tryCompare(key(0, 0), "visible", true);
        compare(key(0, 0).modelData.label, "https://");
        keyClick(Qt.Key_Up); keyClick(Qt.Key_Up);
        keyClick(Qt.Key_Return);
        compare(child("shortcutUrl").text, "https://example.test.ru");
        keyClick(Qt.Key_Escape);
        tryCompare(home, "shortcutTypingField", "");
        focused("shortcutUrlKeyboard");
        verify(home.shortcutEditorOpen);
        keyClick(Qt.Key_Down); focused("shortcutSave");
        keyClick(Qt.Key_Return);
        compare(saves.count, 1);
        compare(saves.signalArguments[0][0], "Exampleg");
        compare(saves.signalArguments[0][1], "https://example.test.ru");
        keyClick(Qt.Key_Right); focused("shortcutCancel");
        keyClick(Qt.Key_Return);
        verify(!home.shortcutEditorOpen);
        focused("entry-one");
    }
    function test_name_suggestions_and_the_empty_name_line() {
        home.shortcutNameSuggestions = [{kind: "title", title: "Media Library", subtitle: "page title", value: "Media Library"},
                                        {kind: "domain", icon: "public", title: "example.test", subtitle: "domain", value: "example.test"}];
        openEditor();
        keyClick(Qt.Key_End);
        keyClick(Qt.Key_Right); focused("shortcutTitleKeyboard");
        keyClick(Qt.Key_Return);
        tryCompare(home, "shortcutTypingField", "title");
        // The empty-name line shows the domain that will be used.
        for (let i = 0; i < 7; ++i) child("shortcutKeyboard").erase();
        compare(child("shortcutTitle").text, "");
        verify(child("shortcutTypingIntent").visible);
        verify(child("shortcutTypingIntent").text.indexOf("example.test") >= 0);
        // Up from the top letter row reaches the suggestions.
        keyClick(Qt.Key_Up); keyClick(Qt.Key_Up);
        tryCompare(child("keyboardSuggestion-0"), "focused", true);
        keyClick(Qt.Key_Return);
        compare(child("shortcutTitle").text, "Media Library");
        verify(!child("shortcutTypingIntent").visible);
        // Up again leaves the panel for the field's Hide button; Back there
        // returns to the form.
        keyClick(Qt.Key_Up); focused("shortcutTypingKeyboard");
        keyClick(Qt.Key_Escape);
        tryCompare(home, "shortcutTypingField", "");
        focused("shortcutTitleKeyboard");
    }
    // A remote's Back hides the panel first, then leaves the editor.
    function test_back_key_hides_the_panel_then_closes_the_editor() {
        openEditor();
        keyClick(Qt.Key_End);
        keyClick(Qt.Key_Right);
        keyClick(Qt.Key_Return);
        tryCompare(home, "shortcutTypingField", "title");
        keyClick(Qt.Key_Back);
        tryCompare(home, "shortcutTypingField", "");
        focused("shortcutTitleKeyboard");
        verify(home.shortcutEditorOpen);
        keyClick(Qt.Key_Back);
        verify(!home.shortcutEditorOpen);
        focused("entry-one");
    }
    function test_home_while_typing_closes_the_editor() {
        openEditor();
        keyClick(Qt.Key_End);
        keyClick(Qt.Key_Right);
        keyClick(Qt.Key_Return);
        tryCompare(home, "shortcutTypingField", "title");
        keyClick(Qt.Key_Home);
        verify(!home.shortcutEditorOpen);
        compare(home.shortcutTypingField, "");
        compare(home.screen, "home");
    }
}
