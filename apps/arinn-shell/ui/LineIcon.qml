// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick

// Line icons drawn after the Material Symbols the handoff names. The shell
// ships no icon font, so they are painted here. On-screen keyboard: keyboard,
// keyboard_hide, shift, backspace, language, chevron_left, chevron_right,
// history and public. Open services: bookmark_add, bookmark_added, close,
// add and volume_up (the filled speaker of the Sound mark). Navigation:
// home, library, download, apps, settings, screen and power. Download
// states (handoff 12): downloading, schedule, pause_circle, check_circle
// and error. `public` is the Open services globe; `close` is Close Arinn.
// Music (#756 M23): stop, queue_music, thumb_up, thumb_down, radio,
// push_pin, refresh and music_note, with play, pause, skip_previous,
// skip_next and schedule (seek). Now playing (#802): favorite (the Like
// heart), favorite_fill (Liked) and more_horiz (More).
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
        case "play":
            ctx.moveTo(7,4);ctx.lineTo(20,12);ctx.lineTo(7,20);ctx.closePath();ctx.fill();break;
        case "pause":
            ctx.rect(6,4,4,16);ctx.rect(14,4,4,16);ctx.fill();break;
        case "check":
            ctx.lineWidth = 2.5;
            ctx.moveTo(5, 12.5); ctx.lineTo(10, 17.5); ctx.lineTo(19.5, 7);
            ctx.stroke();
            break;
        // Player OSD previous / next (Hearth light brief, *Player OSD*).
        case "skip_previous":
            ctx.rect(5,5,3,14);ctx.moveTo(19,5);ctx.lineTo(9,12);ctx.lineTo(19,19);ctx.closePath();ctx.fill();break;
        case "skip_next":
            ctx.rect(16,5,3,14);ctx.moveTo(5,5);ctx.lineTo(15,12);ctx.lineTo(5,19);ctx.closePath();ctx.fill();break;
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
        case "close":
            ctx.moveTo(5, 5); ctx.lineTo(19, 19);
            ctx.moveTo(19, 5); ctx.lineTo(5, 19);
            ctx.stroke();
            break;
        case "add":
            ctx.moveTo(12, 5); ctx.lineTo(12, 19);
            ctx.moveTo(5, 12); ctx.lineTo(19, 12);
            ctx.stroke();
            break;
        case "volume_up":
            // The filled speaker with two arcs, as Settings draws it.
            ctx.moveTo(3, 9); ctx.lineTo(7, 9); ctx.lineTo(12, 4); ctx.lineTo(12, 20); ctx.lineTo(7, 15); ctx.lineTo(3, 15);
            ctx.closePath();
            ctx.fill();
            ctx.beginPath();
            ctx.arc(12, 12, 4.5, -0.85, 0.85);
            ctx.stroke();
            ctx.beginPath();
            ctx.arc(12, 12, 8.5, -0.95, 0.95);
            ctx.stroke();
            break;
        case "history":
            ctx.arc(13, 12, 8, Math.PI * 0.95, Math.PI * 2.75);
            ctx.moveTo(13, 8); ctx.lineTo(13, 12.5); ctx.lineTo(16, 14.5);
            ctx.moveTo(2.5, 9.5); ctx.lineTo(5, 12.5); ctx.lineTo(8, 10);
            ctx.stroke();
            break;
        case "home":
            ctx.moveTo(4, 11); ctx.lineTo(12, 4); ctx.lineTo(20, 11);
            ctx.moveTo(7, 10.5); ctx.lineTo(7, 20); ctx.lineTo(17, 20); ctx.lineTo(17, 10.5);
            ctx.stroke();
            break;
        case "library":
            ctx.moveTo(6, 4); ctx.lineTo(6, 20); ctx.lineTo(12, 17); ctx.lineTo(18, 20); ctx.lineTo(18, 4);
            ctx.lineTo(12, 7); ctx.closePath();
            ctx.stroke();
            break;
        case "download":
            ctx.moveTo(12, 3.5); ctx.lineTo(12, 14);
            ctx.moveTo(7.5, 10.5); ctx.lineTo(12, 15); ctx.lineTo(16.5, 10.5);
            ctx.moveTo(5, 19.5); ctx.lineTo(19, 19.5);
            ctx.stroke();
            break;
        case "apps":
            ctx.roundedRect(3.5, 3.5, 7, 7, 1.5, 1.5);
            ctx.roundedRect(13.5, 3.5, 7, 7, 1.5, 1.5);
            ctx.roundedRect(3.5, 13.5, 7, 7, 1.5, 1.5);
            ctx.roundedRect(13.5, 13.5, 7, 7, 1.5, 1.5);
            ctx.stroke();
            break;
        case "settings":
            ctx.moveTo(4, 7); ctx.lineTo(20, 7);
            ctx.moveTo(4, 12); ctx.lineTo(20, 12);
            ctx.moveTo(4, 17); ctx.lineTo(20, 17);
            ctx.stroke();
            ctx.beginPath();
            ctx.arc(9, 7, 2.2, 0, Math.PI * 2);
            ctx.arc(15, 12, 2.2, 0, Math.PI * 2);
            ctx.arc(10, 17, 2.2, 0, Math.PI * 2);
            ctx.fill();
            break;
        case "screen":
            ctx.roundedRect(3, 4, 18, 12, 2, 2);
            ctx.moveTo(8, 20); ctx.lineTo(16, 20);
            ctx.moveTo(12, 16); ctx.lineTo(12, 20);
            ctx.stroke();
            break;
        case "downloading":
            // An arrow into a tray, inside an open ring (Material `downloading`).
            ctx.arc(12, 12, 9, Math.PI * 0.62, Math.PI * 2.38);
            ctx.moveTo(12, 7); ctx.lineTo(12, 15);
            ctx.moveTo(8.5, 11.5); ctx.lineTo(12, 15); ctx.lineTo(15.5, 11.5);
            ctx.stroke();
            break;
        case "schedule":
            ctx.arc(12, 12, 9, 0, Math.PI * 2);
            ctx.moveTo(12, 7); ctx.lineTo(12, 12); ctx.lineTo(15.5, 14);
            ctx.stroke();
            break;
        case "pause_circle":
            ctx.arc(12, 12, 9, 0, Math.PI * 2);
            ctx.moveTo(10, 8.5); ctx.lineTo(10, 15.5);
            ctx.moveTo(14, 8.5); ctx.lineTo(14, 15.5);
            ctx.stroke();
            break;
        case "check_circle":
            ctx.arc(12, 12, 9, 0, Math.PI * 2);
            ctx.moveTo(8, 12.5); ctx.lineTo(11, 15.5); ctx.lineTo(16.5, 9.5);
            ctx.stroke();
            break;
        case "error":
            ctx.arc(12, 12, 9, 0, Math.PI * 2);
            ctx.moveTo(12, 7.5); ctx.lineTo(12, 13);
            ctx.stroke();
            ctx.beginPath();
            ctx.arc(12, 16.3, 1.2, 0, Math.PI * 2);
            ctx.fill();
            break;
        case "stop":
            ctx.rect(5, 5, 14, 14);
            ctx.fill();
            break;
        case "queue_music":
            for (const y of [5, 11, 17]) { ctx.moveTo(3, y); ctx.lineTo(16, y); }
            ctx.stroke();
            ctx.beginPath();
            ctx.moveTo(19, 14); ctx.lineTo(23, 17); ctx.lineTo(19, 20); ctx.closePath();
            ctx.fill();
            break;
        case "thumb_up":
        case "thumb_down":
            if (root.name === "thumb_down") { ctx.translate(24, 24); ctx.rotate(Math.PI); }
            ctx.moveTo(8, 10); ctx.lineTo(13, 3); ctx.lineTo(15, 4); ctx.lineTo(14, 9);
            ctx.lineTo(21, 9); ctx.lineTo(18, 21); ctx.lineTo(8, 21); ctx.closePath();
            ctx.rect(3, 10, 3, 11);
            ctx.stroke();
            break;
        case "favorite":
        case "favorite_fill":
            ctx.moveTo(12, 20);
            ctx.bezierCurveTo(4, 14.5, 2.5, 11, 3.5, 7.8);
            ctx.bezierCurveTo(4.6, 4.6, 9.4, 3.6, 12, 7.4);
            ctx.bezierCurveTo(14.6, 3.6, 19.4, 4.6, 20.5, 7.8);
            ctx.bezierCurveTo(21.5, 11, 20, 14.5, 12, 20);
            ctx.closePath();
            if (root.name === "favorite_fill")
                ctx.fill();
            else
                ctx.stroke();
            break;
        case "more_horiz":
            for (const x of [5, 12, 19]) {
                ctx.moveTo(x + 2, 12);
                ctx.arc(x, 12, 2, 0, Math.PI * 2);
            }
            ctx.fill();
            break;
        case "radio":
            ctx.arc(12, 12, 2, 0, Math.PI * 2);
            ctx.fill();
            for (const r of [6, 10]) {
                ctx.beginPath(); ctx.arc(12, 12, r, -0.8, 0.8); ctx.stroke();
                ctx.beginPath(); ctx.arc(12, 12, r, 2.35, 3.95); ctx.stroke();
            }
            break;
        case "push_pin":
            ctx.moveTo(8, 3); ctx.lineTo(16, 3); ctx.lineTo(15, 10); ctx.lineTo(19, 14);
            ctx.lineTo(5, 14); ctx.lineTo(9, 10); ctx.closePath();
            ctx.moveTo(12, 14); ctx.lineTo(12, 22);
            ctx.stroke();
            break;
        case "refresh":
            ctx.arc(12, 12, 8, 0.6, Math.PI * 2);
            ctx.moveTo(20, 4); ctx.lineTo(20, 11); ctx.lineTo(13, 11);
            ctx.stroke();
            break;
        case "music_note":
            ctx.moveTo(10, 18); ctx.lineTo(10, 4); ctx.lineTo(21, 2); ctx.lineTo(21, 16);
            ctx.stroke();
            ctx.beginPath();
            ctx.ellipse(3, 15, 7, 6);
            ctx.ellipse(14, 13, 7, 6);
            ctx.fill();
            break;
        case "power":
            ctx.arc(12, 13.5, 7.2, Math.PI * 0.72, Math.PI * 2.28);
            ctx.moveTo(12, 3); ctx.lineTo(12, 12);
            ctx.stroke();
            break;
        }
    }
}
