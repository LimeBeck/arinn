// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
Canvas {
    property string name: "play"
    property color color: "white"
    onNameChanged: requestPaint()
    onColorChanged: requestPaint()
    onPaint: {
        const c = getContext("2d"); c.reset(); c.scale(width / 24, height / 24);
        c.fillStyle = color; c.strokeStyle = color; c.lineWidth = 1.8; c.lineCap = "round"; c.lineJoin = "round";
        c.beginPath();
        if (name === "pause") { c.fillRect(6, 4, 4, 16); c.fillRect(14, 4, 4, 16); }
        else if (name === "previous" || name === "next") {
            const previous = name === "previous"; c.moveTo(previous ? 17 : 7, 5); c.lineTo(previous ? 8 : 16, 12); c.lineTo(previous ? 17 : 7, 19); c.closePath(); c.fill(); c.fillRect(previous ? 5 : 17, 5, 2, 14);
        } else if (name === "queue") {
            for (let y = 5; y < 20; y += 6) { c.moveTo(3, y); c.lineTo(16, y); } c.stroke(); c.beginPath(); c.moveTo(19, 14); c.lineTo(23, 17); c.lineTo(19, 20); c.fill();
        } else if (name === "like" || name === "dislike") {
            if (name === "dislike") { c.translate(24, 24); c.rotate(Math.PI); }
            c.moveTo(8, 10); c.lineTo(13, 3); c.lineTo(15, 4); c.lineTo(14, 9); c.lineTo(21, 9); c.lineTo(18, 21); c.lineTo(8, 21); c.closePath(); c.stroke(); c.strokeRect(3, 10, 3, 11);
        } else if (name === "radio") {
            c.arc(12, 12, 2, 0, 2 * Math.PI); c.fill(); c.beginPath(); c.arc(12, 12, 6, -0.8, 0.8); c.stroke(); c.beginPath(); c.arc(12, 12, 6, 2.35, 3.95); c.stroke(); c.beginPath(); c.arc(12, 12, 10, -0.8, 0.8); c.stroke(); c.beginPath(); c.arc(12, 12, 10, 2.35, 3.95); c.stroke();
        } else if (name === "seek") { c.arc(12, 12, 9, 0, Math.PI * 2); c.moveTo(12, 6); c.lineTo(12, 12); c.lineTo(17, 12); c.stroke(); }
        else if (name === "stop") c.fillRect(5, 5, 14, 14);
        else if (name === "music") { c.moveTo(10, 18); c.lineTo(10, 4); c.lineTo(21, 2); c.lineTo(21, 16); c.stroke(); c.beginPath(); c.ellipse(3, 15, 7, 6); c.ellipse(14, 13, 7, 6); c.fill(); }
        else { c.moveTo(7, 3); c.lineTo(21, 12); c.lineTo(7, 21); c.closePath(); c.fill(); }
    }
}
