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
        compare(child("detailTitle").font.pixelSize, 112);
        compare(child("detailTitle").role, "display");
        compare(child("detailBackdropMark").text, "backdrop · Bravo");
        compare(child("detailEyebrow").text, "Film");
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

    function test_back_key_closes_detail_then_requests_back() {
        createLibrary(catalogue());
        focused("tab-films");
        keyClick(Qt.Key_Down);
        focused("poster-a");
        keyClick(Qt.Key_Return);
        focused("detailPlay");
        keyClick(Qt.Key_Back);
        focused("poster-a");
        compare(library.detailOpen, false);
        compare(backs.count, 0);
        keyClick(Qt.Key_Back);
        compare(backs.count, 1);
        focused("poster-a");
    }

    function test_missing_file_badge_and_remove_intent() {
        createLibrary(catalogue());
        focused("tab-films");
        keyClick(Qt.Key_Down);
        focused("poster-a");
        compare(child("missing-a").visible, false);
        compare(child("missing-d").visible, true);
        compare(child("missing-d").parent.text, "Delta");
        compare(child("missingLabel-d").text, "No file");
        compare(child("missing-d").height, 40);
        verify(child("missing-d").y > child("poster-d").height * 0.7);
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
        compare(child("episode-e1").width, 420);
        compare(child("episode-e1").height, 236);
        compare(child("episode-e1").cornerRadius, 22);
        compare(child("episodeMark-e1").text, "still");
        compare(findChild(library, "season-s1"), null);
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
        compare(child("skeleton-0").width, 210);
        compare(child("skeleton-0").height, 315);
        compare(child("skeleton-0").radius, 20);
        compare(child("skeleton-7").visible, true);
        const skeletons = child("librarySkeletons").mapToItem(library, 0, 0);
        compare(skeletons.x, 120);
        compare(skeletons.y, 310);
        verify(findChild(library, "skeleton-13") !== null);
        compare(findChild(library, "skeleton-14"), null);
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

    function ember(color) {
        verify(Math.abs(color.r - 1) < 0.02);
        verify(Math.abs(color.g - 138 / 255) < 0.02);
        verify(Math.abs(color.b - 61 / 255) < 0.02);
    }

    function test_default_columns_are_seven() {
        const entries = [];
        for (let i = 0; i < 8; ++i)
            entries.push({id: "p" + i, tabId: "films", title: "Plate " + i, year: String(2000 + i)});
        createLibrary({tabs: [{id: "films", title: "Films"}], entries: entries});
        compare(library.columnCount, 7);
        compare(library.width, 1920);
        compare(library.height, 1080);
        tryCompare(child("poster-p1").parent, "x", 250);
        tryCompare(child("poster-p7").parent, "y", 427 + 48);
        const first = child("poster-p0").mapToItem(library, 0, 0);
        const second = child("poster-p1").mapToItem(library, 0, 0);
        const eighth = child("poster-p7").mapToItem(library, 0, 0);
        compare(first.x, 120);
        compare(first.y, 310);
        compare(second.x, 120 + 210 + 40);
        compare(second.y, first.y);
        compare(eighth.x, first.x);
        compare(eighth.y, first.y + 427 + 48);
        compare(child("posterYear-p0").text, "2000");
        compare(child("posterYear-p0").font.pixelSize, 24);
    }

    function test_handoff_grid_frame() {
        createLibrary(catalogue());
        compare(library.columnCount, 4);
        const heading = child("libraryHeading");
        compare(heading.x, 120);
        compare(heading.y, 96);
        compare(heading.font.pixelSize, 56);
        compare(heading.font.weight, 700);
        const tab = child("tab-films");
        compare(tab.height, 64);
        compare(tab.cornerRadius, 32);
        const tabAt = tab.mapToItem(library, 0, 0);
        compare(tabAt.x, 120);
        compare(tabAt.y, 190);
        compare(child("tabLabel-films").font.pixelSize, 28);
        compare(child("tabUnderline-films").visible, true);
        compare(child("tabUnderline-series").visible, false);
        ember(child("tabUnderline-films").color);
        compare(child("libraryStage").y, 282);
        const poster = child("poster-a");
        compare(poster.width, 210);
        compare(poster.height, 315);
        compare(poster.cornerRadius, 20);
        const at = poster.mapToItem(library, 0, 0);
        compare(at.x, 120);
        compare(at.y, 310);
        tryCompare(child("poster-b").parent, "x", 250);
        const beside = child("poster-b").mapToItem(library, 0, 0);
        compare(beside.x, 370);
        compare(beside.y, 310);
        tryCompare(child("poster-e").parent, "y", 427 + 48);
        const below = child("poster-e").mapToItem(library, 0, 0);
        compare(below.x, 120);
        compare(below.y, 310 + 427 + 48);
        compare(child("posterMark-a").text, "poster");
        compare(child("posterMark-a").visible, true);
        compare(child("posterPlate-a").visible, false);
        compare(child("posterYear-a").text, "2001");
        compare(child("posterYear-a").font.pixelSize, 24);
        compare(child("posterLabel-a").font.pixelSize, 26);
        focused("tab-films");
        keyClick(Qt.Key_Down);
        focused("poster-a");
        verify(child("posterCaption-a").lift < 0);
        library.pointerMode = false;
        compare(child("posterCaption-a").lift, -10);
        library.pointerMode = true;
        compare(child("posterCaption-a").lift, -4);
        tryVerify(function() { return child("poster-a").scale > 1.01; });
        keyClick(Qt.Key_Up);
        focused("tab-films");
        keyClick(Qt.Key_Right);
        focused("tab-series");
        compare(child("tabUnderline-series").visible, true);
        compare(child("tabUnderline-films").visible, false);
        compare(child("posterKind-s").visible, true);
        compare(child("posterKindLabel-s").text, "Series");
    }

    function test_no_art_poster_keeps_the_title() {
        createLibrary({
            tabs: [{id: "films", title: "Films"}],
            entries: [{id: "n", tabId: "films", title: "Noir", year: "1999", noArt: true, kind: "film"}]
        });
        compare(child("posterMark-n").visible, false);
        compare(child("posterPlate-n").visible, true);
        compare(child("posterArtTitle-n").text, "Noir");
        compare(child("posterGlyph-n").width, 44);
        compare(child("posterLabel-n").text, "Noir");
        compare(child("posterYear-n").text, "1999");
        compare(child("missing-n").visible, false);
    }

    function test_series_detail_has_seasons_and_stills() {
        createLibrary({
            tabs: [{id: "series", title: "Series"}],
            entries: [{
                id: "show",
                tabId: "series",
                title: "Serial",
                subtitle: "2011",
                kind: "series",
                description: "A seasonal entry.",
                seasons: [{id: "s1", title: "Season 1"}, {id: "s2", title: "Season 2"}],
                episodes: [
                    {id: "p1", seasonId: "s1", title: "Pilot", duration: "40 min", number: 1},
                    {id: "p2", seasonId: "s2", title: "Next", duration: "41 min", number: 1, missing: true}
                ]
            }]
        });
        focused("tab-series");
        keyClick(Qt.Key_Down);
        focused("poster-show");
        keyClick(Qt.Key_Return);
        focused("detailPlay");
        compare(child("detailTitle").font.pixelSize, 112);
        compare(child("detailTitle").text, "Serial");
        compare(child("detailBackdropMark").text, "backdrop · Serial");
        compare(child("detailEyebrow").text, "Series");
        compare(child("detailMeta").text, "2011");
        compare(child("detailMeta").font.pixelSize, 30);
        keyClick(Qt.Key_Down);
        focused("season-s1");
        ember(child("seasonUnderline-s1").color);
        compare(child("seasonUnderline-s2").visible, false);
        compare(child("episode-p1").width, 420);
        compare(child("episode-p1").height, 236);
        compare(child("episodeMark-p1").text, "still");
        compare(child("episodeDuration-p1").text, "40 min");
        compare(findChild(library, "episode-p2"), null);
        keyClick(Qt.Key_Down);
        focused("episode-p1");
        keyClick(Qt.Key_Right);
        focused("episode-p1");
        keyClick(Qt.Key_Up);
        focused("season-s1");
        keyClick(Qt.Key_Right);
        focused("season-s2");
        compare(child("seasonUnderline-s2").visible, true);
        compare(findChild(library, "episode-p1"), null);
        keyClick(Qt.Key_Down);
        focused("episode-p2");
        compare(child("episode-missing-p2").visible, true);
        keyClick(Qt.Key_Up);
        focused("season-s2");
        keyClick(Qt.Key_Up);
        focused("detailPlay");
    }

    function test_operation_waiting_and_failure() {
        createLibrary(catalogue());
        focused("tab-films");
        keyClick(Qt.Key_Down);
        focused("poster-a");
        keyClick(Qt.Key_Return);
        focused("detailPlay");
        library.launchingId = "a";
        library.operationState = "pending";
        compare(child("detailPlay").busy, true);
        compare(child("operationPending").visible, true);
        compare(child("operationBanner").visible, false);
        library.operationState = "Unavailable";
        compare(child("operationPending").visible, false);
        compare(child("operationBanner").visible, true);
        compare(child("operationBannerText").text, "This title is not available.");
        library.operationState = "succeeded";
        compare(child("operationBanner").visible, false);
        compare(child("operationPending").visible, false);
    }

    function test_episode_still_is_busy_while_its_launch_is_pending() {
        createLibrary({
            tabs: [{id: "series", title: "Series"}],
            entries: [{
                id: "show",
                tabId: "series",
                title: "Serial",
                kind: "series",
                seasons: [{id: "s1", title: "Season 1"}],
                episodes: [
                    {id: "p1", seasonId: "s1", title: "Pilot", number: 1},
                    {id: "p2", seasonId: "s1", title: "Next", number: 2}
                ]
            }]
        });
        focused("tab-series");
        keyClick(Qt.Key_Down);
        keyClick(Qt.Key_Return);
        focused("detailPlay");
        keyClick(Qt.Key_Down);
        focused("season-s1");
        keyClick(Qt.Key_Down);
        focused("episode-p1");
        library.launchingId = "p2";
        library.operationState = "pending";
        compare(child("episode-p2").busy, true);
        compare(child("episode-p1").busy, false);
        compare(child("detailPlay").busy, true);
        keyClick(Qt.Key_Right);
        focused("episode-p2");
        episodes.clear();
        keyClick(Qt.Key_Return);
        compare(episodes.count, 0);
    }
}
