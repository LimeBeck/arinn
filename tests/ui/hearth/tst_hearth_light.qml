// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import QtTest
import "../../../apps/arinn-shell/ui" as Ui

// Own process from ui-components (#733): that suite aborts on
// keyClick(Qt.Key_MediaPause) in tst_home.qml, so a later case in the same
// process never runs. ctest target hearth-light loads only this directory.
TestCase {
    id: suite
    name: "HearthLight"
    width: 1920; height: 1080
    visible: true
    when: windowShown
    property var home
    Component {
        id: factory
        Ui.HomeView {
            continueEntries: [
                {id: "a", title: "Activity", subtitle: "Return", running: true},
                {id: "b", title: "Second"}, {id: "c", title: "Third"},
                {id: "d", title: "Fourth"}, {id: "e", title: "Fifth"}
            ]
            favoriteEntries: [{id: "f", title: "Favorite", monogram: "F"}, {id: "g", title: "Another", monogram: "A"}]
            newEntries: [{id: "n", title: "New"}]
        }
    }
    function child(name) { const item = findChild(home, name); verify(item !== null, name); return item; }
    function focused(name) { tryCompare(child(name), "activeFocus", true, 5000, name); }
    function createHome() {
        mouseMove(suite, 10, 10);
        Ui.InputMode.reset();
        if (home)
            home.destroy();
        Ui.Theme.followsTime = true;
        Ui.Theme.hour = -1;
        Ui.Theme.clockText = "12:00";
        home = createTemporaryObject(factory, suite);
        verify(home);
        home.clockText = "12:00";
        home.lightFollowsTime = true;
        focused("entry-a");
    }
    function init() { createHome(); }
    function cleanup() {
        if (home) {
            home.destroy();
            home = null;
        }
        Ui.Theme.followsTime = true;
        Ui.Theme.hour = -1;
        Ui.Theme.clockText = "12:00";
    }
    function near(a, b) {
        return Math.abs(a.r - b.r) < 0.004 && Math.abs(a.g - b.g) < 0.004
            && Math.abs(a.b - b.b) < 0.004 && Math.abs(a.a - b.a) < 0.004;
    }

    // #368: the light field takes the artwork's hue, or the identity hue.
    function test_ambient_glow_follows_focus() {
        const glow = child("ambientGlow");
        home.continueEntries = [
            {id: "film", title: "Film", kind: "media", artwork: Qt.resolvedUrl("../fixture-glow.svg")},
            {id: "plain", title: "Plain", kind: "media"}
        ];
        child("entry-film").forceActiveFocus(); focused("entry-film");
        tryVerify(() => glow.shown && Math.abs(glow.hue - 30) <= 3, 5000, "artwork hue " + glow.hue);
        const film = child("entry-film");
        const filmX = film.mapToItem(glow, film.width / 2, film.height / 2).x;
        tryVerify(() => Math.abs(glow.poolCenter - filmX) < 16, 3000,
                  "pool " + glow.poolCenter + " card " + filmX);
        verify(Math.abs(glow.poolCenterY - glow.height * 1.08) < 2);
        keyClick(Qt.Key_Right); focused("entry-plain");
        compare(glow.tint, Qt.tint(Ui.Theme.identityColor("plain"), "transparent"));
        const identity = glow.toOklch(glow.tint).h;
        tryVerify(() => Math.abs(glow.hue - identity) < 0.5, 5000, "identity hue " + glow.hue);
        keyClick(Qt.Key_Left); focused("entry-film");
        tryVerify(() => Math.abs(glow.hue - 30) <= 3, 5000, "artwork hue again " + glow.hue);
        keyClick(Qt.Key_Right); focused("entry-plain");
        tryVerify(() => Math.abs(glow.hue - identity) < 0.5, 5000, "identity hue again " + glow.hue);
        verify(glow.lit);
        home.currentServiceId = "tab-1";
        home.currentServiceTitle = "Page";
        child("entry-current-service").forceActiveFocus(); focused("entry-current-service");
        compare(child("entry-current-service").visualIdentity, "tab-1");
        tryCompare(glow, "tint", Qt.tint(Ui.Theme.identityColor("tab-1"), "transparent"));
        home.currentServiceId = "tab-2";
        tryCompare(glow, "tint", Qt.tint(Ui.Theme.identityColor("tab-2"), "transparent"));
        home.ambientSuppressed = true;
        verify(!glow.lit);
        home.ambientSuppressed = false;
        home.ambientEnabled = false;
        verify(!glow.lit);
        home.ambientEnabled = true;
        verify(glow.lit);
        keyClick(Qt.Key_Menu); focused("nav-0");
        keyClick(Qt.Key_Down); keyClick(Qt.Key_Return);
        compare(home.screen, "library");
        verify(!glow.lit);
    }
    // ADR-0136: cool hues reduce pool chroma from 0.10 to 0.065 at 240°,
    // and the reduction is gone 70° away from that hue.
    function test_cool_hue_chroma_is_capped() {
        const glow = child("ambientGlow");
        compare(glow.poolChroma(240), 0.065);
        compare(glow.poolChroma(170), 0.10);
        compare(glow.poolChroma(310), 0.10);
        const mid = glow.poolChroma(205);
        verify(Math.abs(mid - 0.0825) < 0.001, "mid chroma " + mid);
        let coolId = "";
        for (let i = 0; i < 80 && !coolId; ++i) {
            const id = "hue" + i;
            const h = glow.toOklch(Qt.color(Ui.Theme.identityColor(id))).h;
            const distance = Math.abs(((h - 240 + 540) % 360) - 180);
            if (distance < 25)
                coolId = id;
        }
        verify(coolId.length > 0, "a cool identity");
        home.continueEntries = [{id: coolId, title: "Cool", kind: "media"}];
        child("entry-" + coolId).forceActiveFocus();
        focused("entry-" + coolId);
        const identity = glow.toOklch(Qt.color(Ui.Theme.identityColor(coolId))).h;
        tryVerify(() => glow.shown && Math.abs(glow.hue - identity) < 0.5, 5000, "shown " + glow.hue);
        verify(glow.poolChroma(glow.hue) < 0.09);
        verify(glow.poolChroma(glow.hue) >= 0.065);
    }
    // Evening from 19:00, night from 23:00, by the shell clock. The setting
    // keeps the daytime light (spec §5.6, ADR-0136).
    function test_time_of_day_and_its_setting() {
        const glow = child("ambientGlow");
        compare(home.lightFollowsTime, true);
        compare(glow.followsTime, true);
        const expect = (hour, period, scale, veil, floorHue) => {
            glow.hour = hour;
            compare(glow.period, period);
            compare(glow.lightScale, scale);
            compare(glow.veilAmount, veil);
            compare(glow.floorHue, floorHue);
        };
        expect(6, "day", 1, 0, 55);
        expect(18, "day", 1, 0, 55);
        expect(19, "evening", 0.85, 0.06, 50);
        expect(22, "evening", 0.85, 0.06, 50);
        expect(23, "night", 0.6, 0.22, 47);
        expect(5, "night", 0.6, 0.22, 47);
        glow.hour = -1;
        home.clockText = "21:15";
        compare(glow.period, "evening");
        home.clockText = "23:00";
        compare(glow.period, "night");
        home.clockText = "06:00";
        compare(glow.period, "day");
        home.lightFollowsTime = false;
        compare(glow.followsTime, false);
        glow.hour = 23;
        compare(glow.period, "day");
        compare(glow.lightScale, 1);
        compare(glow.veilAmount, 0);
        compare(glow.floorHue, 55);
        home.lightFollowsTime = true;
        compare(glow.period, "night");
        verify(child("hearthVeil").visible);
        compare(glow.nightText2, Ui.Theme.nightText2);
    }
    // Weakest text, with the veil, at the brightest point of the light
    // (bottom centre, pool under the sample, hue 55°). Night text-3 and
    // evening hints must hold at least 4.5:1. The brief's 0.71 and the day
    // hint do not, which is why those two colours are lighter.
    function test_night_and_evening_text_holds_contrast() {
        const glow = child("ambientGlow");
        home.clockText = "23:30";
        compare(glow.period, "night");
        compare(Ui.Theme.period, "night");
        compare(Ui.Theme.textPrimary, glow.oklch(0.97, 0.006, 75, 1));
        compare(Ui.Theme.textSecondary, glow.oklch(0.84, 0.008, 75, 1));
        compare(Ui.Theme.textHint, glow.oklch(0.75, 0.008, 75, 1));
        compare(Ui.Theme.textHint, Ui.Theme.nightText3);
        verify(near(child("heroTitle").color, Ui.Theme.textPrimary), child("heroTitle").color);
        verify(near(child("heroMeta").color, Ui.Theme.textSecondary), child("heroMeta").color);
        const flat = Ui.Theme.veiledContrast(Ui.Theme.textHint, glow.veilAmount);
        verify(flat >= 4.7, "night text-3 on the flat background " + flat);
        const night = glow.weakestTextContrast(Ui.Theme.textHint);
        const brief = glow.weakestTextContrast(glow.oklch(0.71, 0.008, 75, 1));
        verify(night.y > glow.height * 0.9, "brightest point y " + night.y);
        verify(night.contrast >= 4.5, "night text-3 at the brightest light " + night.contrast);
        verify(brief.contrast < 4.5, "brief text-3 0.71 at that point " + brief.contrast);
        console.log("night text-3 at the brightest light " + night.contrast.toFixed(2)
                    + ":1 (brief 0.71 is " + brief.contrast.toFixed(2) + ":1, flat background "
                    + flat.toFixed(2) + ":1)");

        home.clockText = "20:30";
        compare(glow.period, "evening");
        verify(near(Ui.Theme.textPrimary, Qt.color("#F4F5F7")), Ui.Theme.textPrimary);
        verify(near(child("heroTitle").color, Ui.Theme.textPrimary), child("heroTitle").color);
        compare(Ui.Theme.textHint, Ui.Theme.eveningText3);
        compare(Ui.Theme.textHint, glow.oklch(0.80, 0.008, 75, 1));
        const evening = glow.weakestTextContrast(Ui.Theme.textHint);
        const dayHint = glow.weakestTextContrast(Qt.color("#9A9DA3"));
        verify(evening.y > glow.height * 0.9, "evening brightest y " + evening.y);
        verify(evening.contrast >= 4.5, "evening hint at the brightest light " + evening.contrast);
        verify(dayHint.contrast < 4.5, "day hint at that point " + dayHint.contrast);
        console.log("evening hint at the brightest light " + evening.contrast.toFixed(2)
                    + ":1 (day hint is " + dayHint.contrast.toFixed(2) + ":1)");

        home.clockText = "12:00";
        compare(glow.period, "day");
        verify(near(Ui.Theme.textPrimary, Qt.color("#F4F5F7")), Ui.Theme.textPrimary);
        verify(near(child("heroTitle").color, Ui.Theme.textPrimary), child("heroTitle").color);
        verify(near(Ui.Theme.textHint, Qt.color("#9A9DA3")), Ui.Theme.textHint);
    }
    // Handoff rule: light under text stays at or below L 0.48. Measured by
    // compositing the same gradient stops the small canvases paint, in sRGB
    // source-over on the screen background, then reading OKLab L. The pool
    // is placed under every sample. Daytime hue 55° is the brightest case.
    function test_light_under_text_stays_within_l_048() {
        const glow = child("ambientGlow");
        glow.hour = 12;
        compare(glow.period, "day");
        compare(glow.strength, 1);
        const floor = child("hearthFloor");
        verify(floor.width * 5 <= glow.width + 5);
        verify(floor.height * 5 <= glow.height + 5);
        const worst = glow.brightestLightness();
        verify(worst.l <= 0.48, "brightest " + worst.l);
        const title = child("heroTitle");
        const at = title.mapToItem(glow, title.width / 2, title.height);
        const underTitle = glow.lightnessAt(at.x, at.y, at.x, 55);
        verify(underTitle <= 0.48, "hero " + underTitle);
        verify(underTitle < worst.l);
    }
}
