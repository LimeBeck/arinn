// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import QtQuick.Templates as T
import "../../" as Ui

// Settings → System → About (#742). Rows are read-only except Copy details.
// `about` is the shell's SystemAbout, or a test fixture with the same fields.
Item {
    id: block
    implicitHeight: rowColumn.implicitHeight
    property var about: null
    // The settings view, so the copy sheet is not clipped by the row list.
    property Item sheetParent: null
    readonly property int count: {
        let total = 0;
        const list = entries;
        for (let i = 0; i < list.length; ++i)
            if (list[i].focusable)
                ++total;
        return total;
    }
    property bool sheetOpen: false
    property bool includePrivate: false
    signal copied(int code)
    signal sheetClosed()

    function itemAt(index) {
        let seen = 0;
        for (let i = 0; i < rowRepeater.count; ++i) {
            const item = rowRepeater.itemAt(i);
            if (!item || !item.focusItem)
                continue;
            if (seen === index)
                return item.focusItem;
            ++seen;
        }
        return null;
    }
    function openSheet() {
        includePrivate = false;
        if (about && about.refreshFileName)
            about.refreshFileName();
        sheetOpen = true;
        Qt.callLater(() => { if (sheetOpen) Ui.Focus.take(privacyButton); });
    }
    function closeSheet() {
        if (!sheetOpen)
            return;
        sheetOpen = false;
        includePrivate = false;
        sheetClosed();
    }
    function toneColor(tone) {
        if (tone === "error")
            return Ui.Theme.error;
        if (tone === "hint")
            return Ui.Theme.textHint;
        return Ui.Theme.textSecondary;
    }

    readonly property var entries: {
        if (!about)
            return [];
        const facts = about;
        const shellShort = facts.shellShort || "";
        const shellBuildId = facts.shellBuildId || "";
        const daemonAvailable = !!facts.daemonAvailable;
        const daemonShort = facts.daemonShort || "";
        const daemonBuildId = facts.daemonBuildId || "";
        const daemonMismatch = !!facts.daemonMismatch && daemonAvailable;
        const channelDate = facts.channelDate || "";
        const providerDetail = facts.providerDetail || "";
        const providerCount = facts.providerCount === undefined || facts.providerCount === null
                              ? -1 : Number(facts.providerCount);
        const osName = facts.osName || "";
        const kernelName = facts.kernel || "";
        const plasmaName = facts.plasma || "";
        const qtName = facts.qtVersion || "";
        const sessionName = facts.session || "";
        const displayName = facts.display || "";
        const audioName = facts.audio || "";
        const connectionName = facts.connection || "";
        const ipName = facts.ipAddress || "";
        const storageName = facts.storage || "";
        const unknown = qsTr("Unknown");
        const unavailable = qsTr("Unavailable");
        function known(text) { return text.length ? text : unknown; }
        function toneFor(text) { return text.length ? "secondary" : "hint"; }
        let providerValue = unavailable;
        let providerTone = "secondary";
        let providerLine = "";
        if (daemonAvailable && !(providerCount >= 0)) {
            providerValue = unknown;
            providerTone = "hint";
        } else if (daemonAvailable) {
            providerValue = String(providerCount);
            providerLine = providerDetail;
            providerTone = "secondary";
        }
        return [
            { kind: "heading", title: qsTr("About") },
            { kind: "action", id: "copy", focusable: true, title: qsTr("Copy details"),
              detail: qsTr("For a bug report: to the clipboard and as a file in Downloads") },
            { kind: "eyebrow", title: "Arinn" },
            { kind: "value", id: "shell", focusable: true, title: qsTr("Arinn version"),
              value: known(shellShort), tone: toneFor(shellShort), buildId: shellBuildId },
            { kind: "value", id: "daemon", focusable: true, title: qsTr("arinnd service"),
              value: daemonAvailable ? known(daemonShort) : unavailable,
              tone: daemonMismatch ? "error" : (daemonAvailable ? toneFor(daemonShort) : "secondary"),
              buildId: daemonAvailable ? daemonBuildId : "",
              mismatch: daemonMismatch, down: !daemonAvailable },
            { kind: "value", id: "channel", focusable: true, title: qsTr("Channel and build date"),
              value: known(channelDate), tone: toneFor(channelDate) },
            { kind: "value", id: "providers", focusable: true, title: qsTr("Providers"),
              value: providerValue, tone: providerTone, detail: providerLine },
            { kind: "eyebrow", title: qsTr("System") },
            { kind: "value", id: "os", focusable: true, title: qsTr("OS"),
              value: known(osName), tone: toneFor(osName) },
            { kind: "value", id: "kernel", focusable: true, title: qsTr("Kernel"),
              value: known(kernelName), tone: toneFor(kernelName) },
            { kind: "value", id: "plasma", focusable: true, title: qsTr("Plasma"),
              value: known(plasmaName), tone: toneFor(plasmaName) },
            { kind: "value", id: "qt", focusable: true, title: qsTr("Qt"),
              value: known(qtName), tone: toneFor(qtName) },
            { kind: "value", id: "session", focusable: true, title: qsTr("Session"),
              value: known(sessionName), tone: toneFor(sessionName) },
            { kind: "eyebrow", title: qsTr("Display and sound") },
            { kind: "value", id: "display", focusable: true, title: qsTr("Display"),
              value: known(displayName), tone: toneFor(displayName) },
            { kind: "value", id: "audio", focusable: true, title: qsTr("Audio output"),
              value: known(audioName), tone: toneFor(audioName) },
            { kind: "eyebrow", title: qsTr("Network") },
            { kind: "value", id: "connection", focusable: true, title: qsTr("Connection"),
              value: known(connectionName), tone: toneFor(connectionName) },
            { kind: "value", id: "ip", focusable: true, title: qsTr("IP address"),
              value: known(ipName), tone: toneFor(ipName) },
            { kind: "eyebrow", title: qsTr("Storage") },
            { kind: "value", id: "storage", focusable: true, title: qsTr("Free space"),
              value: known(storageName), tone: toneFor(storageName),
              detail: qsTr("Disk with the Downloads folder") }
        ];
    }

    Column {
        id: rowColumn
        width: block.width
        spacing: 8
    Repeater {
        id: rowRepeater
        model: block.entries
        delegate: Item {
            id: rowItem
            required property var modelData
            required property int index
            width: block.width
            height: modelData.kind === "heading" ? headingText.implicitHeight + 6
                  : modelData.kind === "eyebrow" ? eyebrowText.implicitHeight + (index > 2 ? 22 : 10)
                  : rowButton.height
            readonly property Item focusItem: modelData.focusable ? rowButton : null
            readonly property int detailCount: {
                let total = 0;
                if (modelData.detail)
                    ++total;
                if (modelData.down)
                    ++total;
                if (modelData.mismatch)
                    ++total;
                if (rowButton.activeFocus && modelData.buildId)
                    ++total;
                return total;
            }

            Ui.UiText {
                id: headingText
                visible: modelData.kind === "heading"
                width: parent.width
                text: modelData.kind === "heading" ? modelData.title : ""
                role: "section"
                objectName: visible ? "aboutHeading" : ""
            }
            Ui.UiText {
                id: eyebrowText
                visible: modelData.kind === "eyebrow"
                width: parent.width
                anchors.bottom: parent.bottom
                text: modelData.kind === "eyebrow" ? modelData.title : ""
                role: "eyebrow"
                color: Ui.Theme.textHint
                objectName: visible ? "about-eyebrow-" + modelData.title : ""
            }
            T.AbstractButton {
                id: rowButton
                visible: !!modelData.focusable
                width: parent.width
                height: Math.max(88 + rowItem.detailCount * 26, rowText.implicitHeight + 36)
                padding: 0
                focusPolicy: Qt.StrongFocus
                activeFocusOnTab: false
                objectName: modelData.id ? "about-" + modelData.id : ""
                onClicked: {
                    if (modelData.kind === "action")
                        block.openSheet();
                }
                Keys.onPressed: event => {
                    if (event.key !== Qt.Key_Return && event.key !== Qt.Key_Enter)
                        return;
                    if (!event.isAutoRepeat)
                        rowButton.down = true;
                    event.accepted = true;
                }
                Keys.onReleased: event => {
                    if (event.key !== Qt.Key_Return && event.key !== Qt.Key_Enter)
                        return;
                    const invoke = rowButton.down;
                    rowButton.down = false;
                    if (invoke && !event.isAutoRepeat)
                        rowButton.clicked();
                    event.accepted = true;
                }
                background: Rectangle {
                    radius: Ui.Theme.containerRadius
                    color: rowButton.activeFocus ? Ui.Theme.plateContainer : "transparent"
                }
                Ui.FocusRing { shown: rowButton.activeFocus; radius: Ui.Theme.containerRadius }
                contentItem: Item {
                    Column {
                        id: rowText
                        x: 32
                        width: parent.width - 64 - (modelData.kind === "action" ? 72 : 360)
                        anchors.verticalCenter: parent.verticalCenter
                        spacing: 2
                        Ui.UiText {
                            width: parent.width
                            text: modelData.title || ""
                            role: "control"
                            color: Ui.Theme.textPrimary
                            elide: Text.ElideRight
                            objectName: modelData.id ? "about-title-" + modelData.id : ""
                        }
                        Ui.UiText {
                            width: parent.width
                            visible: !!(modelData.down)
                            text: visible ? qsTr("The arinnd service is not responding") : ""
                            role: "meta"
                            color: Ui.Theme.textSecondary
                            wrapMode: Text.WordWrap
                            objectName: visible && modelData.id ? "about-detail-" + modelData.id : ""
                        }
                        Ui.UiText {
                            width: parent.width
                            visible: !!(modelData.mismatch)
                            text: visible ? qsTr("Different build from Arinn. Restart Arinn") : ""
                            role: "meta"
                            color: Ui.Theme.error
                            wrapMode: Text.WordWrap
                            objectName: visible && modelData.id ? "about-mismatch-" + modelData.id : ""
                        }
                        Ui.UiText {
                            width: parent.width
                            visible: rowButton.activeFocus && !!modelData.buildId
                            text: visible ? qsTr("Build %1").arg(modelData.buildId) : ""
                            role: "meta"
                            color: Ui.Theme.textSecondary
                            elide: Text.ElideRight
                            objectName: visible && modelData.id ? "about-build-" + modelData.id : ""
                        }
                        Ui.UiText {
                            width: parent.width
                            visible: !!modelData.detail
                            text: modelData.detail || ""
                            role: "meta"
                            color: Ui.Theme.textSecondary
                            wrapMode: Text.WordWrap
                            maximumLineCount: 2
                            elide: Text.ElideRight
                            objectName: visible && modelData.id && !modelData.down ? "about-detail-" + modelData.id : ""
                        }
                    }
                    Row {
                        anchors.right: parent.right
                        anchors.rightMargin: 32
                        anchors.verticalCenter: parent.verticalCenter
                        spacing: 8
                        Ui.UiText {
                            visible: modelData.kind !== "action"
                            text: {
                                if (modelData.kind === "action")
                                    return "";
                                return modelData.mismatch ? "• " + (modelData.value || "") : (modelData.value || "");
                            }
                            role: "meta"
                            color: block.toneColor(modelData.tone)
                            horizontalAlignment: Text.AlignRight
                            objectName: modelData.id ? "about-value-" + modelData.id : ""
                        }
                        Ui.UiText {
                            visible: modelData.kind === "action"
                            text: "›"
                            role: "control"
                            color: Ui.Theme.textSecondary
                            objectName: modelData.kind === "action" ? "about-chevron" : ""
                        }
                    }
                }
            }
        }
    }
    }

    Item {
        id: sheet
        parent: block.sheetParent || block
        anchors.fill: parent
        visible: block.sheetOpen
        z: 12
        Rectangle { anchors.fill: parent; color: Qt.rgba(0, 0, 0, 0.45) }
        Rectangle {
            id: panel
            objectName: "aboutSheet"
            x: 1920 - 720 - 48
            y: 72
            width: 720
            height: 1080 - 144
            radius: 44
            color: Qt.rgba(0.09, 0.09, 0.1, 0.96)
            Column {
                x: 48
                y: 48
                width: parent.width - 96
                spacing: 18
                Ui.UiText {
                    width: parent.width
                    role: "eyebrow"
                    color: Ui.Theme.textSecondary
                    text: qsTr("About")
                }
                Ui.UiText {
                    width: parent.width
                    role: "title"
                    wrapMode: Text.WordWrap
                    text: qsTr("Copy details")
                    objectName: "aboutSheetTitle"
                }
                Ui.UiText {
                    width: parent.width
                    role: "meta"
                    color: Ui.Theme.textSecondary
                    wrapMode: Text.WordWrap
                    text: qsTr("To the clipboard and as a file in Downloads.")
                }
                T.AbstractButton {
                    id: privacyButton
                    objectName: "aboutPrivacy"
                    width: parent.width
                    height: Math.max(112, privacyText.implicitHeight + 36)
                    padding: 0
                    focusPolicy: Qt.StrongFocus
                    activeFocusOnTab: false
                    onClicked: block.includePrivate = !block.includePrivate
                    Keys.onPressed: event => {
                        if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) {
                            if (!event.isAutoRepeat)
                                privacyButton.clicked();
                            event.accepted = true;
                        } else if (event.key === Qt.Key_Down) {
                            Ui.Focus.take(copyButton);
                            event.accepted = true;
                        } else if (event.key === Qt.Key_Escape || event.key === Qt.Key_Backspace || event.key === Qt.Key_Back) {
                            block.closeSheet();
                            event.accepted = true;
                        }
                    }
                    background: Rectangle {
                        radius: Ui.Theme.containerRadius
                        color: privacyButton.activeFocus ? Ui.Theme.plateContainer : "transparent"
                    }
                    Ui.FocusRing { shown: privacyButton.activeFocus; radius: Ui.Theme.containerRadius }
                    contentItem: Item {
                        Column {
                            id: privacyText
                            x: 8
                            width: parent.width - 88 - 36
                            anchors.verticalCenter: parent.verticalCenter
                            spacing: 4
                            Ui.UiText {
                                width: parent.width
                                text: qsTr("Network and device name")
                                role: "control"
                                wrapMode: Text.WordWrap
                            }
                            Ui.UiText {
                                width: parent.width
                                text: block.includePrivate
                                      ? qsTr("IP address, connection and name. On — included in the details")
                                      : qsTr("IP address, connection and name. Off — left out")
                                role: "meta"
                                color: Ui.Theme.textSecondary
                                wrapMode: Text.WordWrap
                                objectName: "aboutPrivacyDetail"
                            }
                        }
                        Item {
                            width: 88
                            height: 50
                            anchors.right: parent.right
                            anchors.rightMargin: 8
                            anchors.verticalCenter: parent.verticalCenter
                            Rectangle {
                                id: privacyTrack
                                objectName: "aboutPrivacyTrack"
                                anchors.fill: parent
                                radius: 25
                                color: block.includePrivate ? Ui.Theme.ember : Qt.rgba(1, 1, 1, 0.20)
                            }
                            Rectangle {
                                objectName: "aboutPrivacyKnob"
                                y: 5
                                x: block.includePrivate ? 43 : 5
                                width: 40
                                height: 40
                                radius: 20
                                color: block.includePrivate ? Ui.Theme.ink : Ui.Theme.textPrimary
                            }
                        }
                    }
                }
                Rectangle {
                    width: parent.width
                    height: summaryText.implicitHeight + 36
                    radius: 22
                    color: Qt.rgba(1, 1, 1, 0.05)
                    Column {
                        id: summaryText
                        x: 24
                        y: 18
                        width: parent.width - 48
                        spacing: 4
                        Ui.UiText {
                            width: parent.width
                            text: qsTr("Included")
                            role: "meta"
                            color: Ui.Theme.textHint
                        }
                        Ui.UiText {
                            width: parent.width
                            text: qsTr("Arinn, arinnd and provider versions")
                            role: "meta"
                            color: Ui.Theme.textPrimary
                            wrapMode: Text.WordWrap
                        }
                        Ui.UiText {
                            width: parent.width
                            text: qsTr("System, display, sound and free space")
                            role: "meta"
                            color: Ui.Theme.textPrimary
                            wrapMode: Text.WordWrap
                        }
                        Ui.UiText {
                            id: excludedLine
                            objectName: "aboutExcluded"
                            width: parent.width
                            text: block.includePrivate
                                  ? qsTr("IP address, connection and device name")
                                  : qsTr("No IP address or device name")
                            role: "meta"
                            color: Ui.Theme.textSecondary
                            wrapMode: Text.WordWrap
                        }
                    }
                }
                Ui.UiText {
                    objectName: "aboutFile"
                    width: parent.width
                    role: "meta"
                    color: Ui.Theme.textSecondary
                    elide: Text.ElideMiddle
                    text: qsTr("File: Downloads/%1").arg((block.about && block.about.fileName) || "")
                }
            }
            Row {
                x: 48
                anchors.bottom: parent.bottom
                anchors.bottomMargin: 40
                spacing: 16
                T.AbstractButton {
                    id: copyButton
                    objectName: "aboutCopyButton"
                    height: 88
                    width: Math.max(200, copyLabel.implicitWidth + 56)
                    padding: 0
                    focusPolicy: Qt.StrongFocus
                    activeFocusOnTab: false
                    onClicked: {
                        let code = 3;
                        if (block.about && block.about.copyReport)
                            code = block.about.copyReport(block.includePrivate);
                        const done = code;
                        block.closeSheet();
                        block.copied(done);
                    }
                    Keys.onPressed: event => {
                        if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) {
                            if (!event.isAutoRepeat)
                                copyButton.clicked();
                            event.accepted = true;
                        } else if (event.key === Qt.Key_Right || event.key === Qt.Key_Down) {
                            Ui.Focus.take(cancelButton);
                            event.accepted = true;
                        } else if (event.key === Qt.Key_Up) {
                            Ui.Focus.take(privacyButton);
                            event.accepted = true;
                        } else if (event.key === Qt.Key_Escape || event.key === Qt.Key_Backspace || event.key === Qt.Key_Back) {
                            block.closeSheet();
                            event.accepted = true;
                        }
                    }
                    background: Rectangle {
                        radius: height / 2
                        color: copyButton.activeFocus ? Ui.Theme.textPrimary : Qt.rgba(1, 1, 1, 0.14)
                    }
                    contentItem: Ui.UiText {
                        id: copyLabel
                        text: qsTr("Copy")
                        horizontalAlignment: Text.AlignHCenter
                        verticalAlignment: Text.AlignVCenter
                        color: copyButton.activeFocus ? Ui.Theme.ink : Ui.Theme.textPrimary
                    }
                }
                T.AbstractButton {
                    id: cancelButton
                    objectName: "aboutCancelButton"
                    height: 88
                    width: Math.max(180, cancelLabel.implicitWidth + 56)
                    padding: 0
                    focusPolicy: Qt.StrongFocus
                    activeFocusOnTab: false
                    onClicked: block.closeSheet()
                    Keys.onPressed: event => {
                        if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) {
                            if (!event.isAutoRepeat)
                                cancelButton.clicked();
                            event.accepted = true;
                        } else if (event.key === Qt.Key_Left || event.key === Qt.Key_Up) {
                            if (event.key === Qt.Key_Up)
                                Ui.Focus.take(privacyButton);
                            else
                                Ui.Focus.take(copyButton);
                            event.accepted = true;
                        } else if (event.key === Qt.Key_Escape || event.key === Qt.Key_Backspace || event.key === Qt.Key_Back) {
                            block.closeSheet();
                            event.accepted = true;
                        }
                    }
                    background: Rectangle {
                        radius: height / 2
                        color: cancelButton.activeFocus ? Ui.Theme.textPrimary : Qt.rgba(1, 1, 1, 0.14)
                    }
                    contentItem: Ui.UiText {
                        id: cancelLabel
                        text: qsTr("Cancel")
                        horizontalAlignment: Text.AlignHCenter
                        verticalAlignment: Text.AlignVCenter
                        color: cancelButton.activeFocus ? Ui.Theme.ink : Ui.Theme.textPrimary
                    }
                }
            }
        }
    }
}
