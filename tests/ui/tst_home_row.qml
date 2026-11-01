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
    // A card's slot in its row, and its clip's horizontal edges in the window.
    function slotOf(card) { return card.parent.parent.parent; }
    function slotRight(card) {
        const slot = slotOf(card);
        return Math.round(slot.mapToItem(suite, slot.width, 0).x);
    }
    function clipEdges(card) {
        const clip = card.parent.parent;
        verify(clip.objectName === "cardClip");
        const left = Math.round(clip.mapToItem(suite, 0, 0).x);
        return [left, left + Math.round(clip.width)];
    }
    // Whether the window pixel at (x, y) matches the backdrop further out.
    function plain(x, y, reference) {
        const pixels = grabImage(suite);
        const at = reference === undefined ? 1905 : reference;
        return Math.abs(pixels.red(x, y) - pixels.red(at, y)) <= 2
            && Math.abs(pixels.green(x, y) - pixels.green(at, y)) <= 2;
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
        const first = findChild(home, "entry-a");
        const last = findChild(home, "entry-" + data.last);
        const next = findChild(home, "entry-" + data.next);
        compare(Math.round(next.mapToItem(suite, 0, 0).x), 1848);
        const y = Math.round(next.mapToItem(suite, 0, next.height / 2).y);

        // At rest nothing from the row is painted past x 1824 (#564).
        compare(slotOf(last).painted, true);
        compare(slotOf(next).painted, false);
        compare(clipEdges(next)[1] - clipEdges(next)[0], 0);
        compare(clipEdges(last)[1], 1824);
        verify(waitForRendering(home));
        tryVerify(() => plain(1860, y), 5000, "a card shows past the safe edge at rest");

        // The first card's glow reaches left of x 120; its clip makes room.
        first.forceActiveFocus();
        tryCompare(first, "activeFocus", true);
        tryVerify(() => clipEdges(first)[0] === 72);

        // In pointer mode the last whole card can take focus without a
        // scroll. Its glow gets room on the right; the next card stays hidden.
        mouseMove(suite, 10, suite.height - 10);
        home.pointerMode = true;
        last.forceActiveFocus();
        tryCompare(last, "activeFocus", true);
        compare(findChild(home, "continueViewport").contentX, 0);
        tryVerify(() => clipEdges(last)[1] === slotRight(last) + 48);
        compare(slotOf(next).painted, false);
        tryVerify(() => plain(1866, y), 5000, "the next card shows in the glow margin");

        // The pointer can neither focus nor activate the hidden card there.
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
        compare(Math.round(poster.mapToItem(suite, 0, 0).x), 1752);
        // Cut at the safe edge while it has no focus.
        compare(clipEdges(poster)[1], 1824);
        const y = Math.round(poster.mapToItem(suite, 0, poster.height / 2).y);
        mouseMove(suite, 10, suite.height - 10);
        home.pointerMode = true;
        mouseMove(suite, 1790, y);
        tryCompare(poster, "activeFocus", true);
        // The card's body ends at the safe edge, so only its glow reaches
        // past it; the next poster stays hidden.
        tryVerify(() => slotRight(poster) === 1824);
        compare(slotOf(findChild(home, "entry-p9")).painted, false);
        verify(waitForRendering(home));
        tryVerify(() => plain(1866, y), 5000, "a poster shows past the safe edge");
        mouseMove(suite, 10, suite.height - 10);
    }

    function test_flicked_cards_stay_inside_the_safe_edges() {
        // A flick moves the row without reveal(). Neither the focused card
        // nor a neighbour may then paint past a safe edge.
        createTemporaryObject(backdropFactory, suite);
        const entries = [];
        for (let i = 0; i < 6; ++i)
            entries.push({id: "abcdef"[i], title: "Item " + i});
        const home = createTemporaryObject(homeRowFactory, suite, {entries: entries});
        tryCompare(home, "count", 6);
        const viewport = findChild(home, "continueViewport");
        const first = findChild(home, "entry-a");
        const second = findChild(home, "entry-b");
        const fifth = findChild(home, "entry-e");
        const y = Math.round(first.mapToItem(suite, 0, first.height / 2).y);
        mouseMove(suite, 10, suite.height - 10);
        home.pointerMode = true;

        // The focused first card crosses the left edge: it is cut at x 120.
        first.forceActiveFocus();
        tryCompare(first, "activeFocus", true);
        tryVerify(() => clipEdges(first)[0] === 72);
        viewport.contentX = 100;
        tryVerify(() => clipEdges(first)[0] === 120);
        verify(waitForRendering(home));
        tryVerify(() => plain(100, y, 30), 5000, "the focused card shows left of x 120");

        // The focused second card sits 40 px inside the left edge, so its
        // glow needs the margin, while 8 px of the first card stay inside.
        // The first card is still cut at x 120.
        second.forceActiveFocus();
        tryCompare(second, "activeFocus", true);
        viewport.contentX = 392;
        tryVerify(() => clipEdges(second)[0] === 160 - 48);
        compare(slotOf(first).painted, true);
        compare(clipEdges(first)[0], 120);
        verify(waitForRendering(home));
        tryVerify(() => plain(100, y, 30), 5000, "the neighbour shows left of x 120");

        // The focused fifth card ends at the safe right edge, then crosses it.
        viewport.contentX = 432;
        fifth.forceActiveFocus();
        tryCompare(fifth, "activeFocus", true);
        compare(viewport.contentX, 432);
        tryVerify(() => clipEdges(fifth)[1] === slotRight(fifth) + 48);
        viewport.contentX = 400;
        tryVerify(() => clipEdges(fifth)[1] === 1824);
        verify(waitForRendering(home));
        tryVerify(() => plain(1840, y), 5000, "the focused card shows past x 1824");
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
