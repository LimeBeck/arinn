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
