// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import QtTest
import "../../apps/arinn-shell/ui" as Ui

TestCase {
    id: suite
    name: "SharedTextFields"
    width: 1920; height: 1080
    visible: true
    when: windowShown
    Component {
        id: frameFactory
        Ui.TextFieldFrame {
            id: frame
            x: 120; y: 160; width: 1480; height: 132
            editor: input
            property bool counterVisible: false
            property alias counter: counter
            Ui.TextField {
                id: input
                anchors.verticalCenter: parent.verticalCenter
                width: parent.width; height: 80
                placeholderText: "Website address or what to find"
                font.pixelSize: 48
                font.weight: 500
            }
            trailing: Ui.UiText {
                id: counter
                visible: frame.counterVisible
                text: "30 / 40"
                role: "meta"
            }
            onKeyboardKeyPressed: event => {
                if (event.key === Qt.Key_Left) {
                    Ui.Focus.take(input);
                    event.accepted = true;
                }
            }
        }
    }
    Component { id: keyboardFactory; Ui.OnScreenKeyboard { y: 500; maxCharacters: 4; context: "text" } }
    SignalSpy { id: submitted; signalName: "submitted" }
    SignalSpy { id: requested; signalName: "keyboardRequested" }
    function cleanup() { submitted.target = null; requested.target = null; }

    function test_native_editing_selection_and_submission() {
        const frame = createTemporaryObject(frameFactory, suite);
        const editor = frame.editor;
        submitted.target = editor;
        submitted.clear();
        Ui.Focus.take(editor);
        editor.text = "A\u{1F600}B";
        editor.cursorPosition = 3;
        keyClick(Qt.Key_Backspace);
        compare(editor.text, "AB");
        editor.select(0, 1);
        compare(editor.selectedText, "A");
        keyClick(Qt.Key_X);
        compare(editor.text, "xB");
        keyPress(Qt.Key_Return);
        compare(submitted.count, 1);
        keyRelease(Qt.Key_Return);
        compare(submitted.count, 1);
    }
    function test_read_only_busy_and_disabled() {
        const frame = createTemporaryObject(frameFactory, suite);
        const editor = frame.editor;
        submitted.target = editor;
        submitted.clear();
        Ui.Focus.take(editor);
        editor.text = "Kept";
        editor.readOnly = true;
        keyClick(Qt.Key_X);
        keyClick(Qt.Key_Return);
        compare(editor.text, "Kept");
        compare(submitted.count, 0);
        compare(frame.keyboardButton.enabled, false);
        editor.readOnly = false;
        editor.busy = true;
        keyClick(Qt.Key_Return);
        compare(submitted.count, 0);
        compare(frame.keyboardButton.enabled, false);
        editor.enabled = false;
        keyClick(Qt.Key_Return);
        compare(submitted.count, 0);
    }
    function test_keyboard_intent_and_local_return() {
        const frame = createTemporaryObject(frameFactory, suite);
        requested.target = frame;
        requested.clear();
        Ui.Focus.take(frame.keyboardButton);
        keyPress(Qt.Key_Return);
        compare(requested.count, 0);
        keyRelease(Qt.Key_Return);
        compare(requested.count, 1);
        frame.keyboardOpen = true;
        compare(frame.editor.cursorVisible, true);
        compare(frame.keyboardButton.panelOpen, true);
        keyClick(Qt.Key_Left);
        tryCompare(frame.editor, "activeFocus", true);
        frame.keyboardOpen = false;
        compare(frame.keyboardButton.panelOpen, false);
    }
    function test_counter_reserves_space_in_the_same_turn() {
        const frame = createTemporaryObject(frameFactory, suite);
        const before = frame.editor.width;
        frame.counterVisible = true;
        verify(frame.editor.width < before);
        verify(frame.editor.mapToItem(suite, frame.editor.width, 0).x
            <= frame.counter.mapToItem(suite, 0, 0).x);
    }
    function test_live_long_labels_error_and_font() {
        const frame = createTemporaryObject(frameFactory, suite);
        const editor = frame.editor;
        const hint = findChild(editor, "textFieldPlaceholder");
        verify(hint);
        for (const label of ["A very long website address or something to find in the library",
                             "Длинный адрес сайта или запрос для поиска в библиотеке"]) {
            editor.placeholderText = label;
            compare(hint.text, label);
            compare(editor.Accessible.name, label);
            compare(hint.font.pixelSize, editor.font.pixelSize);
            verify(hint.width <= editor.width);
        }
        editor.errorText = "Synthetic validation error";
        compare(frame.error, true);
        compare(editor.Accessible.description, "Synthetic validation error");
        editor.errorText = "";
        compare(frame.error, false);
    }
    function test_native_editor_is_an_osk_target_with_unicode_limits() {
        const frame = createTemporaryObject(frameFactory, suite);
        const keyboard = createTemporaryObject(keyboardFactory, suite, {target: frame.editor});
        frame.keyboardOpen = true;
        frame.editor.text = "ab";
        frame.editor.cursorPosition = 2;
        keyboard.start();
        keyboard.insertText("\u{1F600}");
        keyboard.insertText("я");
        keyboard.insertText("x");
        compare(frame.editor.text, "ab\u{1F600}я");
        compare([...frame.editor.text].length, 4);
        verify(frame.editor.cursorVisible);
        keyboard.finish();
    }
    function test_pointer_and_dpad_keyboard_focus() {
        const frame = createTemporaryObject(frameFactory, suite, {pointerMode: true});
        wait(0); // Polish the translated keyboard label and its hit rectangle.
        mouseMove(frame.keyboardButton, 30, 30);
        mouseMove(frame.keyboardButton, 31, 30);
        tryCompare(frame.keyboardButton, "activeFocus", true);
        compare(frame.keyboardButton.pointerMode, true);
        keyClick(Qt.Key_Left);
        tryCompare(frame.editor, "activeFocus", true);
        frame.pointerMode = false;
        compare(frame.keyboardButton.pointerMode, false);
    }
}
