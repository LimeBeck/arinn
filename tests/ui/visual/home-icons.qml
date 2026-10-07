// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtTest
import "../../../apps/arinn-shell/ui" as Ui
import CaptureRequest 1.0 as CaptureRequest

// Optional focused evidence capture. Return stays opaque pending #212;
// these two same-title sessions must remain separate visually.
TestCase {
    id: suite
    name: "HomeIconCapture"
    visible: true
    when: windowShown
    width: CaptureRequest.Request.scale() === "4k-layout" ? 3840 : 1920
    height: CaptureRequest.Request.scale() === "4k-layout" ? 2160 : 1080
    Ui.LogicalCanvas {
        anchors.fill: parent
        Ui.HomeView {
            id: home
            homeState: "ready"
            clockText: "20:42"
            openServices: [{id: "svc-1", title: "North", letter: "N"}, {id: "svc-2", title: "South", letter: "S"}]
            continueEntries: [
                {id: "{aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa}", title: "Return", subtitle: "Previous activity"},
                {id: "session-a", title: "Same title", subtitle: "Independent session A"},
                {id: "session-b", title: "Same title", subtitle: "Independent session B"}
            ]
            favoriteEntries: [
                {id: "arinn", title: "Arinn", iconSource: Qt.resolvedUrl("../../../docs/design/handoff/assets/arinn-mark.svg")},
                {id: "missing", title: "Missing icon"},
                {id: "explore", title: "Explore"},
                {id: "play", title: "Play"},
                {id: "listen", title: "Listen"},
                {id: "tools", title: "Tools"}
            ]
        }
    }
    function test_capture() {
        mouseMove(suite, 5, 5);
        const current = findChild(home, "entry-{aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa}");
        verify(current);
        current.forceActiveFocus();
        tryCompare(current, "activeFocus", true);
        verify(findChild(home, "entry-session-a"));
        verify(findChild(home, "entry-session-b"));
        wait(400);
        grabImage(suite).save(CaptureRequest.Request.output() + "/home-service.png");
        const tile = findChild(home, "entry-arinn");
        tryCompare(tile, "iconReady", true);
        tile.forceActiveFocus();
        wait(400);
        grabImage(suite).save(CaptureRequest.Request.output() + "/home-icon-focus.png");
        keyClick(Qt.Key_Right);
        const missing = findChild(home, "entry-missing");
        tryCompare(missing, "activeFocus", true);
        compare(missing.iconReady, false);
        wait(400);
        grabImage(suite).save(CaptureRequest.Request.output() + "/home-fallback-focus.png");
    }
}
