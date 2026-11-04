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
            property int placeReads: 0
            function refreshPlaces() { ++placeReads; }
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
            row: {providerId: "library", id: "roots", type: "path", libraryFolders: true, currentValue: paths,
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
        make(["/media/user/Share"], {row: {providerId: "library", id: "roots", type: "path", libraryFolders: true,
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
        // Review of #780: "Folder removed" waits for the saved row.
        verify(!child("foldersToast").visible);
        view.row = Object.assign({}, view.row, {pending: true});
        verify(!child("foldersToast").visible);
        view.row = Object.assign({}, view.row, {currentValue: ["/home/user/Films"], pending: false});
        verify(child("foldersToast").visible);
        compare(child("foldersToastText").text, "Folder removed");
        focused("folderRow-0");
    }
    // Review of #780: a removal the daemon refuses keeps the folder and says
    // why in words, not as an error code.
    function test_failed_removal_shows_the_error() {
        make(["/home/user/Films", "/home/user/Series"]);
        focused("folderRow-0");
        keyClick(Qt.Key_Right); focused("folderRemove-0");
        keyClick(Qt.Key_Return);
        focused("foldersConfirmCancel");
        keyClick(Qt.Key_Right); keyClick(Qt.Key_Return);
        view.row = Object.assign({}, view.row, {pending: true});
        view.row = Object.assign({}, view.row, {pending: false, error: "Timeout"});
        compare(view.folders.length, 2);
        verify(child("foldersToast").visible);
        compare(child("foldersToastText").text, "Arinn did not reply. Try again.");
        verify(child("foldersError").visible);
        compare(child("foldersError").text, "Arinn did not reply. Try again.");
    }
    // Review of #780: while a change is saved, Add and Remove explain
    // instead of starting a second change the catalog would drop.
    function test_pending_change_blocks_add_and_remove() {
        make(["/media/user/Disk/Movies"]);
        focused("folderRow-0");
        keyClick(Qt.Key_Up); focused("foldersAdd");
        keyClick(Qt.Key_Return);
        const picker = child("folderPicker");
        verify(picker.visible);
        tryCompare(browser, "path", "/home/user");
        const values = createTemporaryObject(spyFactory, suite, {target: view, signalName: "valuesRequested"});
        // The first save is still pending when a folder is chosen.
        view.row = Object.assign({}, view.row, {pending: true});
        tryCompare(findChild(picker, "pickerFolder-0"), "activeFocus", true);
        keyClick(Qt.Key_Return);
        tryCompare(browser, "path", "/home/user/Films");
        keyClick(Qt.Key_Up);
        tryCompare(findChild(picker, "pickerChoose"), "activeFocus", true);
        keyClick(Qt.Key_Return);
        compare(values.count, 0);
        verify(picker.visible);
        compare(picker.errorText, "Wait until the last change is saved");
        keyClick(Qt.Key_Escape); keyClick(Qt.Key_Escape); keyClick(Qt.Key_Escape);
        tryCompare(picker, "visible", false);
        focused("foldersAdd");
        keyClick(Qt.Key_Return);
        verify(!picker.visible);
        compare(child("foldersToastText").text, "Wait until the last change is saved");
        keyClick(Qt.Key_Down); keyClick(Qt.Key_Right); focused("folderRemove-0");
        keyClick(Qt.Key_Return);
        verify(!child("foldersConfirm").visible);
        compare(values.count, 0);
        view.row = Object.assign({}, view.row, {pending: false, enabled: false});
        keyClick(Qt.Key_Return);
        compare(child("foldersToastText").text, "These folders cannot be changed now");
    }
    // LF-2: no folders, Add focused.
    function test_empty_offers_add() {
        make([]);
        verify(child("foldersEmpty").visible);
        focused("foldersEmptyAdd");
    }
    // LF-5: at the limit Add stays focusable and explains.
    function test_limit_explains_instead_of_opening_the_picker() {
        make(["/a", "/b"], {row: {providerId: "library", id: "roots", type: "path", libraryFolders: true, currentValue: ["/a", "/b"],
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
        // Review of #780: the catalog keeps the old value while the save is
        // pending; the new folder shows at once, "Adding to the library…".
        view.row = Object.assign({}, view.row, {pending: true});
        focused("folderRow-1");
        compare(child("folderStatus-1").text, "Adding to the library…");
        compare(child("foldersCount").text, "2");
        view.row = Object.assign({}, view.row, {currentValue: ["/home/user/Films", "/media/user/Disk"], pending: false});
        focused("folderRow-1");
        verify(!child("folderStatus-1").visible);
    }
    // Review of #780: an added folder the daemon refuses leaves the list
    // with the error in words.
    function test_refused_add_leaves_the_list() {
        make(["/home/user/Series"]);
        focused("folderRow-0");
        keyClick(Qt.Key_Up); keyClick(Qt.Key_Return);
        const picker = child("folderPicker");
        tryCompare(browser, "path", "/home/user");
        keyClick(Qt.Key_Up);
        tryCompare(findChild(picker, "pickerChoose"), "activeFocus", true);
        keyClick(Qt.Key_Return);
        view.row = Object.assign({}, view.row, {pending: true});
        compare(view.folders.length, 2);
        view.row = Object.assign({}, view.row, {pending: false, error: "InvalidArgument"});
        compare(view.folders, ["/home/user/Series"]);
        compare(child("foldersToastText").text, "This value is not accepted.");
    }
    // Review of #780: "/home/user/" and "/home/user" are one folder.
    function test_trailing_slash_is_the_same_folder() {
        make(["/home/user/"]);
        focused("folderRow-0");
        keyClick(Qt.Key_Up); keyClick(Qt.Key_Return);
        const picker = child("folderPicker");
        tryCompare(browser, "path", "/home/user");
        keyClick(Qt.Key_Up);
        tryCompare(findChild(picker, "pickerChoose"), "activeFocus", true);
        keyClick(Qt.Key_Return);
        verify(picker.visible);
        compare(picker.errorText, "This folder is already in the library");
    }
    // Review of #780: another provider's folder list gets plain folder
    // wording and its own title, not the library's.
    function test_other_folder_lists_use_plain_wording() {
        make(["/srv/backup"], {row: {providerId: "backup", id: "dirs", type: "path", title: "Backup folders",
              description: "Folders to copy each night.", currentValue: ["/srv/backup", "/srv/gone"],
              constraints: {multiple: true, directory: true}, unavailablePaths: ["/srv/gone"]}});
        focused("folderRow-0");
        compare(child("foldersTitle").text, "Backup folders");
        compare(child("folderStatus-1").text, "Unavailable");
        keyClick(Qt.Key_Right); focused("folderRemove-0");
        keyClick(Qt.Key_Return);
        compare(child("foldersConfirmTitle").text, "Remove “backup” from the list?");
        keyClick(Qt.Key_Escape);
        view.row = Object.assign({}, view.row, {currentValue: []});
        tryCompare(child("foldersEmptyBody"), "text", "Folders to copy each night.");
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
    // Review of #780: Places failed before any folder opened; the picker
    // says so and Retry asks for the places again.
    function test_picker_without_places_offers_retry() {
        make([]);
        browser.places = [];
        browser.state = "unavailable";
        focused("foldersEmptyAdd");
        keyClick(Qt.Key_Return);
        const picker = child("folderPicker");
        verify(picker.visible);
        compare(browser.placeReads, 1);
        compare(findChild(picker, "pickerStatusTitle").text, "Folders cannot be read right now");
        tryCompare(findChild(picker, "pickerRetry"), "activeFocus", true);
        keyClick(Qt.Key_Return);
        compare(browser.placeReads, 2);
    }
    // Review of #780: a start point that ends in "/" joins its children
    // with one slash and still counts as the start point.
    function test_start_point_with_a_trailing_slash() {
        make([]);
        browser.places = [{kind: "home", path: "/home/user/", title: "user"}];
        browser.tree = Object.assign({}, browser.tree, {"/home/user/": ["Films"]});
        focused("foldersEmptyAdd");
        keyClick(Qt.Key_Return);
        const picker = child("folderPicker");
        tryCompare(browser, "path", "/home/user/");
        tryCompare(findChild(picker, "pickerFolder-0"), "activeFocus", true);
        keyClick(Qt.Key_Return);
        tryCompare(browser, "path", "/home/user/Films");
        verify(findChild(picker, "pickerPlaceDot-0").visible);
        keyClick(Qt.Key_Escape);
        tryCompare(browser, "path", "/home/user");
        // At the start point, Back goes to the start points.
        keyClick(Qt.Key_Escape);
        tryCompare(findChild(picker, "pickerPlace-0"), "activeFocus", true);
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
