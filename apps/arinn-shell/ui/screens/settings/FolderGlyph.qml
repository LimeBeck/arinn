// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import "../.." as Ui

// Material Symbols `folder`, `folder_off`, `create_new_folder`, `lock`,
// `usb`, `lan`, `home` and `computer` as line drawings (handoff LF, PK).
Canvas {
    id: glyph
    property string name: "folder"
    property color color: Ui.Theme.textSecondary
    width: 38; height: 38
    onNameChanged: requestPaint()
    onColorChanged: requestPaint()
    onWidthChanged: requestPaint()
    onPaint: {
        const ctx = getContext("2d");
        ctx.reset();
        const s = Math.min(width, height) / 24;
        ctx.scale(s, s);
        ctx.strokeStyle = glyph.color;
        ctx.fillStyle = glyph.color;
        ctx.lineWidth = 2;
        ctx.lineCap = "round";
        ctx.lineJoin = "round";
        const folder = () => {
            ctx.beginPath();
            ctx.moveTo(3, 6.5); ctx.lineTo(9, 6.5); ctx.lineTo(11, 8.5); ctx.lineTo(21, 8.5);
            ctx.lineTo(21, 19); ctx.lineTo(3, 19); ctx.closePath();
            ctx.stroke();
        };
        switch (glyph.name) {
        case "folder": folder(); break;
        case "folder_off":
            folder();
            ctx.beginPath(); ctx.moveTo(4, 3.5); ctx.lineTo(20, 21.5); ctx.stroke();
            break;
        case "create_new_folder":
            folder();
            ctx.beginPath(); ctx.moveTo(12, 11); ctx.lineTo(12, 17); ctx.moveTo(9, 14); ctx.lineTo(15, 14); ctx.stroke();
            break;
        case "lock":
            ctx.beginPath(); ctx.rect(5, 11, 14, 10); ctx.stroke();
            ctx.beginPath(); ctx.arc(12, 11, 4, Math.PI, 0); ctx.stroke();
            break;
        case "usb":
            ctx.beginPath(); ctx.rect(8, 9, 8, 12); ctx.moveTo(10, 9); ctx.lineTo(10, 3); ctx.lineTo(14, 3); ctx.lineTo(14, 9); ctx.stroke();
            break;
        case "lan":
            ctx.beginPath(); ctx.rect(9, 3, 6, 5); ctx.rect(3, 16, 6, 5); ctx.rect(15, 16, 6, 5);
            ctx.moveTo(12, 8); ctx.lineTo(12, 12); ctx.moveTo(6, 16); ctx.lineTo(6, 12); ctx.lineTo(18, 12); ctx.lineTo(18, 16);
            ctx.stroke();
            break;
        case "home":
            ctx.beginPath(); ctx.moveTo(4, 11); ctx.lineTo(12, 4); ctx.lineTo(20, 11);
            ctx.moveTo(7, 10.5); ctx.lineTo(7, 20); ctx.lineTo(17, 20); ctx.lineTo(17, 10.5); ctx.stroke();
            break;
        case "computer":
            ctx.beginPath(); ctx.rect(3, 4, 18, 12); ctx.moveTo(8, 20); ctx.lineTo(16, 20); ctx.moveTo(12, 16); ctx.lineTo(12, 20); ctx.stroke();
            break;
        case "link_off":
            ctx.beginPath(); ctx.arc(8, 12, 4, Math.PI * 0.5, Math.PI * 1.5); ctx.arc(16, 12, 4, Math.PI * 1.5, Math.PI * 0.5);
            ctx.moveTo(4, 4); ctx.lineTo(20, 20); ctx.stroke();
            break;
        }
    }
}
