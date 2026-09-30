// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import QtTest
import "../../apps/arinn-shell/ui" as Ui

TestCase {
    id: suite
    name: "UiComponents"
    width: 1920; height: 1080
    visible: true
    when: windowShown
    Component { id: buttonFactory; Ui.ActionButton { x: 150; y: 150; text: "Continue" } }
    Component { id: cardFactory; Ui.ContentCard { x: 150; y: 150; text: "Content" } }
    Component { id: tileFactory; Ui.AppTile { x: 150; y: 150; text: "Library"; monogram: "L" } }
    Component { id: paneFactory; Ui.StatusPane { x: 150; y: 150; title: "Status"; actionText: "Retry" } }
    Component { id: trackFactory; Ui.ProgressTrack {} }
    Component { id: canvasFactory; Ui.LogicalCanvas {} }
    Component { id: textFactory; Ui.UiText {} }
    SignalSpy { id: activated; signalName: "activated" }

    function init() { activated.clear(); }
    function cleanup() { activated.target = null; }

    function test_activation_data() {
        return [ { tag: "button", factory: buttonFactory },
                 { tag: "card", factory: cardFactory },
                 { tag: "tile", factory: tileFactory } ];
    }
    function test_activation(data) {
        const control = createTemporaryObject(data.factory, suite);
        activated.target = control;
        control.forceActiveFocus();
        tryCompare(control, "activeFocus", true);
        keyClick(Qt.Key_Return);
        compare(activated.count, 1);
        mouseClick(control, control.width / 2, control.height / 2);
        compare(activated.count, 2);
        control.busy = true;
        keyClick(Qt.Key_Return);
        mouseClick(control, control.width / 2, control.height / 2);
        compare(activated.count, 2);
        control.busy = false;
        control.enabled = false;
        keyClick(Qt.Key_Return);
        mouseClick(control, control.width / 2, control.height / 2);
        compare(activated.count, 2);
    }
    function test_focus_ring_and_pointer_lift() {
        const card = createTemporaryObject(cardFactory, suite);
        card.forceActiveFocus();
        tryCompare(card, "activeFocus", true);
        tryCompare(card, "scale", 1.06);
        compare(card.lift, -6);
        const ring = findChild(card, "focusRing");
        verify(ring.z > card.contentItem.z);
        tryCompare(ring, "opacity", 1);
        card.pointerMode = true;
        tryCompare(card, "scale", 1.03);
        compare(card.lift, -2);
        keyPress(Qt.Key_Space);
        tryCompare(card, "down", true);
        tryVerify(() => card.scale < 1.03);
        keyRelease(Qt.Key_Space);
    }
    function test_artwork_loading() {
        const card = createTemporaryObject(cardFactory, suite, {
            artwork: Qt.resolvedUrl("../../apps/arinn-shell/gallery/fixture-artwork.svg")
        });
        tryCompare(card, "artworkReady", true);
        // The canvas paints after the image is ready. Wait for those pixels.
        tryVerify(() => {
            const rendered = grabImage(card);
            return rendered.blue(100, 30) > rendered.red(100, 30);
        });
        card.artwork = "";
        tryCompare(card, "artworkReady", false);
    }
    function test_hover_suppression() {
        const first = createTemporaryObject(buttonFactory, suite);
        const second = createTemporaryObject(buttonFactory, suite, { y: 400, pointerMode: true, hoverFocusEnabled: false });
        first.forceActiveFocus();
        mouseMove(second, second.width / 2, second.height / 2);
        verify(first.activeFocus);
        second.hoverFocusEnabled = true;
        mouseMove(suite, 10, 10);
        mouseMove(second, second.width / 2, second.height / 2);
        tryCompare(second, "activeFocus", true);
    }
    function test_local_navigation_while_busy() {
        const first = createTemporaryObject(buttonFactory, suite, {busy: true});
        const second = createTemporaryObject(buttonFactory, suite, {x: 600});
        first.KeyNavigation.right = second;
        first.forceActiveFocus();
        keyClick(Qt.Key_Right);
        tryCompare(second, "activeFocus", true);
        keyClick(Qt.Key_Left);
        tryCompare(first, "activeFocus", true);
    }
    function test_long_cyrillic_label() {
        const button = createTemporaryObject(buttonFactory, suite, {text: "Продолжить просмотр с последнего момента"});
        verify(button.width >= button.contentItem.implicitWidth + 80);
        verify(button.contentItem.font.pixelSize >= 26);
        compare(button.contentItem.font.family, Ui.Theme.fontFamily);
        tryVerify(() => Ui.Theme.onest.status === FontLoader.Ready);
    }
    function test_progress_bounds() {
        const track = createTemporaryObject(trackFactory, suite);
        for (const value of [-1, 0, 0.5, 1, 2, NaN, Infinity]) {
            track.value = value;
            verify(track.fraction >= 0 && track.fraction <= 1);
        }
    }
    function test_status_data() {
        return ["empty", "loading", "error", "unavailable"].map(kind => ({tag: kind, kind: kind}));
    }
    function test_status(data) {
        const pane = createTemporaryObject(paneFactory, suite, {kind: data.kind});
        const action = findChild(pane, "statusAction");
        compare(action.visible, data.kind !== "loading");
        if (action.visible) {
            pane.forceActiveFocus();
            tryCompare(action, "activeFocus", true);
        }
    }
    function test_canvas_data() {
        return [{tag: "1080p", w: 1920, h: 1080, scale: 1},
                {tag: "4k", w: 3840, h: 2160, scale: 2},
                {tag: "letterbox", w: 2560, h: 1080, scale: 1}];
    }
    function test_canvas(data) {
        const canvas = createTemporaryObject(canvasFactory, suite, {width: data.w, height: data.h});
        compare(canvas.canvasScale, data.scale);
    }
}
