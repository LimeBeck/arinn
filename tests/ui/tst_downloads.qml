// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import QtTest
import "../../apps/arinn-shell/ui" as Ui
import "../../apps/arinn-shell/ui/screens/downloads" as Downloads

TestCase {
    id: suite
    name: "Downloads"
    width: 1920
    height: 1080
    visible: true
    when: windowShown

    property var view
    Component {
        id: factory
        Downloads.DownloadsView {
            jobs: [
                {id: "active", title: "Feature", subtitle: "Part two", state: "downloading", progress: 0.82, statusText: "82%", availableActions: ["pause"], importState: "pending"},
                {id: "waiting", title: "Queued item", state: "queued", availableActions: ["start_now"], importState: "pending"},
                {id: "held", title: "Held item", state: "paused", progress: 0.4, availableActions: ["resume"], importState: "pending"},
                {id: "failed", title: "Failed item", state: "error", errorText: "Could not download", availableActions: ["retry"], importState: "pending"},
                {id: "ready", title: "Ready item", state: "completed", importState: "imported"},
                {id: "extra", title: "Extra file", state: "completed", importState: "unsupported"}
            ]
        }
    }
    SignalSpy { id: pauses; signalName: "pauseRequested" }
    SignalSpy { id: resumes; signalName: "resumeRequested" }
    SignalSpy { id: starts; signalName: "startNowRequested" }
    SignalSpy { id: retries; signalName: "retryRequested" }
    SignalSpy { id: watches; signalName: "watchRequested" }
    SignalSpy { id: refreshes; signalName: "refreshRequested" }
    SignalSpy { id: backs; signalName: "backRequested" }

    function child(name) {
        const item = findChild(view, name);
        verify(item !== null, name);
        return item;
    }
    function focused(name) { tryCompare(child(name), "activeFocus", true, 5000, name); }
    function insideView(item) {
        let cursor = item;
        while (cursor) {
            if (cursor === view) return true;
            cursor = cursor.parent;
        }
        return false;
    }
    function arm(target) {
        pauses.target = target; pauses.clear();
        resumes.target = target; resumes.clear();
        starts.target = target; starts.clear();
        retries.target = target; retries.clear();
        watches.target = target; watches.clear();
        refreshes.target = target; refreshes.clear();
        backs.target = target; backs.clear();
    }
    function createView(props) {
        mouseMove(suite, 10, 10);
        view = createTemporaryObject(factory, suite, props || {});
        verify(view);
        arm(view);
    }
    function init() { createView(); }
    function cleanup() { arm(null); }

    function test_queue_rows_progress_and_actions() {
        focused("job-active");
        compare(child("downloadsTitle").text, "Downloads");
        compare(child("downloadsSummary").text, "1 downloading");
        compare(child("title-active").text, "Feature");
        compare(child("subtitle-active").text, "Part two");
        compare(child("status-active").text, "82%");
        verify(child("progress-active").visible);
        compare(child("progress-active").fraction, 0.82);
        compare(child("artwork-active").text, "");
        tryCompare(child("action-active"), "visible", true);
        compare(child("action-active").text, "Pause");
        compare(child("status-waiting").text, "Queued");
        verify(!child("progress-waiting").visible);
        compare(child("action-waiting").text, "Start now");
        compare(child("status-held").text, "Paused · 40%");
        verify(child("progress-held").visible);
        compare(child("action-held").text, "Resume");
        compare(child("status-failed").text, "Could not download");
        compare(child("status-failed").color, Ui.Theme.error);
        compare(child("action-failed").text, "Retry");
        compare(child("status-ready").text, "Done · Added to the library");
        compare(child("status-ready").color, Ui.Theme.ember);
        compare(child("action-ready").text, "Watch");
        compare(child("status-extra").text, "Done");
        compare(child("action-extra").text, "");
        verify(!child("action-extra").visible);
        compare(view.toastMessage, "");
        verify(!child("completionToast").visible);
    }

    function test_intent_signals() {
        focused("job-active");
        keyClick(Qt.Key_Return);
        compare(pauses.count, 1);
        compare(pauses.signalArguments[0][0], "active");
        keyClick(Qt.Key_Down); focused("job-waiting");
        keyClick(Qt.Key_Return);
        compare(starts.count, 1);
        compare(starts.signalArguments[0][0], "waiting");
        keyClick(Qt.Key_Down); focused("job-held");
        keyClick(Qt.Key_Space);
        compare(resumes.count, 1);
        compare(resumes.signalArguments[0][0], "held");
        keyClick(Qt.Key_Down); focused("job-failed");
        keyClick(Qt.Key_Enter);
        compare(retries.count, 1);
        compare(retries.signalArguments[0][0], "failed");
        keyClick(Qt.Key_Down); focused("job-ready");
        tryCompare(child("action-ready"), "visible", true);
        mouseClick(child("action-ready"), child("action-ready").width / 2, child("action-ready").height / 2);
        compare(watches.count, 1);
        compare(watches.signalArguments[0][0], "ready");
        keyClick(Qt.Key_Down); focused("job-extra");
        keyClick(Qt.Key_Return);
        compare(watches.count, 1);
        compare(pauses.count, 1);
    }

    function test_missing_action_is_not_invented() {
        view.jobs = [{id: "active", title: "Feature", state: "downloading", progress: 0.5, availableActions: []}];
        focused("job-active");
        compare(child("action-active").text, "");
        keyClick(Qt.Key_Return);
        compare(pauses.count, 0);
        compare(starts.count, 0);
    }

    function test_unknown_progress_and_checking() {
        view.jobs = [
            {id: "open", title: "Open size", state: "downloading", availableActions: ["pause"]},
            {id: "check", title: "Verify", state: "verifying", progress: 0.9},
            {id: "over", title: "Over", state: "paused", progress: 2, availableActions: ["resume"]}
        ];
        focused("job-open");
        compare(child("status-open").text, "Downloading");
        verify(!child("progress-open").visible);
        compare(child("status-check").text, "Checking…");
        verify(!child("progress-check").visible);
        verify(child("progress-over").visible);
        compare(child("progress-over").fraction, 1);
        compare(child("status-over").text, "Paused · 100%");
    }

    function test_back_requested_data() {
        return [
            {tag: "escape", key: Qt.Key_Escape, state: "populated"},
            {tag: "backspace", key: Qt.Key_Backspace, state: "populated"},
            {tag: "back", key: Qt.Key_Back, state: "populated"},
            {tag: "escape-error", key: Qt.Key_Escape, state: "error"},
            {tag: "backspace-unavailable", key: Qt.Key_Backspace, state: "unavailable"},
            {tag: "back-loading", key: Qt.Key_Back, state: "loading"},
            {tag: "escape-empty", key: Qt.Key_Escape, state: "empty"}
        ];
    }
    function test_back_requested(data) {
        view.viewState = data.state;
        if (data.state === "populated") focused("job-active");
        else if (data.state === "error" || data.state === "unavailable") focused("statusAction");
        else verify(insideView(view.Window.window.activeFocusItem));
        keyClick(data.key);
        compare(backs.count, 1);
        compare(pauses.count, 0);
        compare(refreshes.count, 0);
        compare(view.viewState, data.state);
        verify(insideView(view.Window.window.activeFocusItem));
        if (data.state === "populated") focused("job-active");
    }

    function test_dpad_stays_inside() {
        focused("job-active");
        keyClick(Qt.Key_Up); focused("job-active");
        keyClick(Qt.Key_Left); focused("job-active");
        keyClick(Qt.Key_Right); focused("job-active");
        keyClick(Qt.Key_Down); focused("job-waiting");
        keyClick(Qt.Key_Up); focused("job-active");
        for (let i = 0; i < 8; ++i) keyClick(Qt.Key_Down);
        focused("job-extra");
        keyClick(Qt.Key_Down); focused("job-extra");
        keyClick(Qt.Key_Tab); focused("job-active");
        keyClick(Qt.Key_Backtab); focused("job-extra");
        verify(insideView(view.Window.window.activeFocusItem));
        compare(view.pointerMode, false);
    }

    function test_removal_and_reorder() {
        keyClick(Qt.Key_Down); keyClick(Qt.Key_Down); focused("job-held");
        view.jobs = [
            {id: "active", title: "Feature", state: "downloading", progress: 0.82, statusText: "82%", availableActions: ["pause"]},
            {id: "waiting", title: "Queued item", state: "queued", availableActions: ["start_now"]},
            {id: "failed", title: "Failed item", state: "error", availableActions: ["retry"]}
        ];
        focused("job-waiting");
        view.jobs = [
            {id: "failed", title: "Failed item", state: "error", availableActions: ["retry"]},
            {id: "waiting", title: "Queued item", state: "queued", availableActions: ["start_now"]}
        ];
        focused("job-waiting");
        keyClick(Qt.Key_Up); focused("job-failed");
    }

    function test_busy_and_disabled_do_not_activate() {
        view.jobs = [
            {id: "active", title: "Feature", state: "downloading", progress: 0.2, availableActions: ["pause"], busy: true},
            {id: "held", title: "Held item", state: "paused", progress: 0.4, availableActions: ["resume"], enabled: false}
        ];
        focused("job-active");
        keyClick(Qt.Key_Return);
        compare(pauses.count, 0);
        keyClick(Qt.Key_Down); focused("job-held");
        keyClick(Qt.Key_Return);
        compare(resumes.count, 0);
    }

    function test_completion_toast_only_after_import() {
        compare(view.toastMessage, "");
        view.jobs = [
            {id: "active", title: "Feature", subtitle: "Part two", state: "downloading", progress: 0.82, statusText: "82%", availableActions: ["pause"], importState: "pending"},
            {id: "waiting", title: "Queued item", state: "queued", availableActions: ["start_now"], importState: "pending"},
            {id: "held", title: "Held item", state: "paused", progress: 0.4, availableActions: ["resume"], importState: "pending"},
            {id: "failed", title: "Failed item", state: "error", errorText: "Could not download", availableActions: ["retry"], importState: "pending"},
            {id: "ready", title: "Ready item", state: "completed", importState: "pending"},
            {id: "extra", title: "Extra file", state: "completed", importState: "unsupported"}
        ];
        focused("job-active");
        compare(view.toastMessage, "");
        compare(child("action-ready").text, "");
        view.jobs = [
            {id: "active", title: "Feature", state: "downloading", progress: 1, availableActions: ["pause"], importState: "pending"},
            {id: "ready", title: "Ready item", state: "completed", importState: "imported"},
            {id: "extra", title: "Extra file", state: "completed", importState: "unsupported"}
        ];
        tryCompare(view, "toastMessage", "Ready item is ready to watch");
        verify(child("completionToast").visible);
        compare(child("completionToastText").text, "Ready item is ready to watch");
        focused("job-active");
        keyClick(Qt.Key_Down);
        focused("job-ready");
        compare(child("action-ready").text, "Watch");
        view.jobs = [{id: "extra", title: "Extra file", state: "completed", importState: "unsupported"}];
        focused("job-extra");
        compare(view.toastMessage, "Ready item is ready to watch");
        wait(2700);
        compare(view.toastMessage, "");
        verify(!child("completionToast").visible);
    }

    function test_non_media_completion_has_no_toast() {
        view.jobs = [{id: "extra", title: "Extra file", state: "downloading", progress: 0.2, availableActions: ["pause"], importState: "pending"}];
        focused("job-extra");
        view.jobs = [{id: "extra", title: "Extra file", state: "completed", importState: "unsupported"}];
        focused("job-extra");
        compare(view.toastMessage, "");
        compare(child("status-extra").text, "Done");
        compare(child("action-extra").text, "");
    }

    function test_states_data() {
        return [
            {tag: "empty", state: "empty", action: false},
            {tag: "loading", state: "loading", action: false},
            {tag: "error", state: "error", action: true},
            {tag: "unavailable", state: "unavailable", action: true}
        ];
    }
    function test_states(data) {
        view.viewState = data.state;
        if (data.state === "loading") {
            tryCompare(child("downloadsSkeletons"), "visible", true);
            verify(child("skeleton-0").visible);
            verify(child("skeleton-3").visible);
            verify(!child("downloadsStatus").visible);
            verify(!child("downloadsQueue").visible);
        } else {
            tryCompare(child("downloadsStatus"), "visible", true);
            verify(!child("downloadsSkeletons").visible);
            verify(!child("downloadsQueue").visible);
            const pane = child("downloadsStatus");
            verify(pane.title.length > 0);
            verify(!/transmission|peer|seed|ratio|tracker/i.test(pane.title + " " + pane.description + " " + pane.actionText));
            compare(child("statusAction").visible, data.action);
        }
        if (data.action) {
            focused("statusAction");
            keyClick(Qt.Key_Up); focused("statusAction");
            keyClick(Qt.Key_Down); focused("statusAction");
            keyClick(Qt.Key_Left); focused("statusAction");
            keyClick(Qt.Key_Right); focused("statusAction");
            keyClick(Qt.Key_Return);
            compare(refreshes.count, 1);
        } else {
            verify(insideView(view.Window.window.activeFocusItem));
            keyClick(Qt.Key_Down);
            keyClick(Qt.Key_Tab);
            verify(insideView(view.Window.window.activeFocusItem));
        }
        view.viewState = "populated";
        focused("job-active");
    }

    function test_explicit_summary_and_status_copy() {
        view.summaryText = "2 downloading";
        view.viewState = "unavailable";
        view.statusTitle = "Offline";
        view.statusDescription = "Try again shortly.";
        tryCompare(child("downloadsStatus"), "visible", true);
        compare(child("downloadsStatus").title, "Offline");
        compare(child("downloadsStatus").description, "Try again shortly.");
        compare(child("downloadsSummary").text, "2 downloading");
    }

    function test_pointer_moves_focus_and_scroll_suppresses_hover() {
        focused("job-active");
        tryCompare(view, "hoverFocusEnabled", true);
        mouseMove(child("job-waiting"), 40, 40);
        tryCompare(view, "pointerMode", true);
        focused("job-waiting");
        const tall = [];
        for (let i = 0; i < 8; ++i)
            tall.push({id: "row" + i, title: "Row " + i, state: "queued", availableActions: ["start_now"]});
        view.jobs = tall;
        focused("job-row0");
        for (let i = 0; i < 7; ++i) keyClick(Qt.Key_Down);
        focused("job-row7");
        verify(child("downloadsQueue").contentY > 0);
        compare(view.hoverFocusEnabled, false);
        mouseMove(child("job-row0"), 30, 30);
        focused("job-row7");
        tryCompare(view, "hoverFocusEnabled", true);
        verify(insideView(view.Window.window.activeFocusItem));
    }
}
