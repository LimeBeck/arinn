// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import QtTest
import "../../apps/arinn-shell/ui" as Ui
import "../../apps/arinn-shell/ui/screens/library" as Library

// One shell-wide input mode: a key press hides the cursor and blocks pointer
// takeover for a cooldown; afterwards only genuine movement returns to
// pointer mode (#308, spec §6.1, ADR-0095).
TestCase {
    id: suite
    name: "InputMode"
    width: 1920; height: 1080
    visible: true
    when: windowShown
    property var home
    Component {
        id: homeFactory
        Ui.HomeView {
            continueEntries: [
                {id: "a", title: "First"}, {id: "b", title: "Second"}, {id: "c", title: "Third"},
                {id: "d", title: "Fourth"}, {id: "e", title: "Fifth"}
            ]
        }
    }
    Component {
        id: libraryFactory
        Library.LibraryView { visible: false }
    }
    function child(name) { const item = findChild(home, name); verify(item !== null, name); return item; }
    function focused(name) { tryCompare(child(name), "activeFocus", true, 5000, name); }
    function init() {
        // Park the pointer outside controls before Home exists.
        mouseMove(suite, 10, 10);
        Ui.InputMode.keyCooldownMs = 2000;
        Ui.InputMode.reset();
        // As when the shell window appears: the next position is a reference.
        Ui.InputMode.forgetPointer();
        home = createTemporaryObject(homeFactory, suite);
        verify(home);
        focused("entry-a");
        tryCompare(home, "hoverFocusEnabled", true);
    }
    function cleanup() {
        Ui.InputMode.keyCooldownMs = 2000;
        Ui.InputMode.reset();
    }
    // Pointer mode over an entry, entered without a preceding key.
    function pointAt(name, x) {
        // Two samples: the first may only set the reference position.
        mouseMove(child(name), (x || 100) - 1, 100);
        mouseMove(child(name), x || 100, 100);
        tryCompare(Ui.InputMode, "pointerActive", true);
        focused(name);
    }

    function test_key_press_enters_dpad_mode_and_keeps_its_action() {
        pointAt("entry-b");
        compare(home.pointerMode, true);
        keyClick(Qt.Key_Right);
        compare(Ui.InputMode.pointerActive, false);
        compare(home.pointerMode, false);
        verify(Ui.InputMode.cooldownActive);
        focused("entry-c");
    }

    function test_pointer_motion_in_cooldown_keeps_keyboard_focus() {
        keyClick(Qt.Key_Right);
        focused("entry-b");
        for (let x = 100; x < 160; x += 20)
            mouseMove(child("entry-d"), x, 100);
        wait(50);
        compare(Ui.InputMode.pointerActive, false);
        focused("entry-b");
        verify(child("entry-b").activeFocus);
        verify(!child("entry-d").activeFocus);
    }

    function test_repeated_keys_extend_the_cooldown() {
        Ui.InputMode.keyCooldownMs = 400;
        keyClick(Qt.Key_Right);
        wait(250);
        keyClick(Qt.Key_Left);
        wait(250);
        // 500 ms after the first key, 250 ms after the latest.
        verify(Ui.InputMode.cooldownActive);
        mouseMove(child("entry-c"), 100, 100);
        compare(Ui.InputMode.pointerActive, false);
        tryCompare(Ui.InputMode, "cooldownActive", false, 1000);
    }

    function test_cursor_stays_hidden_after_cooldown_until_movement() {
        Ui.InputMode.keyCooldownMs = 200;
        pointAt("entry-c");
        keyClick(Qt.Key_Left);
        focused("entry-b");
        tryCompare(Ui.InputMode, "cooldownActive", false, 1000);
        // Time alone neither reveals the cursor nor focuses the hovered entry.
        wait(300);
        compare(Ui.InputMode.pointerActive, false);
        verify(child("entry-b").activeFocus);
        // Genuine movement restores pointer interaction.
        mouseMove(child("entry-c"), 120, 100);
        tryCompare(Ui.InputMode, "pointerActive", true);
        focused("entry-c");
    }

    function test_resting_pointer_takes_nothing_after_cooldown() {
        Ui.InputMode.keyCooldownMs = 200;
        pointAt("entry-c");
        keyClick(Qt.Key_Left);
        focused("entry-b");
        tryCompare(Ui.InputMode, "cooldownActive", false, 1000);
        // Scrolling and a model refresh move content under the resting
        // pointer; the queued hover updates are not movement.
        child("continueViewport").contentX = 120;
        home.continueEntries = home.continueEntries.slice();
        wait(700);
        compare(Ui.InputMode.pointerActive, false);
        focused("entry-b");
        // A screen shown under the resting pointer reveals nothing either.
        const library = createTemporaryObject(libraryFactory, suite, {visible: true});
        verify(library);
        wait(300);
        compare(Ui.InputMode.pointerActive, false);
        compare(library.pointerMode, false);
    }

    function test_screens_share_one_mode_and_hidden_screens_take_no_focus() {
        const library = createTemporaryObject(libraryFactory, suite);
        verify(library);
        compare(library.pointerMode, false);
        pointAt("entry-b");
        compare(library.pointerMode, true);
        // The hidden library's queued hover and focus calls cannot take
        // focus from the screen on display.
        library.focusInitial();
        wait(50);
        focused("entry-b");
        keyClick(Qt.Key_Right);
        compare(library.pointerMode, false);
        compare(home.pointerMode, false);
        focused("entry-c");
    }

    function test_keys_still_type_and_act_in_dpad_mode() {
        // The input mode never consumes a key: Return still activates.
        const entries = signalSpy.createObject(suite, {target: home, signalName: "entryRequested"});
        keyClick(Qt.Key_Return);
        compare(entries.count, 1);
        compare(entries.signalArguments[0][0], "a");
        compare(Ui.InputMode.pointerActive, false);
    }
    Component { id: signalSpy; SignalSpy {} }
}
