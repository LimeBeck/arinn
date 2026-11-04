// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import QtTest
import QtQuick.Window
import "../../apps/arinn-shell/ui" as Ui
import "../../apps/arinn-shell/ui/screens/running" as Running

// Running applications and the close flow (#759, handoff *Closing
// applications* CL-4…11, CL-13…16).
TestCase {
    id: suite
    name: "Running"
    width: 1920
    height: 1080
    visible: true
    when: windowShown

    property var view
    property var flow
    Component {
        id: viewFactory
        Running.RunningView {
            windows: [
                {id: "w1", title: "Northwind", applicationId: "north", providerId: "p", terminable: true},
                {id: "w2", title: "Harbor", applicationId: "harbor", providerId: "p", terminable: true},
                {id: "w3", title: "Harbor", applicationId: "harbor", providerId: "p", terminable: true},
                {id: "w4", title: "Locked", applicationId: "locked", providerId: "p", terminable: false}
            ]
            currentId: "w2"
        }
    }
    Component {
        id: flowFactory
        Running.CloseFlow {
            closeWaitMs: 300
            windows: [{id: "w1", title: "Northwind", terminable: true},
                      {id: "w4", title: "Locked", terminable: false}]
        }
    }
    Component { id: spyFactory; SignalSpy {} }
    Component { id: openerFactory; Rectangle { width: 10; height: 10; focus: true; activeFocusOnTab: true } }

    function child(item, name) {
        const found = findChild(item, name);
        verify(found !== null, name);
        return found;
    }
    function focused(item, name) { tryCompare(child(item, name), "activeFocus", true, 5000, name); }
    function init() {
        mouseMove(suite, 10, 10);
        Ui.InputMode.reset();
    }
    function makeView(props) {
        view = createTemporaryObject(viewFactory, suite, props || {});
        verify(view);
        return view;
    }

    function test_rows_order_status_and_focus() {
        makeView();
        // The window on screen first (CL-4).
        compare(view.orderedWindows[0].id, "w2");
        focused(view, "runningRow-w2");
        compare(child(view, "runningTitle").text, "Running applications");
        compare(child(view, "runningCount").text, "4");
        verify(child(view, "runningOnScreen-w2").visible);
        verify(!child(view, "runningOnScreen-w1").visible);
        // CL-14: two windows of one application say so.
        compare(child(view, "runningStatus-w2").text, "One of 2 windows · On screen");
        compare(child(view, "runningStatus-w1").text, "In the background");
        // CL-15: Arinn cannot close it.
        compare(child(view, "runningStatus-w4").text, "Arinn cannot close this");
        compare(child(view, "runningClose-w4").opacity, 0.4);
        verify(child(view, "runningBlockIcon-w4").visible);
        compare(child(view, "runningRow-w2").height, 128);
        compare(child(view, "runningClose-w2").width, 128);
        compare(child(view, "runningHint").text, "OK Go to · → Close · Back Exit");
    }
    function test_keys_and_intents() {
        makeView();
        const activates = createTemporaryObject(spyFactory, suite, {target: view, signalName: "activateRequested"});
        const closes = createTemporaryObject(spyFactory, suite, {target: view, signalName: "closeRequested"});
        const navigation = createTemporaryObject(spyFactory, suite, {target: view, signalName: "navigationRequested"});
        const backs = createTemporaryObject(spyFactory, suite, {target: view, signalName: "backRequested"});
        focused(view, "runningRow-w2");
        keyClick(Qt.Key_Return);
        compare(activates.count, 1); compare(activates.signalArguments[0][0], "w2");
        keyClick(Qt.Key_Right); focused(view, "runningClose-w2");
        keyClick(Qt.Key_Down); focused(view, "runningClose-w1");
        keyClick(Qt.Key_Return);
        compare(closes.count, 1); compare(closes.signalArguments[0][0], "w1");
        keyClick(Qt.Key_Left); focused(view, "runningRow-w1");
        keyClick(Qt.Key_Left);
        compare(navigation.count, 1);
        // CL-15: no dialog, a note instead.
        keyClick(Qt.Key_Down); keyClick(Qt.Key_Down); focused(view, "runningRow-w4");
        keyClick(Qt.Key_Right); focused(view, "runningClose-w4");
        keyClick(Qt.Key_Return);
        compare(closes.count, 1);
        verify(child(view, "runningToast").visible);
        compare(child(view, "runningToastText").text,
                "Arinn cannot close “Locked”. Close it in the application itself.");
        keyClick(Qt.Key_Escape);
        compare(backs.count, 1);
    }
    function test_closing_and_did_not_close_states() {
        makeView({closingIds: ["w1"], failedIds: ["w3"]});
        compare(child(view, "runningStatus-w1").text, "Closing…");
        compare(child(view, "runningRow-w1").parent.opacity, 0.5);
        verify(child(view, "runningClosing-w1").visible);
        verify(!child(view, "runningCloseIcon-w1").visible);
        compare(child(view, "runningStatus-w3").text, "One of 2 windows · Did not close");
    }
    // CL-9: closing the focused window moves focus to the next row.
    function test_closing_moves_focus_on() {
        makeView();
        focused(view, "runningRow-w2");
        keyClick(Qt.Key_Right); focused(view, "runningClose-w2");
        view.closingIds = ["w2"];
        focused(view, "runningRow-w1");
    }
    // Review of #777: confirming a close opened from the Running screen
    // leaves focus on the next row; the dialog does not hand it back to the
    // closing row's Close button.
    function test_confirm_from_running_keeps_the_focus_move() {
        makeView();
        flow = createTemporaryObject(flowFactory, suite, {windows: view.windows});
        view.closingIds = Qt.binding(() => flow.closingIds);
        focused(view, "runningRow-w2");
        keyClick(Qt.Key_Right); focused(view, "runningClose-w2");
        verify(flow.ask("w2"));
        focused(flow, "closeFlowCancel");
        keyClick(Qt.Key_Right); focused(flow, "closeFlowConfirm");
        keyClick(Qt.Key_Return);
        compare(flow.closingIds, ["w2"]);
        focused(view, "runningRow-w1");
        wait(50);
        verify(child(view, "runningRow-w1").activeFocus);
        verify(!child(view, "runningClose-w2").activeFocus);
    }
    function test_nothing_running() {
        makeView({windows: []});
        verify(child(view, "runningEmpty").visible);
        focused(view, "runningHome");
        const homes = createTemporaryObject(spyFactory, suite, {target: view, signalName: "homeRequested"});
        keyClick(Qt.Key_Return);
        compare(homes.count, 1);
        verify(!child(view, "runningHint").visible);
    }
    function test_many_rows_scroll_and_position() {
        const list = [];
        for (let i = 0; i < 9; ++i)
            list.push({id: "m" + i, title: "Window " + i, applicationId: "a" + i, providerId: "p", terminable: true});
        makeView({windows: list, currentId: ""});
        focused(view, "runningRow-m0");
        for (let i = 1; i < 9; ++i) {
            keyClick(Qt.Key_Down);
            focused(view, "runningRow-m" + i);
            const row = child(view, "runningRow-m" + i);
            const scroller = child(view, "runningList");
            tryVerify(() => {
                const bottom = row.mapToItem(scroller, 0, row.height).y;
                return bottom <= scroller.height && row.mapToItem(scroller, 0, 0).y >= 0;
            }, 2000, "row " + i);
        }
        verify(child(view, "runningPosition").visible);
        compare(child(view, "runningPosition").text, "9 of 9");
        verify(child(view, "runningTopFade").visible);
    }

    // CL-8…11, CL-16.
    function test_close_flow_confirm_closing_and_closed() {
        const opener = createTemporaryObject(openerFactory, suite);
        opener.forceActiveFocus();
        flow = createTemporaryObject(flowFactory, suite);
        const terminations = createTemporaryObject(spyFactory, suite, {target: flow, signalName: "terminateRequested"});
        const closed = createTemporaryObject(spyFactory, suite, {target: flow, signalName: "closed"});
        verify(!flow.ask("w4"), "a window Arinn cannot close is not asked");
        verify(flow.ask("w1"));
        compare(flow.phase, "confirm");
        focused(flow, "closeFlowCancel");
        compare(child(flow, "closeFlowTitle").text, "Close Northwind?");
        compare(child(flow, "closeFlowScrim").color.a.toFixed(2), "0.58");
        compare(child(flow, "closeConfirmDialog").width, 900);
        // Back is Cancel, focus returns to the opener.
        keyClick(Qt.Key_Escape);
        compare(flow.phase, "");
        compare(terminations.count, 0);
        tryCompare(opener, "activeFocus", true);
        // Review of #777: a system scope above hides the dialog; it comes
        // back afterwards.
        flow.ask("w1");
        focused(flow, "closeFlowCancel");
        flow.suppressed = true;
        verify(!flow.visible);
        verify(!flow.enabled);
        flow.suppressed = false;
        verify(flow.visible);
        flow.restoreFocus();
        focused(flow, "closeFlowCancel");
        keyClick(Qt.Key_Escape);
        // Home closes the dialog without closing the application and asks
        // the shell for Home.
        const homes = createTemporaryObject(spyFactory, suite, {target: flow, signalName: "homeRequested"});
        flow.ask("w1");
        focused(flow, "closeFlowCancel");
        keyClick(Qt.Key_Home);
        compare(flow.phase, "");
        compare(terminations.count, 0);
        compare(homes.count, 1);
        flow.ask("w1");
        focused(flow, "closeFlowCancel");
        keyClick(Qt.Key_Right); focused(flow, "closeFlowConfirm");
        keyClick(Qt.Key_Return);
        compare(terminations.count, 1); compare(terminations.signalArguments[0][0], "w1");
        compare(flow.closingIds, ["w1"]);
        // The window goes: closed, with the toast.
        flow.windows = [{id: "w4", title: "Locked", terminable: false}];
        compare(flow.closingIds, []);
        compare(closed.count, 1);
        compare(flow.closedIds, ["w1"]);
        verify(child(flow, "closeFlowToast").visible);
        compare(child(flow, "closeFlowToastText").text, "Northwind closed");
    }
    // Review of #777: a list the shell does not trust (daemon gone, catalog
    // reloading) is no evidence that a window closed.
    function test_close_flow_waits_while_the_list_is_unknown() {
        flow = createTemporaryObject(flowFactory, suite);
        const closed = createTemporaryObject(spyFactory, suite, {target: flow, signalName: "closed"});
        flow.ask("w1");
        flow.confirm();
        flow.windowsKnown = false;
        flow.windows = [];
        compare(flow.closingIds, ["w1"]);
        compare(closed.count, 0);
        verify(!child(flow, "closeFlowToast").visible);
        // The wait (300 ms here) does not end it while the list is unknown.
        wait(500);
        compare(flow.closingIds, ["w1"]);
        compare(flow.phase, "");
        flow.windows = [{id: "w1", title: "Northwind", terminable: true}];
        flow.startedAt = ({"w1": Date.now()});
        flow.windowsKnown = true;
        compare(flow.closingIds, ["w1"]);
        flow.windows = [];
        compare(closed.count, 1);
    }
    // Review of #777: a window that cannot be activated still takes focus.
    function test_non_activatable_row_takes_focus() {
        makeView({windows: [
            {id: "a", title: "Alpha", applicationId: "a", providerId: "p", terminable: true},
            {id: "b", title: "Beta", applicationId: "b", providerId: "p", terminable: true, enabled: false}
        ], currentId: ""});
        const activates = createTemporaryObject(spyFactory, suite, {target: view, signalName: "activateRequested"});
        focused(view, "runningRow-a");
        keyClick(Qt.Key_Down);
        focused(view, "runningRow-b");
        keyClick(Qt.Key_Return);
        compare(activates.count, 0);
        keyClick(Qt.Key_Right);
        focused(view, "runningClose-b");
    }
    // Review of #777: a failure while another confirmation is open waits
    // for it instead of replacing it.
    function test_close_flow_queues_a_failure_behind_a_confirmation() {
        flow = createTemporaryObject(flowFactory, suite, {windows: [
            {id: "w1", title: "Northwind", terminable: true},
            {id: "w2", title: "Harbor", terminable: true}]});
        flow.ask("w1");
        flow.confirm();
        verify(flow.ask("w2"));
        focused(flow, "closeFlowCancel");
        flow.operationFinished("w1", "failed");
        compare(flow.phase, "confirm");
        compare(flow.target.id, "w2");
        // A queued failure whose window goes is dropped.
        flow.windows = [{id: "w2", title: "Harbor", terminable: true}];
        compare(flow.queuedFailures, []);
        flow.windows = [{id: "w1", title: "Northwind", terminable: true},
                        {id: "w2", title: "Harbor", terminable: true}];
        flow.cancel();
        flow.ask("w1");
        flow.confirm();
        verify(flow.ask("w2"));
        flow.operationFinished("w1", "failed");
        focused(flow, "closeFlowCancel");
        keyClick(Qt.Key_Escape);
        tryCompare(flow, "phase", "failed");
        compare(flow.target.id, "w1");
        compare(child(flow, "closeFlowTitle").text, "Northwind did not close");
    }
    function test_close_flow_did_not_close() {
        flow = createTemporaryObject(flowFactory, suite);
        const switches = createTemporaryObject(spyFactory, suite, {target: flow, signalName: "switchRequested"});
        flow.ask("w1");
        focused(flow, "closeFlowCancel");
        flow.confirm();
        compare(flow.closingIds, ["w1"]);
        // Still there after the wait (here 300 ms): did not close.
        tryCompare(flow, "phase", "failed", 3000);
        compare(flow.closingIds, []);
        compare(flow.failedIds, ["w1"]);
        // A window that did not close is not among the closed ones.
        compare(flow.closedIds, []);
        focused(flow, "closeFlowSwitch");
        compare(child(flow, "closeFlowTitle").text, "Northwind did not close");
        compare(child(flow, "closeFlowSwitch").text, "Go to Northwind");
        keyClick(Qt.Key_Return);
        compare(switches.count, 1); compare(switches.signalArguments[0][0], "w1");
        compare(flow.phase, "");
        // A failed request says so at once.
        flow.ask("w1");
        flow.confirm();
        flow.operationFinished("w1", "failed");
        compare(flow.phase, "failed");
        focused(flow, "closeFlowSwitch");
        keyClick(Qt.Key_Right);
        focused(flow, "closeFlowStay");
        keyClick(Qt.Key_Return);
        compare(flow.phase, "");
    }
}
