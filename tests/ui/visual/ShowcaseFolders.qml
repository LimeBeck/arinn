// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import "../../../apps/arinn-shell/ui/screens/settings" as Settings

// Production folder screens with a synthetic Folders1-shaped model. No disk access.
Item {
    id: root
    width: 1920; height: 1080
    property string variant: "list"
    property alias view: folders
    readonly property var paths: variant === "empty" ? []
        : variant === "limit" ? ["/srv/films", "/srv/series", "/srv/archive", "/srv/shorts"]
        : variant === "unavailable" || variant === "remove-unavailable"
            ? ["/srv/films", "/srv/series", "/mnt/archive"]
            : ["/srv/films", "/srv/series"]
    property var browser: QtObject {
        id: folderBrowser
        property var places: [{kind: "home", path: "/home/fixture", title: "Home"},
                              {kind: "mount", path: "/media/archive", title: "Archive"},
                              {kind: "root", path: "/", title: "/"}]
        property string path: ""
        property var folders: []
        property string state: "loading"
        function refreshPlaces() {}
        function open(value) {
            path = value;
            folderBrowser.folders = value === "/home/fixture" ? ["Films", "Music", "Series"]
                : value === "/" ? ["home", "media", "srv"] : [];
            state = folderBrowser.folders.length ? "ready" : "empty";
        }
        function up() { open(path === "/home/fixture" ? "/home" : "/"); }
    }
    Settings.LibraryFoldersView {
        id: folders
        anchors.fill: parent
        browser: root.browser
        row: ({providerId: "fixture-library", id: "roots", type: "path", libraryFolders: true,
               currentValue: root.paths, constraints: {multiple: true, directory: true, maxItems: root.variant === "limit" ? 4 : 64},
               unavailablePaths: root.variant === "unavailable" || root.variant === "remove-unavailable" ? ["/mnt/archive"] : []})
    }
    function show() {
        folders.open();
        if (variant === "picker") folders.add();
        else if (variant === "remove" || variant === "remove-unavailable")
            folders.askRemove(variant === "remove" ? "/srv/films" : "/mnt/archive");
        else if (variant === "limit") { folders.side = "add"; folders.restore(); }
        else if (variant === "adding") {
            folders.chosen("/srv/archive");
            folders.row = Object.assign({}, folders.row, {pending: true});
        }
    }
}
