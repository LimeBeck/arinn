// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtTest
import "../../apps/arinn-shell/ui" as Ui

TestCase {
    id: suite
    name: "SharedButtons"
    width: 1920; height: 1080
    visible: true
    when: windowShown
    Component {
        id: actionFactory
        Ui.ActionButton {
            x: 150; y: 150; text: "Continue"
            pointerMode: Ui.InputMode.pointerActive
            onDpadRequested: Ui.InputMode.keyPressed()
        }
    }
    Component {
        id: iconFactory
        Ui.IconButton {
            x: 150; y: 150; text: "Play"; iconName: "play"
            pointerMode: Ui.InputMode.pointerActive
            onDpadRequested: Ui.InputMode.keyPressed()
        }
    }
    Component { id: spyFactory; SignalSpy { signalName: "activated" } }
    function init() {
        mouseMove(suite, 5, 5);
        Ui.InputMode.reset();
    }
    function test_activation_guards_data() {
        return [{tag: "action", factory: actionFactory}, {tag: "icon", factory: iconFactory}];
    }
    function test_activation_guards(data) {
        const button = createTemporaryObject(data.factory, suite);
        verify(button);
        const spy = createTemporaryObject(spyFactory, suite, {target: button});
        Ui.Focus.take(button);
        keyPress(Qt.Key_Return); compare(spy.count, 0);
        tryCompare(button, "down", true);
        keyRelease(Qt.Key_Return); compare(spy.count, 1);
        mouseClick(button, button.width / 2, button.height / 2); compare(spy.count, 2);
        button.busy = true;
        keyClick(Qt.Key_Return);
        mouseClick(button, button.width / 2, button.height / 2);
        compare(spy.count, 2);
        button.busy = false;
        button.enabled = false;
        keyClick(Qt.Key_Return);
        mouseClick(button, button.width / 2, button.height / 2);
        compare(spy.count, 2);
    }
    function test_optional_icon_and_stable_labels() {
        const button = createTemporaryObject(actionFactory, suite,
            {text: "Like", labels: ["Like", "Liked", "Saving…", "Not saved"]});
        verify(button);
        tryVerify(() => Ui.Theme.onest.status === FontLoader.Ready);
        verify(!button.iconItem.visible);
        const textWidth = button.width;
        button.iconName = "favorite";
        tryVerify(() => button.width > textWidth);
        const width = button.width;
        for (const text of ["Liked", "Saving…", "Not saved", "Like"]) {
            button.text = text;
            compare(button.width, width);
            compare(button.Accessible.name, text);
        }
        button.busyText = "Saving…";
        button.busy = true;
        compare(button.labelItem.text, "Saving…");
        compare(button.width, width);
        button.busy = false;
        button.selected = true;
        tryCompare(button.background, "color", Ui.Theme.platePrimary);
        button.selected = false;
        button.primary = true;
        tryCompare(button.background, "color", Ui.Theme.platePrimary);
        button.primary = false;
        button.destructive = true;
        tryCompare(button.background, "color", Qt.rgba(0.91, 0.28, 0.31, 0.22));
    }
    function test_bounded_localized_labels_update_without_focus_loss_data() {
        return [{tag: "english", text: "Continue playing the selected collection from the current position"},
                {tag: "russian", text: "Продолжить воспроизведение выбранной коллекции с текущего момента"}];
    }
    function test_bounded_localized_labels_update_without_focus_loss(data) {
        const button = createTemporaryObject(actionFactory, suite,
            {iconName: "queue_music", maximumWidth: 480});
        verify(button);
        Ui.Focus.take(button);
        button.text = data.text;
        tryCompare(button, "width", 480);
        tryCompare(button.labelItem, "truncated", true);
        verify(button.activeFocus);
        compare(button.labelItem.text, data.text);
        compare(button.Accessible.name, data.text);
        tryVerify(() => {
            const p = button.labelItem.mapToItem(button, 0, 0);
            return p.x >= 0 && p.x + button.labelItem.width <= button.width;
        });
    }
    function test_icon_busy_feedback_and_accessible_name() {
        const button = createTemporaryObject(iconFactory, suite);
        verify(button);
        compare(button.width, button.height);
        verify(!button.labelItem.visible);
        compare(button.Accessible.name, "Play");
        button.busy = true;
        compare(button.iconItem.name, "refresh");
        verify(button.Accessible.description.length > 0);
        tryVerify(() => button.iconItem.rotation > 0);
        button.busy = false;
        compare(button.iconItem.name, "play");
        tryCompare(button.iconItem, "rotation", 0);
        compare(button.Accessible.name, "Play");
    }
    function test_pointer_and_remote_navigation_share_state() {
        const first = createTemporaryObject(iconFactory, suite);
        const second = createTemporaryObject(actionFactory, suite, {x: 450});
        verify(first); verify(second);
        first.KeyNavigation.right = second;
        Ui.InputMode.pointerMoved();
        mouseMove(first, first.width / 2, first.height / 2);
        tryCompare(first, "activeFocus", true);
        keyClick(Qt.Key_Right);
        tryCompare(second, "activeFocus", true);
        compare(Ui.InputMode.pointerActive, false);
        second.busy = true;
        keyClick(Qt.Key_Left);
        tryCompare(first, "activeFocus", true);
    }
    function test_scope_highlight_does_not_take_keyboard_focus() {
        const owner = createTemporaryObject(actionFactory, suite, {x: 450});
        const button = createTemporaryObject(iconFactory, suite,
            {focusPolicy: Qt.NoFocus, hoverFocusEnabled: false, highlighted: true});
        verify(owner); verify(button);
        Ui.Focus.take(owner);
        compare(button.background.color, Ui.Theme.textPrimary);
        compare(button.iconItem.color, Ui.Theme.ink);
        mouseClick(button, button.width / 2, button.height / 2);
        verify(owner.activeFocus);
        verify(!button.activeFocus);
    }
}
