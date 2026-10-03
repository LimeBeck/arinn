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
    function test_keyboard_unavailable_keeps_cancel_reachable() {
        home.keyboardAvailable = false;
        keyClick(Qt.Key_Menu); focused("shortcutCancel");
        verify(child("shortcutTitle").readOnly);
        verify(!child("shortcutSave").visible);
        keyClick(Qt.Key_Return);
        verify(!home.shortcutEditorOpen);
        focused("entry-one");
    }
}
