// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import QtTest
import "../../apps/arinn-shell/ui" as Ui

// Hearth light typography (ADR-0136, #722): content titles in Ysabeau with
// lining figures, system text in Onest, sentence-case eyebrows, and content
// titles that fit in steps instead of eliding at their largest size.
TestCase {
    id: suite
    name: "Typography"
    width: 1920
    height: 1080
    visible: true
    when: windowShown

    Component { id: textFactory; Ui.UiText { color: "white" } }

    function make(props) {
        const item = createTemporaryObject(textFactory, suite, props);
        verify(item);
        return item;
    }

    function test_content_face_loads() {
        tryCompare(Ui.Theme.ysabeau, "status", FontLoader.Ready);
        verify(Ui.Theme.contentFontFamily.length > 0);
        verify(Ui.Theme.contentFontFamily !== Ui.Theme.fontFamily);
        compare(Ui.Theme.background, "#0F1115");
        compare(Ui.Theme.ember, "#FF8A3D");
        compare(Ui.Theme.chip, "#1C1E23");
        compare(Ui.Theme.hearthLightness, 0.44);
        compare(Ui.Theme.hearthChroma, 0.07);
        compare(Ui.Theme.hearthHue, 55);
        compare(Ui.Theme.poolLightness, 0.50);
        compare(Ui.Theme.poolChromaWarm, 0.10);
        compare(Ui.Theme.poolChromaCool, 0.065);
        compare(Ui.Theme.matThickness, 5);
    }

    function test_content_roles_use_the_content_face_data() {
        return [
            {tag: "display", role: "display", size: 220},
            {tag: "hero", role: "hero", size: 96},
            {tag: "detailTitle", role: "detailTitle", size: 104}
        ];
    }
    function test_content_roles_use_the_content_face(data) {
        const item = make({role: data.role, text: "Тихая гавань 2024"});
        compare(item.contentFace, true);
        compare(item.font.family, Ui.Theme.contentFontFamily);
        compare(item.font.pixelSize, data.size);
        compare(item.font.weight, 600);
        compare(item.font.letterSpacing, 0);
        // Lining figures: a year or a clock never uses old-style figures.
        compare(item.font.features["lnum"], 1);
        compare(item.font.features["tnum"], 1);
    }

    function test_system_roles_stay_in_onest_data() {
        return [
            {tag: "title", role: "title", size: 56, weight: 600, spacing: 0},
            {tag: "eyebrow", role: "eyebrow", size: 26, weight: 500, spacing: 0},
            {tag: "body", role: "body", size: 30, weight: 400, spacing: 0},
            {tag: "section", role: "section", size: 30, weight: 600, spacing: 0}
        ];
    }
    function test_system_roles_stay_in_onest(data) {
        const item = make({role: data.role, text: "Settings"});
        compare(item.contentFace, false);
        compare(item.font.family, Ui.Theme.fontFamily);
        compare(item.font.pixelSize, data.size);
        compare(item.font.weight, data.weight);
        fuzzyCompare(item.font.letterSpacing, data.spacing, 0.01);
        compare(item.font.capitalization, Font.MixedCase);
    }

    // The heavier content weight draws more ink than 400: the wght axis
    // reaches Ysabeau as it reaches Onest (#546).
    function test_content_weight_reaches_the_axis() {
        tryCompare(Ui.Theme.ysabeau, "status", FontLoader.Ready);
        const regular = make({role: "hero", text: "Hamburgefonstiv", wrapMode: Text.NoWrap});
        regular.font.weight = 400;
        const heavy = make({role: "hero", text: "Hamburgefonstiv", wrapMode: Text.NoWrap, y: 200});
        waitForRendering(heavy);
        verify(heavy.implicitWidth > regular.implicitWidth,
               "600 is wider than 400: " + heavy.implicitWidth + " vs " + regular.implicitWidth);
    }

    function test_hero_fits_in_steps_data() {
        return [
            {tag: "one line", text: "Тихая гавань", step: 0, size: 96, lines: 1, truncated: false},
            {tag: "two lines", text: "Путешествие на край долгой ночи и обратно", step: 1, size: 68, lines: 2, truncated: false},
            {tag: "too long", text: "Путешествие на край долгой ночи и обратно, а потом ещё раз туда и снова домой через всю страну", step: 1, size: 68, lines: 2, truncated: true}
        ];
    }
    function test_hero_fits_in_steps(data) {
        tryCompare(Ui.Theme.ysabeau, "status", FontLoader.Ready);
        const item = make({role: "hero", width: 1300, elide: Text.ElideRight,
                           fitSteps: [[96, 1], [68, 2]], text: data.text});
        compare(item.fitStep, data.step);
        compare(item.font.pixelSize, data.size);
        compare(item.maximumLineCount, data.lines);
        compare(item.truncated, data.truncated);
    }

    function test_detail_fits_in_steps_data() {
        return [
            {tag: "two lines", text: "Тихая гавань", step: 0, size: 104, lines: 2, truncated: false},
            {tag: "three lines", text: "Путешествие на край долгой ночи и обратно домой через всю страну", step: 1, size: 80, lines: 3, truncated: false},
            {tag: "too long", text: "Путешествие на край долгой ночи и обратно, а потом ещё раз туда и снова домой через всю страну и дальше за горизонт без остановки", step: 1, size: 80, lines: 3, truncated: true}
        ];
    }
    function test_detail_fits_in_steps(data) {
        tryCompare(Ui.Theme.ysabeau, "status", FontLoader.Ready);
        const item = make({role: "detailTitle", width: 1140, elide: Text.ElideRight,
                           fitSteps: [[104, 2], [80, 3]], text: data.text});
        compare(item.fitStep, data.step);
        compare(item.font.pixelSize, data.size);
        compare(item.maximumLineCount, data.lines);
        compare(item.truncated, data.truncated);
    }

    // A later, shorter title goes back to the largest step.
    function test_fit_returns_to_the_largest_step() {
        tryCompare(Ui.Theme.ysabeau, "status", FontLoader.Ready);
        const item = make({role: "hero", width: 1300, elide: Text.ElideRight, fitSteps: [[96, 1], [68, 2]],
                           text: "Путешествие на край долгой ночи и обратно"});
        compare(item.font.pixelSize, 68);
        item.text = "Тихая гавань";
        compare(item.fitStep, 0);
        compare(item.font.pixelSize, 96);
        compare(item.maximumLineCount, 1);
    }

    // A face that arrives after the first measurement changes the family:
    // the title is measured again.
    function test_family_change_refits() {
        const item = make({role: "hero", width: 1300, elide: Text.ElideRight, fitSteps: [[96, 1], [68, 2]],
                           text: "Тихая гавань"});
        const before = item.fitCount;
        compare(item.fittedFamily, item.font.family);
        item.font.family = Ui.Theme.fontFamily;
        compare(item.fitCount, before + 1);
        compare(item.fittedFamily, Ui.Theme.fontFamily);
        // A size change made by fit() itself does not measure again.
        const after = item.fitCount;
        item.font.pixelSize = 50;
        compare(item.fitCount, after);
    }

    // Without steps a role keeps its own size.
    function test_no_steps_keeps_the_role_size() {
        const item = make({role: "hero", width: 300, text: "Путешествие на край долгой ночи и обратно"});
        compare(item.fitStep, -1);
        compare(item.font.pixelSize, 96);
    }
}
