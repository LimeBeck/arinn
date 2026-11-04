// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import QtTest
import "../../apps/arinn-shell/ui" as Ui
import "../../apps/arinn-shell/ui/screens/settings" as Settings

// Library folders and the folder picker (#764, handoff LF-1…8, PK-1…7).
TestCase {
    id: suite
    name: "LibraryFolders"
    width: 1920
    height: 1080
    visible: true
    when: windowShown

    property var view
    property var browser
    // A Folders1 stand-in with the FolderBrowser shape.
    Component {
        id: browserFactory
        QtObject {
            property var places: [{kind: "home", path: "/home/user", title: "user"},
                                  {kind: "mount", path: "/media/user/Disk", title: "Disk"},
                                  {kind: "root", path: "/", title: "/"}]
            property string path: ""
            property var folders: []
            property string state: "loading"
            property var tree: ({"/home/user": ["Films", "Series"], "/home/user/Films": [],
                                 "/media/user/Disk": ["Movies"], "/": ["home", "media"],
                                 "/home": ["user"]})
            property var opened: []
            function refreshPlaces() {}
            function open(value) {
                opened = opened.concat([value]);
                path = value;
                const list = tree[value];
                if (value === "/secret") { folders = []; state = "denied"; return; }
                if (list === undefined) { folders = []; state = "gone"; return; }
                folders = list;
                state = list.length ? "ready" : "empty";
            }
            function up() {
                if (path === "/") return;
                const at = path.lastIndexOf("/");
                open(at <= 0 ? "/" : path.slice(0, at));
            }
        }
    }
    Component {
        id: viewFactory
        Settings.LibraryFoldersView {}
    }
    Component { id: spyFactory; SignalSpy {} }

    function child(name) {
        const item = findChild(view, name);
        verify(item !== null, name);
        return item;
    }
    function focused(name) { tryCompare(child(name), "activeFocus", true, 5000, name); }
    function make(paths, extra) {
        mouseMove(suite, 10, 10);
        Ui.InputMode.reset();
        browser = createTemporaryObject(browserFactory, suite);
        view = createTemporaryObject(viewFactory, suite, Object.assign({
            browser: browser,
            row: {providerId: "library", id: "roots", type: "path", currentValue: paths,
                  constraints: {multiple: true, directory: true}, unavailablePaths: []}
        }, extra || {}));
        verify(view);
        view.open();
        return view;
    }

    function test_rows_follow_the_handoff() {
        make(["/home/user/Films", "/media/user/Disk/Series"]);
        focused("folderRow-0");
        compare(child("foldersTitle").text, "Library folders");
        compare(child("foldersCount").text, "2");
        compare(child("folderName-0").text, "Films");
        compare(child("folderPath-0").text, "/home/user/Films");
        compare(child("folderRow-0").height, 128);
        compare(child("folderRemove-0").width, 128);
        keyClick(Qt.Key_Right); focused("folderRemove-0");
        keyClick(Qt.Key_Left); focused("folderRow-0");
        keyClick(Qt.Key_Down); focused("folderRow-1");
        keyClick(Qt.Key_Up); focused("folderRow-0");
        keyClick(Qt.Key_Up); focused("foldersAdd");
    }
    // LF-4: an unavailable folder stays in place and can be removed.
    function test_unavailable_folder() {
        make(["/media/user/Share"], {row: {providerId: "library", id: "roots", type: "path",
              currentValue: ["/media/user/Share"], constraints: {multiple: true, directory: true},
              unavailablePaths: ["/media/user/Share"]}});
        focused("folderRow-0");
        compare(child("folderStatus-0").text, "Unavailable. Its films stay in the library as unavailable");
        verify(child("folderStatus-0").visible);
    }
    // LF-6, LF-7: Remove asks first, Cancel is the default, nothing is
    // deleted; then focus goes to the neighbour with a toast.
    function test_remove_asks_and_keeps_the_verb() {
        make(["/home/user/Films", "/home/user/Series"]);
        const values = createTemporaryObject(spyFactory, suite, {target: view, signalName: "valuesRequested"});
        focused("folderRow-0");
        keyClick(Qt.Key_Down); keyClick(Qt.Key_Right); focused("folderRemove-1");
        keyClick(Qt.Key_Return);
        verify(child("foldersConfirm").visible);
        focused("foldersConfirmCancel");
        compare(child("foldersConfirmTitle").text, "Remove “Series” from the library?");
        keyClick(Qt.Key_Escape);
        verify(!child("foldersConfirm").visible);
        compare(values.count, 0);
        focused("folderRemove-1");
        keyClick(Qt.Key_Return);
        focused("foldersConfirmCancel");
        keyClick(Qt.Key_Right); focused("foldersConfirmRemove");
        keyClick(Qt.Key_Return);
        compare(values.count, 1);
        compare(values.signalArguments[0][0], ["/home/user/Films"]);
        verify(child("foldersToast").visible);
        compare(child("foldersToastText").text, "Folder removed");
        view.row = Object.assign({}, view.row, {currentValue: ["/home/user/Films"]});
        focused("folderRow-0");
    }
    // LF-2: no folders, Add focused.
    function test_empty_offers_add() {
        make([]);
        verify(child("foldersEmpty").visible);
        focused("foldersEmptyAdd");
    }
    // LF-5: at the limit Add stays focusable and explains.
    function test_limit_explains_instead_of_opening_the_picker() {
        make(["/a", "/b"], {row: {providerId: "library", id: "roots", type: "path", currentValue: ["/a", "/b"],
              constraints: {multiple: true, directory: true, maxItems: 2}, unavailablePaths: []}});
        focused("folderRow-0");
        keyClick(Qt.Key_Up); focused("foldersAdd");
        compare(child("foldersAdd").opacity, 0.4);
        verify(child("foldersLimit").visible);
        keyClick(Qt.Key_Return);
        verify(!child("folderPicker").visible);
        verify(child("foldersToast").visible);
    }
    // PK-1…4, LF-8: pick a folder through the start points and the list,
    // then the new row is appended and focused.
    function test_pick_a_folder() {
        make(["/home/user/Films"]);
        const values = createTemporaryObject(spyFactory, suite, {target: view, signalName: "valuesRequested"});
        focused("folderRow-0");
        keyClick(Qt.Key_Up); focused("foldersAdd");
        keyClick(Qt.Key_Return);
        const picker = child("folderPicker");
        verify(picker.visible);
        // The first start point opens: the home folder.
        tryCompare(browser, "path", "/home/user");
        tryCompare(findChild(picker, "pickerFolder-0"), "activeFocus", true);
        compare(findChild(picker, "pickerPathName").text, "user");
        verify(findChild(picker, "pickerPlaceDot-0").visible);
        compare(findChild(picker, "pickerPlace-0").text, "Home folder");
        compare(findChild(picker, "pickerPlace-2").text, "Whole computer");
        keyClick(Qt.Key_Down);
        tryCompare(findChild(picker, "pickerFolder-1"), "activeFocus", true);
        keyClick(Qt.Key_Return);
        tryCompare(browser, "path", "/home/user/Series");
        // Back goes up one level.
        keyClick(Qt.Key_Escape);
        tryCompare(browser, "path", "/home/user");
        // Left: the start points; OK on the disk opens it.
        keyClick(Qt.Key_Left);
        tryCompare(findChild(picker, "pickerPlace-0"), "activeFocus", true);
        keyClick(Qt.Key_Down);
        keyClick(Qt.Key_Return);
        tryCompare(browser, "path", "/media/user/Disk");
        tryCompare(findChild(picker, "pickerFolder-0"), "activeFocus", true);
        keyClick(Qt.Key_Up);
        tryCompare(findChild(picker, "pickerChoose"), "activeFocus", true);
        keyClick(Qt.Key_Return);
        compare(values.count, 1);
        compare(values.signalArguments[0][0], ["/home/user/Films", "/media/user/Disk"]);
        verify(!picker.visible);
        view.row = Object.assign({}, view.row, {currentValue: ["/home/user/Films", "/media/user/Disk"], pending: true});
        focused("folderRow-1");
        compare(child("folderStatus-1").text, "Adding to the library…");
    }
    // PK-10: a folder already in the list is refused in the picker.
    function test_picker_refuses_a_listed_folder() {
        make(["/home/user"]);
        focused("folderRow-0");
        keyClick(Qt.Key_Up); keyClick(Qt.Key_Return);
        const picker = child("folderPicker");
        tryCompare(browser, "path", "/home/user");
        keyClick(Qt.Key_Up);
        tryCompare(findChild(picker, "pickerChoose"), "activeFocus", true);
        keyClick(Qt.Key_Return);
        verify(picker.visible);
        verify(findChild(picker, "pickerError").visible);
    }
    // PK-5, PK-6, PK-7.
    function test_picker_states() {
        make([]);
        focused("foldersEmptyAdd");
        keyClick(Qt.Key_Return);
        const picker = child("folderPicker");
        tryCompare(browser, "path", "/home/user");
        browser.open("/home/user/Films");
        tryCompare(findChild(picker, "pickerChoose"), "activeFocus", true);
        compare(findChild(picker, "pickerStatusTitle").text, "No subfolders");
        browser.open("/secret");
        compare(findChild(picker, "pickerStatusTitle").text, "No access to this folder");
        compare(findChild(picker, "pickerChoose").opacity, 0.4);
        browser.open("/media/user/Gone");
        compare(findChild(picker, "pickerStatusTitle").text, "The network folder disconnected");
        tryCompare(findChild(picker, "pickerRetry"), "activeFocus", true);
        // Back from a start point's list: the start points, then out.
        browser.open("/home/user");
        keyClick(Qt.Key_Escape);
        tryCompare(findChild(picker, "pickerPlace-0"), "activeFocus", true);
        keyClick(Qt.Key_Escape);
        verify(!picker.visible);
        focused("foldersEmptyAdd");
    }
}
