// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import QtTest
import "../../apps/arinn-shell/ui" as Ui

// ADR-0076: Focus.take() is the only way to move focus, and it never lets a
// hidden or disabled item take focus from the item that holds it.
TestCase {
    id: suite
    name: "FocusOwnership"
    width: 400; height: 300
    visible: true
    when: windowShown
    Component {
        id: factory
        FocusScope {
            anchors.fill: parent
            property alias shown: shown
            property alias hiddenScreen: hiddenScreen
            property alias hiddenChild: hiddenChild
            property alias disabledScreen: disabledScreen
            property alias disabledChild: disabledChild
            Item { id: shown; width: 10; height: 10; focus: true }
            FocusScope {
                id: hiddenScreen
                visible: false
                Item { id: hiddenChild; width: 10; height: 10 }
            }
            FocusScope {
                id: disabledScreen
                enabled: false
                Item { id: disabledChild; width: 10; height: 10 }
            }
        }
    }
    property var scope
    function init() {
        scope = createTemporaryObject(factory, suite);
        verify(scope);
        verify(Ui.Focus.take(scope.shown));
        tryCompare(scope.shown, "activeFocus", true);
    }

    function test_refuses_items_of_a_hidden_screen() {
        compare(Ui.Focus.take(scope.hiddenChild), false);
        compare(Ui.Focus.take(scope.hiddenScreen), false);
        verify(scope.shown.activeFocus);
    }
    function test_refuses_items_of_a_disabled_screen() {
        compare(Ui.Focus.take(scope.disabledChild), false);
        verify(scope.shown.activeFocus);
    }
    function test_refuses_null() {
        compare(Ui.Focus.take(null), false);
        compare(Ui.Focus.take(undefined), false);
        verify(scope.shown.activeFocus);
    }
    function test_takes_a_shown_item_with_a_reason() {
        scope.hiddenScreen.visible = true;
        verify(Ui.Focus.take(scope.hiddenChild, Qt.MouseFocusReason));
        verify(scope.hiddenChild.activeFocus);
    }
}
