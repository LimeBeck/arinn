// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import "../.." as Ui

// The folder picker (handoff PK-1…7, PK-9, PK-10): start points on the left,
// the subfolders of the folder shown on the right, the path line above them
// and "Choose this folder" for the path shown. It reads a FolderBrowser-like
// object (`browser`: places, path, folders, state; open(), up()) and never
// lists files. Typing a path (PK-8) is not part of this screen yet.
FocusScope {
    id: root
    width: 1920
    height: 1080

    property var browser: null
    property bool pointerMode: Ui.InputMode.pointerActive
    // The daemon's text when a choice was refused (PK-10).
    property string errorText: ""
    property int placeIndex: 0
    property int folderIndex: 0
    property string side: "folders" // "places", "folders", "choose" or "status"
    readonly property var places: browser ? (browser.places || []) : []
    readonly property var folders: browser ? (browser.folders || []) : []
    readonly property string path: browser ? (browser.path || "") : ""
    readonly property string listingState: browser ? (browser.state || "loading") : "loading"
    readonly property bool canChoose: path.length > 0 && listingState !== "denied" && listingState !== "gone"
                                      && listingState !== "loading" && listingState !== "unavailable"
    // A start point may end in "/" ("/home/user/"); compare and join
    // without it (review of #780).
    function trimmed(value) {
        const text = String(value || "");
        return text.length > 1 ? (text.replace(/\/+$/, "") || "/") : text;
    }
    readonly property int currentPlace: {
        let best = -1, length = -1;
        for (let i = 0; i < places.length; ++i) {
            const p = trimmed(places[i].path);
            const inside = trimmed(path) === p || path.startsWith(p === "/" ? "/" : p + "/");
            if (inside && p.length > length) { best = i; length = p.length; }
        }
        return best;
    }

    signal chosen(string path)
    signal cancelled()

    function placeTitle(place) {
        if (!place) return "";
        if (place.kind === "home") return qsTr("Home folder");
        if (place.kind === "root") return qsTr("Whole computer");
        return place.title || place.path;
    }
    function placeIcon(place) {
        if (!place) return "folder";
        if (place.kind === "home") return "home";
        if (place.kind === "root") return "computer";
        return place.path.indexOf("/mnt/") === 0 ? "lan" : "usb";
    }
    // Long paths collapse their middle to "/…/" (PK-1).
    function shownParent(raw) {
        // A start point may end in "/" (review of #780).
        const value = trimmed(raw);
        const parent = value.lastIndexOf("/") > 0 ? value.slice(0, value.lastIndexOf("/") + 1) : "/";
        if (value === "/") return "";
        if (parent.length <= 44) return parent;
        const parts = parent.split("/").filter(part => part.length);
        return "/" + parts[0] + "/…/" + parts[parts.length - 1] + "/";
    }
    function lastSegment(raw) {
        const value = trimmed(raw);
        if (value === "/") return "/";
        return value.slice(value.lastIndexOf("/") + 1);
    }
    function start() {
        if (browser) {
            browser.refreshPlaces();
            // The folder shown last is read again: it may have changed or
            // gone while the picker was closed (review of #780).
            if (path.length) browser.open(path);
            else {
                const first = places.length ? places[0].path : "";
                if (first.length) browser.open(first);
            }
        }
        errorText = "";
        folderIndex = 0;
        side = "folders";
        Qt.callLater(settle);
    }
    // Places arrive after the picker opens: open the first one then.
    onPlacesChanged: if (visible && !path.length && places.length && browser) browser.open(places[0].path)
    // While a listing loads, Choose holds focus for the folders: the first
    // folder takes it once the listing is ready (review of #780).
    property bool holdFolders: false
    function settle() {
        if (!visible) return;
        if (side === "status" && !statusActions) side = "folders";
        if (side === "folders" && listingState === "loading") {
            holdFolders = true;
            Ui.Focus.take(chooseButton);
            holdFolders = false;
            return;
        }
        if (side === "places" && focusPlace(placeIndex)) return;
        if (side === "folders" && focusFolder(folderIndex)) return;
        if (side === "status" && (retryButton.activeFocus || homeButton.activeFocus)) return;
        if (focusStatus(false)) return;
        Ui.Focus.take(chooseButton);
    }
    onFoldersChanged: { folderIndex = 0; if (visible && side === "folders") Qt.callLater(settle); }
    onListingStateChanged: if (visible) Qt.callLater(settle)
    function focusFolder(index) {
        if (!folders.length || listingState !== "ready") return false;
        const clamped = Math.max(0, Math.min(index, folders.length - 1));
        const item = folderList.itemAtIndex(clamped);
        folderList.positionViewAtIndex(clamped, ListView.Contain);
        folderIndex = clamped;
        side = "folders";
        const target = folderList.itemAtIndex(clamped) || item;
        return !!target && Ui.Focus.take(target);
    }
    function focusPlace(index) {
        if (!places.length) return false;
        const clamped = Math.max(0, Math.min(index, places.length - 1));
        const item = placeRepeater.itemAt(clamped);
        placeIndex = clamped;
        side = "places";
        if (!item || !Ui.Focus.take(item)) return false;
        // Many mounts scroll: the focused start point stays on screen
        // (review of #780).
        const top = item.y, bottom = item.y + item.height;
        if (top < placeFlick.contentY) placeFlick.contentY = top;
        else if (bottom > placeFlick.contentY + placeFlick.height)
            placeFlick.contentY = bottom - placeFlick.height;
        return true;
    }
    // The recovery actions of a gone or unavailable listing.
    readonly property bool statusActions: listingState === "gone" || listingState === "unavailable"
    function focusStatus(home) {
        if (!statusActions) return false;
        side = "status";
        return Ui.Focus.take(home && homeButton.visible ? homeButton : retryButton);
    }
    function openChild(name) {
        if (!browser) return;
        side = "folders";
        const parent = trimmed(path);
        browser.open(parent === "/" ? "/" + name : parent + "/" + name);
    }
    // Back: up one level; at a start point, to the start points, then out.
    function back() {
        if (side === "places") { cancelled(); return; }
        const atPlace = places.some(place => trimmed(place.path) === trimmed(path));
        // Without a start point to go to (Places failed), Back leaves the
        // picker instead of doing nothing (review of #780).
        if (atPlace || path === "/" || !path.length) {
            if (!focusPlace(Math.max(0, currentPlace))) cancelled();
            return;
        }
        side = "folders";
        browser.up();
    }
    function choose() {
        if (!canChoose) return;
        chosen(path);
    }
    Keys.onPressed: event => {
        if ([Qt.Key_Up, Qt.Key_Down, Qt.Key_Left, Qt.Key_Right].includes(event.key)) Ui.InputMode.keyPressed();
        if (event.key === Qt.Key_Escape || event.key === Qt.Key_Backspace || event.key === Qt.Key_Back) {
            if (!event.isAutoRepeat) back();
        } else if (side === "status" && (event.key === Qt.Key_Left || event.key === Qt.Key_Right
                                          || event.key === Qt.Key_Up || event.key === Qt.Key_Down)) {
            // Retry and "To the home folder" sit in a row (review of #780).
            if (event.key === Qt.Key_Right) focusStatus(true);
            else if (event.key === Qt.Key_Left) {
                if (homeButton.activeFocus) focusStatus(false);
                else focusPlace(Math.max(0, currentPlace));
            } else if (event.key === Qt.Key_Up) { side = "choose"; Ui.Focus.take(chooseButton); }
        } else if (event.key === Qt.Key_Up) {
            if (side === "folders" && folderIndex > 0) focusFolder(folderIndex - 1);
            else if (side === "places" && placeIndex > 0) focusPlace(placeIndex - 1);
            else if (side === "folders" || side === "choose") { side = "choose"; Ui.Focus.take(chooseButton); }
        } else if (event.key === Qt.Key_Down) {
            if (side === "choose") { if (!focusFolder(0) && !focusStatus(false)) Ui.Focus.take(chooseButton); }
            else if (side === "folders") focusFolder(folderIndex + 1);
            else focusPlace(placeIndex + 1);
        } else if (event.key === Qt.Key_Left) {
            if (side !== "places") focusPlace(Math.max(0, currentPlace));
        } else if (event.key === Qt.Key_Right) {
            if (side === "places") { if (!focusFolder(folderIndex) && !focusStatus(false)) Ui.Focus.take(chooseButton); }
            else if (side === "folders" && folders[folderIndex] !== undefined) openChild(folders[folderIndex]);
        } else if (event.key === Qt.Key_Tab || event.key === Qt.Key_Backtab) {
        } else return;
        event.accepted = true;
    }

    Rectangle { anchors.fill: parent; color: Ui.Theme.background }
    Ui.LogicalCanvas {
        anchors.fill: parent
        Item {
            anchors.fill: parent
            Ui.UiText {
                objectName: "pickerTitle"
                x: 120; y: 96
                text: qsTr("Choose a folder")
                role: "title"
            }
            Rectangle {
                objectName: "pickerPath"
                x: 120; y: 200
                width: 1180; height: 104
                radius: 30
                color: Qt.rgba(1, 1, 1, 0.07)
                Row {
                    x: 32
                    anchors.verticalCenter: parent.verticalCenter
                    width: parent.width - 64
                    Ui.UiText {
                        objectName: "pickerPathParent"
                        text: root.shownParent(root.path)
                        font.family: "monospace"
                        font.pixelSize: 32
                        color: Ui.Theme.textSecondary
                    }
                    Ui.UiText {
                        objectName: "pickerPathName"
                        text: root.lastSegment(root.path)
                        font.family: "monospace"
                        font.pixelSize: 32
                        color: Ui.Theme.textPrimary
                        elide: Text.ElideRight
                    }
                }
            }
            Ui.ActionButton {
                id: chooseButton
                objectName: "pickerChoose"
                x: 120 + 1180 + 24
                y: 200 + (104 - 88) / 2
                height: 88
                primary: true
                text: qsTr("Choose this folder")
                opacity: root.canChoose ? 1 : 0.4
                pointerMode: root.pointerMode
                onActiveFocusChanged: if (activeFocus && !root.holdFolders) root.side = "choose"
                onActivated: root.choose()
            }
            Rectangle {
                objectName: "pickerError"
                visible: root.errorText.length > 0
                x: 700; y: 330
                width: 1100; height: 72
                radius: 26
                color: Qt.rgba(0.91, 0.28, 0.31, 0.22)
                Ui.UiText {
                    x: 28; anchors.verticalCenter: parent.verticalCenter
                    width: parent.width - 56
                    text: root.errorText
                    color: Ui.Theme.textPrimary
                    elide: Text.ElideRight
                }
            }
            // Start points (PK-2, PK-9). They scroll when there are more than
            // fit (review of #780).
            Flickable {
                id: placeFlick
                objectName: "pickerPlacesView"
                x: 120; y: 340
                width: 540; height: 1080 - 340 - 104
                clip: true
                contentHeight: placeColumn.height
                interactive: root.pointerMode
                boundsBehavior: Flickable.StopAtBounds
            Column {
                id: placeColumn
                objectName: "pickerPlaces"
                width: 540
                spacing: 8
                Repeater {
                    id: placeRepeater
                    model: root.places
                    delegate: Ui.Surface {
                        id: placeRow
                        required property var modelData
                        required property int index
                        objectName: "pickerPlace-" + index
                        width: 540; height: 88
                        shapeRole: "container"
                        cornerRadius: Ui.Theme.containerRadius
                        focusScale: 1; focusLift: 0; liftFromBottom: false
                        text: root.placeTitle(modelData)
                        pointerMode: root.pointerMode
                        onActiveFocusChanged: if (activeFocus) { root.placeIndex = index; root.side = "places"; }
                        onActivated: { root.side = "folders"; root.browser.open(modelData.path); }
                        contentItem: Item {
                            Rectangle {
                                objectName: "pickerPlaceDot-" + placeRow.index
                                x: 6; anchors.verticalCenter: parent.verticalCenter
                                width: 10; height: 10; radius: 5
                                color: Ui.Theme.ember
                                visible: placeRow.index === root.currentPlace
                            }
                            FolderGlyph {
                                x: 28; anchors.verticalCenter: parent.verticalCenter
                                name: root.placeIcon(placeRow.modelData)
                                color: placeRow.activeFocus ? Ui.Theme.textPrimary : Ui.Theme.textSecondary
                            }
                            Ui.UiText {
                                x: 28 + 38 + 20
                                width: parent.width - x - 20
                                anchors.verticalCenter: parent.verticalCenter
                                text: placeRow.text
                                role: "control"
                                color: placeRow.activeFocus ? Ui.Theme.textPrimary : Ui.Theme.textSecondary
                                elide: Text.ElideRight
                            }
                        }
                    }
                }
                Ui.UiText {
                    objectName: "pickerNoMounts"
                    width: 540
                    visible: !root.places.some(place => place.kind === "mount")
                    text: qsTr("Drives and network folders appear here when the system connects them")
                    font.pixelSize: 24
                    color: Ui.Theme.textHint
                    wrapMode: Text.WordWrap
                }
            }
            }
            Ui.UiText {
                objectName: "pickerCount"
                x: 700; y: 340
                visible: root.listingState === "ready"
                text: qsTr("%n folder(s)", "", root.folders.length) + (root.folders.length
                      ? " · " + qsTr("%1 of %2").arg(root.folderIndex + 1).arg(root.folders.length) : "")
                role: "eyebrow"
                color: Ui.Theme.textSecondary
            }
            ListView {
                id: folderList
                objectName: "pickerFolders"
                x: 700; y: 390
                width: 1100; height: 1080 - 390 - 72
                visible: root.listingState === "ready"
                clip: true
                spacing: 8
                interactive: root.pointerMode
                boundsBehavior: Flickable.StopAtBounds
                highlightFollowsCurrentItem: false
                model: root.folders
                delegate: Ui.Surface {
                    id: folderRow
                    required property string modelData
                    required property int index
                    objectName: "pickerFolder-" + index
                    width: folderList.width
                    height: 96
                    shapeRole: "container"
                    cornerRadius: 26
                    focusScale: 1; focusLift: 0; liftFromBottom: false
                    text: modelData
                    pointerMode: root.pointerMode
                    onActiveFocusChanged: if (activeFocus) { root.folderIndex = index; root.side = "folders"; }
                    onActivated: root.openChild(modelData)
                    contentItem: Item {
                        FolderGlyph {
                            x: 28; anchors.verticalCenter: parent.verticalCenter
                            name: "folder"
                            color: folderRow.activeFocus ? Ui.Theme.textPrimary : Ui.Theme.textSecondary
                        }
                        Ui.UiText {
                            x: 28 + 38 + 20
                            width: parent.width - x - 80
                            anchors.verticalCenter: parent.verticalCenter
                            text: folderRow.modelData
                            role: "control"
                            maximumLineCount: 2
                            wrapMode: Text.Wrap
                            elide: Text.ElideRight
                        }
                        Ui.LineIcon {
                            anchors.right: parent.right; anchors.rightMargin: 28
                            anchors.verticalCenter: parent.verticalCenter
                            width: 30; height: 30
                            name: "right"
                            color: Ui.Theme.textSecondary
                        }
                    }
                }
            }
            // Empty, no access, share gone (PK-5, PK-6, PK-7).
            Column {
                objectName: "pickerStatus"
                x: 700; y: 420
                width: 1000
                spacing: 20
                visible: root.listingState === "empty" || root.listingState === "denied" || root.listingState === "gone"
                         || root.listingState === "unavailable"
                FolderGlyph {
                    width: 80; height: 80
                    name: root.listingState === "denied" ? "lock" : root.listingState === "gone" ? "link_off" : "folder"
                    color: root.listingState === "gone" ? Ui.Theme.error : Ui.Theme.textHint
                }
                Ui.UiText {
                    objectName: "pickerStatusTitle"
                    width: parent.width
                    text: root.listingState === "empty" ? qsTr("No subfolders")
                        : root.listingState === "denied" ? qsTr("No access to this folder")
                        : root.listingState === "gone" ? qsTr("The network folder disconnected")
                        : qsTr("Folders cannot be read right now")
                    font.pixelSize: 44
                    font.weight: 700
                    wrapMode: Text.WordWrap
                }
                Ui.UiText {
                    width: parent.width
                    text: root.listingState === "empty" ? qsTr("Files are not shown here. Choose this folder, or go back.")
                        : root.listingState === "denied" ? qsTr("Arinn may not read this folder. Choose another one.")
                        : root.listingState === "gone" ? qsTr("Arinn never mounts anything itself. Connect the share and try again.")
                        : qsTr("Try again in a moment.")
                    color: Ui.Theme.textSecondary
                    wrapMode: Text.WordWrap
                }
                Row {
                    spacing: 20
                    visible: root.listingState === "gone" || root.listingState === "unavailable"
                    Ui.ActionButton {
                        id: retryButton
                        objectName: "pickerRetry"
                        text: qsTr("Retry")
                        pointerMode: root.pointerMode
                        // Before the first folder opens, Places failed:
                        // ask for them again (review of #780).
                        onActivated: root.path.length ? root.browser.open(root.path) : root.browser.refreshPlaces()
                    }
                    Ui.ActionButton {
                        id: homeButton
                        objectName: "pickerHome"
                        text: qsTr("To the home folder")
                        visible: root.places.some(place => place.kind === "home")
                        pointerMode: root.pointerMode
                        onActivated: {
                            const home = root.places.find(place => place.kind === "home");
                            if (home) root.browser.open(home.path);
                        }
                    }
                }
            }
            Ui.UiText {
                x: 120; y: 1080 - 72
                text: qsTr("OK Open · Back Up one level · ↑ Choose")
                font.pixelSize: 24
                color: Ui.Theme.textHint
            }
        }
    }
}
