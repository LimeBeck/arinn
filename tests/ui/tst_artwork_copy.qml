// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import QtTest
import "../../apps/arinn-shell/ui/artwork.js" as Artwork

TestCase {
    id: suite
    name: "ArtworkCopy"
    width: 400; height: 300
    visible: true
    when: windowShown
    // 20×30, red middle third and blue outer thirds, as in tst_components.
    readonly property string portrait: "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAABQAAAAeCAIAAACjcKk8AAAAJUlEQVR42mPQ0LhDNmIY1TwyNN+hQPeo5lHNg1nzaGEwqpkQAgBlZb8/WLTFKQAAAABJRU5ErkJggg=="
    // A red 13×26 image: 120 / (120 / 13) rounds above 13 in crop math.
    readonly property string roundingPortrait: "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAA0AAAAaCAIAAADjWkEIAAAAGUlEQVR4nGP4z8BADCJK0ai6UXWj6oa0OgBeUVC+BLb1ogAAAABJRU5ErkJggg=="
    Component {
        id: copyFactory
        Canvas {
            width: 200; height: 100
            property url source: suite.portrait
            property real imageWidth: 20
            property real imageHeight: 30
            property string paintError: ""
            property bool drawn: false
            property int paints: 0
            function sync() {
                if (available && !isImageLoaded(source))
                    loadImage(source);
                requestPaint();
            }
            onAvailableChanged: sync()
            onImageLoaded: requestPaint()
            onPaint: {
                const ctx = getContext("2d");
                ctx.reset();
                if (!isImageLoaded(source))
                    return;
                paintError = "";
                // Catch only in this fixture so the old exception is an
                // explicit failing assertion, rather than an ignored warning.
                try {
                    drawn = Artwork.drawCover(ctx, source, imageWidth, imageHeight, width, height);
                } catch (error) {
                    paintError = String(error);
                }
                paints++;
            }
        }
    }
    function test_crop_uses_the_canvas_copy_data() {
        return [
            {tag: "larger-image", iw: 2000, ih: 3000, width: 200, height: 100},
            {tag: "different-aspect", iw: 3000, ih: 20, width: 200, height: 100},
            {tag: "smaller-image", iw: 2, ih: 3, width: 200, height: 100},
            {tag: "resized-slot", iw: 2000, ih: 3000, width: 160, height: 90},
            {tag: "rounding-bounds", iw: 13, ih: 26, width: 120, height: 90, source: roundingPortrait}
        ];
    }
    function test_crop_uses_the_canvas_copy(data) {
        const copy = createTemporaryObject(copyFactory, suite, {
            source: data.source || portrait,
            imageWidth: data.iw, imageHeight: data.ih, width: data.width, height: data.height
        });
        verify(copy);
        tryVerify(() => copy.paints > 0);
        compare(copy.paintError, "", "a stale Image size must not exceed the actual Canvas copy");
        compare(copy.drawn, true);
        tryVerify(() => {
            const pixels = grabImage(copy);
            // Correct centre cropping draws the red middle, independently
            // of the other Image's size or aspect ratio.
            return pixels.red(80, 20) > 150 && pixels.blue(80, 20) < 100;
        });
        const before = copy.paints;
        copy.width = 120;
        copy.requestPaint();
        tryVerify(() => copy.paints > before);
        compare(copy.paintError, "");
        compare(copy.drawn, true);
    }
}
