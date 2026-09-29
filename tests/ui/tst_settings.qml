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
                { id: "media", title: "Media", capability: "library", icon: "video_library" },
                { id: "playback", title: "Playback", capability: "playback", icon: "play_circle" },
                { id: "sound", title: "Sound", icon: "volume_up" },
                { id: "network", title: "Network", capability: "network", icon: "wifi" },
                { id: "applications", title: "Applications", icon: "apps" },
                { id: "appearance", title: "Appearance", icon: "palette" },
                { id: "system", title: "System", icon: "memory" }
            ]
            rows: [
                { id: "scan", categoryId: "media", title: "Scan folders", description: "Look for new files",
                  type: "boolean", currentValue: false, constraints: {}, enabled: true },
                { id: "download-location", categoryId: "media", title: "Download location",
                  description: "Where completed files are stored", type: "path", currentValue: "",
                  constraints: { kind: "directory" }, enabled: true, capability: "download" },
                { id: "autonext", categoryId: "playback", title: "Play next", description: "Continue to the next item",
                  type: "boolean", currentValue: true, constraints: {}, enabled: true },
                { id: "output", categoryId: "sound", title: "Output", description: "Audio output",
                  type: "enum", currentValue: "stereo", constraints: { values: ["stereo", "surround"] }, enabled: true },
                { id: "label", categoryId: "applications", title: "Label", description: "Shown under the icon",
                  type: "string", currentValue: "note", constraints: { maximumLength: 40 }, enabled: true },
                { id: "density", categoryId: "appearance", title: "Density", description: "Interface density",
                  type: "enum", currentValue: "regular", constraints: { values: ["regular", "compact"] }, enabled: true },
                { id: "idle", categoryId: "system", title: "Idle minutes", description: "Wait before the ambient screen",
                  type: "integer", currentValue: 10, constraints: { minimum: 1, maximum: 60, step: 1 }, enabled: true }
            ]
            capabilities: [
                { id: "library", available: true },
                { id: "playback", available: false, title: "Playback unavailable",
                  description: "No playback capability is registered." },
                { id: "network", available: false, title: "Network unavailable",
                  description: "No network capability is registered." }
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
        // Qt Quick Test retains the pointer between fixtures. Park it away
        // from rows so a queued hover cannot steal the initial focus.
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

    function test_category_focus() {
        focused("category-media");
        keyClick(Qt.Key_Down);
        focused("category-playback");
        keyClick(Qt.Key_Right);
        focused("category-playback");
        keyClick(Qt.Key_Down);
        focused("category-sound");
        keyClick(Qt.Key_Right);
        focused("setting-output");
        verify(findChild(child("category-sound"), "focusRing") === null);
        compare(child("category-dot-sound").opacity, 1);
        compare(child("category-dot-sound").color, Ui.Theme.ember);
        verify(insideView(windowFocus()));
        keyClick(Qt.Key_Left);
        focused("category-sound");
        keyClick(Qt.Key_Left);
        focused("category-sound");
        keyClick(Qt.Key_Down);
        focused("category-network");
        keyClick(Qt.Key_Down);
        focused("category-applications");
        keyClick(Qt.Key_Down);
        focused("category-appearance");
        keyClick(Qt.Key_Down);
        focused("category-system");
        keyClick(Qt.Key_Down);
        focused("category-system");
        keyClick(Qt.Key_Right);
        focused("setting-idle");
        keyClick(Qt.Key_Down);
        focused("setting-idle");
        keyClick(Qt.Key_Up);
        focused("setting-idle");
        keyClick(Qt.Key_Left);
        focused("category-system");
        for (let step = 0; step < 8; ++step) keyClick(Qt.Key_Up);
        focused("category-media");
        keyClick(Qt.Key_Up);
        focused("category-media");
        verify(insideView(windowFocus()));
    }

    function test_boolean_change_intent() {
        focused("category-media");
        keyClick(Qt.Key_Right);
        focused("setting-scan");
        verify(findChild(view, "value-scan") === null);
        compare(child("knob-scan").x, 5);
        compare(child("track-scan").color, Qt.rgba(1, 1, 1, 0.20));
        keyClick(Qt.Key_Return);
        compare(changes.count, 1);
        compare(changes.signalArguments[0][0], "scan");
        compare(changes.signalArguments[0][1], true);
        compare(child("knob-scan").x, 5);
        verify(findChild(view, "value-scan") === null);
        compare(view.rows[0].currentValue, false);
    }

    function test_enum_change_intent() {
        keyClick(Qt.Key_Down);
        keyClick(Qt.Key_Down);
        focused("category-sound");
        keyClick(Qt.Key_Right);
        focused("setting-output");
        compare(child("value-output").text, "stereo");
        verify(child("chevron-output").visible);
        keyClick(Qt.Key_Return);
        compare(changes.count, 1);
        compare(changes.signalArguments[0][0], "output");
        compare(changes.signalArguments[0][1], "surround");
        compare(child("value-output").text, "stereo");
    }

    function test_missing_capability_unavailable_data() {
        return [
            { tag: "playback", category: "playback", steps: 1, title: "Playback unavailable",
              description: "No playback capability is registered." },
            { tag: "network", category: "network", steps: 3, title: "Network unavailable",
              description: "No network capability is registered." }
        ];
    }
    function test_missing_capability_unavailable(data) {
        for (let step = 0; step < data.steps; ++step) keyClick(Qt.Key_Down);
        focused("category-" + data.category);
        const pane = child("unavailablePane");
        compare(pane.kind, "unavailable");
        compare(pane.title, data.title);
        compare(pane.description, data.description);
        compare(pane.actionText, "");
        const action = findChild(pane, "statusAction");
        verify(action !== null);
        compare(action.visible, false);
        verify(findChild(view, "setting-autonext") === null);
        verify(findChild(view, "value-autonext") === null);
        verify(findChild(view, "toggle-autonext") === null);
        keyClick(Qt.Key_Return);
        keyClick(Qt.Key_Right);
        compare(changes.count, 0);
        focused("category-" + data.category);
    }

    function test_missing_download_capability_unavailable() {
        focused("category-media");
        const state = child("state-download-location");
        compare(state.text, "Unavailable");
        compare(state.color, Ui.Theme.error);
        verify(findChild(view, "setting-download-location") === null);
        verify(findChild(view, "value-download-location") === null);
        verify(findChild(view, "toggle-download-location") === null);
        compare(child("knob-scan").x, 5);
        verify(findChild(view, "value-scan") === null);
        keyClick(Qt.Key_Right);
        focused("setting-scan");
        keyClick(Qt.Key_Down);
        focused("unavailable-download-location");
        keyClick(Qt.Key_Return);
        compare(changes.count, 0);
        compare(child("knob-scan").x, 5);
        verify(findChild(view, "value-scan") === null);
        verify(insideView(windowFocus()));
    }

    function test_back_key() {
        focused("category-media");
        keyClick(Qt.Key_Back);
        compare(dismissals.count, 1);
        focused("category-media");
        verify(insideView(windowFocus()));
        dismissals.clear();
        keyClick(Qt.Key_Right);
        focused("setting-scan");
        keyClick(Qt.Key_Back);
        focused("category-media");
        compare(dismissals.count, 0);
        verify(insideView(windowFocus()));
    }

    function test_text_backspace_stays_in_row() {
        for (let step = 0; step < 4; ++step) keyClick(Qt.Key_Down);
        focused("category-applications");
        keyClick(Qt.Key_Right);
        focused("setting-label");
        keyClick(Qt.Key_Return);
        const editor = child("editor-label");
        tryCompare(editor, "visible", true);
        tryCompare(editor, "activeFocus", true);
        compare(editor.text, "note");
        keyClick(Qt.Key_Backspace);
        compare(editor.text, "not");
        compare(view.editingId, "label");
        tryCompare(editor, "activeFocus", true);
        compare(changes.count, 0);
        compare(dismissals.count, 0);
        compare(child("category-applications").activeFocus, false);
        verify(insideView(windowFocus()));
    }
}
