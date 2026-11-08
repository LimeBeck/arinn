// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import QtTest
import "../../apps/arinn-shell/ui" as Ui
import "../../apps/arinn-shell/ui/screens/music" as Music

TestCase {
    id: suite
    name: "ArtworkCards"
    width: 1920; height: 1080
    visible: true
    when: windowShown
    readonly property string portrait: "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAABQAAAAeCAIAAACjcKk8AAAAJUlEQVR42mPQ0LhDNmIY1TwyNN+hQPeo5lHNg1nzaGEwqpkQAgBlZb8/WLTFKQAAAABJRU5ErkJggg=="
    Component { id: frameFactory; Ui.ArtworkFrame { x: 200; y: 200; width: 200; height: 100 } }
    Component {
        id: musicFactory
        Music.MusicCard {
            x: 120; y: 200
            entry: ({id: "fixture", kind: "album", title: "Harbor", artist: "Fixture"})
        }
    }
    function test_music_caption_stays_stationary_data() {
        return [{tag: "dpad", pointer: false}, {tag: "pointer", pointer: true}];
    }
    function test_music_caption_stays_stationary(data) {
        const card = createTemporaryObject(musicFactory, suite, {pointerMode: data.pointer});
        verify(card);
        wait(0);
        const caption = findChild(card, "musicCardTitle");
        verify(caption);
        const rest = caption.mapToItem(suite, 0, 0);
        Ui.Focus.take(card);
        tryCompare(card, "activeFocus", true);
        wait(Ui.Theme.focusDuration + 50);
        const focused = caption.mapToItem(suite, 0, 0);
        compare(focused.x, rest.x, "focus scales only the artwork");
        compare(focused.y, rest.y, "focus lifts only the artwork");
        compare(card.scale, 1);
    }
    function test_frame_crop_source_replacement_and_release() {
        const frame = createTemporaryObject(frameFactory, suite, {source: portrait});
        tryCompare(frame, "loaded", true);
        tryVerify(() => {
            const pixels = grabImage(frame);
            return pixels.red(100, 20) > 150 && pixels.blue(100, 20) < 100;
        });
        const old = frame.canvas.loadedSource;
        frame.source = Qt.resolvedUrl("../../apps/arinn-shell/gallery/fixture-artwork.svg");
        tryCompare(frame, "loaded", true);
        tryVerify(() => frame.canvas.loadedSource !== old);
        verify(!frame.canvas.isImageLoaded(old));
        frame.width = 160;
        frame.height = 90;
        tryVerify(() => frame.loaded && frame.canvas.width === 160 && frame.canvas.height === 90);
        frame.source = "";
        tryCompare(frame, "loaded", false);
        compare(frame.canvas.loadedSource, "");
        compare(frame.plate.visible, true);
        frame.source = portrait;
        tryCompare(frame, "loaded", true);
    }
    function test_frame_image_focus_and_safe_edge() {
        const frame = createTemporaryObject(frameFactory, suite, {
            alignStart: true, rowBrightness: Ui.Theme.rowDimBrightness
        });
        tryVerify(() => frame.shade.opacity > 0.3);
        frame.focused = true;
        tryCompare(frame, "imageScale", Ui.Theme.contentFocusScale);
        tryCompare(frame.shade, "opacity", 0);
        const mat = findChild(frame, "focusMat");
        tryCompare(mat, "opacity", 1);
        verify(mat.mapToItem(suite, 0, 0).x - Ui.Theme.rimThickness * frame.imageScale >= frame.x - 0.1);
        frame.pointerMode = true;
        tryCompare(frame, "imageScale", 1 + (Ui.Theme.contentFocusScale - 1) / 2);
        compare(frame.imageLift, -2);
        compare(frame.decoration.bounce, false);
    }
    function test_unavailable_music_layer_contains_transformed_rim() {
        const card = createTemporaryObject(musicFactory, suite, {
            entry: {id: "fixture", kind: "album", title: "Harbor", available: false}
        });
        Ui.Focus.take(card);
        tryCompare(card, "imageScale", Ui.Theme.contentFocusScale);
        const mat = findChild(card, "focusMat");
        const at = mat.mapToItem(card, 0, 0);
        const rim = Ui.Theme.rimThickness * card.imageScale;
        const bounds = card.layer.sourceRect;
        verify(bounds.x <= at.x - rim);
        verify(bounds.y <= at.y - rim);
        verify(bounds.x + bounds.width >= at.x + mat.width * card.imageScale + rim);
    }
}
