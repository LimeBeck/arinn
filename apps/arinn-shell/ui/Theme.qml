// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

pragma Singleton
import QtQuick

QtObject {
    readonly property color background: "#131416"
    readonly property color plate: Qt.rgba(1, 1, 1, 0.06)
    readonly property color plateFocus: Qt.rgba(1, 1, 1, 0.20)
    // Daytime handoff colours. Night replaces them; the numbers stay here
    // until the token task (#722) owns them. The hearth light publishes the
    // clock below, and every text binding reads the result.
    readonly property color dayText1: "#F4F5F7"
    readonly property color dayText2: "#C9CBCF"
    readonly property color dayText3: "#9A9DA3"
    // Brief night text. The issue's "text-1 oklch(0.84 …)" is text-2.
    // Text-3 in the brief is 0.71, about 4.8:1 on the flat background under
    // the 22% veil and about 4.06:1 where the floor and the pool are
    // brightest. 0.75 keeps that point at or above 4.5:1. Evening does not
    // raise titles; its hints move to 0.80, because day text-3 is about
    // 3.35:1 on that same point under the 6% veil.
    readonly property color nightText1: oklch(0.97, 0.006, 75, 1)
    readonly property color nightText2: oklch(0.84, 0.008, 75, 1)
    readonly property color nightText3: oklch(0.75, 0.008, 75, 1)
    readonly property color eveningText3: oklch(0.80, 0.008, 75, 1)
    // Written by the hearth light from the shell clock. Empty clock and
    // hour < 0 are daytime, so a screen with no clock yet is not tied to
    // the wall clock.
    property bool followsTime: true
    property string clockText: ""
    property int hour: -1
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
    readonly property color textPrimary: period === "night" ? nightText1 : dayText1
    readonly property color textSecondary: period === "night" ? nightText2 : dayText2
    readonly property color textHint: period === "night" ? nightText3 : period === "evening" ? eveningText3 : dayText3
    readonly property color ink: "#131417"
    readonly property color ember: "#FF8A3D"
    readonly property color error: "#E8474F"
    readonly property color ring: Qt.rgba(1, 1, 1, 0.45)
    readonly property int canvasWidth: 1920
    readonly property int canvasHeight: 1080
    readonly property int safeLeft: 120
    readonly property int safeRight: 96
    readonly property int cardRadius: 26
    readonly property int focusDuration: 220
    readonly property int pressDuration: 120
    readonly property int lightDuration: 900
    readonly property var easing: [0.2, 0.8, 0.2, 1, 1, 1]
    readonly property FontLoader onest: FontLoader { source: "fonts/Onest.ttf" }
    readonly property string fontFamily: onest.name
    // Onest is a variable font (wght 100–900). Qt does not match its named
    // instances by font.weight, so every weight would render as the default
    // 400 (#546). The text components call this whenever their font
    // changes: it sets the wght axis to the requested weight. A 400 text
    // gets no axis at all.
    function applyWeight(item) {
        if (!item)
            return;
        const axes = item.font.variableAxes || {};
        const current = axes["wght"] === undefined ? 400 : axes["wght"];
        if (current !== item.font.weight)
            item.font.variableAxes = Object.assign({}, axes, {"wght": item.font.weight});
    }
    // Stable generic colour for an identity without artwork or a logo. The
    // identity is opaque: no product names, only a hash of the string.
    readonly property var identityColors: ["#536fa6", "#487b69", "#80649c", "#9a625f", "#857541", "#477f8c"]
    // OKLCH -> sRGB. Shared with the hearth light so the pool and the night
    // text use one conversion.
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
    function relativeLuminance(color) {
        const lin = v => v <= 0.04045 ? v / 12.92 : Math.pow((v + 0.055) / 1.055, 2.4);
        return 0.2126 * lin(color.r) + 0.7152 * lin(color.g) + 0.0722 * lin(color.b);
    }
    // The warm veil, rgb(12 7 2), in sRGB source-over. Same paint as the light.
    function veilOver(color, veilAlpha) {
        const veil = Qt.rgba(12 / 255, 7 / 255, 2 / 255, 1);
        return Qt.rgba(veil.r * veilAlpha + color.r * (1 - veilAlpha),
                       veil.g * veilAlpha + color.g * (1 - veilAlpha),
                       veil.b * veilAlpha + color.b * (1 - veilAlpha), 1);
    }
    // WCAG contrast of two opaque colours.
    function contrast(a, b) {
        const lighter = Math.max(relativeLuminance(a), relativeLuminance(b));
        const darker = Math.min(relativeLuminance(a), relativeLuminance(b));
        return (lighter + 0.05) / (darker + 0.05);
    }
    // Contrast after the veil is composited over the text and the flat background.
    function veiledContrast(textColor, veilAlpha) {
        return contrast(veilOver(textColor, veilAlpha), veilOver(background, veilAlpha));
    }
    function identityColor(identity) {
        const text = String(identity || "");
        let hash = 0;
        for (let i = 0; i < text.length; ++i)
            hash = (hash * 31 + text.charCodeAt(i)) >>> 0;
        return identityColors[hash % identityColors.length];
    }
}
