// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick

// Rim light (ADR-0136, D-7). A 2 px stroke, bright at the bottom and fading
// towards the top. Ember is never the focus mark, and there is no white ring.
// Callers place this item; content artwork insets it by the mat plus the stroke.
Item {
    id: ring
    objectName: "focusRing"
    property bool shown: false
    property real radius: 0
    property real thickness: Theme.rimThickness
    anchors.fill: parent
    z: 100
    opacity: shown ? 1 : 0
    Behavior on opacity { NumberAnimation { duration: Theme.focusDuration } }

    function css(color) {
        return "rgba(" + Math.round(color.r * 255) + "," + Math.round(color.g * 255)
                + "," + Math.round(color.b * 255) + "," + color.a + ")";
    }
    Canvas {
        id: stroke
        anchors.fill: parent
        antialiasing: true
        onWidthChanged: requestPaint()
        onHeightChanged: requestPaint()
        onPaint: {
            const ctx = getContext("2d");
            ctx.reset();
            if (width < 2 || height < 2)
                return;
            const line = Math.max(1, ring.thickness);
            const inset = line / 2;
            const w = width - line;
            const h = height - line;
            const r = Math.max(0, Math.min(ring.radius, w / 2, h / 2));
            const gradient = ctx.createLinearGradient(0, height, 0, 0);
            gradient.addColorStop(0, ring.css(Theme.rimBottom));
            gradient.addColorStop(0.45, ring.css(Theme.rimMid));
            gradient.addColorStop(1, ring.css(Theme.rimTop));
            ctx.strokeStyle = gradient;
            ctx.lineWidth = line;
            ctx.beginPath();
            ctx.moveTo(inset + r, inset);
            ctx.lineTo(inset + w - r, inset);
            ctx.quadraticCurveTo(inset + w, inset, inset + w, inset + r);
            ctx.lineTo(inset + w, inset + h - r);
            ctx.quadraticCurveTo(inset + w, inset + h, inset + w - r, inset + h);
            ctx.lineTo(inset + r, inset + h);
            ctx.quadraticCurveTo(inset, inset + h, inset, inset + h - r);
            ctx.lineTo(inset, inset + r);
            ctx.quadraticCurveTo(inset, inset, inset + r, inset);
            ctx.closePath();
            ctx.stroke();
        }
        Connections {
            target: ring
            function onRadiusChanged() { stroke.requestPaint(); }
            function onThicknessChanged() { stroke.requestPaint(); }
            function onShownChanged() { stroke.requestPaint(); }
        }
    }
}
