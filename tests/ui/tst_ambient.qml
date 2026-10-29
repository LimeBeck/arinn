// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import QtTest
import "../../apps/arinn-shell/ui/screens/power" as Power

// #307, spec §5.7: the ambient screen and its idle timer. The timer runs on a
// shortened timeout and direct calls instead of ten real minutes.
TestCase {
    id: suite
    name: "Ambient"
    width: 1920
    height: 1080
    visible: true
    when: windowShown

    // A screen beneath that counts every key and click it receives.
    Component {
        id: screenFactory
        FocusScope {
            width: 1920
            height: 1080
            property int keys: 0
            property int clicks: 0
            property alias button: target
            Rectangle {
                id: target
                objectName: "underneath"
                width: 400; height: 200
                focus: true
                activeFocusOnTab: true
                Keys.onPressed: event => { parent.keys += 1; event.accepted = true; }
                MouseArea { anchors.fill: parent; onClicked: target.parent.clicks += 1 }
            }
        }
    }
    Component {
        id: ambientFactory
        Power.AmbientScreen {
            clockText: "21:40"
            dateText: "Wednesday, 21 October"
            driftInterval: 200
            driftDuration: 50
        }
    }
    Component {
        id: idleFactory
        Power.AmbientIdle { timeout: 150 }
    }
    Component {
        id: defaultIdleFactory
        Power.AmbientIdle {}
    }
    SignalSpy { id: wakeSpy; signalName: "wakeRequested" }
    SignalSpy { id: idleSpy; signalName: "idle" }

    property var screen
    property var ambient
    function init() {
        mouseMove(suite, 10, 10);
        screen = createTemporaryObject(screenFactory, suite);
        ambient = createTemporaryObject(ambientFactory, suite);
        wakeSpy.target = ambient;
        wakeSpy.clear();
        screen.button.forceActiveFocus();
        verify(screen.button.activeFocus);
    }
    function cleanup() {
        wakeSpy.target = null;
        idleSpy.target = null;
    }
    function startAmbient() {
        ambient.start();
        verify(ambient.visible);
        tryCompare(ambient, "armed", true);
    }

    function test_looks_like_the_handoff() {
        startAmbient();
        const clock = findChild(ambient, "ambientClock");
        compare(clock.text, "21:40");
        compare(clock.font.pixelSize, 160);
        compare(clock.font.weight, 500);
        // Tabular numerals and −3 % tracking (#487).
        compare(clock.font.features["tnum"], 1);
        // Qt keeps letter spacing in 1/64 px.
        fuzzyCompare(clock.font.letterSpacing, -0.03 * 160, 1 / 64);
        const date = findChild(ambient, "ambientDate");
        compare(date.font.pixelSize, 32);
        compare(date.font.weight, 500);
        compare(date.leftPadding, 6);
        compare(date.x, clock.x);
        // A 160 px clock line (line-height 1), the date 10 px below it.
        compare(clock.height, 160);
        compare(date.y, clock.y + 170);
        // oklch(0.60 0.004 260) and oklch(0.45 0.004 260) converted exactly;
        // bg-deep keeps its token-table hex.
        verify(Qt.colorEqual(clock.color, "#7F8083"));
        verify(Qt.colorEqual(date.color, "#545557"));
        verify(Qt.colorEqual(findChild(ambient, "ambientBackground").color, "#050506"));
        // The block moves so no static bright pixels remain (handoff 20).
        const block = findChild(ambient, "ambientBlock");
        const start = Qt.point(block.x, block.y);
        tryVerify(() => block.x !== start.x || block.y !== start.y, 3000);
        verify(block.x >= 0 && block.x + block.width <= 1920);
        verify(block.y >= 0 && block.y + block.height <= 1080);
    }

    // Review of #545: the block is as wide as its text, so a longer date (at
    // midnight, or after a locale change) widens it where the last drift
    // left it. It stays on screen at once, not only after the next drift.
    function test_block_stays_on_screen_when_the_date_grows() {
        ambient.driftInterval = 600000;
        startAmbient();
        const block = findChild(ambient, "ambientBlock");
        ambient.moveTo(1920, 1080);
        tryVerify(() => Math.abs(block.x + block.width - 1920) < 0.5
                        && Math.abs(block.y + block.height - 1080) < 0.5, 2000);
        const width = block.width;
        ambient.dateText = "Wednesday, 30 September, a much longer localized date";
        // The column lays out on the next polish; then it is on screen.
        tryVerify(() => block.width > width);
        verify(block.x >= 0 && block.x + block.width <= 1920, "x " + block.x + " width " + block.width);
        verify(block.y >= 0 && block.y + block.height <= 1080);
        // A drift under way keeps it on screen too.
        ambient.driftDuration = 1000;
        ambient.moveTo(0, 0);
        ambient.moveTo(1920, 1080);
        wait(100);
        ambient.dateText = "Wednesday, 30 September, an even longer localized date than before";
        for (let i = 0; i < 12; ++i) {
            verify(block.x >= 0 && block.x + block.width <= 1920.5, "x " + block.x + " width " + block.width);
            wait(100);
        }
    }

    // Review of #545: a drift picks a place anywhere the block fits. Places
    // are never piled up on the right or bottom edge or in one corner,
    // which would leave static pixels there (handoff 20, OLED).
    function test_drift_spreads_over_the_free_area() {
        ambient.driftInterval = 600000;
        ambient.driftDuration = 0;
        startAmbient();
        const block = findChild(ambient, "ambientBlock");
        tryVerify(() => block.width > 0 && block.height > 0);
        const maxX = 1920 - block.width;
        const maxY = 1080 - block.height;
        const runs = 400;
        let right = 0, bottom = 0, left = 0, top = 0;
        const xs = new Set(), ys = new Set();
        for (let i = 0; i < runs; ++i) {
            ambient.drift();
            const x = ambient.driftX, y = ambient.driftY;
            verify(x >= 0 && x <= maxX && y >= 0 && y <= maxY, "x " + x + " y " + y);
            if (maxX - x < 0.5) ++right;
            if (maxY - y < 0.5) ++bottom;
            if (x < 0.5) ++left;
            if (y < 0.5) ++top;
            xs.add(Math.round(x));
            ys.add(Math.round(y));
        }
        // Uniform over the free area, a half-pixel strip at an edge holds
        // about 0.1 % of the places (0.3 % on the bottom); clamped samples
        // put about 20 % there. Five in 400 cannot flake either way.
        verify(right <= 5, "right edge " + right + "/" + runs);
        verify(bottom <= 5, "bottom edge " + bottom + "/" + runs);
        verify(left <= 5 && top <= 5, "left " + left + ", top " + top);
        verify(xs.size > runs * 0.6, "distinct x " + xs.size);
        verify(ys.size > runs * 0.4, "distinct y " + ys.size);
    }

    function test_first_key_is_consumed() {
        startAmbient();
        compare(wakeSpy.count, 0);
        keyClick(Qt.Key_Return);
        compare(wakeSpy.count, 1);
        verify(!ambient.active);
        // The screen beneath never saw the wake key.
        compare(screen.keys, 0);
        // The caller restores focus; the next key works normally.
        screen.button.forceActiveFocus();
        keyClick(Qt.Key_Return);
        compare(screen.keys, 1);
        compare(wakeSpy.count, 1);
    }

    function test_opening_key_is_not_a_wake() {
        // A key held while ambient starts releases into it: no wake.
        keyPress(Qt.Key_Return);
        ambient.start();
        keyRelease(Qt.Key_Return);
        tryCompare(ambient, "armed", true);
        verify(ambient.active);
        compare(wakeSpy.count, 0);
        compare(screen.keys, 1);
    }

    function test_pointer_movement_wakes_and_click_is_consumed() {
        mouseMove(suite, 300, 300);
        startAmbient();
        // A resting pointer is the anchor; only real movement wakes.
        mouseMove(suite, 300, 300);
        compare(wakeSpy.count, 0);
        mouseMove(suite, 640, 360);
        tryCompare(wakeSpy, "count", 1);
        verify(!ambient.active);

        startAmbient();
        mouseClick(suite, 100, 100);
        compare(wakeSpy.count, 2);
        compare(screen.clicks, 0);
    }

    function test_stop_is_not_a_wake() {
        startAmbient();
        ambient.stop();
        verify(!ambient.active);
        compare(wakeSpy.count, 0);
    }

    function test_idle_timeout_starts_ambient() {
        const idle = createTemporaryObject(idleFactory, suite);
        idleSpy.target = idle;
        idleSpy.clear();
        compare(idle.counting, false);
        idle.shellShown = true;
        verify(idle.eligible);
        verify(idle.counting);
        idleSpy.wait(2000);
        compare(idleSpy.count, 1);
        // Activity keeps it from firing.
        idle.activity();
        idleSpy.clear();
        for (let i = 0; i < 6; ++i) {
            wait(50);
            idle.activity();
        }
        compare(idleSpy.count, 0);
        idleSpy.wait(2000);
    }

    function test_default_timeout_is_ten_minutes() {
        const idle = createTemporaryObject(defaultIdleFactory, suite);
        verify(idle);
        compare(idle.timeout, 600000);
    }

    function test_automatic_ambient_is_inhibited_data() {
        return [
            {tag: "media playing", key: "mediaPlaying"},
            {tag: "locked", key: "locked"},
            {tag: "launching", key: "launching"},
            {tag: "power request pending", key: "powerPending"},
            {tag: "already on", key: "ambientActive"}
        ];
    }
    function test_automatic_ambient_is_inhibited(data) {
        const idle = createTemporaryObject(idleFactory, suite, {shellShown: true});
        idleSpy.target = idle;
        idleSpy.clear();
        verify(idle.counting);
        idle[data.key] = true;
        verify(!idle.eligible);
        verify(!idle.counting);
        idle.activity();
        wait(400);
        compare(idleSpy.count, 0);
        // Allowed again: a full timeout from now.
        idle[data.key] = false;
        verify(idle.counting);
        idleSpy.wait(2000);
    }

    function test_hidden_shell_never_counts() {
        const idle = createTemporaryObject(idleFactory, suite);
        idleSpy.target = idle;
        idleSpy.clear();
        idle.activity();
        wait(400);
        compare(idleSpy.count, 0);
        verify(!idle.counting);
    }

    function test_what_counts_as_playing() {
        const idle = createTemporaryObject(idleFactory, suite);
        verify(!idle.playing([], [], false));
        verify(!idle.playing([{running: false}], [{audible: false}], false));
        verify(idle.playing([{running: true}], [], false));
        verify(idle.playing([], [{audible: true}], false));
        verify(idle.playing([], [], true));
        verify(!idle.playing(undefined, undefined, false));
    }
}
