// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick

// Line icons drawn after the Material Symbols the handoff names. The shell
// ships no icon font, so they are painted here. On-screen keyboard: keyboard,
// keyboard_hide, shift, backspace, language, chevron_left, chevron_right,
// history and public. Open services: bookmark_add and bookmark_added.
Canvas {
    id: root
    property string name: ""
    property color color: Theme.textPrimary
    implicitWidth: 38
    implicitHeight: 38
    onNameChanged: requestPaint()
    onColorChanged: requestPaint()
    onWidthChanged: requestPaint()
    onPaint: {
        const ctx = getContext("2d");
        ctx.reset();
        const s = Math.min(width, height) / 24;
        ctx.scale(s, s);
        ctx.strokeStyle = root.color;
        ctx.fillStyle = root.color;
        ctx.lineWidth = 2;
        ctx.lineCap = "round";
        ctx.lineJoin = "round";
        ctx.beginPath();
        switch (root.name) {
        case "keyboard":
        case "keyboard_hide":
            ctx.roundedRect(2.5, root.name === "keyboard" ? 5 : 3, 19, 13, 2, 2);
            ctx.stroke();
            ctx.beginPath();
            for (const y of root.name === "keyboard" ? [9, 12.5] : [7, 10.5])
                for (let x = 6; x <= 18; x += 3)
                    ctx.rect(x - 0.8, y - 0.8, 1.6, 1.6);
            ctx.fill();
            ctx.beginPath();
            if (root.name === "keyboard") {
                ctx.moveTo(8, 15.5); ctx.lineTo(16, 15.5);
            } else {
                ctx.moveTo(9, 13.5); ctx.lineTo(15, 13.5);
                ctx.moveTo(9.5, 19); ctx.lineTo(12, 21.5); ctx.lineTo(14.5, 19);
            }
            ctx.stroke();
            break;
        case "shift":
            ctx.moveTo(12, 3.5); ctx.lineTo(20.5, 12.5); ctx.lineTo(16, 12.5);
            ctx.lineTo(16, 20); ctx.lineTo(8, 20); ctx.lineTo(8, 12.5);
            ctx.lineTo(3.5, 12.5); ctx.closePath();
            ctx.stroke();
            break;
        case "backspace":
            ctx.moveTo(8, 5); ctx.lineTo(21, 5); ctx.lineTo(21, 19); ctx.lineTo(8, 19);
            ctx.lineTo(2.5, 12); ctx.closePath();
            ctx.moveTo(11.5, 9); ctx.lineTo(17.5, 15);
            ctx.moveTo(17.5, 9); ctx.lineTo(11.5, 15);
            ctx.stroke();
            break;
        case "language":
        case "public":
            ctx.arc(12, 12, 9, 0, Math.PI * 2);
            ctx.moveTo(3, 12); ctx.lineTo(21, 12);
            ctx.moveTo(12, 3);
            ctx.bezierCurveTo(7, 8, 7, 16, 12, 21);
            ctx.moveTo(12, 3);
            ctx.bezierCurveTo(17, 8, 17, 16, 12, 21);
            ctx.stroke();
            break;
        case "left":
            ctx.moveTo(14.5, 6); ctx.lineTo(8.5, 12); ctx.lineTo(14.5, 18);
            ctx.stroke();
            break;
        case "right":
            ctx.moveTo(9.5, 6); ctx.lineTo(15.5, 12); ctx.lineTo(9.5, 18);
            ctx.stroke();
            break;
        case "bookmark_add":
        case "bookmark_added":
            // A ribbon with a notch, open at the top right for the badge.
            ctx.moveTo(13, 4); ctx.lineTo(7, 4);
            ctx.quadraticCurveTo(5, 4, 5, 6);
            ctx.lineTo(5, 20.5); ctx.lineTo(12, 17.5); ctx.lineTo(19, 20.5);
            ctx.lineTo(19, 12);
            if (root.name === "bookmark_add") {
                ctx.moveTo(19, 2); ctx.lineTo(19, 8);
                ctx.moveTo(16, 5); ctx.lineTo(22, 5);
            } else {
                ctx.moveTo(15.5, 5.5); ctx.lineTo(18, 8); ctx.lineTo(22.5, 3.5);
            }
            ctx.stroke();
            break;
        case "history":
            ctx.arc(13, 12, 8, Math.PI * 0.95, Math.PI * 2.75);
            ctx.moveTo(13, 8); ctx.lineTo(13, 12.5); ctx.lineTo(16, 14.5);
            ctx.moveTo(2.5, 9.5); ctx.lineTo(5, 12.5); ctx.lineTo(8, 10);
            ctx.stroke();
            break;
        }
    }
}
