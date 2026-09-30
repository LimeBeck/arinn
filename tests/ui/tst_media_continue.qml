// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import QtTest
import "../../apps/arinn-shell/ui" as Ui

TestCase {
    id: suite
    name: "MediaContinue"
    width: 1920
    height: 1080
    visible: true
    when: windowShown

    property var home
    Component {
        id: factory
        Ui.HomeView {
            homeState: "ready"
            continueEntries: [
                {id: "m1", title: "Piece", subtitle: "Line", kind: "media", running: true},
                {id: "app", title: "Activity", subtitle: "Return", running: true}
            ]
        }
    }
    SignalSpy { id: sessions; signalName: "sessionRequested" }
    SignalSpy { id: entries; signalName: "entryRequested" }

    function child(name) {
        const item = findChild(home, name);
        verify(item !== null, name);
        return item;
    }
    function focused(name) { tryCompare(child(name), "activeFocus", true, 5000, name); }
    function init() {
        mouseMove(suite, 10, 10);
        home = createTemporaryObject(factory, suite);
        verify(home);
        sessions.target = home;
        entries.target = home;
        sessions.clear();
        entries.clear();
    }
    function cleanup() {
        sessions.target = null;
        entries.target = null;
    }

    function test_media_card_opens_session() {
        focused("entry-m1");
        keyClick(Qt.Key_Return);
        compare(sessions.count, 1);
        compare(sessions.signalArguments[0][0], "m1");
        compare(entries.count, 0);
        keyClick(Qt.Key_Right);
        focused("entry-app");
        keyClick(Qt.Key_Return);
        compare(entries.count, 1);
        compare(entries.signalArguments[0][0], "app");
        compare(sessions.count, 1);
    }
}
