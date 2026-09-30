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
        focused("category-media");
    }
    function init() { createView(); }
    function cleanup() {
        changes.target = null;
        dismissals.target = null;
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
        focused("minus-shell-idle");
        keyClick(Qt.Key_Right);
        focused("setting-shell-idle");
        keyClick(Qt.Key_Right);
        focused("plus-shell-idle");
        keyClick(Qt.Key_Right);
        focused("plus-shell-idle");
        keyClick(Qt.Key_Down);
        focused("plus-shell-idle");
        keyClick(Qt.Key_Left);
        keyClick(Qt.Key_Left);
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

    function test_integer_stepper_and_number() {
        down(6);
        focused("category-system");
        keyClick(Qt.Key_Right);
        focused("minus-shell-idle");
        keyClick(Qt.Key_Return);
        compare(changes.count, 1);
        compare(changes.signalArguments[0][0], "shell");
        compare(changes.signalArguments[0][1], "idle");
        compare(changes.signalArguments[0][2], 9);
        changes.clear();
        keyClick(Qt.Key_Right);
        keyClick(Qt.Key_Right);
        focused("plus-shell-idle");
        keyClick(Qt.Key_Return);
        compare(changes.signalArguments[0][2], 11);
        changes.clear();
        keyClick(Qt.Key_Left);
        focused("setting-shell-idle");
        keyClick(Qt.Key_Return);
        const editor = child("editor-shell-idle");
        tryCompare(editor, "visible", true);
        tryCompare(editor, "activeFocus", true);
        keyClick(Qt.Key_Backspace);
        keyClick(Qt.Key_Backspace);
        keyClick(Qt.Key_1);
        keyClick(Qt.Key_5);
        keyClick(Qt.Key_Return);
        compare(changes.count, 1);
        compare(changes.signalArguments[0][2], 15);
        focused("minus-shell-idle");
        changes.clear();
        keyClick(Qt.Key_Right);
        focused("setting-shell-idle");
        keyClick(Qt.Key_Return);
        tryCompare(editor, "activeFocus", true);
        keyClick(Qt.Key_Backspace);
        keyClick(Qt.Key_Backspace);
        keyClick(Qt.Key_6);
        keyClick(Qt.Key_1);
        keyClick(Qt.Key_Return);
        compare(changes.count, 0);
        tryCompare(editor, "activeFocus", true);
        keyClick(Qt.Key_Escape);
        focused("minus-shell-idle");
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

    function test_disabled_pending_and_error() {
        down(4);
        focused("category-applications");
        keyClick(Qt.Key_Right);
        keyClick(Qt.Key_Down);
        focused("setting-shell-locked");
        compare(child("value-shell-locked").text, "fixed");
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
        focused("minus-shell-broken");
        compare(child("error-shell-broken").text, "That value is not allowed.");
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
}
