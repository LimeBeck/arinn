// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import QtQuick.Window
import QtTest
import "../../apps/arinn-shell/ui" as Ui
import "../../apps/arinn-shell/ui/screens/settings" as Settings

TestCase {
    id: suite
    name: "SettingsView"
    width: 1920
    height: 1080
    visible: true
    when: windowShown
    property var view
    // Keys the view leaves unhandled reach the suite, as they reach Main.qml.
    property int menuKeys: 0
    Keys.onPressed: event => { if (event.key === Qt.Key_Menu) ++menuKeys; }

    Component {
        id: factory
        Settings.SettingsView {
            categories: [
                { id: "media", title: "Media", icon: "video_library" },
                { id: "playback", title: "Playback", icon: "play_circle" },
                { id: "sound", title: "Sound", icon: "volume_up" },
                { id: "network", title: "Network", icon: "wifi" },
                { id: "applications", title: "Applications", icon: "apps" },
                { id: "appearance", title: "Appearance", icon: "palette" },
                { id: "system", title: "System", icon: "memory" }
            ]
            rows: [
                { providerId: "library", providerName: "Library", id: "scan", categoryId: "media",
                  title: "Scan folders", description: "Look for new files", type: "boolean",
                  currentValue: false, constraints: {}, enabled: true },
                { providerId: "library", providerName: "Library", id: "roots", categoryId: "media",
                  title: "Folders", description: "Places to read", type: "path",
                  currentValue: ["/videos"], constraints: { directory: true, multiple: true, maxItems: 3 },
                  enabled: true },
                { providerId: "library", providerName: "Library", id: "folder", categoryId: "media",
                  title: "Download location", description: "Where files go", type: "path",
                  currentValue: "", constraints: { directory: true, multiple: false }, enabled: true },
                { providerId: "audio", providerName: "Audio", id: "output", categoryId: "sound",
                  title: "Output", description: "Audio output", type: "enum", currentValue: "stereo",
                  constraints: { options: [{ value: "stereo", title: "Stereo" }, { value: "surround", title: "Surround" }] },
                  enabled: true },
                { providerId: "shell", providerName: "Shell", id: "label", categoryId: "applications",
                  title: "Label", description: "Shown under the icon", type: "string", currentValue: "note",
                  constraints: { maxLength: 40 }, enabled: true },
                { providerId: "shell", providerName: "Shell", id: "locked", categoryId: "applications",
                  title: "Locked", description: "Cannot be changed", type: "string", currentValue: "fixed",
                  constraints: { maxLength: 40 }, enabled: false },
                { providerId: "shell", providerName: "Shell", id: "held", categoryId: "appearance",
                  title: "Theme", description: "Appearance name", type: "string", currentValue: "dark",
                  constraints: { maxLength: 20 }, enabled: true, pending: true },
                { providerId: "shell", providerName: "Shell", id: "broken", categoryId: "appearance",
                  title: "Scale", description: "Interface scale", type: "integer", currentValue: 1,
                  constraints: { min: 1, max: 3 }, enabled: true, error: "InvalidArgument" },
                { providerId: "shell", providerName: "Shell", id: "idle", categoryId: "system",
                  title: "Idle minutes", description: "Wait before the ambient screen", type: "integer",
                  currentValue: 10, constraints: { min: 1, max: 60 }, enabled: true }
            ]
        }
    }

    SignalSpy { id: changes; signalName: "valueChangeRequested" }
    SignalSpy { id: dismissals; signalName: "dismissRequested" }
    SignalSpy { id: homes; signalName: "homeRequested" }

    function child(name) {
        const item = findChild(view, name);
        verify(item !== null, name);
        return item;
    }
    function focused(name) {
        tryCompare(child(name), "activeFocus", true, 5000, name);
    }
    function insideView(item) {
        let current = item;
        while (current) {
            if (current === view) return true;
            current = current.parent;
        }
        return false;
    }
    function windowFocus() {
        const window = view.Window.window;
        verify(window !== null);
        return window.activeFocusItem;
    }
    function createView() {
        mouseMove(suite, 10, 10);
        view = createTemporaryObject(factory, suite);
        verify(view);
        changes.target = view;
        changes.clear();
        dismissals.target = view;
        dismissals.clear();
        homes.target = view;
        homes.clear();
        focused("category-media");
    }
    function init() { createView(); }
    function cleanup() {
        changes.target = null;
        dismissals.target = null;
        homes.target = null;
        view = null;
    }
    function down(steps) {
        for (let i = 0; i < steps; ++i) keyClick(Qt.Key_Down);
    }
    function copyRows(mutate) {
        const next = [];
        for (let i = 0; i < view.rows.length; ++i) {
            const row = Object.assign({}, view.rows[i]);
            if (mutate) mutate(row);
            next.push(row);
        }
        return next;
    }

    function test_category_focus() {
        focused("category-media");
        keyClick(Qt.Key_Down);
        focused("category-playback");
        keyClick(Qt.Key_Right);
        focused("category-playback");
        compare(child("emptyPane").visible, true);
        keyClick(Qt.Key_Down);
        focused("category-sound");
        keyClick(Qt.Key_Right);
        focused("setting-audio-output");
        compare(child("category-dot-sound").opacity, 1);
        compare(child("category-dot-sound").color, Ui.Theme.ember);
        verify(insideView(windowFocus()));
        keyClick(Qt.Key_Left);
        focused("category-sound");
        keyClick(Qt.Key_Left);
        focused("category-sound");
        down(4);
        focused("category-system");
        keyClick(Qt.Key_Down);
        focused("category-system");
        keyClick(Qt.Key_Right);
        focused("setting-shell-idle");
        keyClick(Qt.Key_Right);
        focused("setting-shell-idle");
        keyClick(Qt.Key_Down);
        focused("setting-shell-idle");
        keyClick(Qt.Key_Left);
        focused("category-system");
        for (let step = 0; step < 8; ++step) keyClick(Qt.Key_Up);
        focused("category-media");
        keyClick(Qt.Key_Up);
        focused("category-media");
        verify(insideView(windowFocus()));
    }

    function test_boolean_toggle() {
        keyClick(Qt.Key_Right);
        focused("setting-library-scan");
        compare(child("provider-library-scan").text, "Library");
        verify(findChild(view, "value-library-scan") === null);
        compare(child("knob-library-scan").x, 5);
        keyClick(Qt.Key_Return);
        compare(changes.count, 1);
        compare(changes.signalArguments[0][0], "library");
        compare(changes.signalArguments[0][1], "scan");
        compare(changes.signalArguments[0][2], true);
        compare(child("knob-library-scan").x, 5);
        compare(view.rows[0].currentValue, false);
    }

    // #329: Home leaves an editor or sheet without committing the draft.
    function test_home_from_editors_and_sheets() {
        down(6);
        focused("category-system");
        keyClick(Qt.Key_Right);
        focused("setting-shell-idle");
        keyClick(Qt.Key_Return);
        focused("integerStepper");
        keyClick(Qt.Key_Left);
        compare(child("integerValue").text, "9");
        keyClick(Qt.Key_Home);
        compare(homes.count, 1);
        compare(view.editingKey, "");
        compare(changes.count, 0);

        createView();
        down(4);
        focused("category-applications");
        keyClick(Qt.Key_Right);
        focused("setting-shell-label");
        keyClick(Qt.Key_Return);
        const editor = child("editor-shell-label");
        tryCompare(editor, "activeFocus", true);
        keyClick(Qt.Key_Backspace);
        keyClick(Qt.Key_Home);
        compare(homes.count, 1);
        compare(view.editingKey, "");
        compare(changes.count, 0);

        createView();
        down(2);
        focused("category-sound");
        keyClick(Qt.Key_Right);
        focused("setting-audio-output");
        keyClick(Qt.Key_Return);
        tryCompare(view, "choosingKey", "audio-output");
        keyClick(Qt.Key_Home);
        compare(homes.count, 1);
        compare(view.choosingKey, "");
        compare(changes.count, 0);
    }

    function test_integer_sheet_bounds_unit_and_hold() {
        down(6);
        focused("category-system");
        keyClick(Qt.Key_Right);
        focused("setting-shell-idle");
        compare(child("value-shell-idle").text, "10");
        keyClick(Qt.Key_Return);
        focused("integerStepper");
        compare(child("integerValue").text, "10");
        compare(child("integerBounds").text, "min. 1 · was 10 · max. 60");
        compare(child("integerUnit").visible, false);
        keyClick(Qt.Key_Left);
        compare(child("integerValue").text, "9");
        compare(changes.count, 0);
        keyClick(Qt.Key_1);
        keyClick(Qt.Key_5);
        compare(child("integerValue").text, "15");
        keyClick(Qt.Key_Return);
        compare(changes.count, 1);
        compare(changes.signalArguments[0][0], "shell");
        compare(changes.signalArguments[0][1], "idle");
        compare(changes.signalArguments[0][2], 15);
        focused("setting-shell-idle");
        changes.clear();
        view.rows = copyRows(row => {
            if (row.id === "idle") row.constraints = { min: 1, max: 60, unit: "min", step: 5 };
        });
        compare(child("value-shell-idle").text, "10 min");
        keyClick(Qt.Key_Return);
        focused("integerStepper");
        compare(child("integerUnit").text, "min");
        view.holdMark = Date.now() - 1100;
        view.nudgeInteger(1, true);
        compare(child("integerValue").text, "60");
        compare(child("integerNote").text, "This is the maximum");
        compare(child("integerPlus").opacity, 0.3);
        view.nudgeInteger(1, false);
        compare(child("integerValue").text, "60");
        compare(changes.count, 0);
        keyClick(Qt.Key_Escape);
        focused("setting-shell-idle");
        compare(changes.count, 0);
        verify(insideView(windowFocus()));
    }

    function test_enum_option_list() {
        down(2);
        focused("category-sound");
        keyClick(Qt.Key_Right);
        focused("setting-audio-output");
        compare(child("value-audio-output").text, "Stereo");
        verify(child("chevron-audio-output").visible);
        keyClick(Qt.Key_Return);
        focused("option-0");
        keyClick(Qt.Key_Down);
        focused("option-1");
        keyClick(Qt.Key_Down);
        focused("option-1");
        keyClick(Qt.Key_Return);
        compare(changes.count, 1);
        compare(changes.signalArguments[0][0], "audio");
        compare(changes.signalArguments[0][1], "output");
        compare(changes.signalArguments[0][2], "surround");
        focused("setting-audio-output");
        compare(child("value-audio-output").text, "Stereo");
        changes.clear();
        keyClick(Qt.Key_Return);
        focused("option-0");
        keyClick(Qt.Key_Escape);
        compare(changes.count, 0);
        focused("setting-audio-output");
        verify(insideView(windowFocus()));
    }

    function test_string_and_path_text() {
        down(4);
        focused("category-applications");
        keyClick(Qt.Key_Right);
        focused("setting-shell-label");
        keyClick(Qt.Key_Return);
        const editor = child("editor-shell-label");
        tryCompare(editor, "visible", true);
        tryCompare(editor, "activeFocus", true);
        compare(editor.text, "note");
        keyClick(Qt.Key_Backspace);
        compare(editor.text, "not");
        compare(view.editingKey, "shell-label");
        compare(changes.count, 0);
        compare(dismissals.count, 0);
        keyClick(Qt.Key_Return);
        compare(changes.count, 1);
        compare(changes.signalArguments[0][2], "not");
        focused("setting-shell-label");
        changes.clear();
        keyClick(Qt.Key_Left);
        focused("category-applications");
        for (let step = 0; step < 4; ++step) keyClick(Qt.Key_Up);
        focused("category-media");
        keyClick(Qt.Key_Right);
        keyClick(Qt.Key_Down);
        keyClick(Qt.Key_Down);
        focused("setting-library-folder");
        compare(child("value-library-folder").text, "Not set");
        keyClick(Qt.Key_Return);
        const pathEditor = child("editor-library-folder");
        tryCompare(pathEditor, "activeFocus", true);
        keyClick(Qt.Key_Slash);
        keyClick(Qt.Key_A);
        keyClick(Qt.Key_Return);
        compare(changes.count, 1);
        compare(changes.signalArguments[0][1], "folder");
        compare(changes.signalArguments[0][2], "/a");
        verify(insideView(windowFocus()));
    }

    function test_path_list() {
        focused("category-media");
        keyClick(Qt.Key_Right);
        keyClick(Qt.Key_Down);
        focused("path-library-roots-0");
        keyClick(Qt.Key_Right);
        focused("remove-library-roots-0");
        keyClick(Qt.Key_Right);
        focused("add-library-roots");
        keyClick(Qt.Key_Return);
        const editor = child("editor-library-roots-add");
        tryCompare(editor, "activeFocus", true);
        keyClick(Qt.Key_Slash);
        keyClick(Qt.Key_E);
        keyClick(Qt.Key_Return);
        compare(changes.count, 1);
        compare(changes.signalArguments[0][0], "library");
        compare(changes.signalArguments[0][1], "roots");
        compare(changes.signalArguments[0][2].length, 2);
        compare(changes.signalArguments[0][2][0], "/videos");
        compare(changes.signalArguments[0][2][1], "/e");
        focused("path-library-roots-0");
        changes.clear();
        keyClick(Qt.Key_Right);
        focused("remove-library-roots-0");
        keyClick(Qt.Key_Return);
        compare(changes.count, 1);
        compare(changes.signalArguments[0][2].length, 0);
        keyClick(Qt.Key_Left);
        keyClick(Qt.Key_Left);
        focused("category-media");
        verify(insideView(windowFocus()));
    }

    // #540 (spec §5.6, ADR-0071, handoff LF-4): a folder the library provider
    // reports unavailable stays in place with its note, is still removable,
    // and returns to normal when the provider reports it again.
    function test_unavailable_folder() {
        view.rows = copyRows(row => {
            if (row.id === "roots") {
                row.currentValue = ["/videos", "/mnt/share"];
                row.unavailablePaths = ["/mnt/share"];
            }
        });
        focused("category-media");
        keyClick(Qt.Key_Right);
        keyClick(Qt.Key_Down);
        focused("path-library-roots-0");
        verify(!child("pathNote-library-roots-0").visible);
        compare(child("path-library-roots-0").height, 72);
        const note = child("pathNote-library-roots-1");
        verify(note.visible);
        compare(note.text, "Unavailable. Its titles stay in the library as unavailable.");
        compare(note.color, Ui.Theme.error);
        verify(child("path-library-roots-1").height > 72);
        compare(child("remove-library-roots-1").height, child("path-library-roots-1").height);
        // Remove still works; it only takes the folder out of the setting.
        keyClick(Qt.Key_Right);
        focused("remove-library-roots-0");
        keyClick(Qt.Key_Right);
        focused("path-library-roots-1");
        keyClick(Qt.Key_Right);
        focused("remove-library-roots-1");
        keyClick(Qt.Key_Return);
        compare(changes.count, 1);
        compare(changes.signalArguments[0][1], "roots");
        compare(changes.signalArguments[0][2].length, 1);
        compare(changes.signalArguments[0][2][0], "/videos");
        // The share is back: the provider no longer reports it.
        view.rows = copyRows(row => { if (row.id === "roots") row.unavailablePaths = []; });
        tryCompare(child("pathNote-library-roots-1"), "visible", false);
        compare(child("path-library-roots-1").height, 72);
    }

    function test_disabled_pending_and_error() {
        down(4);
        focused("category-applications");
        keyClick(Qt.Key_Right);
        keyClick(Qt.Key_Down);
        focused("setting-shell-locked");
        compare(child("value-shell-locked").text, "fixed");
        compare(child("detail-shell-locked").text, "Cannot be changed now");
        compare(child("lock-shell-locked").visible, true);
        verify(findChild(view, "chevron-shell-locked") === null);
        keyClick(Qt.Key_Return);
        compare(changes.count, 0);
        keyClick(Qt.Key_Left);
        focused("category-applications");
        keyClick(Qt.Key_Down);
        focused("category-appearance");
        keyClick(Qt.Key_Right);
        focused("setting-shell-held");
        compare(child("pending-shell-held").text, "Saving…");
        keyClick(Qt.Key_Return);
        compare(changes.count, 0);
        keyClick(Qt.Key_Down);
        focused("setting-shell-broken");
        compare(child("error-shell-broken").text, "InvalidArgument");
        keyClick(Qt.Key_Return);
        compare(changes.count, 0);
        compare(child("error-shell-broken").color, Ui.Theme.error);
        verify(insideView(windowFocus()));
    }

    function test_string_rejects_overlong_draft() {
        down(4);
        keyClick(Qt.Key_Right);
        focused("setting-shell-label");
        keyClick(Qt.Key_Return);
        const editor = child("editor-shell-label");
        tryCompare(editor, "activeFocus", true);
        for (let i = 0; i < 40; ++i) keyClick(Qt.Key_A);
        keyClick(Qt.Key_Return);
        compare(changes.count, 0);
        tryCompare(editor, "activeFocus", true);
        keyClick(Qt.Key_Escape);
        focused("setting-shell-label");
        compare(child("value-shell-label").text, "note");
    }

    function test_back_key() {
        focused("category-media");
        keyClick(Qt.Key_Back);
        compare(dismissals.count, 1);
        focused("category-media");
        dismissals.clear();
        keyClick(Qt.Key_Right);
        focused("setting-library-scan");
        keyClick(Qt.Key_Back);
        focused("category-media");
        compare(dismissals.count, 0);
        verify(insideView(windowFocus()));
    }

    function test_focus_survives_row_republish() {
        keyClick(Qt.Key_Right);
        focused("setting-library-scan");
        keyClick(Qt.Key_Return);
        compare(changes.count, 1);
        view.rows = copyRows(row => { if (row.id === "scan") row.pending = true; });
        focused("setting-library-scan");
        verify(windowFocus() !== view);
        view.rows = copyRows(row => { if (row.id === "scan") row.pending = false; });
        focused("setting-library-scan");
        verify(windowFocus() !== view);
        keyClick(Qt.Key_Return);
        compare(changes.count, 2);
    }

    function test_pending_boolean_select_does_nothing() {
        view.rows = copyRows(row => { if (row.id === "scan") row.pending = true; });
        focused("category-media");
        keyClick(Qt.Key_Right);
        focused("setting-library-scan");
        compare(child("pending-library-scan").text, "Saving…");
        keyClick(Qt.Key_Return);
        compare(changes.count, 0);
    }

    function test_unavailable_screen() {
        view.screenState = "unavailable";
        tryCompare(child("screenStatus"), "visible", true);
        compare(child("screenStatus").kind, "unavailable");
        compare(child("category-media").parent.visible, false);
        const action = findChild(child("screenStatus"), "statusAction");
        verify(action !== null);
        compare(action.visible, true);
        keyClick(Qt.Key_Back);
        compare(dismissals.count, 1);
        verify(insideView(windowFocus()));
    }

    function test_enum_paging() {
        down(2);
        focused("category-sound");
        keyClick(Qt.Key_Right);
        const options = [];
        for (let i = 0; i < 8; ++i)
            options.push({
                value: "o" + i,
                title: i === 2 ? "A long option title that wraps onto a second line instead of being cut off" : ("Option " + i)
            });
        view.rows = copyRows(row => {
            if (row.id === "output") {
                row.currentValue = "o0";
                row.constraints = { options: options };
            }
        });
        focused("setting-audio-output");
        keyClick(Qt.Key_Return);
        focused("option-0");
        compare(child("optionPosition").text, "1 of 8");
        verify(child("option-2").height > 104);
        keyClick(Qt.Key_PageDown);
        focused("option-6");
        compare(child("optionPosition").text, "7 of 8");
        keyClick(Qt.Key_ChannelUp);
        focused("option-0");
        keyClick(Qt.Key_Escape);
        compare(changes.count, 0);
        focused("setting-audio-output");
        verify(insideView(windowFocus()));
    }

    // The Flickable that scrolls an option: the nearest ancestor with contentY.
    function optionViewport(item) {
        let current = item.parent;
        while (current && current.contentY === undefined)
            current = current.parent;
        verify(current !== null, "option viewport");
        return current;
    }
    // The option lies wholly inside the sheet's visible part. Returns how far
    // its middle is from the viewport's middle.
    function optionShown(index) {
        const item = child("option-" + index);
        const flick = optionViewport(item);
        const top = item.mapToItem(flick, 0, 0).y;
        verify(top >= 0 && top + item.height <= flick.height,
               "option-" + index + " at " + top + " in a viewport of " + flick.height
               + " (contentY " + flick.contentY + ")");
        return top + item.height / 2 - flick.height / 2;
    }

    // #537 (handoff EN-2): a long choice sheet opens on the current option,
    // scrolled so it is on screen and centred, not at the top of the list
    // with the focused option out of sight. Moving keeps it on screen.
    function test_long_enum_opens_on_the_current_option() {
        down(2);
        focused("category-sound");
        keyClick(Qt.Key_Right);
        const options = [];
        for (let i = 0; i < 64; ++i)
            options.push({ value: "o" + i, title: "Option " + i });
        view.rows = copyRows(row => {
            if (row.id === "output") {
                row.currentValue = "o44";
                row.constraints = { options: options };
            }
        });
        focused("setting-audio-output");
        keyClick(Qt.Key_Return);
        focused("option-44");
        compare(child("optionPosition").text, "45 of 64");
        // Centred: the option's middle is within one option of the middle.
        verify(Math.abs(optionShown(44)) <= child("option-44").height);
        // The remote's arrows and paging keep the focused option on screen.
        keyClick(Qt.Key_Down);
        focused("option-45");
        optionShown(45);
        keyClick(Qt.Key_Up);
        keyClick(Qt.Key_Up);
        focused("option-43");
        optionShown(43);
        for (let i = 0; i < 4; ++i)
            keyClick(Qt.Key_PageDown);
        focused("option-63");
        optionShown(63);
        compare(child("optionPosition").text, "64 of 64");
        for (let i = 0; i < 11; ++i)
            keyClick(Qt.Key_PageUp);
        focused("option-0");
        optionShown(0);
        keyClick(Qt.Key_Escape);
        focused("setting-audio-output");
        compare(changes.count, 0);
        // Opened again on another value, it goes straight to that option.
        view.rows = copyRows(row => { if (row.id === "output") row.currentValue = "o20"; });
        focused("setting-audio-output");
        keyClick(Qt.Key_Return);
        focused("option-20");
        verify(Math.abs(optionShown(20)) <= child("option-20").height);
        keyClick(Qt.Key_Escape);
        focused("setting-audio-output");
        compare(changes.count, 0);
    }

    function keyAt(row, column) { return child("keyboardKey-" + row + "-" + column); }

    // STR-2 is no longer a dead end: without a hardware keyboard, the remote
    // types through the on-screen keyboard (KB-4, ADR-0103).
    function test_string_without_keyboard() {
        view.keyboardAvailable = false;
        down(4);
        focused("category-applications");
        keyClick(Qt.Key_Right);
        focused("setting-shell-label");
        keyClick(Qt.Key_Return);
        focused("stringKeyboard");
        verify(child("stringSave").visible);
        child("stringOnScreenKeyboard").model.forget();
        keyClick(Qt.Key_Return);
        tryCompare(view, "stringTyping", true);
        focused("keyboardKeys");
        compare(child("stringTypingInput").text, "note");
        // No context row: the fifth key of the middle letter row.
        tryCompare(keyAt(1, 4), "focused", true);
        keyClick(Qt.Key_Return);
        compare(child("editor-shell-label").text, "noteg");
        // Back returns to the sheet's keyboard button without saving.
        keyClick(Qt.Key_Escape);
        tryCompare(view, "stringTyping", false);
        focused("stringKeyboard");
        compare(changes.count, 0);
        // A key from a hardware keyboard still wakes the field.
        keyClick(Qt.Key_A);
        focused("editor-shell-label");
        compare(child("editor-shell-label").text, "notega");
        verify(insideView(windowFocus()));
    }

    function test_string_panel_saves_with_its_main_key() {
        down(4);
        keyClick(Qt.Key_Right);
        focused("setting-shell-label");
        keyClick(Qt.Key_Return);
        focused("editor-shell-label");
        keyClick(Qt.Key_End);
        keyClick(Qt.Key_Right);
        focused("stringKeyboard");
        child("stringOnScreenKeyboard").model.forget();
        keyClick(Qt.Key_Return);
        tryCompare(view, "stringTyping", true);
        keyClick(Qt.Key_Down);
        keyClick(Qt.Key_Down);
        for (let i = 0; i < 8; ++i) keyClick(Qt.Key_Right);
        tryCompare(keyAt(3, 7), "focused", true);
        compare(keyAt(3, 7).modelData.label, "Save");
        keyClick(Qt.Key_Escape);
        tryCompare(view, "stringTyping", false);
        compare(changes.count, 0);
        keyClick(Qt.Key_Return);
        tryCompare(view, "stringTyping", true);
        tryCompare(keyAt(3, 7), "focused", true);
        keyClick(Qt.Key_Return);
        compare(changes.count, 1);
        compare(changes.signalArguments[0][2], "note");
        tryCompare(view, "stringTyping", false);
    }

    function test_string_panel_counter_and_limit() {
        down(4);
        keyClick(Qt.Key_Right);
        focused("setting-shell-label");
        keyClick(Qt.Key_Return);
        focused("editor-shell-label");
        keyClick(Qt.Key_End);
        keyClick(Qt.Key_Right);
        keyClick(Qt.Key_Return);
        tryCompare(view, "stringTyping", true);
        const input = child("stringTypingInput");
        const counter = child("stringTypingCounter");
        // The fixture allows 40 characters: the counter shows from 30.
        input.text = "a".repeat(29);
        verify(!counter.visible);
        const fullWidth = input.width;
        input.text = "a".repeat(30);
        verify(counter.visible);
        compare(counter.text, "30 / 40");
        // The field ends before the counter, so the caret is never under it.
        verify(input.width < fullWidth);
        verify(input.mapToItem(null, input.width, 0).x
               <= counter.mapToItem(null, 0, 0).x);
        // The limit counts characters, as the settings contract does: an
        // emoji outside the BMP is one character, not two UTF-16 units.
        input.text = "a".repeat(39);
        child("stringOnScreenKeyboard").insertText("\u{1F600}");
        compare([...input.text].length, 40);
        compare(counter.color, Qt.color("#E8474F"));
        // Input past the limit is ignored.
        child("stringOnScreenKeyboard").insertText("b");
        compare([...input.text].length, 40);
        compare([...view.editingDraft].length, 40);
        // Key text outside the BMP counts as typed text for every handler in
        // the sheet and the layer (QtTest cannot send it as a key).
        verify(view.printable("\u{1F600}"));
        verify(view.printable("a"));
        verify(!view.printable("\u0001"));
        verify(!view.printable(""));
        // Menu is left to the shell's navigation, as on every screen.
        suite.menuKeys = 0;
        keyClick(Qt.Key_Menu);
        compare(suite.menuKeys, 1);
        verify(view.stringTyping);
        keyClick(Qt.Key_Home);
        tryCompare(view, "stringTyping", false);
        compare(changes.count, 0);
    }

    // The path panel's own items: the string panel uses the same names.
    function pathKeyAt(row, column) {
        const item = findChild(child("pathOnScreenKeyboard"), "keyboardKey-" + row + "-" + column);
        verify(item !== null, row + "-" + column);
        return item;
    }
    function pathKeysFocused() {
        const keys = findChild(child("pathOnScreenKeyboard"), "keyboardKeys");
        verify(keys !== null);
        tryCompare(keys, "activeFocus", true);
    }
    // Moves the path panel's focus along its current row to a labelled key.
    function moveToPathKey(label) {
        const model = child("pathOnScreenKeyboard").model;
        const keys = model.rows[model.row];
        let column = -1;
        for (let i = 0; i < keys.length; ++i)
            if (keys[i].label === label) column = i;
        verify(column >= 0, label);
        for (let step = 0; step < 12 && model.column > column; ++step) keyClick(Qt.Key_Left);
        for (let step = 0; step < 12 && model.column < column; ++step) keyClick(Qt.Key_Right);
        tryCompare(pathKeyAt(model.row, column), "focused", true);
    }

    // KB-5 (#426): without a hardware keyboard a single path opens on its
    // keyboard pill, and the remote types and saves it through the panel.
    function test_path_without_keyboard_types_with_the_panel() {
        view.keyboardAvailable = false;
        keyClick(Qt.Key_Right);
        focused("setting-library-scan");
        keyClick(Qt.Key_Down);
        keyClick(Qt.Key_Down);
        focused("setting-library-folder");
        keyClick(Qt.Key_Return);
        focused("keyboard-library-folder");
        verify(child("editor-library-folder").visible);
        compare(child("keyboard-library-folder").text, "Keyboard");
        child("pathOnScreenKeyboard").model.forget();
        keyClick(Qt.Key_Return);
        tryCompare(view, "pathTyping", true);
        pathKeysFocused();
        compare(child("pathTypingInput").text, "");
        compare(child("pathTypingKeyboard").text, "Hide");
        // The path context row on top; English letters below it, focus on
        // the fifth key of the middle letter row.
        const context = child("pathOnScreenKeyboard").model.rows[0].map(key => key.label);
        compare(context, ["/", "~/", "/media/", "/run/media/", "/mnt/", "..", "-", "_", "."]);
        tryCompare(pathKeyAt(2, 4), "focused", true);
        compare(pathKeyAt(2, 4).modelData.label, "g");
        keyClick(Qt.Key_Up);
        keyClick(Qt.Key_Up);
        moveToPathKey("/media/");
        keyClick(Qt.Key_Return);
        compare(child("pathTypingInput").text, "/media/");
        compare(child("editor-library-folder").text, "/media/");
        keyClick(Qt.Key_Down);
        moveToPathKey("e");
        keyClick(Qt.Key_Return);
        compare(child("pathTypingInput").text, "/media/e");
        compare(changes.count, 0);
        // The main key of an inline editor is Save; it proposes the path.
        for (let i = 0; i < 4; ++i) keyClick(Qt.Key_Down);
        moveToPathKey("Save");
        compare(child("pathOnScreenKeyboard").model.row, 4);
        keyClick(Qt.Key_Return);
        compare(changes.count, 1);
        compare(changes.signalArguments[0][1], "folder");
        compare(changes.signalArguments[0][2], "/media/e");
        tryCompare(view, "pathTyping", false);
        compare(view.editingKey, "");
        focused("setting-library-folder");
        verify(insideView(windowFocus()));
    }

    // KB-5: a path list item keeps Back, Home, Menu and a hardware keyboard
    // working while the panel is open, and the panel never covers the field.
    function test_path_panel_keys_and_layout() {
        keyClick(Qt.Key_Right);
        keyClick(Qt.Key_Down);
        focused("path-library-roots-0");
        keyClick(Qt.Key_Return);
        focused("editor-library-roots-0");
        compare(child("editor-library-roots-0").text, "/videos");
        // Right at the end of the text reaches the pill; Left goes back.
        keyClick(Qt.Key_Right);
        focused("keyboard-library-roots-0");
        keyClick(Qt.Key_Left);
        focused("editor-library-roots-0");
        keyClick(Qt.Key_Right);
        focused("keyboard-library-roots-0");
        keyClick(Qt.Key_Return);
        tryCompare(view, "pathTyping", true);
        pathKeysFocused();
        compare(child("pathTypingInput").text, "/videos");
        const field = child("pathTypingField");
        const panel = child("pathOnScreenKeyboard");
        verify(field.y + field.height <= panel.y, "field ends at " + (field.y + field.height) + ", panel at " + panel.y);
        // A hardware keyboard types into the field and keeps the panel open.
        keyClick(Qt.Key_Slash);
        keyClick(Qt.Key_A);
        compare(child("pathTypingInput").text, "/videos/a");
        compare(child("editor-library-roots-0").text, "/videos/a");
        keyClick(Qt.Key_Backspace);
        compare(child("pathTypingInput").text, "/videos/");
        verify(view.pathTyping);
        // Menu is left to the shell's navigation.
        suite.menuKeys = 0;
        keyClick(Qt.Key_Menu);
        compare(suite.menuKeys, 1);
        verify(view.pathTyping);
        // A catalog publish rebuilds the rows; typing goes on in the panel.
        view.rows = copyRows();
        pathKeysFocused();
        verify(view.pathTyping);
        // Back hides the panel and returns to the pill; the text stays.
        keyClick(Qt.Key_Escape);
        tryCompare(view, "pathTyping", false);
        focused("keyboard-library-roots-0");
        compare(child("editor-library-roots-0").text, "/videos/");
        compare(view.editingDraft, "/videos/");
        compare(changes.count, 0);
        // Reopened, Home leaves without proposing the draft.
        keyClick(Qt.Key_Return);
        tryCompare(view, "pathTyping", true);
        pathKeysFocused();
        keyClick(Qt.Key_Home);
        compare(homes.count, 1);
        tryCompare(view, "pathTyping", false);
        compare(view.editingKey, "");
        compare(changes.count, 0);
        // A second Back on the pill closes the editor.
        createView();
        keyClick(Qt.Key_Right);
        keyClick(Qt.Key_Down);
        focused("path-library-roots-0");
        keyClick(Qt.Key_Return);
        keyClick(Qt.Key_Right);
        focused("keyboard-library-roots-0");
        keyClick(Qt.Key_Escape);
        compare(view.editingKey, "");
        focused("path-library-roots-0");
        compare(changes.count, 0);
        verify(insideView(windowFocus()));
    }

    // Review of #472: a keyboard pill is not a text field, so Backspace (a
    // remote's Back on some devices) is Back there (§6.2, AC-18).
    function test_backspace_on_a_keyboard_pill_is_back() {
        // A single path without a hardware keyboard opens on its pill.
        view.keyboardAvailable = false;
        keyClick(Qt.Key_Right);
        keyClick(Qt.Key_Down);
        keyClick(Qt.Key_Down);
        focused("setting-library-folder");
        keyClick(Qt.Key_Return);
        focused("keyboard-library-folder");
        keyClick(Qt.Key_Backspace);
        compare(view.editingKey, "");
        focused("setting-library-folder");
        // A path list item, from the pill after Back from the panel.
        keyClick(Qt.Key_Up);
        focused("path-library-roots-0");
        keyClick(Qt.Key_Return);
        focused("keyboard-library-roots-0");
        keyClick(Qt.Key_Return);
        tryCompare(view, "pathTyping", true);
        keyClick(Qt.Key_Escape);
        tryCompare(view, "pathTyping", false);
        focused("keyboard-library-roots-0");
        keyClick(Qt.Key_Backspace);
        compare(view.editingKey, "");
        focused("path-library-roots-0");
        // The string sheet's pill, which predates KB-5, behaves the same.
        keyClick(Qt.Key_Left);
        focused("category-media");
        down(4);
        focused("category-applications");
        keyClick(Qt.Key_Right);
        focused("setting-shell-label");
        keyClick(Qt.Key_Return);
        focused("stringKeyboard");
        keyClick(Qt.Key_Backspace);
        compare(view.editingKey, "");
        focused("setting-shell-label");
        compare(changes.count, 0);
        compare(dismissals.count, 0);
        verify(insideView(windowFocus()));
    }

    // KB-5: Add path through the panel; an empty entry is not proposed.
    function test_add_path_with_the_panel() {
        keyClick(Qt.Key_Right);
        keyClick(Qt.Key_Down);
        focused("path-library-roots-0");
        keyClick(Qt.Key_Right);
        keyClick(Qt.Key_Right);
        focused("add-library-roots");
        keyClick(Qt.Key_Return);
        focused("editor-library-roots-add");
        keyClick(Qt.Key_Right);
        focused("keyboard-library-roots-add");
        child("pathOnScreenKeyboard").model.forget();
        keyClick(Qt.Key_Return);
        tryCompare(view, "pathTyping", true);
        for (let i = 0; i < 4; ++i) keyClick(Qt.Key_Down);
        moveToPathKey("Save");
        keyClick(Qt.Key_Return);
        compare(changes.count, 0);
        verify(view.pathTyping);
        pathKeysFocused();
        keyClick(Qt.Key_Slash);
        keyClick(Qt.Key_E);
        keyClick(Qt.Key_Return);
        compare(changes.count, 1);
        compare(changes.signalArguments[0][1], "roots");
        compare(changes.signalArguments[0][2].length, 2);
        compare(changes.signalArguments[0][2][0], "/videos");
        compare(changes.signalArguments[0][2][1], "/e");
        tryCompare(view, "pathTyping", false);
        focused("path-library-roots-0");
        verify(insideView(windowFocus()));
    }

    function test_saving_feedback() {
        down(2);
        focused("category-sound");
        keyClick(Qt.Key_Right);
        focused("setting-audio-output");
        keyClick(Qt.Key_Return);
        focused("option-0");
        keyClick(Qt.Key_Down);
        focused("option-1");
        keyClick(Qt.Key_Return);
        compare(changes.count, 1);
        compare(changes.signalArguments[0][2], "surround");
        view.rows = copyRows(row => { if (row.id === "output") row.pending = true; });
        tryVerify(() => {
            const spinner = findChild(view, "optionSpinner");
            return spinner !== null && spinner.visible;
        });
        keyClick(Qt.Key_Escape);
        focused("setting-audio-output");
        view.rows = copyRows(row => {
            if (row.id === "output") {
                row.pending = false;
                row.currentValue = "surround";
                row.deferred = true;
            }
        });
        tryCompare(child("settingsToastTitle"), "text", "Saved");
        compare(child("settingsToastDetail").text, "Applies after Arinn restarts");
        compare(child("detail-audio-output").text, "Applies after Arinn restarts");
        compare(child("value-audio-output").text, "Surround");
        verify(insideView(windowFocus()));
    }

    function test_rejected_text_stays_in_the_sheet() {
        down(4);
        focused("category-applications");
        keyClick(Qt.Key_Right);
        focused("setting-shell-label");
        keyClick(Qt.Key_Return);
        focused("editor-shell-label");
        keyClick(Qt.Key_Return);
        compare(changes.count, 1);
        view.rows = copyRows(row => { if (row.id === "label") row.pending = true; });
        view.rows = copyRows(row => {
            if (row.id === "label") {
                row.pending = false;
                row.error = "Name is not allowed";
            }
        });
        tryCompare(child("stringError"), "text", "Name is not allowed");
        compare(view.editingKey, "shell-label");
        focused("editor-shell-label");
        keyClick(Qt.Key_X);
        tryCompare(child("stringError"), "visible", false);
        compare(changes.count, 1);
        verify(insideView(windowFocus()));
    }

    function test_boolean_rejection_until_focus_leaves() {
        keyClick(Qt.Key_Right);
        focused("setting-library-scan");
        keyClick(Qt.Key_Return);
        compare(changes.count, 1);
        view.rows = copyRows(row => { if (row.id === "scan") row.pending = true; });
        view.rows = copyRows(row => {
            if (row.id === "scan") {
                row.pending = false;
                row.error = "HDMI is off";
            }
        });
        tryCompare(child("settingsToastDetail"), "text", "HDMI is off");
        compare(child("settingsToastTitle").text, "Could not save");
        compare(child("detail-library-scan").text, "HDMI is off");
        keyClick(Qt.Key_Down);
        tryCompare(child("detail-library-scan"), "text", "Look for new files");
        verify(insideView(windowFocus()));
    }
}
