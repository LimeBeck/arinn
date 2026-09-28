// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import QtTest
import "../../apps/arinn-shell/ui/screens/library" as Library

TestCase {
    id: suite
    name: "LibraryScreen"
    width: 1920
    height: 1080
    visible: true
    when: windowShown

    property var library
    Component {
        id: factory
        Library.LibraryView {}
    }
    SignalSpy { id: tabsSpy; signalName: "tabRequested" }
    SignalSpy { id: entriesSpy; signalName: "entryRequested" }
    SignalSpy { id: plays; signalName: "playRequested" }
    SignalSpy { id: episodes; signalName: "episodeRequested" }
    SignalSpy { id: removes; signalName: "removeRequested" }
    SignalSpy { id: retries; signalName: "retryRequested" }
    SignalSpy { id: backs; signalName: "backRequested" }
    SignalSpy { id: homes; signalName: "homeRequested" }

    function child(name) {
        const item = findChild(library, name);
        verify(item !== null, name);
        return item;
    }
    function focused(name) {
        tryCompare(child(name), "activeFocus", true, 5000, name);
    }
    function arm() {
        tabsSpy.target = library; tabsSpy.clear();
        entriesSpy.target = library; entriesSpy.clear();
        plays.target = library; plays.clear();
        episodes.target = library; episodes.clear();
        removes.target = library; removes.clear();
        retries.target = library; retries.clear();
        backs.target = library; backs.clear();
        homes.target = library; homes.clear();
    }
    function createLibrary(props) {
        // Qt Quick Test retains the pointer between fixtures. Park it outside
        // controls before creating the view so a queued hover cannot steal focus.
        mouseMove(suite, 10, 10);
        if (library)
            library.destroy();
        library = createTemporaryObject(factory, suite, props || {});
        verify(library);
        arm();
        return library;
    }
    function cleanup() {
        tabsSpy.target = null;
        entriesSpy.target = null;
        plays.target = null;
        episodes.target = null;
        removes.target = null;
        retries.target = null;
        backs.target = null;
        homes.target = null;
        if (library) {
            library.destroy();
            library = null;
        }
    }

    function catalogue() {
        return {
            columns: 4,
            tabs: [{id: "films", title: "Films"}, {id: "series", title: "Series"}],
            entries: [
                {id: "a", tabId: "films", title: "Alpha", subtitle: "2001", kind: "film"},
                {id: "b", tabId: "films", title: "Bravo", subtitle: "2002", kind: "film"},
                {id: "c", tabId: "films", title: "Charlie", subtitle: "2003", kind: "film", enabled: false},
                {id: "d", tabId: "films", title: "Delta", subtitle: "2004", kind: "film", missing: true},
                {id: "e", tabId: "films", title: "Echo", subtitle: "2005", kind: "film", progress: 0.4,
                    description: "A resumed entry.", episodes: [{id: "e1", title: "One"}, {id: "e2", title: "Two", missing: true}]},
                {id: "f", tabId: "films", title: "Foxtrot", subtitle: "2006", kind: "film"},
                {id: "g", tabId: "films", title: "Golf", subtitle: "2007", kind: "film"},
                {id: "h", tabId: "films", title: "Hotel", subtitle: "2008", kind: "film"},
                {id: "s", tabId: "series", title: "Serial", subtitle: "2010", kind: "series"}
            ]
        };
    }

    function test_focus_stays_inside_the_grid() {
        createLibrary(catalogue());
        focused("tab-films");
        compare(library.currentTabId, "films");
        for (let i = 0; i < 4; ++i)
            keyClick(Qt.Key_Left);
        focused("tab-films");
        keyClick(Qt.Key_Right);
        focused("tab-series");
        compare(library.currentTabId, "series");
        compare(tabsSpy.count, 1);
        compare(tabsSpy.signalArguments[0][0], "series");
        keyClick(Qt.Key_Down);
        focused("poster-s");
        compare(findChild(library, "poster-a"), null);
        keyClick(Qt.Key_Left);
        keyClick(Qt.Key_Right);
        keyClick(Qt.Key_Down);
        focused("poster-s");
        keyClick(Qt.Key_Up);
        focused("tab-series");
        keyClick(Qt.Key_Left);
        focused("tab-films");
        keyClick(Qt.Key_Down);
        focused("poster-a");
        keyClick(Qt.Key_Right);
        focused("poster-b");
        // Disabled Charlie is skipped. The row does not wrap onto the next line.
        keyClick(Qt.Key_Right);
        focused("poster-d");
        keyClick(Qt.Key_Right);
        focused("poster-d");
        keyClick(Qt.Key_Down);
        focused("poster-h");
        for (let i = 0; i < 5; ++i)
            keyClick(Qt.Key_Down);
        focused("poster-h");
        keyClick(Qt.Key_Right);
        focused("poster-h");
        keyClick(Qt.Key_Up);
        focused("poster-d");
        keyClick(Qt.Key_Up);
        focused("tab-films");
        for (let i = 0; i < 24; ++i)
            keyClick(Qt.Key_Tab);
        verify(library.activeFocus, "Tab left the library");
    }

    function test_detail_back_restores_poster_and_catalog_back_stays() {
        createLibrary(catalogue());
        focused("tab-films");
        keyClick(Qt.Key_Down);
        keyClick(Qt.Key_Right);
        focused("poster-b");
        keyClick(Qt.Key_Return);
        tryCompare(entriesSpy, "count", 1);
        compare(entriesSpy.signalArguments[0][0], "b");
        focused("detailPlay");
        compare(child("detailMissing").visible, false);
        keyClick(Qt.Key_Return);
        compare(plays.count, 1);
        compare(plays.signalArguments[0][0], "b");
        keyClick(Qt.Key_Backspace);
        focused("poster-b");
        compare(library.detailOpen, false);
        keyClick(Qt.Key_Escape);
        compare(backs.count, 1);
        focused("poster-b");
        keyClick(Qt.Key_Home);
        compare(homes.count, 1);
        focused("poster-b");
    }

    function test_missing_file_badge_and_remove_intent() {
        createLibrary(catalogue());
        focused("tab-films");
        keyClick(Qt.Key_Down);
        focused("poster-a");
        compare(child("missing-a").visible, false);
        compare(child("missing-d").visible, true);
        compare(child("missing-d").parent.text, "Delta");
        keyClick(Qt.Key_Right);
        keyClick(Qt.Key_Right);
        focused("poster-d");
        keyClick(Qt.Key_Return);
        focused("detailRemove");
        compare(child("detailPlay").visible, false);
        compare(child("detailMissing").visible, true);
        keyClick(Qt.Key_Return);
        compare(removes.count, 1);
        compare(removes.signalArguments[0][0], "d");
        compare(plays.count, 0);
        // Removal is an intent. The fixture entry stays until the caller updates it.
        compare(child("detailTitle").text, "Delta");
        keyClick(Qt.Key_Escape);
        focused("poster-d");
        compare(child("missing-d").visible, true);
    }

    function test_resume_and_episode_focus_stay_in_detail() {
        createLibrary(catalogue());
        focused("tab-films");
        keyClick(Qt.Key_Down);
        keyClick(Qt.Key_Down);
        focused("poster-e");
        keyClick(Qt.Key_Return);
        focused("detailPlay");
        compare(child("detailPlay").text, "Continue");
        compare(child("detailProgress").visible, true);
        compare(child("detailMissing").visible, false);
        keyClick(Qt.Key_Down);
        focused("episode-e1");
        compare(child("episode-missing-e1").visible, false);
        keyClick(Qt.Key_Right);
        focused("episode-e2");
        compare(child("episode-missing-e2").visible, true);
        keyClick(Qt.Key_Right);
        focused("episode-e2");
        keyClick(Qt.Key_Return);
        compare(episodes.count, 1);
        compare(episodes.signalArguments[0][0], "e");
        compare(episodes.signalArguments[0][1], "e2");
        keyClick(Qt.Key_Up);
        focused("detailPlay");
        keyClick(Qt.Key_Left);
        focused("detailPlay");
    }

    function test_states_empty_loading_error_unavailable() {
        createLibrary({libraryState: "loading", tabs: [{id: "films", title: "Films"}]});
        focused("tab-films");
        compare(child("librarySkeletons").visible, true);
        compare(child("skeleton-0").visible, true);
        compare(child("skeleton-7").visible, true);
        compare(child("libraryStatus").visible, false);
        compare(child("libraryGrid").visible, false);

        library.libraryState = "empty";
        focused("tab-films");
        compare(child("librarySkeletons").visible, false);
        compare(child("libraryStatus").visible, true);
        compare(child("libraryStatus").kind, "empty");
        keyClick(Qt.Key_Down);
        focused("statusAction");
        keyClick(Qt.Key_Return);
        compare(retries.count, 1);

        retries.clear();
        library.libraryState = "error";
        compare(child("libraryStatus").kind, "error");
        focused("statusAction");
        keyClick(Qt.Key_Return);
        compare(retries.count, 1);

        library.libraryState = "unavailable";
        compare(child("libraryStatus").kind, "unavailable");
        focused("statusAction");
        keyClick(Qt.Key_Up);
        focused("tab-films");
        keyClick(Qt.Key_Down);
        focused("statusAction");
        for (let i = 0; i < 4; ++i)
            keyClick(Qt.Key_Down);
        focused("statusAction");

        library.libraryState = "ready";
        library.entries = [{id: "a", tabId: "films", title: "Alpha"}];
        focused("tab-films");
        keyClick(Qt.Key_Down);
        focused("poster-a");
        compare(child("libraryStatus").visible, false);
        compare(child("missing-a").visible, false);
    }

    function test_removed_poster_focuses_previous() {
        createLibrary(catalogue());
        focused("tab-films");
        keyClick(Qt.Key_Down);
        keyClick(Qt.Key_Right);
        focused("poster-b");
        library.entries = [
            {id: "a", tabId: "films", title: "Alpha"},
            {id: "d", tabId: "films", title: "Delta", missing: true},
            {id: "e", tabId: "films", title: "Echo"}
        ];
        focused("poster-a");
    }
}
