// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import QtQuick.Window
import "../.." as Ui

// Download queue. Callers pass job snapshots and listen for intents; the
// shell binds it to DownloadCatalog. This screen does not open a service,
// import files, or leave its focus scope.
FocusScope {
    id: root
    width: 1920
    height: 1080
    focus: true

    // populated, empty, loading, error, unavailable
    property string viewState: "populated"
    property string statusTitle: ""
    property string statusDescription: ""
    property string summaryText: ""
    property var jobs: []
    property bool pointerMode: false
    property string selectedId: ""
    property int selectedIndex: 0
    property string toastMessage: ""
    // The shell announces completions itself (once per download per
    // session) and sets this to false; fixtures keep the view's own toast.
    property bool autoToast: true
    readonly property bool hoverFocusEnabled: !hoverDelay.running
    readonly property bool queueVisible: viewState === "populated" && jobCount > 0
    readonly property bool loadingVisible: viewState === "loading"
    readonly property bool statusVisible: viewState === "empty" || viewState === "error" || viewState === "unavailable"
    readonly property int jobCount: jobs ? jobs.length : 0
    readonly property string summaryLine: {
        if (summaryText.length) return summaryText;
        let active = 0;
        for (let i = 0; i < jobCount; ++i) if (jobs[i] && jobs[i].state === "downloading") active += 1;
        return qsTr("%n downloading", "", active);
    }
    readonly property string resolvedTitle: {
        if (statusTitle.length) return statusTitle;
        if (viewState === "empty") return qsTr("No downloads");
        if (viewState === "error") return qsTr("Downloads could not be loaded");
        if (viewState === "unavailable") return qsTr("Downloads unavailable");
        return "";
    }
    readonly property string resolvedDescription: {
        if (statusDescription.length) return statusDescription;
        if (viewState === "empty") return qsTr("Nothing is waiting in the queue.");
        if (viewState === "error") return qsTr("The queue could not be read. Try again.");
        if (viewState === "unavailable")
            return qsTr("The download service could not be reached. Check the connection and try again.");
        return "";
    }

    signal pauseRequested(string jobId)
    signal resumeRequested(string jobId)
    signal startNowRequested(string jobId)
    signal retryRequested(string jobId)
    signal watchRequested(string jobId)
    signal refreshRequested()
    signal backRequested()

    // Screen-local timings. Callers do not set these, and they are not theme tokens.
    readonly property int toastDurationMs: 2600
    readonly property int hoverSuppressMs: 500

    property var knownImports: ({})
    property bool importsReady: false
    property bool refocusPending: false

    function allows(job, name) {
        const actions = job && job.availableActions ? job.availableActions : [];
        return actions.indexOf(name) !== -1;
    }
    function primaryAction(job) {
        if (!job) return "";
        if (job.state === "downloading" && allows(job, "pause")) return "pause";
        if (job.state === "paused" && allows(job, "resume")) return "resume";
        if (job.state === "queued" && allows(job, "start_now")) return "start_now";
        if (job.state === "error" && allows(job, "retry")) return "retry";
        if (job.state === "completed" && job.importState === "imported") return "watch";
        return "";
    }
    function actionLabel(action) {
        if (action === "pause") return qsTr("Pause");
        if (action === "resume") return qsTr("Resume");
        if (action === "start_now") return qsTr("Start now");
        if (action === "retry") return qsTr("Retry");
        if (action === "watch") return qsTr("Watch");
        return "";
    }
    function progressValue(job) {
        if (!job || job.progress === undefined || job.progress === null) return NaN;
        const value = Number(job.progress);
        return isFinite(value) && value >= 0 ? value : NaN;
    }
    function showsProgress(job) {
        return !!job && (job.state === "downloading" || job.state === "paused") && isFinite(progressValue(job));
    }
    function statusLine(job) {
        if (!job) return "";
        if (job.statusText) return job.statusText;
        const value = progressValue(job);
        const percent = isFinite(value) ? Math.round(Math.max(0, Math.min(1, value)) * 100) + "%" : "";
        if (job.state === "downloading") return percent || qsTr("Downloading");
        if (job.state === "queued") return qsTr("Queued");
        if (job.state === "paused") return percent ? qsTr("Paused · %1").arg(percent) : qsTr("Paused");
        if (job.state === "verifying") return qsTr("Checking…");
        if (job.state === "completed")
            return job.importState === "imported" ? qsTr("Done · Added to the library") : qsTr("Done");
        if (job.state === "error") return job.errorText || qsTr("Could not download");
        return "";
    }
    function activateJob(job) {
        if (!job || job.busy || job.enabled === false || !job.id) return;
        const action = primaryAction(job);
        if (action === "pause") pauseRequested(job.id);
        else if (action === "resume") resumeRequested(job.id);
        else if (action === "start_now") startNowRequested(job.id);
        else if (action === "retry") retryRequested(job.id);
        else if (action === "watch") watchRequested(job.id);
    }
    function resolvedIndex() {
        if (!jobCount) return -1;
        if (selectedId.length) {
            const found = jobs.findIndex(entry => entry && entry.id === selectedId);
            if (found >= 0) return found;
            return Math.max(0, Math.min(selectedIndex, jobCount) - 1);
        }
        return 0;
    }
    function focusJob(index) {
        if (!queueVisible) return false;
        const clamped = Math.max(0, Math.min(index, jobCount - 1));
        selectedIndex = clamped;
        selectedId = jobs[clamped] && jobs[clamped].id ? jobs[clamped].id : "";
        const item = queue.itemAt(clamped);
        if (!item) {
            if (!refocusPending) {
                refocusPending = true;
                Qt.callLater(() => { refocusPending = false; focusJob(clamped); });
            }
            return false;
        }
        Ui.Focus.take(item);
        const top = item.y;
        const bottom = top + item.height;
        let next = scroller.contentY;
        if (top < next) next = top;
        else if (bottom > next + scroller.height) next = bottom - scroller.height;
        next = Math.max(0, next);
        if (next !== scroller.contentY) {
            scroller.contentY = next;
            hoverDelay.restart();
        }
        return true;
    }
    function moveFocus(key, wrap) {
        if (hoverPoint.hovered)
            hoverPoint.previousPosition = hoverPoint.point.scenePosition;
        pointerMode = false;
        if (!queueVisible) return;
        if (key !== Qt.Key_Up && key !== Qt.Key_Down) return;
        const step = key === Qt.Key_Up ? -1 : 1;
        let next = selectedIndex + step;
        if (wrap) next = (next + jobCount) % jobCount;
        else if (next < 0 || next >= jobCount) return;
        focusJob(next);
    }
    function restore() {
        if (queueVisible) {
            focusJob(resolvedIndex());
            return;
        }
        if (statusVisible) Ui.Focus.take(statusPane);
        else Ui.Focus.take(root);
    }
    function syncImports() {
        const previous = knownImports;
        const next = {};
        let notice = "";
        for (let i = 0; i < jobCount; ++i) {
            const job = jobs[i];
            if (!job || !job.id) continue;
            const imported = job.importState === "imported";
            next[job.id] = imported;
            if (importsReady && imported && previous[job.id] === false && !notice.length)
                notice = qsTr("%1 is ready to watch").arg(job.title || "");
        }
        knownImports = next;
        importsReady = true;
        if (notice.length && autoToast) showToast(notice);
    }
    // Notices that arrive while one is shown wait their turn, so two imports
    // in one read each get their toast. While the screen is hidden they are
    // kept, and shown once it is on display (review of #310).
    property var toastQueue: []
    function showToast(message) {
        if (!message.length) return;
        if (toastMessage.length || !visible) {
            toastQueue = toastQueue.concat([message]);
            return;
        }
        toastMessage = message;
        toastTimer.restart();
    }
    function nextToast() {
        toastMessage = "";
        if (!toastQueue.length || !visible) return;
        const next = toastQueue[0];
        toastQueue = toastQueue.slice(1);
        showToast(next);
    }
    onVisibleChanged: {
        if (visible) {
            if (!toastMessage.length) nextToast();
        } else if (toastMessage.length) {
            // A notice cut short by leaving is shown again on return.
            toastTimer.stop();
            toastQueue = [toastMessage].concat(toastQueue);
            toastMessage = "";
        }
    }
    function acceptDirection(event) {
        if (event.key === Qt.Key_Up || event.key === Qt.Key_Down || event.key === Qt.Key_Left || event.key === Qt.Key_Right) {
            moveFocus(event.key, false);
            event.accepted = true;
        } else if (event.key === Qt.Key_Tab || event.key === Qt.Key_Backtab) {
            moveFocus(event.key === Qt.Key_Backtab ? Qt.Key_Up : Qt.Key_Down, true);
            event.accepted = true;
        }
    }
    // Esc, Backspace and the remote Back key ask the parent to leave. The view stays put.
    function acceptKey(event) {
        if (event.key === Qt.Key_Escape || event.key === Qt.Key_Backspace || event.key === Qt.Key_Back) {
            if (!event.isAutoRepeat) backRequested();
            event.accepted = true;
            return;
        }
        acceptDirection(event);
    }

    Component.onCompleted: Qt.callLater(() => { syncImports(); restore(); })
    onViewStateChanged: Qt.callLater(restore)
    onJobsChanged: Qt.callLater(() => { syncImports(); restore(); })
    // A queue read while the screen was hidden could not focus a row. When
    // the scope itself receives focus, hand it to the selected row.
    onActiveFocusChanged: if (activeFocus && Window.activeFocusItem === root) Qt.callLater(() => {
        if (activeFocus && Window.activeFocusItem === root) restore();
    })
    Timer { id: hoverDelay; interval: hoverSuppressMs }
    Timer { id: toastTimer; interval: toastDurationMs; onTriggered: root.nextToast() }
    HoverHandler {
        id: hoverPoint
        property point previousPosition: Qt.point(-1, -1)
        onPointChanged: {
            const next = point.scenePosition;
            if (next.x === previousPosition.x && next.y === previousPosition.y)
                return;
            previousPosition = next;
            root.pointerMode = true;
        }
    }
    Keys.onPressed: event => root.acceptKey(event)

    Rectangle { anchors.fill: parent; color: Ui.Theme.background }
    Ui.LogicalCanvas {
        anchors.fill: parent
        Item {
            id: page
            anchors.fill: parent
            anchors.leftMargin: Ui.Theme.safeLeft
            anchors.rightMargin: Ui.Theme.safeRight
            anchors.topMargin: 64
            anchors.bottomMargin: 48

            Ui.UiText {
                id: heading
                objectName: "downloadsTitle"
                text: qsTr("Downloads")
                role: "title"
            }
            Ui.UiText {
                id: summary
                objectName: "downloadsSummary"
                anchors.top: heading.bottom
                anchors.topMargin: 8
                width: parent.width
                role: "meta"
                color: Ui.Theme.textSecondary
                text: root.summaryLine
                visible: root.viewState === "populated"
            }
            Flickable {
                id: scroller
                objectName: "downloadsQueue"
                // The summary is shown only with a queue; a Text whose height
                // is bound to collapse loops through its implicit height.
                anchors.top: root.viewState === "populated" ? summary.bottom : heading.bottom
                anchors.topMargin: 28
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.bottom: parent.bottom
                visible: root.queueVisible
                clip: true
                contentWidth: width
                contentHeight: queueColumn.height
                boundsBehavior: Flickable.StopAtBounds
                flickableDirection: Flickable.VerticalFlick
                onMovementStarted: hoverDelay.restart()
                Column {
                    id: queueColumn
                    width: scroller.width
                    spacing: 16
                    Repeater {
                        id: queue
                        model: root.jobs
                        delegate: FocusScope {
                            id: row
                            required property var modelData
                            required property int index
                            objectName: "job-" + (modelData.id || index)
                            width: queueColumn.width
                            height: 156
                            readonly property string actionName: root.primaryAction(modelData)
                            function hoverFocus() {
                                if (!(root.pointerMode && root.hoverFocusEnabled)) return;
                                root.selectedIndex = index;
                                root.selectedId = modelData.id || "";
                                Ui.Focus.take(row, Qt.MouseFocusReason);
                            }
                            Keys.onPressed: event => {
                                if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter || event.key === Qt.Key_Space) {
                                    if (!event.isAutoRepeat) root.activateJob(row.modelData);
                                    event.accepted = true;
                                } else root.acceptKey(event);
                            }
                            HoverHandler {
                                onHoveredChanged: if (hovered) Qt.callLater(row.hoverFocus)
                                onPointChanged: Qt.callLater(row.hoverFocus)
                            }
                            Accessible.role: Accessible.ListItem
                            Accessible.name: (modelData.title || "") + ". " + root.statusLine(modelData)
                            Rectangle {
                                anchors.fill: parent
                                radius: 30
                                color: row.activeFocus ? Ui.Theme.plateFocus : Ui.Theme.plate
                            }
                            Rectangle {
                                anchors.fill: parent
                                radius: 30
                                z: 3
                                color: "transparent"
                                border.width: 3
                                border.color: Ui.Theme.ring
                                opacity: row.activeFocus ? 1 : 0
                            }
                            Item {
                                x: 18; y: 18; width: 214; height: 120; clip: true
                                Ui.ContentCard {
                                    objectName: "artwork-" + row.modelData.id
                                    width: 214; height: 120
                                    text: ""
                                    artwork: row.modelData.artwork || ""
                                    focusPolicy: Qt.NoFocus
                                    pointerMode: false
                                    hoverFocusEnabled: false
                                    activeFocusOnTab: false
                                }
                            }
                            Column {
                                x: 252
                                width: parent.width - 252 - 280
                                anchors.verticalCenter: parent.verticalCenter
                                spacing: 6
                                Ui.UiText {
                                    objectName: "title-" + row.modelData.id
                                    width: parent.width
                                    role: "section"
                                    text: row.modelData.title || ""
                                    maximumLineCount: 1
                                    elide: Text.ElideRight
                                }
                                Ui.UiText {
                                    objectName: "subtitle-" + row.modelData.id
                                    width: parent.width
                                    role: "meta"
                                    color: Ui.Theme.textSecondary
                                    text: row.modelData.subtitle || ""
                                    visible: text.length > 0
                                    maximumLineCount: 1
                                    elide: Text.ElideRight
                                }
                                Ui.UiText {
                                    objectName: "status-" + row.modelData.id
                                    width: parent.width
                                    role: "meta"
                                    color: row.modelData.state === "error" ? Ui.Theme.error
                                         : (row.modelData.state === "completed" && row.modelData.importState === "imported" ? Ui.Theme.ember : Ui.Theme.textSecondary)
                                    text: root.statusLine(row.modelData)
                                    maximumLineCount: 1
                                    elide: Text.ElideRight
                                }
                                Ui.ProgressTrack {
                                    objectName: "progress-" + row.modelData.id
                                    width: Math.min(420, parent.width)
                                    visible: root.showsProgress(row.modelData)
                                    value: {
                                        const amount = root.progressValue(row.modelData);
                                        return isFinite(amount) ? amount : 0;
                                    }
                                }
                            }
                            Ui.ActionButton {
                                id: actionButton
                                objectName: "action-" + row.modelData.id
                                anchors.right: parent.right
                                anchors.rightMargin: 28
                                anchors.verticalCenter: parent.verticalCenter
                                focusPolicy: Qt.NoFocus
                                activeFocusOnTab: false
                                hoverFocusEnabled: false
                                text: root.actionLabel(row.actionName)
                                visible: row.actionName.length > 0 && row.activeFocus
                                enabled: row.modelData.enabled !== false
                                busy: !!row.modelData.busy
                                pointerMode: root.pointerMode
                                onActivated: root.activateJob(row.modelData)
                            }
                        }
                    }
                }
            }
            Column {
                id: skeletons
                objectName: "downloadsSkeletons"
                anchors.top: root.viewState === "populated" ? summary.bottom : heading.bottom
                anchors.topMargin: 28
                width: parent.width
                spacing: 16
                visible: root.loadingVisible
                Repeater {
                    model: 4
                    delegate: Ui.Skeleton {
                        required property int index
                        objectName: "skeleton-" + index
                        width: skeletons.width
                        height: 156
                    }
                }
            }
            Ui.StatusPane {
                id: statusPane
                objectName: "downloadsStatus"
                anchors.horizontalCenter: parent.horizontalCenter
                anchors.top: heading.bottom
                anchors.topMargin: 120
                width: 820
                visible: root.statusVisible
                kind: root.viewState === "loading" || root.viewState === "populated" ? "empty" : root.viewState
                title: root.resolvedTitle
                description: root.resolvedDescription
                actionText: root.viewState === "error" || root.viewState === "unavailable" ? qsTr("Retry") : ""
                pointerMode: root.pointerMode
                hoverFocusEnabled: root.hoverFocusEnabled
                onActionRequested: root.refreshRequested()
                onDpadRequested: root.pointerMode = false
                Keys.onPressed: event => root.acceptKey(event)
            }
            Rectangle {
                id: toast
                objectName: "completionToast"
                z: 10
                visible: root.toastMessage.length > 0
                anchors.horizontalCenter: parent.horizontalCenter
                anchors.bottom: parent.bottom
                height: 84
                width: Math.min(parent.width, toastLabel.implicitWidth + 96)
                radius: height / 2
                color: Ui.Theme.plate
                border.width: 1
                border.color: Qt.rgba(1, 1, 1, 0.08)
                Ui.UiText {
                    id: toastLabel
                    objectName: "completionToastText"
                    anchors.centerIn: parent
                    width: Math.min(toast.parent.width - 96, implicitWidth)
                    horizontalAlignment: Text.AlignHCenter
                    text: root.toastMessage
                    role: "meta"
                }
            }
        }
    }
}
