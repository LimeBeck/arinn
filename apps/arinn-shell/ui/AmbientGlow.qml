// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import QtQuick.Window

// Hearth light (ADR-0136, #723): a dark ceiling, a warm floor and a pool
// under the focused item. The floor and the pool are painted once into a
// small canvas and scaled up. Artwork is sampled at 24×24; nothing
// full-resolution is blurred (spec §6.5).
//
// Floor and pool centres are Theme tokens (#722). This light publishes the
// period; Theme paints night text and the evening hint from it. This item
// does not choose text colours.
//
//   floor   Theme.hearth, oklch(0.44 0.07 55), 22% below the bottom edge
//   pool    lightness Theme.poolLightness, chroma warm→cool, hue of the item
//   ceiling black at 50% across the top 44% of the height
//   veil    rgb(12 7 2) at 6% in the evening and 22% at night
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
    // 1 on Home, 0.6 on Applications and in the Library, 0.5 on a detail.
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

    // Brief centres, owned by Theme (#722).
    readonly property real floorLightness: Theme.hearthLightness
    readonly property real floorChroma: Theme.hearthChroma
    readonly property real floorHueDay: Theme.hearthHue
    readonly property real poolLightness: Theme.poolLightness
    readonly property real poolChromaWarm: Theme.poolChromaWarm
    readonly property real poolChromaCool: Theme.poolChromaCool

    // Publishing keeps one period when several hosts of this light are alive.
    function publishClock() {
        Theme.followsTime = followsTime;
        Theme.clockText = clockText;
        Theme.hour = hour;
    }
    onFollowsTimeChanged: publishClock()
    onClockTextChanged: publishClock()
    onHourChanged: publishClock()

    readonly property string period: Theme.period
    readonly property real lightScale: period === "night" ? 0.6 : period === "evening" ? 0.85 : 1
    readonly property real veilAmount: period === "night" ? 0.22 : period === "evening" ? 0.06 : 0
    // Identity colour of the Activity behind Home; transparent keeps the
    // warm hearth. Over an Activity the floor takes the application's hue
    // (owner decision on #756, ADR-0140).
    property color floorTint: "transparent"
    readonly property real floorBaseHue: floorTint.a > 0 ? toOklch(floorTint).h : floorHueDay
    // Warmer is a lower hue: evening −5°, night −8°.
    readonly property real floorHue: floorBaseHue - (period === "night" ? 8 : period === "evening" ? 5 : 0)

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
    // Ceiling, floor and pool composited over the screen background. The pool
    // is taken under `poolAtX`. Same stops the canvases paint, in sRGB
    // source-over. The warm veil is not in this colour; it darkens further.
    function litColorAt(x, y, poolAtX, hueDegrees) {
        if (width < 2 || height < 2)
            return Theme.background;
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
        return Qt.rgba(r, g, b, 1);
    }
    function lightnessAt(x, y, poolAtX, hueDegrees) {
        return toOklch(litColorAt(x, y, poolAtX, hueDegrees)).l;
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

    function toOklch(color) { return Theme.toOklch(color); }
    function oklch(lightness, chroma, hueDegrees, alpha) { return Theme.oklch(lightness, chroma, hueDegrees, alpha); }
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
    Component.onCompleted: { publishClock(); settle(); }
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

    // Warm veil over the screen that hosts this light, including its text and
    // artwork. Settings and Downloads host one with the floor and the pool
    // off. The player does not: video stays uncovered.
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
