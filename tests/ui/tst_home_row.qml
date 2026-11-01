// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import QtTest
import "../../apps/arinn-shell/ui" as Ui

// Home rows receive a new entries array on every upstream notification. Cards
// must survive an equal array and update in place when one entry changes;
// recreating them made Home flicker (#441).
TestCase {
    id: suite
    name: "HomeRow"
    width: 1920
    height: 1080
    visible: true
    when: windowShown

    property var row
    Component {
        id: factory
        Ui.HomeRow {
            width: 1600
            entries: [
                {id: "a", title: "Alpha", subtitle: "One", artwork: ""},
                {id: "b", title: "Beta", subtitle: "Two", progress: 0.25}
            ]
        }
    }

    // A row in Home's geometry: from the safe left x 120 to the safe right
    // x 1824 (handoff 01).
    Component {
        id: homeRowFactory
        Ui.HomeRow {
            objectName: "continue"
            x: Ui.Theme.safeLeft; y: 420
            width: Ui.Theme.canvasWidth - Ui.Theme.safeLeft - Ui.Theme.safeRight
        }
    }
    SignalSpy { id: rowActivated; signalName: "activated" }
    Component {
        id: backdropFactory
        Rectangle { width: suite.width; height: suite.height; z: -1; color: Ui.Theme.background }
    }

    function child(name) {
        const item = findChild(row, name);
        verify(item !== null, name);
        return item;
    }
    function init() {
        row = createTemporaryObject(factory, suite);
        verify(row);
        tryCompare(row, "count", 2);
    }

    function test_equal_entries_keep_cards() {
        const a = child("entry-a");
        const b = child("entry-b");
        row.entries = [
            {id: "a", title: "Alpha", subtitle: "One", artwork: ""},
            {id: "b", title: "Beta", subtitle: "Two", progress: 0.25}
        ];
        wait(0);
        verify(child("entry-a") === a, "an equal array kept the first card");
        verify(child("entry-b") === b, "an equal array kept the second card");
    }

    function test_changed_entry_updates_in_place() {
        const a = child("entry-a");
        const b = child("entry-b");
        row.entries = [
            {id: "a", title: "Alpha", subtitle: "One", artwork: ""},
            {id: "b", title: "Beta", subtitle: "Two", progress: 0.5}
        ];
        tryCompare(b, "progress", 0.5);
        verify(child("entry-a") === a, "an unchanged entry kept its card");
        verify(child("entry-b") === b, "a changed entry updated its card");
    }

    function test_focus_survives_an_update() {
        child("entry-b").forceActiveFocus();
        tryCompare(child("entry-b"), "activeFocus", true);
        row.entries = [
            {id: "a", title: "Alpha", subtitle: "Changed", artwork: ""},
            {id: "b", title: "Beta", subtitle: "Two", progress: 0.25}
        ];
        tryCompare(child("entry-a"), "subtitle", "Changed");
        compare(child("entry-b").activeFocus, true);
        compare(row.selectedId, "b");
    }

    function test_focused_card_follows_its_entry() {
        const a = child("entry-a");
        a.forceActiveFocus();
        tryCompare(a, "activeFocus", true);
        row.entries = [
            {id: "c", title: "Gamma"},
            {id: "a", title: "Alpha", subtitle: "One", artwork: ""},
            {id: "b", title: "Beta", subtitle: "Two", progress: 0.25}
        ];
        tryCompare(row, "count", 3);
        verify(child("entry-a") === a, "the card moved with its entry");
        compare(a.text, "Alpha");
        compare(a.activeFocus, true);
        tryCompare(row, "selectedIndex", 1);
        compare(child("entry-c").text, "Gamma");
    }

    function test_rows_stop_at_the_safe_edges_data() {
        return [
            // The fifth 400 px card and the seventh 264 px tile start at x 1848.
            {tag: "content", variant: "content", count: 6, last: "d", next: "e"},
            {tag: "app", variant: "app", count: 8, last: "f", next: "g"}
        ];
    }
    function test_rows_stop_at_the_safe_edges(data) {
        createTemporaryObject(backdropFactory, suite);
        const entries = [];
        for (let i = 0; i < data.count; ++i)
            entries.push({id: "abcdefgh"[i], title: "Item " + i, monogram: "I"});
        const home = createTemporaryObject(homeRowFactory, suite, {variant: data.variant, entries: entries});
        tryCompare(home, "count", data.count);
        const clip = findChild(home, "continueSafeClip");
        verify(clip);
        const edges = () => {
            const left = Math.round(clip.mapToItem(suite, 0, 0).x);
            return [left, left + Math.round(clip.width)];
        };
        const last = findChild(home, "entry-" + data.last);
        const next = findChild(home, "entry-" + data.next);
        compare(Math.round(next.mapToItem(suite, 0, 0).x), 1848);
        const y = Math.round(next.mapToItem(suite, 0, next.height / 2).y);
        // Nothing from the row is painted past the safe edge: the strip
        // right of x 1824 matches the backdrop further right.
        const plain = (x) => {
            const pixels = grabImage(suite);
            return Math.abs(pixels.red(x, y) - pixels.red(1905, y)) <= 2
                && Math.abs(pixels.green(x, y) - pixels.green(1905, y)) <= 2;
        };

        // At rest the row paints from x 120 to x 1824 (#564).
        compare(edges(), [120, 1824]);
        compare(last.parent.opacity, 1);
        compare(next.parent.opacity, 0);
        verify(waitForRendering(home));
        tryVerify(() => plain(1860), 5000, "a card shows past the safe edge at rest");

        // The first card's glow reaches left of x 120; the clip makes room.
        findChild(home, "entry-a").forceActiveFocus();
        tryCompare(findChild(home, "entry-a"), "activeFocus", true);
        tryVerify(() => edges()[0] === 72 && edges()[1] === 1824);

        // In pointer mode the last whole card can take focus without a
        // scroll. Its glow gets room on the right; the next card stays hidden.
        mouseMove(suite, 10, suite.height - 10);
        home.pointerMode = true;
        last.forceActiveFocus();
        tryCompare(last, "activeFocus", true);
        compare(findChild(home, "continueViewport").contentX, 0);
        tryVerify(() => edges()[0] === 120 && edges()[1] === 1872);
        compare(next.parent.opacity, 0);
        tryVerify(() => plain(1866), 5000, "the next card shows in the glow margin");

        // The hidden card lies inside the widened clip, but the pointer can
        // neither focus nor activate it there.
        rowActivated.target = home;
        rowActivated.clear();
        mouseMove(suite, 1860, y);
        wait(50);
        compare(next.activeFocus, false, "hover focused the hidden card");
        compare(last.activeFocus, true);
        mouseClick(suite, 1860, y);
        wait(50);
        compare(rowActivated.count, 0, "a click activated the hidden card");
        compare(last.activeFocus, true);
        compare(findChild(home, "continueViewport").contentX, 0);
        rowActivated.target = null;
        mouseMove(suite, 10, suite.height - 10);
    }

    function test_pointer_focus_brings_a_partly_visible_card_inside() {
        // New has 180 px posters: the ninth spans x 1752 to 1932 at rest,
        // across the safe right edge x 1824.
        createTemporaryObject(backdropFactory, suite);
        const entries = [];
        for (let i = 0; i < 12; ++i)
            entries.push({id: "p" + i, title: "Poster " + i});
        const home = createTemporaryObject(homeRowFactory, suite, {variant: "poster", entries: entries});
        tryCompare(home, "count", 12);
        const poster = findChild(home, "entry-p8");
        const slot = poster.parent;
        compare(Math.round(slot.mapToItem(suite, 0, 0).x), 1752);
        const y = Math.round(slot.mapToItem(suite, 0, slot.height / 2).y);
        mouseMove(suite, 10, suite.height - 10);
        home.pointerMode = true;
        mouseMove(suite, 1790, y);
        tryCompare(poster, "activeFocus", true);
        // The card's body ends at the safe edge, so only its glow uses the
        // widened clip; the next poster stays hidden.
        tryVerify(() => Math.round(slot.mapToItem(suite, slot.width, 0).x) === 1824);
        compare(findChild(home, "entry-p9").parent.opacity, 0);
        verify(waitForRendering(home));
        tryVerify(() => {
            const pixels = grabImage(suite);
            return Math.abs(pixels.red(1866, y) - pixels.red(1905, y)) <= 2;
        }, 5000, "a poster shows past the safe edge");
        mouseMove(suite, 10, suite.height - 10);
    }

    function test_flicked_focused_card_stays_inside_the_safe_edges() {
        // A flick moves the row without reveal(). A focused card that ends up
        // across a safe edge is cut there; the clip widens only for the glow
        // of a card whose body is inside.
        createTemporaryObject(backdropFactory, suite);
        const entries = [];
        for (let i = 0; i < 6; ++i)
            entries.push({id: "abcdef"[i], title: "Item " + i});
        const home = createTemporaryObject(homeRowFactory, suite, {entries: entries});
        tryCompare(home, "count", 6);
        const clip = findChild(home, "continueSafeClip");
        const viewport = findChild(home, "continueViewport");
        const edges = () => {
            const left = Math.round(clip.mapToItem(suite, 0, 0).x);
            return [left, left + Math.round(clip.width)];
        };
        const first = findChild(home, "entry-a");
        const y = Math.round(first.mapToItem(suite, 0, first.height / 2).y);
        const plain = (x, reference) => {
            const pixels = grabImage(suite);
            return Math.abs(pixels.red(x, y) - pixels.red(reference, y)) <= 2
                && Math.abs(pixels.green(x, y) - pixels.green(reference, y)) <= 2;
        };
        mouseMove(suite, 10, suite.height - 10);
        home.pointerMode = true;

        // Left edge: the first card keeps focus while the row moves 100 px.
        first.forceActiveFocus();
        tryCompare(first, "activeFocus", true);
        tryVerify(() => edges()[0] === 72);
        viewport.contentX = 100;
        tryVerify(() => edges()[0] === 120 && edges()[1] === 1824);
        verify(waitForRendering(home));
        tryVerify(() => plain(100, 30), 5000, "the focused card shows left of x 120");

        // Right edge: the fifth card ends at the safe edge, then the row moves
        // back 32 px and the card crosses it.
        viewport.contentX = 432;
        const fifth = findChild(home, "entry-e");
        fifth.forceActiveFocus();
        tryCompare(fifth, "activeFocus", true);
        compare(viewport.contentX, 432);
        tryVerify(() => edges()[1] === 1872);
        viewport.contentX = 400;
        tryVerify(() => edges()[0] === 120 && edges()[1] === 1824);
        verify(waitForRendering(home));
        tryVerify(() => plain(1840, 1905), 5000, "the focused card shows past x 1824");
        mouseMove(suite, 10, suite.height - 10);
    }

    function test_added_and_removed_entries() {
        // A removed card must not first bind to an empty entry.
        failOnWarning(/Unable to assign/);
        row.entries = [
            {id: "c", title: "Gamma"},
            {id: "a", title: "Alpha", subtitle: "One", artwork: ""}
        ];
        tryCompare(row, "count", 2);
        tryCompare(child("entry-c"), "text", "Gamma");
        compare(findChild(row, "entry-b"), null);
        row.entries = [];
        tryCompare(row, "count", 0);
        compare(findChild(row, "entry-a"), null);
    }
}
