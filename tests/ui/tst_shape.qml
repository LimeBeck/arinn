// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import QtTest
import "../../apps/arinn-shell/ui" as Ui

// Shape roles and focus (ADR-0136, #724): captions under artwork, plates by
// role, the mat and rim, and row brightness.
TestCase {
    id: suite
    name: "ShapeFocus"
    width: 1920
    height: 1080
    visible: true
    when: windowShown

    Component { id: cardFactory; Ui.ContentCard { text: "Harbor"; subtitle: "S1 E4" } }
    Component { id: buttonFactory; Ui.ActionButton { text: "Play" } }
    Component { id: tileFactory; Ui.AppTile { text: "Library"; monogram: "L" } }
    Component {
        id: rowFactory
        Ui.HomeRow { width: 1600; variant: "content" }
    }

    function test_content_caption_sits_below_the_image() {
        const card = createTemporaryObject(cardFactory, suite, {
            showProgress: true, progress: 0.4
        });
        compare(card.cornerRadius, Ui.Theme.stillRadius);
        compare(card.frameHeight, 225);
        const title = findChild(card, "cardTitle");
        const caption = findChild(card, "cardCaption");
        tryVerify(() => title.y >= 0);
        verify(caption.y >= card.frameHeight, "caption y " + caption.y);
        compare(findChild(card, "artworkScrim"), null);
        const line = findChild(card, "emberLine");
        const band = findChild(card, "emberBand");
        verify(line !== null && band !== null);
        compare(line.height, Ui.Theme.emberLine);
        compare(band.height, Ui.Theme.emberBand);
        verify(line.mapToItem(card, 0, line.height).y <= card.frameHeight + 0.5);
        verify(band.mapToItem(card, 0, band.height).y <= line.mapToItem(card, 0, 0).y + 0.5);
    }

    function test_artwork_focus_is_mat_then_rim() {
        const card = createTemporaryObject(cardFactory, suite);
        card.forceActiveFocus();
        tryCompare(card, "activeFocus", true);
        tryCompare(card, "scale", Ui.Theme.contentFocusScale);
        compare(card.lift, Ui.Theme.contentFocusLift);
        compare(card.transformOrigin, Item.Bottom);
        const mat = findChild(card, "focusMat");
        const ring = findChild(card, "focusRing");
        const bounce = findChild(card, "focusBounce");
        const cast = findChild(card, "castLight");
        verify(mat !== null && ring !== null && bounce !== null && cast !== null);
        tryCompare(mat, "opacity", 1);
        tryCompare(ring, "opacity", 1);
        tryCompare(cast, "opacity", 1);
        compare(mat.border.width, Ui.Theme.matThickness);
        verify(Qt.colorEqual(mat.border.color, Ui.Theme.mat));
        compare(ring.thickness, Ui.Theme.rimThickness);
        compare(findChild(card, "rowShade").opacity, 0);
    }

    function test_action_plates() {
        const button = createTemporaryObject(buttonFactory, suite);
        verify(Qt.colorEqual(button.background.color, Ui.Theme.plateAction));
        button.primary = true;
        tryVerify(() => Qt.colorEqual(button.background.color, Ui.Theme.platePrimary));
        button.forceActiveFocus();
        tryCompare(button, "activeFocus", true);
        tryVerify(() => Qt.colorEqual(button.background.color, Ui.Theme.textPrimary));
        verify(Qt.colorEqual(button.contentItem.color, Ui.Theme.ink));
        compare(button.focusLift, 0);
        compare(findChild(button, "focusRing"), null);
    }

    function test_object_and_container_tiles() {
        const tile = createTemporaryObject(tileFactory, suite);
        compare(tile.cornerRadius, Ui.Theme.objectRadius);
        verify(Qt.colorEqual(tile.background.color, Ui.Theme.plateObject));
        tile.forceActiveFocus();
        tryCompare(tile, "activeFocus", true);
        tryVerify(() => Qt.colorEqual(tile.background.color, Ui.Theme.plateObjectFocus));
        tryCompare(tile, "scale", Ui.Theme.contentFocusScale);
        compare(tile.lift, Ui.Theme.contentFocusLift);

        const row = createTemporaryObject(tileFactory, suite, {shapeRole: "container", y: 400});
        compare(row.cornerRadius, Ui.Theme.containerRadius);
        verify(row.background.color.a < 0.01);
        compare(row.focusLift, 0);
        row.forceActiveFocus();
        tryCompare(row, "activeFocus", true);
        tryVerify(() => Qt.colorEqual(row.background.color, Ui.Theme.plateContainer));
        tryCompare(findChild(row, "focusRing"), "opacity", 1);
    }

    function test_row_brightness() {
        const entries = [
            {id: "a", title: "Alpha", subtitle: "One"},
            {id: "b", title: "Beta", subtitle: "Two"}
        ];
        const lit = createTemporaryObject(rowFactory, suite, {entries: entries, y: 40});
        const dim = createTemporaryObject(rowFactory, suite, {entries: entries, y: 500, objectName: "other"});
        tryCompare(lit, "count", 2);
        const first = findChild(lit, "entry-a");
        const second = findChild(lit, "entry-b");
        const other = findChild(dim, "entry-a");
        first.forceActiveFocus();
        tryCompare(first, "activeFocus", true);
        tryCompare(findChild(first, "rowShade"), "opacity", 0);
        tryVerify(() => Math.abs(findChild(second, "rowShade").opacity - (1 - Ui.Theme.rowLitBrightness)) < 0.02);
        tryVerify(() => Math.abs(findChild(other, "rowShade").opacity - (1 - Ui.Theme.rowDimBrightness)) < 0.02);
        compare(second.rowBrightness, Ui.Theme.rowLitBrightness);
        compare(other.rowBrightness, Ui.Theme.rowDimBrightness);
    }
}
