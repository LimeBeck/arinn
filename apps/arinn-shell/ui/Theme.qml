// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

pragma Singleton
import QtQuick

QtObject {
    readonly property color background: "#131416"
    readonly property color plate: Qt.rgba(1, 1, 1, 0.06)
    readonly property color plateFocus: Qt.rgba(1, 1, 1, 0.20)
    readonly property color textPrimary: "#F4F5F7"
    readonly property color textSecondary: "#C9CBCF"
    readonly property color textHint: "#9A9DA3"
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
    function identityColor(identity) {
        const text = String(identity || "");
        let hash = 0;
        for (let i = 0; i < text.length; ++i)
            hash = (hash * 31 + text.charCodeAt(i)) >>> 0;
        return identityColors[hash % identityColors.length];
    }
}
