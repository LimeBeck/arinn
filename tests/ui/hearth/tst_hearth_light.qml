// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import QtTest
import "../../../apps/arinn-shell/ui" as Ui
import "../../../apps/arinn-shell/ui/screens/settings" as Settings
import "../../../apps/arinn-shell/ui/screens/downloads" as Downloads
import "../../../apps/arinn-shell/ui/screens/player" as Player

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
        id: settingsFactory
        Settings.SettingsView { width: 1920; height: 1080 }
    }
    // Rows live in QML so a path list stays an array. A JS property passed
    // through createTemporaryObject does not.
    Component {
        id: nightSettingsFactory
        Settings.SettingsView {
            width: 1920; height: 1080
            clockText: "23:30"
            lightFollowsTime: true
            categories: [{ id: "media", title: "Media", icon: "video_library" }]
            rows: [
                { providerId: "library", providerName: "Library", id: "scan", categoryId: "media",
                  title: "Scan folders", description: "Look for new files", type: "boolean",
                  currentValue: false, constraints: {}, enabled: true },
                { providerId: "library", providerName: "Library", id: "roots", categoryId: "media",
                  title: "Folders", description: "Places to read", type: "path",
                  currentValue: ["/videos"], constraints: { directory: true, multiple: true, maxItems: 3 },
                  enabled: true },
                { providerId: "library", providerName: "Library", id: "locked", categoryId: "media",
                  title: "Locked choice", description: "Cannot be changed now", type: "enum",
                  currentValue: "a", constraints: { options: ["a"] }, enabled: false }
            ]
        }
    }
    Component {
        id: downloadsFactory
        Downloads.DownloadsView { width: 1920; height: 1080 }
    }
    Component {
        id: playerFactory
        Player.PlayerView { width: 1920; height: 1080 }
    }
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
    function near(a, b) {
        return Math.abs(a.r - b.r) < 0.004 && Math.abs(a.g - b.g) < 0.004
            && Math.abs(a.b - b.b) < 0.004 && Math.abs(a.a - b.a) < 0.004;
    }
    // Source-over of a translucent coat on an opaque base.
    function over(base, coat) {
        const a = coat.a;
        return Qt.rgba(coat.r * a + base.r * (1 - a),
                       coat.g * a + base.g * (1 - a),
                       coat.b * a + base.b * (1 - a), 1);
    }
    // Contrast of text on a surface, both under this light's veil.
    function veiled(glow, textColor, surface) {
        return Ui.Theme.contrast(Ui.Theme.veilOver(textColor, glow.veilAmount),
                                 Ui.Theme.veilOver(surface, glow.veilAmount));
    }
    // Brightest lit pixel. The pool sits under each sample at the daytime
    // floor hue, which is the brightest the light can be there.
    function brightestLit(glow) {
        let best = null, bestLum = -1, bx = 0, by = 0;
        for (let iy = 0; iy <= 40; ++iy) {
            const y = glow.height * iy / 40;
            for (let ix = 0; ix <= 20; ++ix) {
                const x = glow.width * ix / 20;
                const bg = glow.litColorAt(x, y, x, glow.floorHueDay);
                const lum = Ui.Theme.relativeLuminance(bg);
                if (lum > bestLum) { bestLum = lum; best = bg; bx = x; by = y; }
            }
        }
        return {color: best, x: bx, y: by};
    }
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
        // Over an Activity the light stays on; its floor takes the
        // Activity's hue (ADR-0140, amending ADR-0027).
        home.ambientSuppressed = true;
        verify(glow.lit);
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
        home.screen = "applications";
        compare(glow.strength, 0.6);
        home.screen = "home";
        compare(glow.strength, 1);
    }
    // The warm veil stays on Settings and Downloads. The floor and the pool
    // stay off. The player has no veil (owner decision on #732).
    function test_veil_covers_settings_and_downloads() {
        // Destroying Downloads used to run a deferred focus restore and throw
        // "activeFocus is not defined" (#732).
        failOnWarning(/activeFocus is not defined/);
        const settings = createTemporaryObject(settingsFactory, suite, {clockText: "12:00", lightFollowsTime: true});
        verify(settings);
        const settingsGlow = findChild(settings, "settingsVeilGlow");
        const settingsVeil = findChild(settings, "hearthVeil");
        verify(settingsGlow);
        verify(settingsVeil);
        verify(!settingsGlow.lit);
        compare(settingsGlow.veilAmount, 0);
        verify(!settingsVeil.visible);
        settings.clockText = "20:30";
        compare(settingsGlow.period, "evening");
        compare(settingsGlow.veilAmount, 0.06);
        verify(settingsVeil.visible);
        settings.clockText = "23:30";
        compare(settingsGlow.period, "night");
        compare(settingsGlow.veilAmount, 0.22);
        verify(settingsVeil.visible);
        settings.lightFollowsTime = false;
        compare(settingsGlow.period, "day");
        verify(!settingsVeil.visible);
        settings.destroy();

        const downloads = createTemporaryObject(downloadsFactory, suite, {clockText: "23:30", lightFollowsTime: true});
        verify(downloads);
        const downloadsGlow = findChild(downloads, "downloadsVeilGlow");
        const downloadsVeil = findChild(downloads, "hearthVeil");
        verify(downloadsGlow && !downloadsGlow.lit);
        verify(downloadsVeil && downloadsVeil.visible);
        compare(downloadsGlow.veilAmount, 0.22);
        downloads.clockText = "12:00";
        compare(downloadsGlow.period, "day");
        verify(!downloadsVeil.visible);
        downloads.destroy();

        const player = createTemporaryObject(playerFactory, suite, {width: 1920, height: 1080, clockText: "23:30"});
        verify(player);
        verify(findChild(player, "hearthVeil") === null);
        verify(findChild(player, "settingsVeilGlow") === null);
        verify(findChild(player, "downloadsVeilGlow") === null);
        player.destroy();
        wait(0);
    }
    // Night text-3 holds textContrastMinimum on every night surface (#722):
    // the veiled background, a resting plate, the night focused plate, Home
    // and Applications at their brightest light, and Settings and Downloads
    // under the 22% veil. The day focus fill does not. Evening hints hold
    // eveningContrastMinimum on the evening light; the day hint does not.
    // Evening Home and the focused plate are both below textContrastMinimum.
    // That gap waits on the owner's threshold decision.
    function test_night_and_evening_text_holds_contrast() {
        const glow = child("ambientGlow");
        const minimum = Ui.Theme.textContrastMinimum;
        home.clockText = "23:30";
        home.screen = "home";
        compare(glow.period, "night");
        compare(Ui.Theme.period, "night");
        compare(Ui.Theme.textPrimary, glow.oklch(0.97, 0.006, 75, 1));
        compare(Ui.Theme.textSecondary, glow.oklch(0.84, 0.008, 75, 1));
        compare(Ui.Theme.textHint, glow.oklch(0.761, 0.008, 75, 1));
        compare(Ui.Theme.textHint, Ui.Theme.nightText3);
        compare(Ui.Theme.plateFocusNightAlpha, 0.095);
        verify(near(Ui.Theme.plateFocus, Qt.rgba(1, 1, 1, Ui.Theme.plateFocusNightAlpha)));
        verify(near(child("heroTitle").color, Ui.Theme.textPrimary), child("heroTitle").color);
        verify(near(child("heroMeta").color, Ui.Theme.textSecondary), child("heroMeta").color);

        const text = Ui.Theme.textHint;
        const flat = Ui.Theme.background;
        const plate = over(flat, Ui.Theme.plate);
        const focused = over(flat, Ui.Theme.plateFocus);
        compare(glow.strength, 1);
        const homeLit = brightestLit(glow);
        verify(homeLit.y > glow.height * 0.9, "brightest point y " + homeLit.y);
        home.screen = "applications";
        compare(glow.strength, 0.6);
        const appsLit = brightestLit(glow);
        home.screen = "home";

        const surfaces = [
            ["veiled background", veiled(glow, text, flat)],
            ["plate", veiled(glow, text, plate)],
            ["focused plate", veiled(glow, text, focused)],
            ["home", veiled(glow, text, homeLit.color)],
            ["applications", veiled(glow, text, appsLit.color)]
        ];
        const settings = createTemporaryObject(settingsFactory, suite, {clockText: "23:30", lightFollowsTime: true});
        verify(settings);
        const settingsGlow = findChild(settings, "settingsVeilGlow");
        verify(settingsGlow && !settingsGlow.lit);
        compare(settingsGlow.veilAmount, 0.22);
        const settingsField = settingsGlow.litColorAt(settingsGlow.width / 2, settingsGlow.height / 2, 0, 55);
        surfaces.push(["settings", veiled(settingsGlow, text, settingsField)]);
        settings.destroy();
        const downloads = createTemporaryObject(downloadsFactory, suite, {clockText: "23:30", lightFollowsTime: true});
        verify(downloads);
        const downloadsGlow = findChild(downloads, "downloadsVeilGlow");
        verify(downloadsGlow && !downloadsGlow.lit);
        compare(downloadsGlow.veilAmount, 0.22);
        const downloadsField = downloadsGlow.litColorAt(downloadsGlow.width / 2, downloadsGlow.height / 2, 0, 55);
        surfaces.push(["downloads", veiled(downloadsGlow, text, downloadsField)]);
        downloads.destroy();

        let report = "night text-3";
        for (let i = 0; i < surfaces.length; ++i) {
            verify(surfaces[i][1] >= minimum, surfaces[i][0] + " " + surfaces[i][1]);
            report += " " + surfaces[i][0] + " " + surfaces[i][1].toFixed(3);
        }
        const brief = glow.oklch(0.71, 0.008, 75, 1);
        const oneStepDown = glow.oklch(0.760, 0.008, 75, 1);
        const dayFill = over(flat, Qt.rgba(1, 1, 1, Ui.Theme.plateFocusAlpha));
        const fillUp = over(flat, Qt.rgba(1, 1, 1, Ui.Theme.plateFocusNightAlpha + 0.001));
        const briefHome = veiled(glow, brief, homeLit.color);
        const downHome = veiled(glow, oneStepDown, homeLit.color);
        const dayFocus = veiled(glow, text, dayFill);
        const fillStep = veiled(glow, text, fillUp);
        verify(briefHome < minimum, "brief text-3 0.71 on Home " + briefHome);
        verify(downHome < minimum, "0.760 on Home " + downHome);
        verify(dayFocus < minimum, "day focus fill " + dayFocus);
        verify(fillStep < minimum, "focus fill one step lighter " + fillStep);
        console.log(report + ":1 (brief 0.71 on Home " + briefHome.toFixed(3)
                    + ":1, 0.760 on Home " + downHome.toFixed(3)
                    + ":1, day focus fill " + dayFocus.toFixed(3)
                    + ":1, fill +0.001 " + fillStep.toFixed(3) + ":1)");

        home.clockText = "20:30";
        compare(glow.period, "evening");
        compare(Ui.Theme.textPrimary, glow.oklch(0.95, 0.008, 75, 1));
        verify(near(child("heroTitle").color, Ui.Theme.textPrimary), child("heroTitle").color);
        compare(Ui.Theme.textHint, Ui.Theme.eveningText3);
        compare(Ui.Theme.textHint, glow.oklch(0.778, 0.008, 75, 1));
        verify(near(Ui.Theme.plateFocus, Qt.rgba(1, 1, 1, Ui.Theme.plateFocusAlpha)));
        compare(glow.strength, 1);
        const eveningLit = brightestLit(glow);
        verify(eveningLit.y > glow.height * 0.9, "evening brightest y " + eveningLit.y);
        const evening = veiled(glow, Ui.Theme.textHint, eveningLit.color);
        const eveningFocus = veiled(glow, Ui.Theme.textHint, over(flat, Ui.Theme.plateFocus));
        const dayHint = veiled(glow, glow.oklch(0.66, 0.008, 75, 1), eveningLit.color);
        const eveningDown = veiled(glow, glow.oklch(0.777, 0.008, 75, 1), eveningLit.color);
        const eveningMinimum = Ui.Theme.eveningContrastMinimum;
        verify(evening >= eveningMinimum, "evening hint at the brightest light " + evening);
        verify(dayHint < eveningMinimum, "day hint at that point " + dayHint);
        verify(eveningDown < eveningMinimum, "0.777 at that point " + eveningDown);
        // Reported, not asserted: both are below the night bar, and the
        // owner has not chosen an evening threshold.
        const nightBar = Ui.Theme.textContrastMinimum;
        let belowNight = [];
        if (evening < nightBar)
            belowNight.push("Home " + evening.toFixed(3) + ":1");
        if (eveningFocus < nightBar)
            belowNight.push("focused plate " + eveningFocus.toFixed(3) + ":1");
        console.log("evening hint at the brightest light " + evening.toFixed(3)
                    + ":1 (day hint is " + dayHint.toFixed(3)
                    + ":1, focused plate " + eveningFocus.toFixed(3)
                    + ":1); below textContrastMinimum " + nightBar + ": "
                    + belowNight.join(", "));

        home.clockText = "12:00";
        compare(glow.period, "day");
        compare(Ui.Theme.textPrimary, glow.oklch(0.95, 0.008, 75, 1));
        verify(near(child("heroTitle").color, Ui.Theme.textPrimary), child("heroTitle").color);
        compare(Ui.Theme.textHint, glow.oklch(0.66, 0.008, 75, 1));
        verify(near(Ui.Theme.plateFocus, Qt.rgba(1, 1, 1, Ui.Theme.plateFocusAlpha)));
        const flatDay = Ui.Theme.contrast(Ui.Theme.textHint, Ui.Theme.background);
        verify(flatDay >= 4.5, "day text-3 on Charcoal " + flatDay);
    }
    // A focused Settings row is a container: an 8% plate and the rim.
    // White at 20% leaves night text-3 under textContrastMinimum; 8% holds it.
    function test_settings_focus_fill_holds_night_hint_contrast() {
        const settings = createTemporaryObject(nightSettingsFactory, suite);
        verify(settings);
        settings.forceActiveFocus();
        // Nested repeater delegates are visual children. findChild does not
        // see the path buttons from the view root.
        function named(item, name) {
            if (!item)
                return null;
            if (item.objectName === name)
                return item;
            const kids = item.children || [];
            for (let n = 0; n < kids.length; ++n) {
                const found = named(kids[n], name);
                if (found)
                    return found;
            }
            return null;
        }
        const glow = findChild(settings, "settingsVeilGlow");
        verify(glow && !glow.lit);
        compare(glow.period, "night");
        compare(glow.veilAmount, 0.22);
        compare(Ui.Theme.textHint, Ui.Theme.nightText3);
        const homeRing = findChild(findChild(home, "entry-a"), "focusRing");
        verify(homeRing !== null, "home focus ring");
        const checks = [
            ["category-media", () => settings.focusCategory(0)],
            ["setting-library-scan", () => settings.focusPart(0, "main")],
            ["path-library-roots-0", () => settings.focusPart(1, "path:0")],
            ["remove-library-roots-0", () => settings.focusPart(1, "remove:0")],
            ["add-library-roots", () => settings.focusPart(1, "add")]
        ];
        let rowRatio = 0;
        for (let i = 0; i < checks.length; ++i) {
            const name = checks[i][0];
            verify(checks[i][1]() !== false, name);
            const button = named(settings, name);
            verify(button !== null, name);
            tryCompare(button, "activeFocus", true, 5000, name);
            verify(near(button.background.color, Ui.Theme.plateContainer),
                   name + " fill " + button.background.color);
            verify(Math.abs(button.background.color.a - 0.20) > 0.05, name + " still at 20%");
            // The row's own outline. A keyboard pill inside the row is a
            // Surface, so it has a ring of its own and a descendant walk
            // would find that one first.
            let ring = null;
            const kids = button.children || [];
            for (let n = 0; n < kids.length; ++n) {
                if (kids[n].objectName === "focusRing") {
                    ring = kids[n];
                    break;
                }
            }
            verify(ring !== null, name + " outline");
            tryCompare(ring, "opacity", 1, 1000, name + " outline");
            compare(ring.thickness, homeRing.thickness);
            compare(ring.thickness, Ui.Theme.rimThickness);
            compare(ring.radius, button.background.radius);
            if (name === "setting-library-scan") {
                const surface = over(Ui.Theme.background, button.background.color);
                rowRatio = veiled(glow, Ui.Theme.textHint, surface);
                const dayFill = over(Ui.Theme.background, Qt.rgba(1, 1, 1, Ui.Theme.plateFocusAlpha));
                const dayRatio = veiled(glow, Ui.Theme.textHint, dayFill);
                verify(dayRatio < Ui.Theme.textContrastMinimum, "hardcoded 0.20 fill " + dayRatio);
                verify(rowRatio >= Ui.Theme.textContrastMinimum,
                       "focused settings row " + rowRatio);
                console.log("night textHint on a focused Settings row " + rowRatio.toFixed(3)
                            + ":1 (white at 20% is " + dayRatio.toFixed(3) + ":1)");
            }
        }
        verify(rowRatio >= Ui.Theme.textContrastMinimum);
        // A locked row has no plate until it is focused. Focus uses
        // Theme.plateDisabled, not the container plate, and draws the same
        // rim as the other rows. The title stays hint text. Night text-3 on
        // that plate is under textContrastMinimum; that is accepted.
        const locked = named(settings, "setting-library-locked");
        verify(locked !== null, "locked row");
        verify(locked.background.color.a < 0.01,
               "unfocused locked fill " + locked.background.color);
        verify(settings.focusPart(2, "main") !== false, "locked row");
        tryCompare(locked, "activeFocus", true, 5000, "locked row");
        verify(near(locked.background.color, Ui.Theme.plateDisabled),
               "locked fill " + locked.background.color);
        verify(!near(locked.background.color, Ui.Theme.plateFocus), "locked row uses the focus plate");
        const lockedTitle = named(settings, "title-library-locked");
        verify(lockedTitle !== null, "locked title");
        verify(near(lockedTitle.color, Ui.Theme.textHint), "locked title " + lockedTitle.color);
        let lockedRing = null;
        const lockedKids = locked.children || [];
        for (let n = 0; n < lockedKids.length; ++n) {
            if (lockedKids[n].objectName === "focusRing") {
                lockedRing = lockedKids[n];
                break;
            }
        }
        verify(lockedRing !== null, "locked outline");
        compare(lockedRing.shown, true);
        tryCompare(lockedRing, "opacity", 1, 1000, "locked outline");
        compare(lockedRing.thickness, homeRing.thickness);
        compare(lockedRing.thickness, Ui.Theme.rimThickness);
        compare(lockedRing.radius, locked.background.radius);
        const lockedSurface = over(Ui.Theme.background, locked.background.color);
        const lockedRatio = veiled(glow, Ui.Theme.textHint, lockedSurface);
        verify(lockedRatio < Ui.Theme.textContrastMinimum,
               "inactive row is not held to the focus bar " + lockedRatio);
        console.log("night textHint on a locked Settings row " + lockedRatio.toFixed(3) + ":1");
        settings.destroy();
        home.clockText = "12:00";
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
