// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import QtTest
import "../../apps/arinn-shell/ui" as Ui

TestCase {
    id: suite
    name: "SelectionControls"
    width: 1920; height: 1080
    visible: true
    when: windowShown
    Component { id: factory; Ui.TabButton { x: 200; y: 200; text: "Films" } }
    SignalSpy { id: activated; signalName: "activated" }
    function cleanup() { activated.target = null; }
    function test_selected_role_and_focus_data() {
        return [{tag: "tab", role: "tab"}, {tag: "pill", role: "pill"}, {tag: "action", role: "action"}];
    }
    function test_selected_role_and_focus(data) {
        const button = createTemporaryObject(factory, suite, {presentation: data.role, selected: true});
        compare(button.text, "Films");
        compare(button.labelItem.text, "Films");
        compare(button.Accessible.role, Accessible.PageTab);
        compare(button.Accessible.selected, true);
        const marker = findChild(button, button.markerName);
        verify(marker.visible);
        Ui.Focus.take(button);
        tryCompare(button, "activeFocus", true);
        compare(marker.visible, false);
        compare(button.selected, true);
        const expected = data.role === "pill" ? Ui.Theme.textPrimary : Ui.Theme.ink;
        // Theme's OKLCH colour may cross QColor's float/RGB conversion.
        // Compare display channels rather than QColor storage precision.
        for (const channel of ["r", "g", "b", "a"])
            fuzzyCompare(button.labelItem.color[channel], expected[channel], 1 / 255);
        button.selected = false;
        compare(button.Accessible.selected, false);
    }
    function test_activation_and_busy_navigation() {
        const first = createTemporaryObject(factory, suite);
        const next = createTemporaryObject(factory, suite, {x: 600, text: "Series"});
        first.KeyNavigation.right = next;
        activated.target = first;
        activated.clear();
        Ui.Focus.take(first);
        keyPress(Qt.Key_Return);
        compare(activated.count, 0);
        keyRelease(Qt.Key_Return);
        compare(activated.count, 1);
        first.busy = true;
        keyClick(Qt.Key_Enter);
        compare(activated.count, 1);
        keyClick(Qt.Key_Right);
        tryCompare(next, "activeFocus", true);
        first.enabled = false;
        mouseClick(first, first.width / 2, first.height / 2);
        compare(activated.count, 1);
    }
    function test_live_localized_label_width_and_accessibility() {
        const button = createTemporaryObject(factory, suite, {presentation: "action"});
        Ui.Focus.take(button);
        for (const text of ["A long connected source for the library", "Очень длинное название подключённого источника"]) {
            button.text = text;
            compare(button.labelItem.text, text);
            compare(button.Accessible.name, text);
            verify(button.implicitWidth >= button.labelItem.implicitWidth + 80);
            verify(button.activeFocus);
        }
        button.width = 300;
        verify(button.labelItem.width <= button.width);
        compare(button.text.charAt(0) === "●", false);
    }
    function test_pointer_then_local_dpad() {
        const first = createTemporaryObject(factory, suite, {pointerMode: true});
        const next = createTemporaryObject(factory, suite, {x: 600, text: "Series"});
        first.KeyNavigation.right = next;
        verify(waitForRendering(first));
        mouseMove(first, 20, 20); mouseMove(first, 21, 20);
        tryCompare(first, "activeFocus", true);
        keyClick(Qt.Key_Right);
        tryCompare(next, "activeFocus", true);
    }
}
