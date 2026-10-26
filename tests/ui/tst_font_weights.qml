// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import QtTest
import "../../apps/arinn-shell/ui" as Ui

// #546: the bundled Onest is a variable font (wght 100–900). Weighted text
// must render at its weight: heavier text has a wider advance and more ink.
// The same string is drawn at 400 and 700 and compared.
TestCase {
    id: suite
    name: "FontWeights"
    width: 1920
    height: 1080
    visible: true
    when: windowShown

    readonly property string sample: "Hamburgefonstiv 0123"

    Component {
        id: uiTextFactory
        Ui.UiText {
            font.pixelSize: 64
            wrapMode: Text.NoWrap
            color: "white"
            text: suite.sample
        }
    }
    // Qt's own matching of the family by weight, without the shell's text
    // component: recorded, not asserted, so the log shows what this Qt does.
    Component {
        id: plainTextFactory
        Text {
            font.family: Ui.Theme.fontFamily
            font.pixelSize: 64
            color: "white"
            text: suite.sample
        }
    }
    Component {
        id: backdropFactory
        Rectangle { width: 1920; height: 140; color: "black" }
    }

    // Lit pixels of the item drawn white on black.
    function ink(item) {
        const image = grabImage(item);
        let lit = 0;
        for (let y = 0; y < item.height; ++y)
            for (let x = 0; x < item.width; ++x)
                lit += image.red(x, y);
        return lit / 255;
    }
    function render(factory, weight) {
        const backdrop = createTemporaryObject(backdropFactory, suite);
        const item = createTemporaryObject(factory, backdrop, {"font.weight": weight});
        waitForRendering(item);
        return {advance: item.contentWidth, ink: ink(item), item: item};
    }

    function test_plain_text_weight_is_recorded() {
        const regular = render(plainTextFactory, 400);
        const bold = render(plainTextFactory, 700);
        console.log("Qt matches the bundled Onest by font.weight:", bold.advance !== regular.advance,
                    "(advance 400:", regular.advance, "700:", bold.advance + ")");
    }

    function test_weights_render_heavier_data() {
        return [
            {tag: "500", weight: 500},
            {tag: "600", weight: 600},
            {tag: "700", weight: 700}
        ];
    }
    function test_weights_render_heavier(data) {
        const regular = render(uiTextFactory, 400);
        const heavier = render(uiTextFactory, data.weight);
        verify(heavier.advance > regular.advance,
               "advance " + data.weight + ": " + heavier.advance + ", 400: " + regular.advance);
        verify(heavier.ink > regular.ink * 1.05,
               "ink " + data.weight + ": " + heavier.ink + ", 400: " + regular.ink);
    }

    // The weight of a role (title 700) renders, and a later change too.
    function test_role_and_changed_weight() {
        const body = render(uiTextFactory, 400);
        const backdrop = createTemporaryObject(backdropFactory, suite);
        const title = createTemporaryObject(uiTextFactory, backdrop, {role: "title"});
        title.font.pixelSize = 64;
        title.font.letterSpacing = 0;
        compare(title.font.weight, 700);
        tryVerify(() => title.contentWidth > body.advance);
        const bold = title.contentWidth;
        title.font.weight = 400;
        tryCompare(title, "contentWidth", body.advance);
        title.role = "section";
        tryVerify(() => title.contentWidth > body.advance && title.contentWidth < bold);
    }
}
