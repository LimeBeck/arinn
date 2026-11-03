// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import QtTest
import "../../apps/arinn-shell/ui" as Ui
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
    Component { id: otherFocusFactory; Item { width: 1; height: 1 } }
    SignalSpy { id: tabsSpy; signalName: "tabRequested" }
    SignalSpy { id: entriesSpy; signalName: "entryRequested" }
    SignalSpy { id: plays; signalName: "playRequested" }
    SignalSpy { id: episodes; signalName: "episodeRequested" }
    SignalSpy { id: removes; signalName: "removeRequested" }
    SignalSpy { id: retries; signalName: "retryRequested" }
    SignalSpy { id: backs; signalName: "backRequested" }
    SignalSpy { id: homes; signalName: "homeRequested" }

    function test_selected_description_is_plain_text_and_scoped_to_its_entry() {
        createLibrary(catalogue());
        library.tabs = [{id: "films", title: "Films", kind: "film"}];
        library.entries = [{id: "a", tabId: "films", kind: "film", title: "First"},
                           {id: "b", tabId: "films", kind: "film", title: "Second"}];
        library.openEntry("a");
        library.descriptionEntryId = "a";
        library.descriptionText = "Русский <b>текст</b> & описание";
        tryCompare(child("detailDescription"), "text", library.descriptionText);
        compare(child("detailDescription").textFormat, Text.PlainText);
        library.openEntry("b");
        compare(child("detailDescription").text, "");
        library.descriptionEntryId = "b";
        library.descriptionText = "Second local description";
        compare(child("detailDescription").text, "Second local description");
    }

    function child(name) {
        const item = findChild(library, name);
        verify(item !== null, name);
        return item;
    }
    // The handoff prototype labels its stand-in art "poster", "backdrop" and
    // "still"; those labels are not part of the UI (#610).
    function wireframeLabels(item) {
        let found = [];
        if (!item || !item.visible)
            return found;
        if (item.text !== undefined && /^(poster|still|backdrop\b)/i.test(String(item.text)))
            found.push(String(item.text));
        for (let i = 0; i < item.children.length; ++i)
            found = found.concat(wireframeLabels(item.children[i]));
        return found;
    }
    function noWireframeLabels() {
        compare(wireframeLabels(library), []);
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
        Ui.InputMode.reset();
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
        // Hearth light: the detail title is a content role in Ysabeau (ADR-0136).
        compare(child("detailTitle").font.pixelSize, 104);
        compare(child("detailTitle").role, "detail");
        compare(child("detailTitle").font.family, Ui.Theme.contentFontFamily);
        compare(child("detailFanart").visible, false);
        noWireframeLabels();
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
        compare(child("episodePlate-e1").visible, true);
        compare(child("episodeGlyph-e1").width, 44);
        noWireframeLabels();
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
        compare(heading.font.weight, 600);
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
        // No artwork URL: the generic plate with the glyph and the title.
        compare(findChild(library, "posterMark-a"), null);
        compare(child("posterPlate-a").visible, true);
        compare(child("posterArtTitle-a").text, "Alpha");
        noWireframeLabels();
        compare(child("posterYear-a").text, "2001");
        compare(child("posterYear-a").font.pixelSize, 24);
        compare(child("posterLabel-a").font.pixelSize, 26);
        focused("tab-films");
        keyClick(Qt.Key_Down);
        focused("poster-a");
        verify(child("posterCaption-a").lift > 0);
        library.pointerMode = false;
        compare(child("posterCaption-a").lift, 14);
        library.pointerMode = true;
        compare(child("posterCaption-a").lift, 6);
        tryVerify(function() { return child("poster-a").scale > 1.01; });
        keyClick(Qt.Key_Up);
        focused("tab-films");
        keyClick(Qt.Key_Right);
        focused("tab-series");
        compare(child("tabUnderline-series").visible, false);
        compare(child("tabUnderline-films").visible, false);
        compare(child("posterKind-s").visible, true);
        compare(child("posterKindLabel-s").text, "Series");
    }

    function test_no_art_poster_keeps_the_title() {
        createLibrary({
            tabs: [{id: "films", title: "Films"}],
            entries: [{id: "n", tabId: "films", title: "Noir", year: "1999", noArt: true, kind: "film"}]
        });
        compare(findChild(library, "posterMark-n"), null);
        compare(child("posterPlate-n").visible, true);
        compare(child("posterArtTitle-n").text, "Noir");
        compare(child("posterGlyph-n").width, 44);
        compare(child("posterLabel-n").text, "Noir");
        compare(child("posterYear-n").text, "1999");
        compare(child("missing-n").visible, false);
        noWireframeLabels();
        // An artwork URL next to noArt does not replace the plate.
        createLibrary({
            tabs: [{id: "films", title: "Films"}],
            entries: [{id: "n", tabId: "films", title: "Noir", year: "1999", noArt: true, kind: "film",
                       artwork: Qt.resolvedUrl("../../apps/arinn-shell/gallery/fixture-artwork.svg")}]
        });
        compare(child("posterImage-n").source.toString(), "");
        compare(child("posterPlate-n").visible, true);
        compare(child("posterArtTitle-n").text, "Noir");
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
        compare(child("detailTitle").font.pixelSize, 104);
        compare(child("detailTitle").text, "Serial");
        compare(child("detailFanart").visible, false);
        compare(child("detailEyebrow").text, "Series");
        compare(child("detailMeta").text, "2011");
        compare(child("detailMeta").font.pixelSize, 30);
        keyClick(Qt.Key_Down);
        focused("season-s1");
        tryVerify(() => Math.abs(child("detailRail").mapToItem(library, 0, 0).y - 170) < 1);
        ember(child("seasonUnderline-s1").color);
        compare(child("seasonUnderline-s2").visible, false);
        compare(child("episode-p1").width, 420);
        compare(child("episode-p1").height, 236);
        compare(child("episodePlate-p1").visible, true);
        noWireframeLabels();
        compare(child("episodeDuration-p1").text, "40 min");
        compare(findChild(library, "episode-p2"), null);
        keyClick(Qt.Key_Down);
        focused("episode-p1");
        verify(child("episodeDuration-p1").mapToItem(library, 0, child("episodeDuration-p1").height).y < library.height);
        keyClick(Qt.Key_Right);
        focused("episode-p1");
        keyClick(Qt.Key_Up);
        focused("season-s1");
        keyClick(Qt.Key_Right);
        focused("season-s2");
        compare(child("seasonUnderline-s2").visible, false);
        compare(findChild(library, "episode-p1"), null);
        keyClick(Qt.Key_Down);
        focused("episode-p2");
        compare(child("episode-missing-p2").visible, true);
        keyClick(Qt.Key_Up);
        focused("season-s2");
        keyClick(Qt.Key_Up);
        focused("detailPlay");
    }

    function test_long_series_episode_navigation_data() {
        return [{tag: "season", withSeason: true}, {tag: "no-season", withSeason: false}];
    }

    function test_long_series_episode_navigation(data) {
        const list = [];
        for (let i = 0; i < 12; ++i)
            list.push({id: "ep" + i, title: "Episode " + (i + 1), seasonId: data.withSeason ? "s1" : "",
                duration: "42 min", number: i + 1});
        createLibrary({tabs: [{id: "series", title: "Series"}], entries: [{id: "show", tabId: "series",
            title: "A very long synthetic series title that wraps across two lines on the detail screen",
            description: "A synthetic description that adds height above the episodes. ".repeat(12),
            kind: "series", year: 2026, progress: 0.4,
            seasons: data.withSeason ? [{id: "s1", title: "Season 1"}] : [], episodes: list}]});
        library.openEntry("show");
        focused("detailPlay");
        keyClick(Qt.Key_Down);
        if (data.withSeason) {
            focused("season-s1");
            keyClick(Qt.Key_Down);
        }
        function visibleEpisode(index) {
            focused("episode-ep" + index);
            const cell = child("episode-ep" + index).parent;
            const caption = child("episodeDuration-ep" + index);
            tryVerify(() => {
                // Check layout bounds; the Surface's decorative focus scale
                // may extend beyond its cell without changing rail position.
                const top = cell.mapToItem(library, 0, 0);
                const right = cell.mapToItem(library, cell.width, 0);
                const bottom = caption.mapToItem(library, caption.width, caption.height);
                return top.x >= 0 && top.y >= 0 && right.x <= library.width
                        && bottom.y <= library.height;
            }, 1500, "Episode " + index + " and caption must be inside the viewport");
        }
        for (let i = 0; i < list.length; ++i) {
            visibleEpisode(i);
            if (i + 1 < list.length)
                keyClick(Qt.Key_Right);
        }
        keyClick(Qt.Key_Return);
        compare(episodes.count, 1);
        compare(episodes.signalArguments[0][1], "ep11");
        for (let i = list.length - 2; i >= 0; --i) {
            keyClick(Qt.Key_Left);
            visibleEpisode(i);
        }
        keyClick(Qt.Key_Up);
        if (data.withSeason) {
            focused("season-s1");
            keyClick(Qt.Key_Up);
        }
        focused("detailPlay");
        tryVerify(() => child("detailPlay").mapToItem(library, 0, 0).y >= 0);
        keyClick(Qt.Key_Escape);
        focused("poster-show");
    }

    function createAnimatedEpisodeRail() {
        const list = [];
        for (let season = 1; season <= 2; ++season)
            for (let i = 0; i < 12; ++i)
                list.push({id: "s" + season + "e" + i, title: "Episode " + i, seasonId: "s" + season});
        createLibrary({tabs: [{id: "series", title: "Series"}], entries: [
            {id: "show", tabId: "series", title: "Show", kind: "series",
                seasons: [{id: "s1", title: "Season 1"}, {id: "s2", title: "Season 2"}], episodes: list},
            {id: "other", tabId: "series", title: "Other", kind: "series", episodes: [{id: "only", title: "Only"}]}]});
        library.openEntry("show"); focused("detailPlay");
        keyClick(Qt.Key_Down); focused("season-s1");
        keyClick(Qt.Key_Down); focused("episode-s1e0");
    }

    function test_episode_scroll_has_intermediate_positions_and_settles() {
        createAnimatedEpisodeRail();
        const rail = child("episodeScroller");
        for (let i = 0; i < 4; ++i) keyClick(Qt.Key_Right);
        focused("episode-s1e4");
        tryVerify(() => rail.contentX > 1, 250);
        const first = rail.contentX;
        wait(30);
        verify(rail.contentX > first, "The row must visibly move through intermediate positions, not jump");
        wait(500);
        const target = rail.contentX;
        verify(target > first);
        wait(50);
        fuzzyCompare(rail.contentX, target, 0.5, "The row must settle after revealing the selected episode");
        for (let i = 0; i < 4; ++i) keyClick(Qt.Key_Left);
        focused("episode-s1e0");
        tryVerify(() => rail.contentX > 1 && rail.contentX < target - 1, 250,
                  "Reverse scrolling must also have intermediate positions");
        tryVerify(() => Math.abs(rail.contentX) < 0.5, 1000);
        for (let i = 0; i < 20; ++i) keyClick(Qt.Key_Right);
        focused("episode-s1e11");
        tryVerify(() => Math.abs(rail.contentX - (rail.contentWidth - rail.width)) < 0.5, 1000);
        keyClick(Qt.Key_Return);
        compare(episodes.count, 1);
        compare(episodes.signalArguments[0][1], "s1e11");
    }

    function test_episode_scroll_retargets_and_resets_for_new_content() {
        createAnimatedEpisodeRail();
        const rail = child("episodeScroller");
        for (let i = 0; i < 4; ++i) keyClick(Qt.Key_Right);
        tryVerify(() => rail.contentX > 1, 250);
        keyClick(Qt.Key_Right);
        keyClick(Qt.Key_Left); keyClick(Qt.Key_Left);
        focused("episode-s1e3");
        // Reversing focus can retarget the running animation. Observe its
        // settled position rather than sampling an intermediate frame.
        wait(550);
        const stopped = rail.contentX;
        wait(100);
        fuzzyCompare(rail.contentX, stopped, 0.5);
        keyClick(Qt.Key_Right);
        tryVerify(() => rail.contentX > stopped + 1, 250);
        keyClick(Qt.Key_Up); focused("season-s1");
        keyClick(Qt.Key_Right); focused("season-s2");
        compare(rail.contentX, 0);
        wait(550); compare(rail.contentX, 0);
        keyClick(Qt.Key_Down); focused("episode-s2e0");
        for (let i = 0; i < 4; ++i) keyClick(Qt.Key_Right);
        tryVerify(() => rail.contentX > 1, 250);
        library.openEntry("other"); focused("detailPlay");
        compare(rail.contentX, 0);
        wait(550); compare(rail.contentX, 0);
        library.openEntry("show");
        verify(waitForRendering(library));
        focused("detailPlay");
        keyClick(Qt.Key_Down); keyClick(Qt.Key_Down);
        focused("episode-s1e0");
        compare(rail.contentX, 0);
    }

    function test_pointer_flick_interrupts_episode_focus_animation() {
        createAnimatedEpisodeRail();
        const rail = child("episodeScroller");
        for (let i = 0; i < 4; ++i) keyClick(Qt.Key_Right);
        tryVerify(() => rail.contentX > 1, 250);
        rail.flick(-1200, 0);
        tryCompare(rail, "flicking", true);
        wait(40);
        rail.cancelFlick();
        const stopped = rail.contentX;
        wait(550);
        fuzzyCompare(rail.contentX, stopped, 0.5);
    }

    readonly property string roundedArtworkFixture: "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAABQAAAAeCAIAAACjcKk8AAAAHElEQVR4nGP4TwFgGNU8qnlU86jmUc2jmoezZgD1FQFiOyY3EQAAAABJRU5ErkJggg=="
    function test_loaded_artwork_corners_data() {
        return [{tag: "poster", episode: false, focused: false},
                {tag: "poster-focused", episode: false, focused: true},
                {tag: "episode", episode: true, focused: false},
                {tag: "episode-focused", episode: true, focused: true},
                {tag: "last-episode-focused", episode: true, focused: true, last: true}];
    }
    function createRoundedArtwork(data) {
        const list = [];
        for (let i = 0; i < (data.last ? 12 : 1); ++i)
            list.push({id: i ? "rounded-ep" + i : "rounded-ep", title: "Episode " + i,
                       seasonId: "s1", artwork: roundedArtworkFixture});
        createLibrary({tabs: [{id: "series", title: "Series"}], entries: [{id: "rounded", tabId: "series",
            title: "Rounded", kind: "series", artwork: roundedArtworkFixture,
            seasons: [{id: "s1", title: "Season 1"}],
            episodes: list}]});
        let artwork;
        if (data.episode) {
            library.openEntry("rounded"); focused("detailPlay");
            keyClick(Qt.Key_Down); focused("season-s1");
            if (data.focused) { keyClick(Qt.Key_Down); focused("episode-rounded-ep"); }
            if (data.last) {
                for (let i = 0; i < 11; ++i) keyClick(Qt.Key_Right);
                focused("episode-rounded-ep11");
            }
            artwork = child(data.last ? "episodeImage-rounded-ep11" : "episodeImage-rounded-ep");
        } else {
            focused("tab-series");
            if (data.focused) { keyClick(Qt.Key_Down); focused("poster-rounded"); }
            artwork = child("posterImage-rounded");
        }
        return artwork;
    }
    function test_loaded_artwork_corners(data) {
        const artwork = createRoundedArtwork(data);
        tryCompare(artwork, "status", Image.Ready);
        wait(550); // Finish focus scale and the detail rail's existing reveal.
        function whiteAt(pixels, x, y) {
            const at = artwork.mapToItem(suite, x, y);
            if (at.x < 0 || at.y < 0 || at.x >= pixels.width || at.y >= pixels.height) return false;
            return pixels.red(Math.round(at.x), Math.round(at.y)) > 240
                    && pixels.green(Math.round(at.x), Math.round(at.y)) > 240
                    && pixels.blue(Math.round(at.x), Math.round(at.y)) > 240;
        }
        tryVerify(() => whiteAt(grabImage(suite), artwork.width / 2, artwork.height / 2),
                  2000, "Loaded artwork must actually render before testing its corners");
        const pixels = grabImage(suite);
        for (const point of [[1, 1], [artwork.width - 2, 1], [1, artwork.height - 2],
                             [artwork.width - 2, artwork.height - 2]])
            verify(!whiteAt(pixels, point[0], point[1]), "Loaded artwork must not bleed into a rounded corner");
        // The grid intentionally fades its top edge. Episode stills must
        // retain their top/left edges even when focus scales and lifts them.
        const edgeY = data.focused && !data.episode ? artwork.height - 6 : 6;
        verify(whiteAt(pixels, artwork.width / 2, edgeY), "Straight edges must retain the artwork");
        verify(whiteAt(pixels, 6, artwork.height / 2), "Straight edges must retain the artwork");
        verify(whiteAt(pixels, artwork.width - 6, artwork.height / 2), "The focused right edge must stay inside the viewport");
        verify(whiteAt(pixels, artwork.width / 2, artwork.height - 6), "The bottom edge must retain the artwork");
    }

    readonly property string roundedPortraitFixture: "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAABQAAAAeCAIAAACjcKk8AAAAI0lEQVR4nGNgYPhPARrVPKqZEBqSjh7VPIQ0D1mHj2oeGpoB5H9Vx748njcAAAAASUVORK5CYII="
    readonly property string roundedLandscapeFixture: "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAB4AAAAUCAIAAAAVyRqTAAAAJklEQVR4nGNgYPiPB+GTY2DAr5eA9KjRo0aPGj1q9KjRo0YPTqMBN4hVx7XADNYAAAAASUVORK5CYII="
    function test_rounded_artwork_crop_and_source_recovery_data() {
        return [{tag: "poster", episode: false, focused: false},
                {tag: "episode", episode: true, focused: false}];
    }
    function test_rounded_artwork_crop_and_source_recovery(data) {
        const artwork = createRoundedArtwork(data);
        const plate = child(data.episode ? "episodePlate-rounded-ep" : "posterPlate-rounded");
        function sample(fx, fy) {
            const pixels = grabImage(suite);
            const at = artwork.mapToItem(suite, artwork.width * fx, artwork.height * fy);
            return {r: pixels.red(Math.round(at.x), Math.round(at.y)),
                    g: pixels.green(Math.round(at.x), Math.round(at.y)),
                    b: pixels.blue(Math.round(at.x), Math.round(at.y))};
        }
        function isWhite() { const p = sample(0.5, 0.5); return p.r > 240 && p.g > 240 && p.b > 240; }
        function croppedRed() {
            // Stretching would put the blue outer third at these points;
            // PreserveAspectCrop keeps the red centre of the source.
            const p = sample(data.episode ? 0.5 : 0.2, data.episode ? 0.15 : 0.5);
            return p.r > 200 && p.b < 50;
        }
        wait(550);
        tryVerify(isWhite);
        artwork.source = data.episode ? roundedPortraitFixture : roundedLandscapeFixture;
        tryVerify(croppedRed, 2000, "Rounded artwork must retain centre cropping");
        compare(plate.visible, false);
        artwork.source = "";
        tryCompare(artwork, "status", Image.Null);
        tryCompare(plate, "visible", true);
        tryVerify(() => !isWhite() && !croppedRed());
        artwork.source = "data:image/png;base64,broken";
        tryCompare(artwork, "status", Image.Error);
        compare(plate.visible, true);
        verify(!isWhite() && !croppedRed());
        artwork.source = roundedArtworkFixture;
        tryVerify(isWhite, 2000, "A source used before the error must load again");
        compare(plate.visible, false);
    }

    function test_back_from_season_rail_returns_to_primary_action_before_grid() {
        createLibrary({tabs: [{id: "series", title: "Series"}], entries: [{id: "show", tabId: "series", title: "Show", kind: "series",
            seasons: [{id: "s1", title: "Season 1"}], episodes: [{id: "ep", title: "One", seasonId: "s1"}]}]});
        library.openEntry("show"); focused("detailPlay");
        keyClick(Qt.Key_Down); focused("season-s1");
        keyClick(Qt.Key_Down); focused("episode-ep");
        keyClick(Qt.Key_Escape); focused("detailPlay");
        compare(library.detailEntryId, "show");
        compare(library.detailRailActive, false);
        keyClick(Qt.Key_Escape); focused("poster-show");
        compare(backs.count, 0);
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
    function foldersFixture() {
        return {
            tabs: [{id: "films", title: "Films"}, {id: "series", title: "Series"}, {id: "@folders", title: "Folders"}],
            currentTabId: "@folders",
            entries: [{id: "film", tabId: "films", title: "North", kind: "film", progress: 0.25}],
            folderEntries: [
                {id: "root", folderId: "", title: "Media", kind: "folder"},
                {id: "offline", folderId: "", title: "Archive", kind: "folder", missing: true},
                {id: "nested", folderId: "root", title: "Nested", kind: "folder"},
                {id: "empty", folderId: "root", title: "Empty", kind: "folder"},
                {id: "film", folderId: "nested", title: "North", kind: "film", progress: 0.25}
            ]
        };
    }
    function test_folder_view_is_separate_and_back_restores_each_opener() {
        createLibrary(foldersFixture());
        focused("libraryMode-folders");
        compare(findChild(library, "tab-@folders"), null);
        compare(child("tab-films").visible, false);
        keyClick(Qt.Key_Down); focused("poster-root");
        verify(child("folderGlyph-root").visible);
        keyClick(Qt.Key_Return);
        tryCompare(library, "currentFolderId", "root"); focused("poster-nested");
        compare(child("libraryFolderBreadcrumb").text, "Folders  /  Media");
        keyClick(Qt.Key_Return);
        tryCompare(library, "currentFolderId", "nested"); focused("poster-film");
        keyClick(Qt.Key_Return); tryCompare(library, "detailEntryId", "film"); focused("detailPlay");
        keyClick(Qt.Key_Escape); focused("poster-film");
        keyClick(Qt.Key_Escape); tryCompare(library, "currentFolderId", "root"); focused("poster-nested");
        keyClick(Qt.Key_Escape); tryCompare(library, "currentFolderId", ""); focused("poster-root");
        compare(backs.count, 0);
        keyClick(Qt.Key_Escape); compare(backs.count, 1);
        library.chooseView(false); focused("libraryMode-catalogue");
        compare(library.folderMode, false);
        keyClick(Qt.Key_Down); focused("poster-film");
        keyClick(Qt.Key_Up); focused("tab-films");
    }
    function test_empty_and_unavailable_folders_remain_navigable() {
        createLibrary(foldersFixture());
        focused("libraryMode-folders");
        keyClick(Qt.Key_Down); focused("poster-root");
        keyClick(Qt.Key_Right); focused("poster-offline");
        compare(child("missingLabel-offline").text, "Unavailable");
        keyClick(Qt.Key_Return); tryCompare(library, "currentFolderId", "offline");
        focused("libraryFolderBack");
        verify(child("libraryFolderUnavailable").visible);
        keyClick(Qt.Key_Escape); focused("poster-offline");
        library.openEntry("root"); focused("poster-nested");
        keyClick(Qt.Key_Right); focused("poster-empty");
        keyClick(Qt.Key_Return); tryCompare(library, "currentFolderId", "empty");
        focused("libraryFolderBack");
        verify(child("libraryFolderEmpty").visible);
        compare(child("libraryStatus").visible, false);
        keyClick(Qt.Key_Escape); focused("poster-empty");
        // Catalogue refresh under the global menu must not take focus.
        library.ownsFocus = false;
        const updated = library.folderEntries.slice(); updated[2] = Object.assign({}, updated[2], {title: "Changed"});
        library.folderEntries = updated;
        wait(50); compare(library.currentFolderId, "root");
    }
    function test_view_switch_restores_each_grid_and_scroll() {
        const fixture = foldersFixture();
        fixture.columns = 1;
        createLibrary(fixture);
        focused("libraryMode-folders");
        library.openEntry("root"); focused("poster-nested");
        tryCompare(child("poster-empty").parent, "y", 475);
        keyClick(Qt.Key_Down); focused("poster-empty");
        const scroll = child("libraryGrid").contentY;
        verify(scroll > 0);
        library.chooseView(false); focused("libraryMode-catalogue");
        keyClick(Qt.Key_Down); focused("poster-film");
        library.chooseView(true); focused("libraryMode-folders");
        keyClick(Qt.Key_Down); focused("poster-empty");
        compare(child("libraryGrid").contentY, scroll);
        // Activating the current mode must not reset its remembered card.
        library.chooseView(true); focused("libraryMode-folders");
        keyClick(Qt.Key_Down); focused("poster-empty");
        verify(child("posterCaption-empty").visible);
        compare(child("posterArtTitle-empty").visible, false);
        compare(child("posterYear-empty").text, "Folder");
        library.openEntry("nested"); focused("poster-film");
        library.ownsFocus = false;
        library.ownsFocus = true;
        focused("poster-film");
    }
    function test_unknown_season_does_not_leak_into_known_seasons() {
        createLibrary({tabs: [{id: "series", title: "Series"}], entries: [{id: "show", tabId: "series", title: "Show", kind: "series",
            seasons: [{id: "s1", title: "Season 1"}, {id: "unknown", title: "Without season"}],
            episodes: [{id: "known", title: "One", seasonId: "s1"}, {id: "extra", title: "Extra", seasonId: "unknown"}]}]});
        library.openEntry("show"); focused("detailPlay");
        compare(library.detailEpisodes.length, 1); compare(library.detailEpisodes[0].id, "known");
        keyClick(Qt.Key_Down); focused("season-s1");
        keyClick(Qt.Key_Right); focused("season-unknown");
        compare(library.detailEpisodes.length, 1); compare(library.detailEpisodes[0].id, "extra");
        keyClick(Qt.Key_Down); focused("episode-extra");
    }
    function test_physical_episode_opens_its_series_and_returns_to_file() {
        createLibrary({tabs: [{id: "series", title: "Series"}, {id: "@folders", title: "Folders"}], currentTabId: "@folders",
            entries: [{id: "show", tabId: "series", title: "Show", kind: "series",
                seasons: [{id: "s1", title: "Season 1"}, {id: "unknown", title: "Without season"}],
                episodes: [{id: "ep", title: "One", seasonId: "s1"}, {id: "extra", title: "Extra", seasonId: "unknown"}]}],
            folderEntries: [{id: "root", folderId: "", title: "Media", kind: "folder"},
                {id: "ep", folderId: "root", title: "One", kind: "episode"},
                {id: "extra", folderId: "root", title: "Extra", kind: "episode"}]});
        focused("libraryMode-folders");
        library.openEntry("root"); focused("poster-ep");
        keyClick(Qt.Key_Right); focused("poster-extra");
        keyClick(Qt.Key_Return); focused("episode-extra");
        compare(library.detailEntryId, "show");
        compare(library.currentSeasonId, "unknown");
        keyClick(Qt.Key_Return); compare(episodes.count, 1);
        compare(episodes.signalArguments[0][0], "show");
        compare(episodes.signalArguments[0][1], "extra");
        keyClick(Qt.Key_Escape); focused("detailPlay");
        keyClick(Qt.Key_Escape); focused("poster-extra");
        compare(library.currentFolderId, "root");
        keyClick(Qt.Key_Escape); focused("poster-root");
    }
    function test_deferred_detail_focus_respects_an_overlay() {
        createLibrary(catalogue()); focused("tab-films");
        const other = createTemporaryObject(otherFocusFactory, suite);
        library.openEntry("a");
        library.ownsFocus = false;
        other.forceActiveFocus();
        wait(50); verify(other.activeFocus);
        library.closeDetail();
        wait(50); verify(other.activeFocus);
        library.ownsFocus = true;
    }
    function test_external_detail_route_does_not_reuse_an_old_opener() {
        createLibrary(catalogue()); focused("tab-films");
        library.openEntry("a"); focused("detailPlay");
        // Home may replace the detail ID through its presentation binding.
        library.detailEntryId = "b";
        keyClick(Qt.Key_Escape); focused("poster-b");
    }
    function test_home_return_restores_card_without_stealing_overlay_focus() {
        createLibrary(foldersFixture()); focused("libraryMode-folders");
        library.openEntry("root"); focused("poster-nested");
        keyClick(Qt.Key_Right); focused("poster-empty");
        library.visible = false;
        library.ownsFocus = false;
        const other = createTemporaryObject(otherFocusFactory, suite);
        other.forceActiveFocus();
        library.visible = true;
        wait(50); verify(other.activeFocus);
        library.ownsFocus = true;
        focused("poster-empty");
        compare(library.currentFolderId, "root");
    }

}
