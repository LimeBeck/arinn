// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import QtQuick.Window

// Hearth light (ADR-0136, #723): a dark ceiling, a warm floor and a pool
// under the focused item. The floor and the pool are painted once into a
// small canvas and scaled up. Artwork is sampled at 24×24; nothing
// full-resolution is blurred (spec §6.5).
//
// Colours are local on purpose. Hearth light 1/6 (#722) owns the shared
// tokens and is on hold; these oklch values are the brief's, grouped so a
// later change can point them at those tokens.
//
//   floor   oklch(0.44 0.07 55) at its centre, 22% below the bottom edge
//   pool    oklch(0.50 c′ h), c′ 0.10 down to 0.065 at hue 240°
//   ceiling black at 50% across the top 44% of the height
//   veil    rgb(12 7 2) at 6% in the evening and 22% at night
//
// Night text in the brief is text-1 oklch(0.97 …), text-2 0.84, text-3 0.71
// (the issue's "text-1 oklch(0.84 …)" is that text-2). Applying it is a
// token change, so the constants sit here and Theme still paints the text.
Item {
    id: root
    clip: true

    // Artwork of the focused item; empty uses `tint`.
    property url source: ""
    // Identity colour of the focused item; transparent means no item.
    property color tint: "transparent"
    // Light field (Settings › Appearance › Light field). Off fades the
    // ceiling, the floor and the pool. The screen is then flat.
    property bool lit: true
    // 1 on Home, 0.8 on Applications, 0.6 in the Library, 0.5 on a detail.
    property real strength: 1
    // Settings › Appearance › Light follows the time of day. Off keeps day.
    property bool followsTime: true
    // The shell's clock, "HH:mm". Empty keeps daytime, so a screen with no
    // clock yet is not tied to the wall clock. `hour` wins when set, so a
    // test can pin a period. Production passes the shell clock every minute.
    property string clockText: ""
    property int hour: -1
    // Where the warm veil is parented, above the screen's content.
    // A second host of this light sets `veilEnabled` off so the veils do
    // not stack.
    property Item veilParent: null
    property int veilZ: 8
    property bool veilEnabled: true
    // Hue in degrees of the pool on screen; NaN while none is shown.
    readonly property real hue: layers.itemAt(front) ? layers.itemAt(front).hue : NaN
    readonly property bool shown: lit && !isNaN(hue)
    // Target and on-screen horizontal centre of the pool, in this item.
    property real poolX: width / 2
    readonly property real poolCenter: pools.x + pools.width / 2
    readonly property real poolCenterY: pools.y + pools.height / 2
    property int front: 0
    property real pendingHue: NaN
    property bool poolReady: false
    property real pendingX: width / 2
    property Item anchor: null
    // The window that owns focus. Read on this item: Window.window from a
    // Timer is not an Item and does not resolve.
    readonly property var hostWindow: Window.window

    // Brief values, one place (#722 replaces them with tokens).
    readonly property real floorLightness: 0.44
    readonly property real floorChroma: 0.07
    readonly property real floorHueDay: 55
    readonly property real poolLightness: 0.50
    readonly property real poolChromaWarm: 0.10
    readonly property real poolChromaCool: 0.065
    readonly property color nightText1: oklch(0.97, 0.006, 75, 1)
    readonly property color nightText2: oklch(0.84, 0.008, 75, 1)
    readonly property color nightText3: oklch(0.71, 0.008, 75, 1)

    readonly property int shownHour: {
        if (hour >= 0 && hour <= 23)
            return hour;
        const match = /^(\d{1,2}):(\d{2})$/.exec(clockText || "");
        if (match) {
            const parsed = parseInt(match[1], 10);
            if (parsed >= 0 && parsed <= 23)
                return parsed;
        }
        return 12;
    }
    // Day 06:00–18:59, evening 19:00–22:59, night 23:00–05:59 (spec §5.6).
    readonly property string period: {
        if (!followsTime)
            return "day";
        const h = shownHour;
        if (h >= 19 && h < 23)
            return "evening";
        if (h >= 23 || h < 6)
            return "night";
        return "day";
    }
    readonly property real lightScale: period === "night" ? 0.6 : period === "evening" ? 0.85 : 1
    readonly property real veilAmount: period === "night" ? 0.22 : period === "evening" ? 0.06 : 0
    // Warmer is a lower hue: evening −5°, night −8°.
    readonly property real floorHue: floorHueDay - (period === "night" ? 8 : period === "evening" ? 5 : 0)

    // 1 at hue 240°, 0 once the hue is 70° or more away. Chroma falls from
    // 0.10 to 0.065 across that span (brief, *Colour*).
    function poolChroma(hueDegrees) {
        const distance = Math.abs(((hueDegrees - 240 + 540) % 360) - 180);
        const cool = Math.max(0, 1 - distance / 70);
        return poolChromaWarm - (poolChromaWarm - poolChromaCool) * cool;
    }
    // Shared with the canvases below. Piecewise linear, matching
    // CanvasGradient colour stops.
    function floorStopList() { return [[0, 0.95], [0.225, 0.7125], [0.45, 0.475], [0.725, 0.2375], [1, 0]]; }
    function poolStopList() { return [[0, 0.95], [0.21, 0.76125], [0.42, 0.4275], [0.71, 0.21375], [1, 0]]; }
    function pieceAlpha(t, stops) {
        if (t <= stops[0][0])
            return stops[0][1];
        if (t >= 1)
            return 0;
        for (let i = 0; i < stops.length - 1; ++i) {
            const t0 = stops[i][0], a0 = stops[i][1];
            const t1 = stops[i + 1][0], a1 = stops[i + 1][1];
            if (t >= t0 && t <= t1)
                return a0 + (a1 - a0) * ((t - t0) / (t1 - t0));
        }
        return 0;
    }
    function addStops(gradient, color, stops) {
        for (let i = 0; i < stops.length; ++i)
            gradient.addColorStop(stops[i][0], Qt.rgba(color.r, color.g, color.b, stops[i][1]));
    }
    // OKLab L of the ceiling, floor and pool composited over the screen
    // background at a pixel. The pool is taken under `poolAtX` (the focused
    // item). This is the handoff's "light under text" measurement: the same
    // stops the canvases paint, evaluated in sRGB source-over, then OKLab.
    // It does not include the warm veil, which darkens further.
    function lightnessAt(x, y, poolAtX, hueDegrees) {
        if (width < 2 || height < 2)
            return 0;
        const nx = x / width, ny = y / height;
        const px = (poolAtX === undefined ? poolX : poolAtX) / width;
        const amount = (lit ? strength : 0);
        const ceilA = (ny < 0.44 ? 0.5 * (1 - ny / 0.44) : 0) * amount;
        const floorT = Math.hypot((nx - 0.5) / 1.0, (ny - 1.22) / 0.62);
        const floorA = (floorT <= 1 ? pieceAlpha(floorT, floorStopList()) : 0) * amount;
        const poolT = Math.hypot((nx - px) / 0.30, (ny - 1.08) / 0.42);
        const poolA = (poolT <= 1 ? pieceAlpha(poolT, poolStopList()) : 0) * amount;
        const floor = oklch(floorLightness * lightScale, floorChroma, floorHue, 1);
        const h = hueDegrees === undefined ? (isNaN(hue) ? floorHueDay : hue) : hueDegrees;
        const pool = oklch(poolLightness * lightScale, poolChroma(h), h, 1);
        const bg = Theme.background;
        let r = bg.r, g = bg.g, b = bg.b;
        const over = (cr, cg, cb, a) => {
            r = cr * a + r * (1 - a);
            g = cg * a + g * (1 - a);
            b = cb * a + b * (1 - a);
        };
        over(0, 0, 0, ceilA);
        over(floor.r, floor.g, floor.b, floorA);
        over(pool.r, pool.g, pool.b, poolA);
        return toOklch(Qt.rgba(r, g, b, 1)).l;
    }
    // The brightest point of the field in daytime. The pool is placed under
    // each sample, which is the brightest it can sit there, at hue 55° (the
    // warm case; other hues composite no brighter).
    function brightestLightness() {
        let best = 0, bx = 0, by = 0;
        const day = period;
        for (let iy = 0; iy <= 40; ++iy) {
            const y = height * iy / 40;
            for (let ix = 0; ix <= 20; ++ix) {
                const x = width * ix / 20;
                const l = lightnessAt(x, y, x, floorHueDay);
                if (l > best) { best = l; bx = x; by = y; }
            }
        }
        return {l: best, x: bx, y: by, period: day};
    }

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
        if (isNaN(hueDegrees))
            return;
        const front = layers.itemAt(root.front);
        if (front && front.hue === hueDegrees)
            return;
        const back = layers.itemAt(1 - root.front);
        if (!back)
            return;
        // The hidden layer may still hold this hue from an earlier focus.
        if (back.hue === hueDegrees)
            root.front = back.index;
        else
            back.paintHue(hueDegrees);
    }
    function anchorCenter() {
        if (!anchor || width <= 0)
            return width > 0 ? width / 2 : 0;
        const point = anchor.mapToItem(root, anchor.width / 2, anchor.height / 2);
        return isFinite(point.x) ? point.x : width / 2;
    }
    function placePool() {
        const x = anchorCenter();
        pendingX = x;
        if (!poolReady) {
            poolX = x;
            if (width > 0)
                poolReady = true;
        } else {
            poolX = x;
        }
    }
    function settle() {
        placePool();
        if (source.toString().length) {
            if (sample.status === Image.Ready)
                sampler.requestPaint();
            // Loading: the sample's status change samples it. A failed load
            // keeps the identity colour.
            else if (sample.status !== Image.Loading)
                show(tintHue());
        } else {
            show(tintHue());
        }
    }
    function insideHost(item) {
        const host = veilParent || parent;
        let cursor = item;
        while (cursor) {
            if (cursor === host)
                return true;
            cursor = cursor.parent;
        }
        return false;
    }
    onSourceChanged: settleTimer.restart()
    onTintChanged: settleTimer.restart()
    onWidthChanged: if (width > 0 && !poolReady) settle()
    onLightScaleChanged: repaintColours()
    onFloorHueChanged: if (floorCanvas.width > 0) floorCanvas.requestPaint()
    Component.onCompleted: settle()
    Timer { id: settleTimer; interval: 200; onTriggered: root.settle() }
    Timer {
        interval: 50
        repeat: true
        running: root.visible && root.width > 0
        onTriggered: {
            const win = root.hostWindow;
            const item = win ? win.activeFocusItem : null;
            if (!item || item === root || !root.insideHost(item))
                return;
            const point = item.mapToItem(root, item.width / 2, item.height / 2);
            if (!isFinite(point.x))
                return;
            if (item !== root.anchor) {
                root.anchor = item;
                root.pendingX = point.x;
                settleTimer.restart();
            } else if (Math.abs(point.x - root.pendingX) > 2) {
                root.pendingX = point.x;
                settleTimer.restart();
            }
        }
    }
    function repaintColours() {
        if (floorCanvas.width > 0)
            floorCanvas.requestPaint();
        for (let i = 0; i < 2; ++i) {
            const layer = layers.itemAt(i);
            if (layer && !isNaN(layer.hue))
                layer.paintHue(layer.hue);
        }
    }

    // A small decode of the artwork; the sampler reads its average colour.
    Image {
        id: sample
        visible: false
        source: root.source
        sourceSize.width: 24
        sourceSize.height: 24
        asynchronous: true
        onStatusChanged: {
            if (status === Image.Ready && !settleTimer.running)
                sampler.requestPaint();
            else if (status === Image.Error && !settleTimer.running)
                root.show(root.tintHue());
        }
    }
    Canvas {
        id: sampler
        width: 24
        height: 24
        opacity: 0
        renderTarget: Canvas.Image
        onPaint: {
            const ctx = getContext("2d");
            ctx.reset();
            if (sample.status !== Image.Ready)
                return;
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
            if (weight / (data.length / 4) < 0.02) {
                root.show(root.tintHue());
                return;
            }
            const h = Math.atan2(b, a) * 180 / Math.PI;
            root.show(Math.round(h < 0 ? h + 360 : h));
        }
    }

    // Ceiling, floor and pool. `lit` fades all three (the light field).
    Item {
        id: field
        anchors.fill: parent
        opacity: root.lit ? root.strength : 0
        Behavior on opacity { NumberAnimation { duration: 600; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easing } }

        // Black 50% at the top edge, gone at 44% of the height.
        Rectangle {
            anchors.fill: parent
            gradient: Gradient {
                GradientStop { position: 0.0; color: Qt.rgba(0, 0, 0, 0.5) }
                GradientStop { position: 0.44; color: Qt.rgba(0, 0, 0, 0) }
                GradientStop { position: 1.0; color: Qt.rgba(0, 0, 0, 0) }
            }
        }
        // Floor: ellipse 100% × 62% (CSS radii), centred 22% below the edge.
        Canvas {
            id: floorCanvas
            objectName: "hearthFloor"
            width: Math.max(1, root.width / 5)
            height: Math.max(1, root.height / 5)
            scale: 5
            transformOrigin: Item.TopLeft
            smooth: true
            renderTarget: Canvas.Image
            onPaint: {
                const ctx = getContext("2d");
                ctx.reset();
                if (root.width < 2)
                    return;
                const color = root.oklch(root.floorLightness * root.lightScale, root.floorChroma, root.floorHue, 1);
                ctx.save();
                ctx.translate(0.5 * width, 1.22 * height);
                ctx.scale(width, 0.62 * height);
                const gradient = ctx.createRadialGradient(0, 0, 0, 0, 0, 1);
                root.addStops(gradient, color, root.floorStopList());
                ctx.fillStyle = gradient;
                ctx.fillRect(-2, -2, 4, 4);
                ctx.restore();
            }
            onWidthChanged: requestPaint()
            onHeightChanged: requestPaint()
            Component.onCompleted: requestPaint()
        }
        // Pool: ellipse 30% × 42%, centred 8% below the edge. Both hue
        // layers move together; the 900 ms cross-fade is their opacity.
        Item {
            id: pools
            width: root.width * 0.60
            height: root.height * 0.84
            x: root.poolX - width / 2
            y: root.height * 1.08 - height / 2
            Behavior on x { enabled: root.poolReady; NumberAnimation { duration: Theme.lightDuration; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easing } }
            Repeater {
                id: layers
                model: 2
                Canvas {
                    id: layer
                    required property int index
                    property real hue: NaN
                    property real painting: NaN
                    objectName: "ambientLayer" + index
                    width: Math.max(1, pools.width / 5)
                    height: Math.max(1, pools.height / 5)
                    scale: 5
                    transformOrigin: Item.TopLeft
                    smooth: true
                    renderTarget: Canvas.Image
                    opacity: !isNaN(hue) && root.front === index ? 1 : 0
                    Behavior on opacity { NumberAnimation { duration: Theme.lightDuration; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easing } }
                    function paintHue(h) { painting = h; requestPaint(); }
                    onPaint: {
                        const ctx = getContext("2d");
                        ctx.reset();
                        if (isNaN(painting) || width < 2)
                            return;
                        const color = root.oklch(root.poolLightness * root.lightScale,
                                                 root.poolChroma(painting), painting, 1);
                        ctx.save();
                        ctx.translate(width / 2, height / 2);
                        ctx.scale(width / 2, height / 2);
                        const gradient = ctx.createRadialGradient(0, 0, 0, 0, 0, 1);
                        // Centre alpha 0.95, 45% of that at 42% of the radius.
                        root.addStops(gradient, color, root.poolStopList());
                        ctx.fillStyle = gradient;
                        ctx.fillRect(-1.2, -1.2, 2.4, 2.4);
                        ctx.restore();
                    }
                    onPainted: {
                        if (isNaN(painting))
                            return;
                        hue = painting;
                        if (painting === root.pendingHue)
                            root.front = index;
                    }
                    onWidthChanged: if (!isNaN(hue)) requestPaint()
                    onHeightChanged: if (!isNaN(hue)) requestPaint()
                }
            }
        }
    }

    // Warm veil over the whole screen, including text and artwork. It never
    // covers video: the player does not host this light (brief, *Time of day*).
    Rectangle {
        id: veil
        objectName: "hearthVeil"
        parent: root.veilParent || root
        anchors.fill: parent
        z: root.veilParent ? root.veilZ : -1
        visible: root.veilEnabled && root.visible && root.veilAmount > 0
        color: Qt.rgba(12 / 255, 7 / 255, 2 / 255, root.veilAmount)
    }
}
