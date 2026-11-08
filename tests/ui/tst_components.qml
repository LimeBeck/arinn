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
    Component {
        id: iconBoardFactory
        Rectangle {
            property alias name: boardIcon.name
            width: 96; height: 96; color: "black"
            Ui.LineIcon { id: boardIcon; anchors.fill: parent; color: "white" }
        }
    }
    // #837: Canvas angles run clockwise from +x; the power ring opens at the
    // top, around its stem, and is closed at the bottom.
    function test_power_icon_opens_at_the_top() {
        const board = createTemporaryObject(iconBoardFactory, suite, {name: "power"});
        // 24-unit glyph at 96 px: 4 px per unit; ring centre (12, 13.5), r 7.2.
        function at(degrees) {
            const a = degrees * Math.PI / 180;
            return {x: Math.round(4 * (12 + 7.2 * Math.cos(a))), y: Math.round(4 * (13.5 + 7.2 * Math.sin(a)))};
        }
        tryVerify(() => {
            const pixels = grabImage(board);
            const bottom = at(90), left = at(180), right = at(0), gap = at(-70);
            return pixels.red(bottom.x, bottom.y) > 128 && pixels.red(left.x, left.y) > 128
                && pixels.red(right.x, right.y) > 128 && pixels.red(gap.x, gap.y) < 64;
        }, 2000, "power ring: closed at the bottom, open at the top");
    }
    Component { id: cardFactory; Ui.ContentCard { x: 150; y: 150; text: "Content" } }
    Component { id: tileFactory; Ui.AppTile { x: 150; y: 150; text: "Library"; monogram: "L" } }
    Component { id: paneFactory; Ui.StatusPane { x: 150; y: 150; title: "Status"; actionText: "Retry" } }
    Component { id: trackFactory; Ui.ProgressTrack {} }
    Component { id: canvasFactory; Ui.LogicalCanvas {} }
    Component { id: textFactory; Ui.UiText {} }
    SignalSpy { id: activated; signalName: "activated" }

    function init() { activated.clear(); }
    function cleanup() { activated.target = null; }

    function test_icon_fallback_and_identity_color() {
        const tile = createTemporaryObject(tileFactory, suite, {visualIdentity: "provider:fixture"});
        const color = tile.accentColor;
        compare(findChild(tile, "applicationFallback").visible, true);
        tile.text = "Renamed";
        compare(tile.accentColor, color);
        tile.iconSource = Qt.resolvedUrl("../../apps/arinn-shell/gallery/fixture-artwork.svg");
        tryCompare(tile, "iconReady", true);
        compare(findChild(tile, "applicationFallback").visible, false);
        const chip = findChild(tile, "applicationChip");
        const icon = findChild(tile, "applicationIcon");
        compare(chip.color, Qt.rgba(1, 1, 1, 0.1));
        compare(icon.width, chip.width);
        compare(icon.height, chip.height);
        // The loaded artwork reaches the chip edge: no invented 8px mat.
        tryVerify(() => {
            const pixels = grabImage(tile);
            const edge = chip.mapToItem(tile, 4, 30);
            return pixels.blue(edge.x, edge.y) > pixels.red(edge.x, edge.y);
        });
        tile.iconSource = Qt.resolvedUrl("../../docs/design/handoff/assets/arinn-mark.svg");
        tryCompare(tile, "iconReady", true);
        compare(chip.color, Qt.rgba(1, 1, 1, 0.1));
        tile.iconSource = "";
        tryCompare(tile, "iconReady", false);
        compare(findChild(tile, "applicationFallback").visible, true);
        // A letter sits on the neutral chip, never on a hashed pastel (#756 C19).
        verify(Qt.colorEqual(chip.color, Ui.Theme.chip));
        tile.forceActiveFocus();
        tryCompare(findChild(tile, "focusRing"), "opacity", 1);
    }
    // A card without artwork shows its letter on the neutral chip, so one
    // service never wears two pastel colours (#756 C19).
    function test_card_letter_chip_is_neutral() {
        const card = createTemporaryObject(cardFactory, suite, {visualIdentity: "provider:fixture"});
        const chip = findChild(card, "cardChip");
        verify(chip.visible);
        verify(Qt.colorEqual(chip.color, Ui.Theme.chip));
    }
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
        // Only the image lifts, from its bottom edge; the card and its
        // caption stay (#837).
        tryCompare(card, "imageScale", Ui.Theme.contentFocusScale);
        compare(card.imageLift, Ui.Theme.contentFocusLift);
        compare(card.scale, 1);
        compare(card.lift, 0);
        const ring = findChild(card, "focusRing");
        verify(ring.z > card.contentItem.z);
        tryCompare(ring, "opacity", 1);
        card.pointerMode = true;
        tryCompare(card, "imageScale", 1.02);
        compare(card.imageLift, -2);
        keyPress(Qt.Key_Space);
        tryCompare(card, "down", true);
        tryVerify(() => card.scale < 1);
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
    // A 20×30 portrait: blue top and bottom thirds, a red middle (#360).
    readonly property string portraitFixture: "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAABQAAAAeCAIAAACjcKk8AAAAJUlEQVR42mPQ0LhDNmIY1TwyNN+hQPeo5lHNg1nzaGEwqpkQAgBlZb8/WLTFKQAAAABJRU5ErkJggg=="
    function test_artwork_is_centre_cropped_and_released() {
        const card = createTemporaryObject(cardFactory, suite, {artwork: portraitFixture});
        compare(card.width, 400);
        compare(card.frameHeight, 225);
        compare(card.height, 225 + Ui.Theme.contentCaptionHeight);
        const canvas = findChild(card, "cardArtworkCanvas");
        verify(canvas);
        tryCompare(canvas, "showsArtwork", true);
        // Centre crop (#609): a stretched portrait shows its blue top third
        // at y 60 of the card; the crop shows the red middle there.
        tryVerify(() => {
            const pixels = grabImage(card);
            return pixels.red(200, 60) > 150 && pixels.blue(200, 60) < 100
                && pixels.red(200, 112) > 150 && pixels.blue(200, 112) < 100;
        });
        // Another artwork releases the previous one from the canvas.
        const portrait = canvas.loadedSource;
        verify(portrait.length > 0);
        card.artwork = Qt.resolvedUrl("../../apps/arinn-shell/gallery/fixture-artwork.svg");
        tryVerify(() => canvas.loadedSource.length > 0 && canvas.loadedSource !== portrait);
        const landscape = canvas.loadedSource;
        tryVerify(() => canvas.isImageLoaded(landscape));
        verify(!canvas.isImageLoaded(portrait));
        tryCompare(canvas, "showsArtwork", true);
        // Back to an image Qt still has cached: the picture may stay Ready
        // without a status change, and the canvas must still follow it.
        card.artwork = portraitFixture;
        tryCompare(canvas, "loadedSource", portrait);
        tryVerify(() => canvas.isImageLoaded(portrait));
        verify(!canvas.isImageLoaded(landscape));
        tryVerify(() => {
            const pixels = grabImage(card);
            return pixels.red(200, 60) > 150 && pixels.blue(200, 60) < 100;
        });
        card.artwork = "";
        tryCompare(canvas, "showsArtwork", false);
        compare(canvas.loadedSource, "");
        verify(!canvas.isImageLoaded(portrait));
    }
    // A poster card (Home's New row) keeps long titles and subtitles inside
    // the card: a word wider than the column wraps or elides instead of
    // running past the edge.
    function test_poster_text_stays_inside_the_card_data() {
        return [
            {tag: "long-word", title: "Edgerunnersextended Supercalifragilistic", subtitle: "Superlongsubtitlewithoutspaces"},
            {tag: "cyrillic", title: "Маленькие демоны путешествуют на Запад", subtitle: "2025"},
            {tag: "release", title: "Cyberpunk Edgerunners S02 2160p NF WEB-DL", subtitle: ""}
        ];
    }
    function test_poster_text_stays_inside_the_card(data) {
        const card = createTemporaryObject(cardFactory, suite, {
            width: 180, text: data.title, subtitle: data.subtitle, showProgress: true, progress: 0.3
        });
        compare(card.frameHeight, 270);
        const title = findChild(card, "cardTitle");
        const subtitle = findChild(card, "cardSubtitle");
        tryVerify(() => title.width > 0);
        verify(title.mapToItem(card, 0, 0).y >= card.frameHeight, "the title sits below the image");
        for (const text of [title, subtitle]) {
            if (!text.visible)
                continue;
            verify(text.contentWidth <= text.width + 0.5,
                   text.objectName + " is " + text.contentWidth + " wide in a column of " + text.width);
            const left = text.mapToItem(card, 0, 0).x;
            verify(left >= 0 && left + text.width <= card.width,
                   text.objectName + " spans " + left + "–" + (left + text.width) + " of " + card.width);
        }
        verify(title.lineCount <= 2);
        verify(title.mapToItem(card, 0, 0).y >= 0, "the title starts inside the card");
    }
    // #656 (owner decision 2026-11-01): a Favorites tile shows its name on up
    // to two lines inside the tile instead of eliding after a few letters.
    function test_tile_name_uses_two_lines_data() {
        return [
            {tag: "site-or-search", title: "Сайт или поиск", whole: true},
            {tag: "series", title: "Сериалы онлайн", whole: true},
            {tag: "one-word", title: "Кинопоиск", whole: true},
            {tag: "too-long", title: "Невероятнодлинноеназвание сервиса для телевизора", whole: false}
        ];
    }
    function test_tile_name_uses_two_lines(data) {
        const tile = createTemporaryObject(tileFactory, suite, {width: 264, height: 132, text: data.title});
        const name = findChild(tile, "tileTitle");
        verify(name);
        tryVerify(() => name.width > 0);
        verify(name.lineCount <= 2);
        compare(name.truncated, !data.whole);
        verify(name.contentWidth <= name.width + 0.5, "name is " + name.contentWidth + " wide in " + name.width);
        const box = name.mapToItem(tile, 0, 0);
        verify(box.x + name.width <= tile.width && box.y >= 0 && box.y + name.height <= tile.height,
               "name box " + box.x + "," + box.y + " " + name.width + "×" + name.height);
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
        verify(button.width >= button.labelItem.implicitWidth + 80);
        verify(button.labelItem.font.pixelSize >= 26);
        compare(button.labelItem.font.family, Ui.Theme.fontFamily);
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
