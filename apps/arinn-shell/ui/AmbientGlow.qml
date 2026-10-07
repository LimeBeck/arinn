// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick

// The warm light field behind Home (handoff "Light field", #368). Three
// radial layers keyed to one hue: the dominant colour of the focused item's
// artwork, or its identity colour when it has none. The field is painted once
// per hue into a small canvas and scaled up, so no full-resolution art is
// blurred per frame (spec §6.5). It follows focus with a 900 ms cross-fade
// after a short settle delay, so fast scrolling does not flicker.
Item {
    id: root
    // Artwork of the focused item; empty uses `tint`.
    property url source: ""
    // Identity colour of the focused item; transparent means no item.
    property color tint: "transparent"
    // False fades the field out (nothing focused, another screen, an
    // Activity background shown).
    property bool lit: true
    // Field opacity: 1 on Home, 0.6 in Library (handoff).
    property real strength: 1
    // Hue in degrees of the field on screen; NaN while none is shown.
    readonly property real hue: layers.itemAt(front) ? layers.itemAt(front).hue : NaN
    readonly property bool shown: lit && !isNaN(hue)
    property int front: 0
    property real pendingHue: NaN

    // sRGB colour -> OKLCH {l, c, h}.
    function toOklch(color) {
        const lin = v => v <= 0.04045 ? v / 12.92 : Math.pow((v + 0.055) / 1.055, 2.4);
        const r = lin(color.r), g = lin(color.g), b = lin(color.b);
        const l = Math.cbrt(0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b);
        const m = Math.cbrt(0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b);
        const s = Math.cbrt(0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b);
        const okL = 0.2104542553 * l + 0.7936177850 * m - 0.0040720468 * s;
        const okA = 1.9779984951 * l - 2.4285922050 * m + 0.4505937099 * s;
        const okB = 0.0259040371 * l + 0.7827717662 * m - 0.8086757660 * s;
        const h = Math.atan2(okB, okA) * 180 / Math.PI;
        return {l: okL, c: Math.sqrt(okA * okA + okB * okB), h: h < 0 ? h + 360 : h};
    }
    // OKLCH -> sRGB colour with alpha.
    function oklch(lightness, chroma, hueDegrees, alpha) {
        const rad = hueDegrees * Math.PI / 180;
        const a = chroma * Math.cos(rad), b = chroma * Math.sin(rad);
        const l = Math.pow(lightness + 0.3963377774 * a + 0.2158037573 * b, 3);
        const m = Math.pow(lightness - 0.1055613458 * a - 0.0638541728 * b, 3);
        const s = Math.pow(lightness - 0.0894841775 * a - 1.2914855480 * b, 3);
        const gamma = v => {
            v = Math.max(0, Math.min(1, v));
            return v <= 0.0031308 ? 12.92 * v : 1.055 * Math.pow(v, 1 / 2.4) - 0.055;
        };
        return Qt.rgba(gamma(4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s),
                       gamma(-1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s),
                       gamma(-0.0041960863 * l - 0.7034186147 * m + 1.7076147010 * s),
                       alpha);
    }
    function tintHue() {
        return tint.a > 0 ? toOklch(tint).h : NaN;
    }
    function show(hueDegrees) {
        pendingHue = hueDegrees;
        if (isNaN(hueDegrees)) return;
        const front = layers.itemAt(root.front);
        if (front && front.hue === hueDegrees) return;
        const back = layers.itemAt(1 - root.front);
        // The hidden layer may still hold this hue from an earlier focus.
        if (back.hue === hueDegrees) root.front = back.index;
        else back.paintHue(hueDegrees);
    }
    function settle() {
        if (source.toString().length) {
            if (sample.status === Image.Ready) sampler.requestPaint();
            // Loading: the sample's status change samples it. A failed load
            // keeps the identity colour.
            else if (sample.status !== Image.Loading) show(tintHue());
        } else show(tintHue());
    }
    onSourceChanged: settleTimer.restart()
    onTintChanged: settleTimer.restart()
    Component.onCompleted: settle()
    Timer { id: settleTimer; interval: 200; onTriggered: root.settle() }

    // A small decode of the artwork; the sampler reads its average colour.
    Image {
        id: sample
        visible: false
        source: root.source
        sourceSize.width: 24; sourceSize.height: 24
        asynchronous: true
        onStatusChanged: {
            if (status === Image.Ready && !settleTimer.running) sampler.requestPaint();
            else if (status === Image.Error && !settleTimer.running) root.show(root.tintHue());
        }
    }
    Canvas {
        id: sampler
        width: 24; height: 24
        opacity: 0
        renderTarget: Canvas.Image
        onPaint: {
            const ctx = getContext("2d");
            ctx.reset();
            if (sample.status !== Image.Ready) return;
            ctx.drawImage(sample, 0, 0, width, height);
            const data = ctx.getImageData(0, 0, width, height).data;
            // Chroma-weighted mean in OKLab: the dominant hue, not grey.
            let a = 0, b = 0, weight = 0;
            for (let i = 0; i < data.length; i += 4) {
                const lch = root.toOklch(Qt.rgba(data[i] / 255, data[i + 1] / 255, data[i + 2] / 255, 1));
                const w = lch.c * (data[i + 3] / 255);
                a += w * Math.cos(lch.h * Math.PI / 180);
                b += w * Math.sin(lch.h * Math.PI / 180);
                weight += w;
            }
            if (weight / (data.length / 4) < 0.02) { root.show(root.tintHue()); return; }
            const h = Math.atan2(b, a) * 180 / Math.PI;
            root.show(Math.round(h < 0 ? h + 360 : h));
        }
    }
    Repeater {
        id: layers
        model: 2
        // A quarter-resolution field, scaled up with filtering.
        Canvas {
            id: layer
            required property int index
            property real hue: NaN
            property real painting: NaN
            objectName: "ambientLayer" + index
            width: root.width / 5; height: root.height / 5
            scale: 5; transformOrigin: Item.TopLeft
            smooth: true
            renderTarget: Canvas.Image
            opacity: root.lit && root.front === index && !isNaN(hue) ? root.strength : 0
            Behavior on opacity { NumberAnimation { duration: Theme.lightDuration; easing.type: Easing.OutCubic } }
            function paintHue(h) { painting = h; requestPaint(); }
            onPaint: {
                const ctx = getContext("2d");
                ctx.reset();
                if (isNaN(painting)) return;
                // Handoff: three radial layers, never lighter than L 0.48.
                const field = [[0.55, 0.70, 0.74, 0.22, root.oklch(0.48, 0.10, painting, 0.8)],
                               [0.38, 0.45, 0.36, 0.58, root.oklch(0.42, 0.09, painting - 7, 0.4)],
                               [0.40, 0.40, 1.00, 0.90, root.oklch(0.32, 0.06, painting + 18, 0.45)]];
                for (const [rx, ry, cx, cy, color] of field) {
                    ctx.save();
                    ctx.translate(cx * width, cy * height);
                    ctx.scale(rx * width, ry * height);
                    const gradient = ctx.createRadialGradient(0, 0, 0, 0, 0, 1);
                    gradient.addColorStop(0, color);
                    gradient.addColorStop(0.7, Qt.rgba(color.r, color.g, color.b, 0));
                    ctx.fillStyle = gradient;
                    ctx.fillRect(-2, -2, 4, 4);
                    ctx.restore();
                }
            }
            onPainted: {
                if (isNaN(painting)) return;
                hue = painting;
                // A later focus may already want another hue.
                if (painting === root.pendingHue) root.front = index;
            }
        }
    }
}
