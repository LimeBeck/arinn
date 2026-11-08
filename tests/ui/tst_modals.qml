// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtTest
import "../../apps/arinn-shell/ui" as Ui

TestCase {
    id: suite
    name: "SharedModals"
    width: 1920; height: 1080
    visible: true
    when: windowShown

    Component {
        id: dialogFactory
        Item {
            id: fixture
            anchors.fill: parent
            property alias dialog: dialog
            property alias cancel: cancel
            property alias accept: accept
            property alias fallback: fallback
            property Item opener: fallback
            property int cancelled: 0
            property int confirmed: 0
            property int backgroundActivations: 0
            property int homes: 0
            property int upKeys: 0
            property string question: "Continue with this collection?"
            property string explanation: "The current selection will be replaced."
            property string cancelLabel: "Cancel"
            property string acceptLabel: "Continue"
            Ui.ActionButton {
                id: fallback
                x: 60; y: 60; text: "Open"
                onActivated: ++fixture.backgroundActivations
            }
            Keys.onPressed: event => {
                if (event.key === Qt.Key_Home) {
                    ++homes;
                    event.accepted = true;
                } else if (event.key === Qt.Key_Up) {
                    ++upKeys;
                    event.accepted = true;
                }
            }
            Ui.ConfirmationDialog {
                id: dialog
                visible: true
                title: fixture.question
                body: fixture.explanation
                initialFocusItem: cancel
                onDismissed: {
                    ++fixture.cancelled;
                    visible = false;
                    if (!Ui.Focus.take(fixture.opener))
                        Ui.Focus.take(fallback);
                }
                leading: Rectangle { objectName: "modalLeading"; width: 64; height: 64 }
                content: Ui.UiText { objectName: "modalContent"; text: "Additional local content" }
                Ui.ActionButton {
                    id: cancel
                    text: fixture.cancelLabel
                    pointerMode: Ui.InputMode.pointerActive
                    onActivated: dialog.dismissed()
                }
                Ui.ActionButton {
                    id: accept
                    text: fixture.acceptLabel
                    pointerMode: Ui.InputMode.pointerActive
                    onActivated: ++fixture.confirmed
                }
            }
        }
    }
    Component { id: openerFactory; Ui.ActionButton { text: "Temporary opener" } }
    Component {
        id: sheetFactory
        Item {
            id: fixture
            anchors.fill: parent
            property alias sheet: sheet
            property alias action: action
            property int dismissed: 0
            property int backgroundActivations: 0
            Ui.ActionButton { x: 60; y: 60; text: "Under sheet"; onActivated: ++fixture.backgroundActivations }
            Ui.SideSheet {
                id: sheet
                onDismissed: ++fixture.dismissed
                Ui.ActionButton { id: action; x: 64; y: 72; text: "Sheet action" }
            }
        }
    }
    function init() {
        mouseMove(suite, 5, 5);
        Ui.InputMode.reset();
    }
    function makeDialog() {
        const fixture = createTemporaryObject(dialogFactory, suite);
        verify(fixture);
        verify(fixture.dialog.focusInitial());
        tryCompare(fixture.cancel, "activeFocus", true);
        return fixture;
    }
    function inside(item, plate) {
        const p = item.mapToItem(plate, 0, 0);
        return p.x >= 0 && p.y >= 0 && p.x + item.width <= plate.width
                && p.y + item.height <= plate.height;
    }
    function test_initial_cancel_navigation_and_single_activation() {
        const f = makeDialog();
        keyClick(Qt.Key_Right); tryCompare(f.accept, "activeFocus", true);
        keyPress(Qt.Key_Return); compare(f.confirmed, 0);
        keyRelease(Qt.Key_Return); compare(f.confirmed, 1);
        f.accept.busy = true;
        keyClick(Qt.Key_Return);
        mouseClick(f.accept, f.accept.width / 2, f.accept.height / 2);
        compare(f.confirmed, 1);
        f.accept.busy = false;
        keyClick(Qt.Key_Tab); tryCompare(f.cancel, "activeFocus", true);
        keyClick(Qt.Key_Backtab); tryCompare(f.accept, "activeFocus", true);
        keyClick(Qt.Key_Tab); tryCompare(f.cancel, "activeFocus", true);
        f.accept.enabled = false;
        keyClick(Qt.Key_Backtab); tryCompare(f.cancel, "activeFocus", true);
        keyClick(Qt.Key_Right); tryCompare(f.cancel, "activeFocus", true);
        f.accept.enabled = true;
        f.accept.visible = false;
        keyClick(Qt.Key_Tab); tryCompare(f.cancel, "activeFocus", true);
        keyClick(Qt.Key_Up); verify(f.dialog.activeFocus);
        keyClick(Qt.Key_Home); compare(f.homes, 1);
    }
    function test_outside_and_back_follow_cancellation_policy() {
        const f = makeDialog();
        f.dialog.dismissible = false;
        keyClick(Qt.Key_Escape);
        mouseClick(f, 70, 70);
        compare(f.cancelled, 0); compare(f.backgroundActivations, 0);
        verify(f.dialog.visible);
        f.dialog.dismissible = true;
        mouseClick(f.dialog.plate, 10, 10); compare(f.cancelled, 0);
        keyClick(Qt.Key_Escape);
        compare(f.cancelled, 1);
        tryCompare(f.fallback, "activeFocus", true);
        f.dialog.visible = true;
        f.dialog.focusInitial();
        mouseClick(f, 70, 70);
        compare(f.cancelled, 2); compare(f.backgroundActivations, 0);
    }
    function test_missing_opener_and_hidden_dialog_do_not_lose_focus() {
        const f = makeDialog();
        const opener = createTemporaryObject(openerFactory, suite);
        f.opener = opener;
        opener.destroy(); wait(0);
        keyClick(Qt.Key_Escape);
        tryCompare(f.fallback, "activeFocus", true);
        verify(!f.dialog.focusInitial());
        verify(f.fallback.activeFocus);
    }
    function test_hidden_dialog_leaves_pending_focus_restoration_to_its_owner() {
        const f = makeDialog();
        // A screen may defer its restoration until the next event-loop turn.
        f.dialog.visible = false;
        keyClick(Qt.Key_Up);
        compare(f.upKeys, 1);
    }
    function test_pointer_and_dpad_stay_inside_the_dialog() {
        const f = makeDialog();
        Ui.InputMode.pointerMoved();
        mouseMove(f.accept, f.accept.width / 2, f.accept.height / 2);
        tryCompare(f.accept, "activeFocus", true);
        keyClick(Qt.Key_Left);
        tryCompare(f.cancel, "activeFocus", true);
        compare(Ui.InputMode.pointerActive, false);
        Ui.InputMode.reset();
        Ui.InputMode.pointerMoved();
        mouseMove(f.accept, f.accept.width / 2, f.accept.height / 2);
        tryCompare(f.accept, "activeFocus", true);
        keyClick(Qt.Key_Tab);
        tryCompare(f.cancel, "activeFocus", true);
        compare(Ui.InputMode.pointerActive, false);
    }
    function test_deferred_navigation_does_not_reclaim_the_owners_focus() {
        const f = makeDialog();
        f.accept.visible = false;
        const action = createTemporaryObject(openerFactory, f.cancel.parent);
        verify(action);
        keyPress(Qt.Key_Right);
        action.destroy();
        f.dialog.visible = false;
        Ui.Focus.take(f.fallback);
        keyRelease(Qt.Key_Right);
        wait(0);
        tryCompare(f.fallback, "activeFocus", true);
    }
    function test_removed_action_keeps_modal_key_ownership() {
        const f = makeDialog();
        f.accept.visible = false;
        const action = createTemporaryObject(openerFactory, f.cancel.parent);
        verify(action);
        Ui.Focus.take(action);
        tryCompare(action, "activeFocus", true);
        action.destroy(); wait(0);
        verify(f.dialog.activeFocus);
        keyClick(Qt.Key_Left);
        tryCompare(f.cancel, "activeFocus", true);
    }
    function test_long_labels_and_live_text_updates_data() {
        return [
            {tag: "english", title: "Replace the current selection with the selected collection and continue browsing?",
             body: "The current collection is kept on disk. Only the selection shown on this screen changes.",
             cancel: "Keep the current selection", accept: "Replace and continue"},
            {tag: "russian", title: "Заменить текущую подборку выбранной коллекцией и продолжить просмотр?",
             body: "Текущая коллекция останется на диске. Изменится только подборка, показанная на этом экране.",
             cancel: "Вернуться к подборке", accept: "Подтвердить и продолжить"}
        ];
    }
    function test_long_labels_and_live_text_updates(data) {
        const f = makeDialog();
        f.question = data.title; f.explanation = data.body;
        f.cancelLabel = data.cancel; f.acceptLabel = data.accept;
        wait(0);
        compare(f.dialog.titleItem.text, data.title);
        compare(f.dialog.bodyItem.text, data.body);
        verify(f.cancel.activeFocus);
        verify(f.dialog.plate.height < f.height);
        verify(inside(f.cancel, f.dialog.plate));
        verify(inside(f.accept, f.dialog.plate));
        verify(inside(findChild(f, "modalLeading"), f.dialog.plate));
        verify(inside(findChild(f, "modalContent"), f.dialog.plate));
    }
    function test_side_sheet_shares_geometry_and_blocks_background() {
        const f = createTemporaryObject(sheetFactory, suite);
        verify(f);
        compare(f.sheet.plate.width, 720);
        compare(f.sheet.plate.height, 1080);
        compare(f.action.mapToItem(f, 0, 0).x, 1264);
        mouseClick(f.sheet.plate, 500, 800); compare(f.dismissed, 0);
        f.sheet.dismissible = false;
        mouseClick(f, 70, 70); compare(f.dismissed, 0);
        compare(f.backgroundActivations, 0);
        f.sheet.dismissible = true;
        mouseClick(f, 70, 70); compare(f.dismissed, 1);
        compare(f.backgroundActivations, 0);
    }
}
