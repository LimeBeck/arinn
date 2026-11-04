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
        // Home closes the dialog without closing the application.
        flow.ask("w1");
        focused(flow, "closeFlowCancel");
        keyClick(Qt.Key_Home);
        compare(flow.phase, "");
        compare(terminations.count, 0);
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
        verify(child(flow, "closeFlowToast").visible);
        compare(child(flow, "closeFlowToastText").text, "Northwind closed");
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
